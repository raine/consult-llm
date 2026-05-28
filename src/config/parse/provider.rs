use std::collections::HashMap;

use crate::models::{PROVIDERS, Provider, ProviderSpec};

use super::super::migrate::{migrate_backend_env, migrate_prefixed_env};
use super::super::types::{Backend, ConfigError, ProviderRuntimeConfig};

/// Parse a single provider's runtime config from environment variables.
fn parse_provider_config(
    spec: &ProviderSpec,
    env: &impl Fn(&str) -> Option<String>,
    opencode_global: &Option<String>,
) -> Result<ProviderRuntimeConfig, ConfigError> {
    // 1. Resolve backend string through migration chain
    let backend_raw = if let Some(legacy_env) = spec.legacy_backend_env {
        migrate_prefixed_env(
            env(spec.backend_env).as_deref(),
            env(legacy_env).as_deref(),
            legacy_env,
            spec.backend_env,
        )
    } else {
        env(spec.backend_env)
    };

    let resolved_backend_str = if let (Some(legacy_mode), Some(cli_value)) =
        (spec.legacy_mode_env, spec.cli_backend_value)
    {
        migrate_backend_env(
            backend_raw.as_deref(),
            env(legacy_mode).as_deref(),
            cli_value,
            legacy_mode,
            spec.backend_env,
        )
    } else {
        backend_raw
    };

    // 2. Validate backend string against provider's allowed values
    if let Some(ref raw) = resolved_backend_str
        && !spec.allowed_backends.contains(&raw.as_str())
    {
        return Err(ConfigError::InvalidBackend {
            env_var: spec.backend_env.to_string(),
            raw: raw.clone(),
            allowed: spec
                .allowed_backends
                .iter()
                .map(|s| s.to_string())
                .collect(),
        });
    }

    let backend = resolved_backend_str
        .as_deref()
        .and_then(Backend::from_str)
        .unwrap_or(Backend::Api);

    // 3. API key
    let api_key = env(spec.api_key_env);

    // 4. OpenCode provider prefix
    let opencode_provider = env(spec.opencode_env)
        .or_else(|| opencode_global.clone())
        .unwrap_or_else(|| spec.default_opencode_provider.to_string());

    Ok(ProviderRuntimeConfig {
        api_key,
        backend,
        opencode_provider,
    })
}

pub fn parse_all_providers(
    env: &impl Fn(&str) -> Option<String>,
) -> Result<HashMap<Provider, ProviderRuntimeConfig>, ConfigError> {
    let opencode_global = env("CONSULT_LLM_OPENCODE_PROVIDER");

    let mut providers = HashMap::new();
    for spec in PROVIDERS {
        let provider_config = parse_provider_config(spec, env, &opencode_global)?;
        providers.insert(spec.provider, provider_config);
    }
    debug_assert_eq!(
        providers.len(),
        crate::models::ALL_PROVIDERS.len(),
        "PROVIDERS is out of sync with ALL_PROVIDERS"
    );
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::super::parse_config;
    use super::super::test_helpers::env_from;
    use super::*;

    #[test]
    fn test_parse_config_invalid_gemini_backend() {
        let env = env_from(&[
            ("CONSULT_LLM_GEMINI_BACKEND", "invalid"),
            ("GEMINI_API_KEY", "key"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBackend { ref raw, .. } if raw == "invalid"));
    }

    #[test]
    fn test_parse_config_invalid_openai_backend() {
        let env = env_from(&[
            ("CONSULT_LLM_OPENAI_BACKEND", "nope"),
            ("OPENAI_API_KEY", "key"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBackend { ref raw, .. } if raw == "nope"));
    }

    #[test]
    fn test_parse_config_invalid_deepseek_backend() {
        let env = env_from(&[
            ("CONSULT_LLM_DEEPSEEK_BACKEND", "codex-cli"),
            ("DEEPSEEK_API_KEY", "key"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBackend { ref raw, .. } if raw == "codex-cli"));
    }

    #[test]
    fn test_parse_config_invalid_anthropic_backend() {
        let env = env_from(&[
            ("CONSULT_LLM_ANTHROPIC_BACKEND", "codex-cli"),
            ("ANTHROPIC_API_KEY", "key"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBackend { ref raw, .. } if raw == "codex-cli"));
    }

    #[test]
    fn test_parse_config_invalid_grok_backend() {
        let env = env_from(&[
            ("CONSULT_LLM_GROK_BACKEND", "codex-cli"),
            ("XAI_API_KEY", "key"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBackend { ref raw, .. } if raw == "codex-cli"));
    }

    #[test]
    fn test_parse_config_with_anthropic_key() {
        let env = env_from(&[("ANTHROPIC_API_KEY", "sk-ant-test")]);
        let (config, registry) = parse_config(env).unwrap();
        assert!(
            config
                .allowed_models
                .contains(&"claude-opus-4-7".to_string())
        );
        assert_eq!(config.providers[&Provider::Anthropic].backend, Backend::Api);
        assert_eq!(
            registry.resolve_model(Some("anthropic")).unwrap(),
            "claude-opus-4-7"
        );
    }

    #[test]
    fn test_parse_config_with_grok_key() {
        let env = env_from(&[("XAI_API_KEY", "xai-test")]);
        let (config, registry) = parse_config(env).unwrap();
        assert!(config.allowed_models.contains(&"grok-4.3".to_string()));
        assert_eq!(config.providers[&Provider::Grok].backend, Backend::Api);
        assert_eq!(registry.resolve_model(Some("grok")).unwrap(), "grok-4.3");
        assert_eq!(config.default_models, vec!["grok-4.3"]);
    }

    #[test]
    fn test_parse_config_without_grok_key_filters_grok() {
        let env = env_from(&[("OPENAI_API_KEY", "key")]);
        let (config, _) = parse_config(env).unwrap();
        assert!(!config.allowed_models.contains(&"grok-4.3".to_string()));
    }

    #[test]
    fn test_parse_config_cli_backend_no_key() {
        let env = env_from(&[("CONSULT_LLM_GEMINI_BACKEND", "gemini-cli")]);
        let (config, _) = parse_config(env).unwrap();
        assert_eq!(
            config.providers[&Provider::Gemini].backend,
            Backend::GeminiCli
        );
        assert!(
            config
                .allowed_models
                .iter()
                .any(|m| m.starts_with("gemini"))
        );
    }

    #[test]
    fn test_provider_registry_completeness() {
        use crate::models::ALL_PROVIDERS;

        for provider in ALL_PROVIDERS {
            let spec = provider.spec();
            assert!(!spec.model_prefixes.is_empty());
            assert!(!spec.builtin_models.is_empty());
            assert!(!spec.allowed_backends.is_empty());
            assert!(!spec.id.is_empty());
        }

        assert_eq!(PROVIDERS.len(), ALL_PROVIDERS.len());
        let mut seen = std::collections::HashSet::new();
        for spec in PROVIDERS {
            assert!(
                seen.insert(spec.provider),
                "Duplicate ProviderSpec for {:?}",
                spec.provider
            );
        }
    }

    #[test]
    fn test_grok_provider_metadata() {
        assert_eq!(Provider::from_model("grok-4.3"), Some(Provider::Grok));
        assert_eq!(Provider::Grok.api_base_url(), Some("https://api.x.ai/v1"));
    }

    #[test]
    fn test_anthropic_provider_uses_messages_protocol() {
        assert_eq!(
            Provider::Anthropic.api_protocol(),
            crate::models::ApiProtocol::AnthropicMessages
        );
        for p in [
            Provider::OpenAI,
            Provider::Gemini,
            Provider::DeepSeek,
            Provider::MiniMax,
            Provider::Grok,
        ] {
            assert!(matches!(
                p.api_protocol(),
                crate::models::ApiProtocol::OpenAiCompat(_)
            ));
        }
    }

    #[test]
    fn test_backend_as_str_roundtrip() {
        let backends = [
            Backend::Api,
            Backend::CodexCli,
            Backend::GeminiCli,
            Backend::CursorCli,
            Backend::OpenCodeCli,
            Backend::AntigravityCli,
        ];
        for b in &backends {
            assert_eq!(Backend::from_str(b.as_str()), Some(b.clone()));
        }
    }
}
