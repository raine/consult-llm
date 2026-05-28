use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskMode {
    Review,
    Debug,
    Plan,
    Create,
    General,
}

impl Default for TaskMode {
    fn default() -> Self {
        Self::General
    }
}

/// Known LLM provider families, determined by model ID prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    OpenAI,
    Gemini,
    DeepSeek,
    MiniMax,
    Anthropic,
    Grok,
    Antigravity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkTagSpec {
    pub start: &'static str,
    pub end: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiExtraBody {
    GoogleThinkingConfig,
}

impl OpenAiExtraBody {
    pub fn applies_to_model(self, model: &str) -> bool {
        match self {
            Self::GoogleThinkingConfig => is_gemini_3_model(model),
        }
    }
}

fn is_gemini_3_model(model: &str) -> bool {
    model
        .strip_prefix("gemini-")
        .is_some_and(|rest| rest == "3" || rest.starts_with("3-") || rest.starts_with("3."))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenAiCompatRuntime {
    pub extra_body: Option<OpenAiExtraBody>,
    pub think_tags: Option<ThinkTagSpec>,
}

/// HTTP wire-format family used when calling the provider's native API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiProtocol {
    /// OpenAI-compatible `/chat/completions` — used by OpenAI, Gemini (OpenAI-compat
    /// endpoint), DeepSeek, MiniMax.
    OpenAiCompat(OpenAiCompatRuntime),
    /// Anthropic `/v1/messages` — `x-api-key` auth, top-level `system`, content-block
    /// responses, distinct usage shape.
    AnthropicMessages,
}

/// Static metadata for a provider — the single place to define provider-specific constants.
/// Adding a new provider means: add a variant to `Provider`, add a `ProviderSpec` to `PROVIDERS`,
/// and add the variant to `ALL_PROVIDERS`. Nothing else outside `models.rs` needs to change
/// (unless the provider introduces a brand-new `ApiProtocol`, which adds one match arm in `llm.rs`).
pub struct ProviderSpec {
    pub provider: Provider,
    /// Lowercase identifier used for logging, YAML keys, and config key generation (e.g. "openai").
    pub id: &'static str,
    /// Prefixes that identify this provider's models (e.g. &["gpt-", "o3-"]).
    pub model_prefixes: &'static [&'static str],
    /// API base URL override (None = default OpenAI-compatible URL).
    pub api_base_url: Option<&'static str>,
    /// API wire format — picks which executor to instantiate in `Backend::Api` mode.
    pub api_protocol: ApiProtocol,
    /// Built-in model IDs shipped with this provider.
    pub builtin_models: &'static [&'static str],
    /// Selector resolution priority: ordered model IDs (best first). Used by
    /// `catalog::resolve_selector` so that e.g. `-m openai` picks the highest-quality
    /// available model. Distinct from `builtin_models` because catalog order doubles as
    /// a stability/fallback hint, not a quality ranking.
    pub selector_priorities: &'static [&'static str],
    /// Env var for the API key (e.g. "OPENAI_API_KEY").
    pub api_key_env: &'static str,
    /// Prefixed backend env var (e.g. "CONSULT_LLM_OPENAI_BACKEND").
    pub backend_env: &'static str,
    /// Legacy unprefixed backend env var, if any (e.g. "OPENAI_BACKEND").
    pub legacy_backend_env: Option<&'static str>,
    /// Legacy mode env var, if any (e.g. "OPENAI_MODE").
    pub legacy_mode_env: Option<&'static str>,
    /// CLI backend value used when migrating legacy mode env (e.g. "codex-cli").
    pub cli_backend_value: Option<&'static str>,
    /// Allowed backend string values for validation.
    pub allowed_backends: &'static [&'static str],
    /// Per-provider opencode provider env var (e.g. "CONSULT_LLM_OPENCODE_OPENAI_PROVIDER").
    pub opencode_env: &'static str,
    /// Default opencode provider prefix (e.g. "openai").
    pub default_opencode_provider: &'static str,
    /// Env var that carries the YAML-block `reasoning_effort` value, if this provider
    /// exposes one (currently only `openai` → `CONSULT_LLM_CODEX_REASONING_EFFORT`).
    pub reasoning_effort_env: Option<&'static str>,
    /// Env var that carries the YAML-block `extra_args` value, if this provider exposes
    /// one (openai → `CONSULT_LLM_CODEX_EXTRA_ARGS`, gemini → `CONSULT_LLM_GEMINI_EXTRA_ARGS`).
    pub extra_args_env: Option<&'static str>,
}

/// All known providers in deterministic order. Derived integrity tests verify this
/// matches `PROVIDERS` and that every variant has exactly one spec.
pub const ALL_PROVIDERS: &[Provider] = &[
    Provider::Gemini,
    Provider::DeepSeek,
    Provider::OpenAI,
    Provider::MiniMax,
    Provider::Anthropic,
    Provider::Grok,
    Provider::Antigravity,
];

/// The provider registry. Order matters: `all_builtin_models()` flattens in this order,
/// which determines the fallback model when gpt-5.2 is not available (first enabled wins).
pub static PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        provider: Provider::Gemini,
        id: "gemini",
        model_prefixes: &["gemini-"],
        api_base_url: Some("https://generativelanguage.googleapis.com/v1beta/openai/"),
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: Some(OpenAiExtraBody::GoogleThinkingConfig),
            think_tags: Some(ThinkTagSpec {
                start: "<thought>",
                end: "</thought>",
            }),
        }),
        builtin_models: &[
            "gemini-2.5-pro",
            "gemini-3-pro-preview",
            "gemini-3.1-pro-preview",
        ],
        selector_priorities: &[
            "gemini-3.1-pro-preview",
            "gemini-3-pro-preview",
            "gemini-2.5-pro",
        ],
        api_key_env: "GEMINI_API_KEY",
        backend_env: "CONSULT_LLM_GEMINI_BACKEND",
        legacy_backend_env: Some("GEMINI_BACKEND"),
        legacy_mode_env: Some("GEMINI_MODE"),
        cli_backend_value: Some("gemini-cli"),
        allowed_backends: &["api", "gemini-cli", "cursor-cli", "opencode"],
        opencode_env: "CONSULT_LLM_OPENCODE_GEMINI_PROVIDER",
        default_opencode_provider: "google",
        reasoning_effort_env: None,
        extra_args_env: Some("CONSULT_LLM_GEMINI_EXTRA_ARGS"),
    },
    ProviderSpec {
        provider: Provider::DeepSeek,
        id: "deepseek",
        model_prefixes: &["deepseek-"],
        api_base_url: Some("https://api.deepseek.com"),
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: None,
            think_tags: None,
        }),
        builtin_models: &["deepseek-v4-pro"],
        selector_priorities: &["deepseek-v4-pro"],
        api_key_env: "DEEPSEEK_API_KEY",
        backend_env: "CONSULT_LLM_DEEPSEEK_BACKEND",
        legacy_backend_env: None,
        legacy_mode_env: None,
        cli_backend_value: None,
        allowed_backends: &["api", "opencode"],
        opencode_env: "CONSULT_LLM_OPENCODE_DEEPSEEK_PROVIDER",
        default_opencode_provider: "deepseek",
        reasoning_effort_env: None,
        extra_args_env: None,
    },
    ProviderSpec {
        provider: Provider::OpenAI,
        id: "openai",
        model_prefixes: &["gpt-"],
        api_base_url: None,
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: None,
            think_tags: None,
        }),
        builtin_models: &[
            "gpt-5.2",
            "gpt-5.4",
            "gpt-5.5",
            "gpt-5.3-codex",
            "gpt-5.2-codex",
        ],
        selector_priorities: &[
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.3-codex",
            "gpt-5.2",
            "gpt-5.2-codex",
        ],
        api_key_env: "OPENAI_API_KEY",
        backend_env: "CONSULT_LLM_OPENAI_BACKEND",
        legacy_backend_env: Some("OPENAI_BACKEND"),
        legacy_mode_env: Some("OPENAI_MODE"),
        cli_backend_value: Some("codex-cli"),
        allowed_backends: &["api", "codex-cli", "cursor-cli", "opencode"],
        opencode_env: "CONSULT_LLM_OPENCODE_OPENAI_PROVIDER",
        default_opencode_provider: "openai",
        reasoning_effort_env: Some("CONSULT_LLM_CODEX_REASONING_EFFORT"),
        extra_args_env: Some("CONSULT_LLM_CODEX_EXTRA_ARGS"),
    },
    ProviderSpec {
        provider: Provider::MiniMax,
        id: "minimax",
        model_prefixes: &["MiniMax-"],
        api_base_url: Some("https://api.minimax.io/v1"),
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: None,
            think_tags: Some(ThinkTagSpec {
                start: "<think>",
                end: "</think>",
            }),
        }),
        builtin_models: &["MiniMax-M2.7"],
        selector_priorities: &["MiniMax-M2.7"],
        api_key_env: "MINIMAX_API_KEY",
        backend_env: "CONSULT_LLM_MINIMAX_BACKEND",
        legacy_backend_env: None,
        legacy_mode_env: None,
        cli_backend_value: None,
        allowed_backends: &["api", "opencode"],
        opencode_env: "CONSULT_LLM_OPENCODE_MINIMAX_PROVIDER",
        default_opencode_provider: "minimax",
        reasoning_effort_env: None,
        extra_args_env: None,
    },
    ProviderSpec {
        provider: Provider::Anthropic,
        id: "anthropic",
        model_prefixes: &["claude-"],
        api_base_url: Some("https://api.anthropic.com"),
        api_protocol: ApiProtocol::AnthropicMessages,
        builtin_models: &["claude-opus-4-7"],
        selector_priorities: &["claude-opus-4-7"],
        api_key_env: "ANTHROPIC_API_KEY",
        backend_env: "CONSULT_LLM_ANTHROPIC_BACKEND",
        legacy_backend_env: None,
        legacy_mode_env: None,
        cli_backend_value: None,
        allowed_backends: &["api"],
        opencode_env: "CONSULT_LLM_OPENCODE_ANTHROPIC_PROVIDER",
        default_opencode_provider: "anthropic",
        reasoning_effort_env: None,
        extra_args_env: None,
    },
    ProviderSpec {
        provider: Provider::Grok,
        id: "grok",
        model_prefixes: &["grok-"],
        api_base_url: Some("https://api.x.ai/v1"),
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: None,
            think_tags: None,
        }),
        builtin_models: &["grok-4.3"],
        selector_priorities: &["grok-4.3"],
        api_key_env: "XAI_API_KEY",
        backend_env: "CONSULT_LLM_GROK_BACKEND",
        legacy_backend_env: None,
        legacy_mode_env: None,
        cli_backend_value: None,
        allowed_backends: &["api"],
        opencode_env: "CONSULT_LLM_OPENCODE_GROK_PROVIDER",
        default_opencode_provider: "xai",
        reasoning_effort_env: None,
        extra_args_env: None,
    },
    // Antigravity CLI (`agy --print`). CLI-only — there is no public API and
    // `agy` v1.0.3 has no CLI flag for model selection: the active model is
    // whatever was last picked in the Antigravity IDE GUI (persisted under
    // ~/Library/Application Support/Antigravity/.../state.vscdb). So we expose
    // a single `agy` model; users switch model from the IDE side.
    // `api_protocol` is a placeholder — only `antigravity-cli` is allowed.
    ProviderSpec {
        provider: Provider::Antigravity,
        id: "antigravity",
        model_prefixes: &["agy"],
        api_base_url: None,
        api_protocol: ApiProtocol::OpenAiCompat(OpenAiCompatRuntime {
            extra_body: None,
            think_tags: None,
        }),
        builtin_models: &["agy"],
        selector_priorities: &["agy"],
        api_key_env: "ANTIGRAVITY_API_KEY",
        backend_env: "CONSULT_LLM_ANTIGRAVITY_BACKEND",
        legacy_backend_env: None,
        legacy_mode_env: None,
        cli_backend_value: None,
        allowed_backends: &["antigravity-cli"],
        opencode_env: "CONSULT_LLM_OPENCODE_ANTIGRAVITY_PROVIDER",
        default_opencode_provider: "antigravity",
        reasoning_effort_env: None,
        extra_args_env: Some("CONSULT_LLM_AGY_EXTRA_ARGS"),
    },
];

impl Provider {
    /// Look up the static spec for this provider.
    pub fn spec(&self) -> &'static ProviderSpec {
        PROVIDERS
            .iter()
            .find(|s| s.provider == *self)
            .expect("every Provider variant must have a ProviderSpec entry")
    }

    /// Determine the provider for a model ID based on its prefix.
    pub fn from_model(model: &str) -> Option<Self> {
        PROVIDERS
            .iter()
            .find(|spec| spec.model_prefixes.iter().any(|p| model.starts_with(p)))
            .map(|spec| spec.provider)
    }

    /// Look up a provider by its short id (e.g. "openai", "gemini"). Used by config-file
    /// deserialization to validate provider-block keys against the registry.
    pub fn from_id(id: &str) -> Option<Self> {
        PROVIDERS
            .iter()
            .find(|spec| spec.id == id)
            .map(|spec| spec.provider)
    }

    /// API base URL for this provider (when using API backend).
    pub fn api_base_url(&self) -> Option<&'static str> {
        self.spec().api_base_url
    }

    /// API wire format for this provider (when using API backend).
    pub fn api_protocol(&self) -> ApiProtocol {
        self.spec().api_protocol
    }
}

/// Collect all builtin model IDs from the provider registry, in deterministic order.
pub fn all_builtin_models() -> Vec<&'static str> {
    PROVIDERS
        .iter()
        .flat_map(|spec| spec.builtin_models.iter().copied())
        .collect()
}

/// Iterate (selector_id, priority_list) for every provider in registry order.
/// Drives selector resolution in `catalog::resolve_selector` and selector listings
/// in error messages.
pub fn selector_priorities() -> impl Iterator<Item = (&'static str, &'static [&'static str])> + Clone
{
    PROVIDERS.iter().map(|s| (s.id, s.selector_priorities))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Golden table mapping every builtin model to its expected provider.
    /// This is the stability anchor for the `provider-registry` phase: if
    /// model→provider routing changes for any current model, this test
    /// fails before the change ships.
    #[test]
    fn model_to_provider_golden() {
        let expected: &[(&str, Provider)] = &[
            ("gemini-2.5-pro", Provider::Gemini),
            ("gemini-3-pro-preview", Provider::Gemini),
            ("gemini-3.1-pro-preview", Provider::Gemini),
            ("deepseek-v4-pro", Provider::DeepSeek),
            ("gpt-5.2", Provider::OpenAI),
            ("gpt-5.4", Provider::OpenAI),
            ("gpt-5.5", Provider::OpenAI),
            ("gpt-5.3-codex", Provider::OpenAI),
            ("gpt-5.2-codex", Provider::OpenAI),
            ("MiniMax-M2.7", Provider::MiniMax),
            ("claude-opus-4-7", Provider::Anthropic),
            ("grok-4.3", Provider::Grok),
            ("agy", Provider::Antigravity),
        ];

        let builtins = all_builtin_models();
        let expected_models: Vec<&str> = expected.iter().map(|(m, _)| *m).collect();
        for m in &builtins {
            assert!(
                expected_models.contains(m),
                "builtin model {m:?} missing from golden table; add it"
            );
        }
        assert_eq!(
            builtins.len(),
            expected.len(),
            "golden table size drifted from builtin catalogue"
        );

        for (model, want) in expected {
            let got =
                Provider::from_model(model).unwrap_or_else(|| panic!("no provider for {model:?}"));
            assert_eq!(got, *want, "provider mismatch for {model:?}");
        }
    }

    #[test]
    fn google_thinking_config_applies_only_to_gemini_3_family() {
        let policy = OpenAiExtraBody::GoogleThinkingConfig;
        assert!(policy.applies_to_model("gemini-3"));
        assert!(policy.applies_to_model("gemini-3-pro-preview"));
        assert!(policy.applies_to_model("gemini-3.1-pro-preview"));
        assert!(policy.applies_to_model("gemini-3-flash-preview"));
        assert!(!policy.applies_to_model("gemini-2.5-pro"));
        assert!(!policy.applies_to_model("gemini-30-pro-preview"));
        assert!(!policy.applies_to_model("notgemini-3-pro-preview"));
        assert!(!policy.applies_to_model("gpt-5.5"));
    }

    #[test]
    fn registry_integrity() {
        assert_eq!(PROVIDERS.len(), ALL_PROVIDERS.len());

        let mut ids = HashSet::new();
        let mut variants = HashSet::new();
        let mut models = HashSet::new();
        for spec in PROVIDERS {
            assert!(!spec.id.is_empty());
            assert!(!spec.model_prefixes.is_empty());
            assert!(!spec.builtin_models.is_empty());
            assert!(!spec.selector_priorities.is_empty());
            assert!(!spec.allowed_backends.is_empty());
            assert!(ids.insert(spec.id), "duplicate provider id {:?}", spec.id);
            assert!(
                variants.insert(spec.provider),
                "duplicate ProviderSpec for {:?}",
                spec.provider
            );
            for m in spec.builtin_models {
                assert!(models.insert(*m), "duplicate builtin model {m:?}");
            }
            for sm in spec.selector_priorities {
                assert!(
                    spec.builtin_models.contains(sm),
                    "selector priority {sm:?} for {:?} not in builtin_models",
                    spec.id
                );
            }
            if let Some(env) = spec.reasoning_effort_env {
                assert!(!env.is_empty());
                assert_eq!(spec.provider, Provider::OpenAI);
            }
            if let Some(env) = spec.extra_args_env {
                assert!(!env.is_empty());
                assert!(matches!(
                    spec.provider,
                    Provider::Gemini | Provider::OpenAI | Provider::Antigravity
                ));
            }
        }

        // Provider model prefixes must not overlap so `Provider::from_model` is unambiguous.
        let prefixes: Vec<(&str, &str)> = PROVIDERS
            .iter()
            .flat_map(|s| s.model_prefixes.iter().map(move |p| (s.id, *p)))
            .collect();
        for (i, (id_a, pa)) in prefixes.iter().enumerate() {
            for (id_b, pb) in &prefixes[i + 1..] {
                assert!(
                    !pa.starts_with(pb) && !pb.starts_with(pa),
                    "model prefixes overlap: {id_a}:{pa:?} vs {id_b}:{pb:?}"
                );
            }
        }

        for p in ALL_PROVIDERS {
            assert_eq!(Provider::from_id(p.spec().id), Some(*p));
        }
    }
}
