use serde::Deserialize;
use serde::de::{Deserializer, Error as DeError};
use serde_yaml::Value;
use std::collections::{BTreeMap, HashMap};

use crate::models::{PROVIDERS, Provider};

use super::types::CliProfile;

/// Top-level keys that map to typed fields rather than provider blocks. Listed once so
/// the custom deserializer below can assert disjointness with provider IDs.
const TYPED_TOP_KEYS: &[&str] = &[
    "default_model",
    "default_models",
    "allowed_models",
    "extra_models",
    "system_prompt_path",
    "no_update_check",
    "opencode",
    "cli_profiles",
];

#[derive(Debug, Default)]
pub struct ConfigFile {
    pub default_model: Option<String>,
    pub default_models: Option<Vec<String>>,
    pub allowed_models: Option<Vec<String>>,
    pub extra_models: Option<Vec<String>>,
    pub system_prompt_path: Option<String>,
    pub no_update_check: Option<bool>,

    /// Provider-keyed YAML blocks (e.g. `gemini:`, `openai:` …). Routed by ID through the
    /// registry so adding a new provider only edits `src/models.rs`.
    pub providers: HashMap<Provider, ProviderBlock>,

    pub opencode: Option<OpencodeBlock>,

    /// Named CLI profile definitions for provider backends.
    pub cli_profiles: BTreeMap<String, CliProfile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBlock {
    pub backend: Option<String>,
    pub opencode_provider: Option<String>,
    pub reasoning_effort: Option<String>,
    pub api_key: Option<String>,
    /// Override the API base URL for the `api` backend (e.g. the z.ai
    /// coding-plan endpoint or a local proxy). Not allowed in the committed
    /// project config because it controls where API credentials are sent.
    pub base_url: Option<String>,
    /// Extra environment variables passed to the selected backend process or API transport.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Extra CLI args appended to the underlying CLI invocation (codex/gemini).
    /// Tokenized with shell-style quoting; only honored for providers whose
    /// active backend is `codex-cli` (openai) or `gemini-cli` (gemini).
    pub extra_args: Option<String>,
    /// Name of a CLI profile to use when the backend is profile-backed.
    pub cli_profile: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpencodeBlock {
    pub default_provider: Option<String>,
}

/// Whether API keys are permitted in this config file layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApiKeyPolicy {
    /// Keys are allowed (user config, project-local config).
    Allow,
    /// Keys are forbidden (committed project config).
    Forbid,
}

/// Error returned when a committed project config contains values that should not be committed.
#[derive(Debug)]
pub enum ProjectConfigPolicyError {
    /// An API key was set in the project config.
    ApiKey { provider: &'static str },
    /// A base_url override was set in a provider block in the project config.
    BaseUrl { provider: &'static str },
    /// A literal env value was set in a provider block in the project config.
    ProviderEnv { provider: &'static str },
    /// A literal env value was set in a CLI profile in the project config.
    CliProfileEnv { profile: String },
}

impl std::fmt::Display for ProjectConfigPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectConfigPolicyError::ApiKey { provider } => {
                write!(
                    f,
                    "API keys are not allowed in the shared project config (.consult-llm.yaml). \
                     Move `{}.api_key` to .consult-llm.local.yaml or ~/.config/consult-llm/config.yaml.",
                    provider
                )
            }
            ProjectConfigPolicyError::BaseUrl { provider } => {
                write!(
                    f,
                    "base_url overrides are not allowed in the shared project config (.consult-llm.yaml) \
                     because they control where API credentials are sent. \
                     Move `{}.base_url` to .consult-llm.local.yaml or ~/.config/consult-llm/config.yaml.",
                    provider
                )
            }
            ProjectConfigPolicyError::ProviderEnv { provider } => {
                write!(
                    f,
                    "Literal env values are not allowed in the shared project config (.consult-llm.yaml). \
                     Move `{}.env` to .consult-llm.local.yaml or ~/.config/consult-llm/config.yaml.",
                    provider
                )
            }
            ProjectConfigPolicyError::CliProfileEnv { profile } => {
                write!(
                    f,
                    "Literal env values are not allowed in the shared project config (.consult-llm.yaml). \
                     Move `cli_profiles.{profile}.env` to .consult-llm.local.yaml or ~/.config/consult-llm/config.yaml.",
                    profile = profile
                )
            }
        }
    }
}

impl<'de> Deserialize<'de> for ConfigFile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        let map = match value {
            Value::Null => return Ok(Self::default()),
            Value::Mapping(m) => m,
            other => {
                return Err(D::Error::custom(format!(
                    "expected a mapping at the top level, got {other:?}"
                )));
            }
        };

        let mut cfg = Self::default();
        for (k, v) in map {
            let key = match k {
                Value::String(s) => s,
                other => {
                    return Err(D::Error::custom(format!(
                        "non-string top-level key: {other:?}"
                    )));
                }
            };

            match key.as_str() {
                "default_model" => cfg.default_model = parse_inner(&key, v)?,
                "default_models" => cfg.default_models = parse_inner(&key, v)?,
                "allowed_models" => cfg.allowed_models = parse_inner(&key, v)?,
                "extra_models" => cfg.extra_models = parse_inner(&key, v)?,
                "system_prompt_path" => cfg.system_prompt_path = parse_inner(&key, v)?,
                "no_update_check" => cfg.no_update_check = parse_inner(&key, v)?,
                "opencode" => cfg.opencode = parse_inner(&key, v)?,
                "cli_profiles" => {
                    let profiles: BTreeMap<String, CliProfile> = parse_inner(&key, v)?;
                    validate_cli_profiles(&profiles).map_err(D::Error::custom)?;
                    cfg.cli_profiles = profiles;
                }
                _ => {
                    let provider = Provider::from_id(&key).ok_or_else(|| {
                        let known = known_top_keys_hint().join(", ");
                        D::Error::custom(format!("unknown field `{key}`, expected one of: {known}"))
                    })?;
                    if cfg.providers.contains_key(&provider) {
                        return Err(D::Error::custom(format!("duplicate field `{key}`")));
                    }
                    let block: ProviderBlock = parse_inner(&key, v)?;
                    validate_provider_block(provider, &block).map_err(D::Error::custom)?;
                    cfg.providers.insert(provider, block);
                }
            }
        }
        Ok(cfg)
    }
}

fn parse_inner<E, T>(field: &str, value: Value) -> Result<T, E>
where
    E: DeError,
    T: for<'de> Deserialize<'de>,
{
    serde_yaml::from_value(value)
        .map_err(|err| E::custom(format!("invalid value for `{field}`: {err}")))
}

fn is_profile_backend(provider: Provider, backend: &str) -> bool {
    backend == "profile" || (provider == Provider::Anthropic && backend == "claude-cli")
}

fn validate_provider_block(provider: Provider, block: &ProviderBlock) -> Result<(), String> {
    let spec = provider.spec();
    if block.opencode_provider.is_some() && !spec.allowed_backends.contains(&"opencode") {
        return Err(format!(
            "unsupported provider field `opencode_provider` for provider `{}`",
            spec.id
        ));
    }
    if block.reasoning_effort.is_some() && spec.reasoning_effort_env.is_none() {
        return Err(format!(
            "unsupported provider field `reasoning_effort` for provider `{}`",
            spec.id
        ));
    }
    for (key, value) in &block.env {
        if key.trim().is_empty() || key.contains('=') {
            return Err(format!(
                "invalid key `{}.env`: env keys must be non-empty and cannot contain '='",
                spec.id
            ));
        }
        validate_no_nul(&format!("{}.env.{key}", spec.id), value)?;
    }
    if block.extra_args.is_some() && spec.extra_args_env.is_none() {
        return Err(format!(
            "unsupported provider field `extra_args` for provider `{}`",
            spec.id
        ));
    }
    if let Some(backend) = &block.backend
        && !spec.allowed_backends.contains(&backend.as_str())
    {
        let allowed = spec.allowed_backends.join(" | ");
        return Err(format!(
            "invalid key `{}.backend`: expected one of: {allowed}, received '{backend}'",
            spec.id
        ));
    }
    if let Some(profile) = &block.cli_profile {
        if profile.trim().is_empty() {
            return Err(format!(
                "invalid key `{}.cli_profile`: profile name must be non-empty",
                spec.id
            ));
        }
        if let Some(backend) = block.backend.as_deref()
            && !is_profile_backend(provider, backend)
        {
            return Err(format!(
                "unsupported provider field `cli_profile` for provider `{}` with backend `{backend}`. expected one of: profile",
                spec.id,
            ));
        }
    }
    Ok(())
}

fn validate_cli_profiles(profiles: &BTreeMap<String, CliProfile>) -> Result<(), String> {
    for (name, profile) in profiles {
        if name.trim().is_empty() || name.contains('.') {
            return Err(format!(
                "invalid key `cli_profiles.{name}`: profile names must be non-empty and cannot contain '.'"
            ));
        }
        validate_no_nul(&format!("cli_profiles.{name}.command"), &profile.command)?;
        if profile.command.trim().is_empty() {
            return Err(format!(
                "invalid key `cli_profiles.{name}.command`: command must be non-empty"
            ));
        }
        for (idx, arg) in profile.args.iter().enumerate() {
            validate_no_nul(&format!("cli_profiles.{name}.args[{idx}]"), arg)?;
        }
        for (key, value) in &profile.env {
            if key.trim().is_empty() || key.contains('=') {
                return Err(format!(
                    "invalid key `cli_profiles.{name}.env`: env keys must be non-empty and cannot contain '='"
                ));
            }
            validate_no_nul(&format!("cli_profiles.{name}.env.{key}"), value)?;
        }
        if let Some(model_env) = &profile.model_env {
            if model_env.trim().is_empty() || model_env.contains('=') {
                return Err(format!(
                    "invalid key `cli_profiles.{name}.model_env`: env keys must be non-empty and cannot contain '='"
                ));
            }
            validate_no_nul(&format!("cli_profiles.{name}.model_env"), model_env)?;
        }
    }
    Ok(())
}

fn validate_no_nul(path: &str, value: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("invalid key `{path}`: value contains NUL byte"));
    }
    Ok(())
}

/// Hint shown to users when an unknown top-level key is encountered. Combines the
/// fixed top-level keys with every registered provider id, so the error is accurate
/// even after providers are added.
fn known_top_keys_hint() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = TYPED_TOP_KEYS.to_vec();
    for spec in PROVIDERS {
        keys.push(spec.id);
    }
    keys
}

impl ConfigFile {
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        if yaml.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_yaml::from_str(yaml)
    }

    /// Convert the config file to an env-var map.
    ///
    /// `policy` controls whether `api_key` fields are permitted. Pass
    /// `ApiKeyPolicy::Forbid` for `.consult-llm.yaml` (committed project
    /// config) and `ApiKeyPolicy::Allow` for `.consult-llm.local.yaml` and
    /// user config.
    ///
    /// Empty `api_key` values are silently skipped (consistent with how the
    /// env-var layer treats empty environment variables).
    pub fn to_env_map(
        &self,
        policy: ApiKeyPolicy,
    ) -> Result<HashMap<String, String>, ProjectConfigPolicyError> {
        let mut m = HashMap::new();
        if let Some(v) = &self.default_model {
            m.insert("CONSULT_LLM_DEFAULT_MODEL".into(), v.clone());
        }
        if let Some(v) = &self.default_models {
            m.insert("CONSULT_LLM_DEFAULT_MODELS".into(), v.join(","));
        }
        if let Some(v) = &self.allowed_models {
            m.insert("CONSULT_LLM_ALLOWED_MODELS".into(), v.join(","));
        }
        if let Some(v) = &self.extra_models {
            m.insert("CONSULT_LLM_EXTRA_MODELS".into(), v.join(","));
        }
        if let Some(v) = &self.system_prompt_path {
            m.insert("CONSULT_LLM_SYSTEM_PROMPT_PATH".into(), v.clone());
        }
        if let Some(v) = self.no_update_check {
            m.insert(
                "CONSULT_LLM_NO_UPDATE_CHECK".into(),
                if v { "1".into() } else { "0".into() },
            );
        }

        // Iterate the registry, every provider's YAML block routes through here, so
        // adding a provider in `src/models.rs` is enough; this loop never grows.
        for spec in PROVIDERS {
            let Some(block) = self.providers.get(&spec.provider) else {
                continue;
            };

            if let Some(v) = &block.api_key {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    if policy == ApiKeyPolicy::Forbid {
                        return Err(ProjectConfigPolicyError::ApiKey { provider: spec.id });
                    }
                    m.insert(spec.api_key_env.to_string(), trimmed.to_string());
                }
                // blank/whitespace-only: silently skip (treat as unset)
            }

            if let Some(v) = &block.base_url {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    if policy == ApiKeyPolicy::Forbid {
                        return Err(ProjectConfigPolicyError::BaseUrl { provider: spec.id });
                    }
                    m.insert(spec.base_url_env.to_string(), trimmed.to_string());
                }
                // blank/whitespace-only: silently skip (treat as unset)
            }

            if policy == ApiKeyPolicy::Forbid && !block.env.is_empty() {
                return Err(ProjectConfigPolicyError::ProviderEnv { provider: spec.id });
            }

            if !block.env.is_empty() {
                let value = serde_json::to_string(&block.env).expect("provider env map serializes");
                m.insert(
                    format!("CONSULT_LLM_{}_ENV", spec.id.to_ascii_uppercase()),
                    value,
                );
            }

            if let Some(v) = &block.backend {
                m.insert(spec.backend_env.to_string(), v.clone());
            }
            if let Some(v) = &block.opencode_provider {
                m.insert(spec.opencode_env.to_string(), v.clone());
            }
            if let (Some(env), Some(v)) = (spec.reasoning_effort_env, &block.reasoning_effort) {
                m.insert(env.to_string(), v.clone());
            }
            if let (Some(env), Some(v)) = (spec.extra_args_env, &block.extra_args) {
                m.insert(env.to_string(), v.clone());
            }
            if let Some(v) = &block.cli_profile {
                m.insert(spec.cli_profile_env.to_string(), v.clone());
            }
        }

        // Reject literal env values in CLI profiles in project config.
        if policy == ApiKeyPolicy::Forbid {
            for (name, profile) in &self.cli_profiles {
                if !profile.env.is_empty() {
                    return Err(ProjectConfigPolicyError::CliProfileEnv {
                        profile: name.clone(),
                    });
                }
            }
        }

        if let Some(oc) = &self.opencode
            && let Some(v) = &oc.default_provider
        {
            m.insert("CONSULT_LLM_OPENCODE_PROVIDER".into(), v.clone());
        }

        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{CliProfileInterface, CliPromptMode};

    /// Test helper: build a `ConfigFile` with the given provider blocks. Keeps tests
    /// readable now that provider blocks live in a HashMap rather than typed fields.
    fn cfg_with(blocks: Vec<(Provider, ProviderBlock)>) -> ConfigFile {
        let mut cfg = ConfigFile::default();
        for (p, b) in blocks {
            cfg.providers.insert(p, b);
        }
        cfg
    }

    #[test]
    fn test_parse_rejects_unknown_top_level_keys() {
        let yaml = "unknown_key: value\n";
        assert!(ConfigFile::parse(yaml).is_err());
    }

    #[test]
    fn test_parse_rejects_unknown_provider_block_keys() {
        let yaml = "gemini:\n  unknown_field: value\n";
        assert!(ConfigFile::parse(yaml).is_err());
    }

    #[test]
    fn test_typed_top_keys_disjoint_from_provider_ids() {
        // Provider ids must not collide with the reserved typed top-level keys; if a
        // future provider id matches one of these, the custom deserializer would
        // silently route the YAML block away from the providers map.
        for spec in PROVIDERS {
            assert!(
                !TYPED_TOP_KEYS.contains(&spec.id),
                "provider id {:?} collides with reserved top-level key",
                spec.id
            );
        }
    }

    #[test]
    fn test_parse_rejects_unknown_field_under_provider_block_via_new_path() {
        // Pin the custom-Deserialize routing: an unknown inner field on a provider
        // block must still be rejected by ProviderBlock's deny_unknown_fields.
        let yaml = "openai:\n  not_a_real_field: oops\n";
        assert!(ConfigFile::parse(yaml).is_err());
    }

    #[test]
    fn test_parse_rejects_unsupported_reasoning_effort_for_provider() {
        let err = ConfigFile::parse("gemini:\n  reasoning_effort: high\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("gemini"));
        assert!(msg.contains("reasoning_effort"));
        assert!(msg.contains("unsupported"));
    }

    #[test]
    fn test_parse_rejects_unsupported_extra_args_for_provider() {
        let err = ConfigFile::parse("deepseek:\n  extra_args: --verbose\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("deepseek"));
        assert!(msg.contains("extra_args"));
        assert!(msg.contains("unsupported"));
    }

    #[test]
    fn test_parse_rejects_unsupported_opencode_provider_for_provider() {
        let err = ConfigFile::parse("anthropic:\n  opencode_provider: anthropic\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("anthropic"));
        assert!(msg.contains("opencode_provider"));
        assert!(msg.contains("unsupported"));
    }

    #[test]
    fn test_parse_accepts_opencode_provider_for_opencode_capable_provider_when_backend_is_api() {
        let cfg =
            ConfigFile::parse("deepseek:\n  backend: api\n  opencode_provider: custom\n").unwrap();
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["CONSULT_LLM_OPENCODE_DEEPSEEK_PROVIDER"], "custom");
    }

    #[test]
    fn test_parse_rejects_every_unsupported_provider_specific_field() {
        for spec in PROVIDERS {
            for field in ["opencode_provider", "reasoning_effort", "extra_args"] {
                let supported = match field {
                    "opencode_provider" => spec.allowed_backends.contains(&"opencode"),
                    "reasoning_effort" => spec.reasoning_effort_env.is_some(),
                    "extra_args" => spec.extra_args_env.is_some(),
                    _ => unreachable!(),
                };
                if supported {
                    continue;
                }

                let yaml = format!("{}:\n  {field}: value\n", spec.id);
                let err = ConfigFile::parse(&yaml).unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains(spec.id),
                    "{field} error should name {}: {msg}",
                    spec.id
                );
                assert!(
                    msg.contains(field),
                    "{field} error should name field: {msg}"
                );
                assert!(
                    msg.contains("unsupported"),
                    "{field} error should say unsupported: {msg}"
                );
            }
        }
    }

    #[test]
    fn test_parse_rejects_duplicate_provider_block() {
        let yaml = "gemini:\n  backend: api\ngemini:\n  backend: gemini-cli\n";
        // serde_yaml itself rejects duplicate mapping keys; this test pins that.
        assert!(ConfigFile::parse(yaml).is_err());
    }

    #[test]
    fn test_parse_routes_every_provider_block() {
        // Round-trip: every currently-supported provider block parses, gets routed into
        // the providers map keyed by the registry's Provider variant, and survives
        // to_env_map(Allow). This is the all-providers fixture required by the phase.
        let yaml = r#"
default_model: gemini
default_models:
  - gemini
  - openai
  - openai
allowed_models:
  - gemini-2.5-pro
  - gpt-5.2
extra_models:
  - custom-model
system_prompt_path: /path/to/prompt.md
no_update_check: true

gemini:
  backend: gemini-cli
  opencode_provider: google
  api_key: gem-key
  env:
    GEMINI_CONFIG_DIR: /tmp/gemini
  extra_args: --yolo
openai:
  backend: api
  reasoning_effort: high
  api_key: sk-test
  env:
    OPENAI_TEST_ENV: value
  extra_args: --dangerously-bypass-approvals-and-sandbox
anthropic:
  backend: api
  reasoning_effort: x-high
  api_key: sk-ant-test
deepseek:
  backend: api
  api_key: ds-test
minimax:
  backend: api
  api_key: mm-test
grok:
  backend: api
  api_key: xai-test
opencode:
  default_provider: copilot
openrouter:
  backend: api
  api_key: or-test
zai:
  backend: profile
  cli_profile: claude-zai
  base_url: https://api.z.ai/api/coding/paas/v4
"#;
        let cfg = ConfigFile::parse(yaml).expect("parses");
        assert_eq!(cfg.providers.len(), 8);
        for spec in PROVIDERS {
            assert!(
                cfg.providers.contains_key(&spec.provider),
                "missing provider {} after parse",
                spec.id
            );
        }

        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["CONSULT_LLM_DEFAULT_MODEL"], "gemini");
        // Duplicates in default_models are intentionally preserved.
        assert_eq!(m["CONSULT_LLM_DEFAULT_MODELS"], "gemini,openai,openai");
        assert_eq!(m["CONSULT_LLM_ALLOWED_MODELS"], "gemini-2.5-pro,gpt-5.2");
        assert_eq!(m["CONSULT_LLM_EXTRA_MODELS"], "custom-model");
        assert_eq!(m["CONSULT_LLM_SYSTEM_PROMPT_PATH"], "/path/to/prompt.md");
        assert_eq!(m["CONSULT_LLM_NO_UPDATE_CHECK"], "1");

        // Per-provider API keys land at the registry-declared env names.
        assert_eq!(m["GEMINI_API_KEY"], "gem-key");
        assert_eq!(m["OPENAI_API_KEY"], "sk-test");
        assert_eq!(m["ANTHROPIC_API_KEY"], "sk-ant-test");
        assert_eq!(m["DEEPSEEK_API_KEY"], "ds-test");
        assert_eq!(m["MINIMAX_API_KEY"], "mm-test");
        assert_eq!(m["XAI_API_KEY"], "xai-test");

        // Backends.
        assert_eq!(m["CONSULT_LLM_GEMINI_BACKEND"], "gemini-cli");
        assert_eq!(m["CONSULT_LLM_OPENAI_BACKEND"], "api");
        assert_eq!(m["CONSULT_LLM_ANTHROPIC_BACKEND"], "api");
        assert_eq!(m["CONSULT_LLM_DEEPSEEK_BACKEND"], "api");
        assert_eq!(m["CONSULT_LLM_MINIMAX_BACKEND"], "api");
        assert_eq!(m["CONSULT_LLM_GROK_BACKEND"], "api");

        // Reasoning + extra args use the spec-declared env names.
        assert_eq!(m["CONSULT_LLM_CODEX_REASONING_EFFORT"], "high");
        assert_eq!(m["CONSULT_LLM_CLAUDE_REASONING_EFFORT"], "x-high");
        assert_eq!(
            m["CONSULT_LLM_CODEX_EXTRA_ARGS"],
            "--dangerously-bypass-approvals-and-sandbox"
        );
        assert_eq!(m["CONSULT_LLM_GEMINI_EXTRA_ARGS"], "--yolo");
        assert_eq!(m["CONSULT_LLM_OPENCODE_GEMINI_PROVIDER"], "google");
        assert_eq!(
            m["CONSULT_LLM_GEMINI_ENV"],
            r#"{"GEMINI_CONFIG_DIR":"/tmp/gemini"}"#
        );
        assert_eq!(
            m["CONSULT_LLM_OPENAI_ENV"],
            r#"{"OPENAI_TEST_ENV":"value"}"#
        );

        let openrouter = ConfigFile::parse("openrouter:\n  reasoning_effort: high\n").unwrap();
        let openrouter_env = openrouter.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(
            openrouter_env["CONSULT_LLM_OPENROUTER_REASONING_EFFORT"],
            "high"
        );

        // Opencode global default.
        assert_eq!(m["CONSULT_LLM_OPENCODE_PROVIDER"], "copilot");

        // base_url override lands at the spec-declared env name.
        assert_eq!(
            m["CONSULT_LLM_ZAI_BASE_URL"],
            "https://api.z.ai/api/coding/paas/v4"
        );
    }

    #[test]
    fn test_base_url_maps_to_env() {
        let cfg = cfg_with(vec![(
            Provider::Zai,
            ProviderBlock {
                base_url: Some("https://api.z.ai/api/coding/paas/v4".into()),
                ..Default::default()
            },
        )]);
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(
            m["CONSULT_LLM_ZAI_BASE_URL"],
            "https://api.z.ai/api/coding/paas/v4"
        );
    }

    #[test]
    fn test_blank_base_url_skipped() {
        let cfg = cfg_with(vec![(
            Provider::Zai,
            ProviderBlock {
                base_url: Some("   ".into()),
                ..Default::default()
            },
        )]);
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert!(!m.contains_key("CONSULT_LLM_ZAI_BASE_URL"));
    }

    #[test]
    fn test_base_url_rejected_in_project_layer() {
        let cfg = cfg_with(vec![(
            Provider::Zai,
            ProviderBlock {
                base_url: Some("https://attacker.invalid/v1".into()),
                ..Default::default()
            },
        )]);
        let err = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap_err();
        assert!(matches!(
            err,
            ProjectConfigPolicyError::BaseUrl { provider } if provider == "zai"
        ));
    }

    #[test]
    fn test_no_update_check_false_emits_zero() {
        let yaml = "no_update_check: false\n";
        let cfg = ConfigFile::parse(yaml).unwrap();
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["CONSULT_LLM_NO_UPDATE_CHECK"], "0");
    }

    #[test]
    fn test_to_env_map_skips_unset_fields() {
        let m = ConfigFile::default()
            .to_env_map(ApiKeyPolicy::Allow)
            .unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_api_key_emitted_in_allow_layer() {
        let cfg = cfg_with(vec![(
            Provider::OpenAI,
            ProviderBlock {
                api_key: Some("sk-test".into()),
                ..Default::default()
            },
        )]);
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["OPENAI_API_KEY"], "sk-test");
    }

    #[test]
    fn test_api_key_rejected_in_project_layer() {
        let cfg = cfg_with(vec![(
            Provider::Gemini,
            ProviderBlock {
                api_key: Some("key-abc".into()),
                ..Default::default()
            },
        )]);
        let err = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap_err();
        assert!(
            matches!(err, ProjectConfigPolicyError::ApiKey { provider } if provider == "gemini")
        );
    }

    #[test]
    fn test_grok_api_key_rejected_in_project_layer() {
        let cfg = cfg_with(vec![(
            Provider::Grok,
            ProviderBlock {
                api_key: Some("xai-test".into()),
                ..Default::default()
            },
        )]);
        let err = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap_err();
        assert!(matches!(err, ProjectConfigPolicyError::ApiKey { provider } if provider == "grok"));
    }

    #[test]
    fn test_blank_api_key_silently_skipped() {
        for blank in &["", "   ", "\t"] {
            let cfg = cfg_with(vec![(
                Provider::OpenAI,
                ProviderBlock {
                    api_key: Some((*blank).into()),
                    ..Default::default()
                },
            )]);
            // Not an error even with Forbid, blank key is not a secret
            let m = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap();
            assert!(
                !m.contains_key("OPENAI_API_KEY"),
                "blank {:?} should be skipped",
                blank
            );
            // Also skipped with Allow
            let m2 = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
            assert!(
                !m2.contains_key("OPENAI_API_KEY"),
                "blank {:?} should be skipped",
                blank
            );
        }
    }

    #[test]
    fn test_api_key_is_trimmed() {
        let cfg = cfg_with(vec![(
            Provider::OpenAI,
            ProviderBlock {
                api_key: Some("  sk-padded  ".into()),
                ..Default::default()
            },
        )]);
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["OPENAI_API_KEY"], "sk-padded");
    }

    #[test]
    fn test_api_key_canonical_env_names() {
        let cfg = cfg_with(vec![
            (
                Provider::Gemini,
                ProviderBlock {
                    api_key: Some("x".into()),
                    ..Default::default()
                },
            ),
            (
                Provider::Anthropic,
                ProviderBlock {
                    api_key: Some("x".into()),
                    ..Default::default()
                },
            ),
            (
                Provider::DeepSeek,
                ProviderBlock {
                    api_key: Some("x".into()),
                    ..Default::default()
                },
            ),
            (
                Provider::MiniMax,
                ProviderBlock {
                    api_key: Some("x".into()),
                    ..Default::default()
                },
            ),
            (
                Provider::Grok,
                ProviderBlock {
                    api_key: Some("x".into()),
                    ..Default::default()
                },
            ),
        ]);
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert!(m.contains_key("GEMINI_API_KEY"));
        assert!(m.contains_key("ANTHROPIC_API_KEY"));
        assert!(m.contains_key("DEEPSEEK_API_KEY"));
        assert!(m.contains_key("MINIMAX_API_KEY"));
        assert!(m.contains_key("XAI_API_KEY"));
    }

    // --- CLI profile tests ---

    #[test]
    fn test_parse_cli_profiles_full_shape() {
        let cfg = ConfigFile::parse(
            r#"
cli_profiles:
  claude:
    command: claude
    args: ["-p", "--output-format", "stream-json"]
    env:
      CLAUDE_CONFIG_DIR: /tmp/claude
    interface: stream-json
    prompt: stdin
"#,
        )
        .unwrap();
        let profile = &cfg.cli_profiles["claude"];
        assert_eq!(profile.command, "claude");
        assert_eq!(profile.args, vec!["-p", "--output-format", "stream-json"]);
        assert_eq!(profile.env["CLAUDE_CONFIG_DIR"], "/tmp/claude");
        assert_eq!(profile.interface, CliProfileInterface::StreamJson);
        assert_eq!(profile.prompt, CliPromptMode::Stdin);
    }

    #[test]
    fn test_provider_cli_profile_maps_to_env_key() {
        let cfg =
            ConfigFile::parse("anthropic:\n  backend: profile\n  cli_profile: claude\n").unwrap();
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["CONSULT_LLM_ANTHROPIC_CLI_PROFILE"], "claude");
    }

    #[test]
    fn test_parse_rejects_empty_cli_profile_name() {
        let err = ConfigFile::parse(
            r#"
cli_profiles:
  "":
    command: claude
    interface: text
    prompt: stdin
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cli_profiles."));
        assert!(msg.contains("non-empty"));
    }

    #[test]
    fn test_parse_rejects_dot_in_cli_profile_name() {
        let err = ConfigFile::parse(
            r#"
cli_profiles:
  my.profile:
    command: claude
    interface: text
    prompt: stdin
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cannot contain '.'"));
    }

    #[test]
    fn test_parse_rejects_empty_cli_profile_command() {
        let err = ConfigFile::parse(
            r#"
cli_profiles:
  test:
    command: ""
    interface: text
    prompt: stdin
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("command"));
        assert!(msg.contains("non-empty"));
    }

    #[test]
    fn test_parse_rejects_unknown_cli_profile_field() {
        let err = ConfigFile::parse(
            r#"
cli_profiles:
  test:
    command: claude
    interface: text
    prompt: stdin
    unknown_field: oops
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown_field"));
    }

    #[test]
    fn test_parse_rejects_blank_provider_cli_profile() {
        let err = ConfigFile::parse("anthropic:\n  cli_profile: \"\"\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cli_profile"));
        assert!(msg.contains("non-empty"));
    }

    #[test]
    fn test_parse_accepts_cli_profile_without_backend_for_layered_config() {
        let cfg = ConfigFile::parse("anthropic:\n  cli_profile: claude\n").unwrap();
        let m = cfg.to_env_map(ApiKeyPolicy::Allow).unwrap();
        assert_eq!(m["CONSULT_LLM_ANTHROPIC_CLI_PROFILE"], "claude");
    }

    #[test]
    fn test_parse_rejects_unsupported_cli_profile_on_explicit_non_profile_backend() {
        let err =
            ConfigFile::parse("anthropic:\n  backend: api\n  cli_profile: claude\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cli_profile"));
        assert!(msg.contains("unsupported"));
    }

    #[test]
    fn test_project_config_rejects_cli_profile_env() {
        let cfg = ConfigFile::parse(
            r#"
cli_profiles:
  claude:
    command: claude
    interface: text
    prompt: stdin
    env:
      SECRET: s3cret
"#,
        )
        .unwrap();
        let err = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap_err();
        assert!(matches!(
            err,
            ProjectConfigPolicyError::CliProfileEnv { .. }
        ));
        let msg = err.to_string();
        assert!(msg.contains("cli_profiles"));
        assert!(msg.contains(".env"));
    }

    #[test]
    fn test_project_config_rejects_provider_env() {
        let cfg = ConfigFile::parse(
            r#"
openai:
  env:
    OPENCODE_PERMISSION: '{}'
"#,
        )
        .unwrap();
        let err = cfg.to_env_map(ApiKeyPolicy::Forbid).unwrap_err();
        assert!(matches!(
            err,
            ProjectConfigPolicyError::ProviderEnv { provider } if provider == "openai"
        ));
        let msg = err.to_string();
        assert!(msg.contains("openai.env"));
        assert!(msg.contains(".consult-llm.local.yaml"));
    }

    #[test]
    fn test_parse_rejects_invalid_backend_value_in_config() {
        let err = ConfigFile::parse("anthropic:\n  backend: invalid-value\n").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("anthropic.backend"));
        assert!(msg.contains("invalid-value"));
    }
}
