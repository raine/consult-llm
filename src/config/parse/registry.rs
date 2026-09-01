use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::{ModelRegistry, build_model_catalog};
use crate::executors::cursor_models::{ModelList, available_models as cursor_available_models};
use crate::logger::log_to_file;
use crate::models::{Provider, all_builtin_models};

use super::super::types::{Backend, ConfigError, ProviderRuntimeConfig};

fn cursor_provider_for_model(
    model: &str,
    providers: &HashMap<Provider, ProviderRuntimeConfig>,
) -> Option<Provider> {
    let provider = Provider::from_cursor_model(model)?;
    providers
        .get(&provider)
        .is_some_and(|cfg| cfg.backend == Backend::CursorCli)
        .then_some(provider)
}

fn cursor_catalog_models(providers: &HashMap<Provider, ProviderRuntimeConfig>) -> Vec<String> {
    if !providers
        .values()
        .any(|cfg| cfg.backend == Backend::CursorCli)
    {
        return Vec::new();
    }

    let list = cursor_available_models();
    let models = match list {
        ModelList::Fresh(models) | ModelList::Stale(models) => models,
        ModelList::Unavailable => return Vec::new(),
    };

    models
        .into_iter()
        .filter(|model| cursor_provider_for_model(model, providers).is_some())
        .collect()
}

fn append_unique(models: &mut Vec<String>, additions: impl IntoIterator<Item = String>) {
    for model in additions {
        if !models.contains(&model) {
            models.push(model);
        }
    }
}

pub fn filter_by_availability(
    models: &[String],
    providers: &HashMap<Provider, ProviderRuntimeConfig>,
) -> Vec<String> {
    models
        .iter()
        .filter(|model| {
            let provider =
                Provider::from_model(model).or_else(|| cursor_provider_for_model(model, providers));
            match provider {
                Some(provider) => {
                    let cfg = &providers[&provider];
                    cfg.has_executable_backend()
                }
                None => {
                    log_to_file(&format!(
                        "WARNING: dropping model '{model}' - unrecognized provider prefix"
                    ));
                    false
                }
            }
        })
        .cloned()
        .collect()
}

pub fn resolve_enabled_models(
    env: &impl Fn(&str) -> Option<String>,
    providers: &HashMap<Provider, ProviderRuntimeConfig>,
) -> Result<Vec<String>, ConfigError> {
    let mut builtin: Vec<String> = all_builtin_models().iter().map(|m| m.to_string()).collect();
    append_unique(&mut builtin, cursor_catalog_models(providers));
    let builtin_refs: Vec<&str> = builtin.iter().map(|m| m.as_str()).collect();
    let catalog_models = build_model_catalog(
        &builtin_refs,
        env("CONSULT_LLM_EXTRA_MODELS").as_deref(),
        env("CONSULT_LLM_ALLOWED_MODELS").as_deref(),
    );
    let enabled_models = filter_by_availability(&catalog_models, providers);
    if enabled_models.is_empty() {
        return Err(ConfigError::NoModelsAvailable);
    }
    Ok(enabled_models)
}

pub fn build_registry(
    enabled_models: Vec<String>,
    default_model: Option<String>,
    default_models: Vec<String>,
) -> Arc<ModelRegistry> {
    let fallback_model = if enabled_models.contains(&"gpt-5.2".to_string()) {
        "gpt-5.2".to_string()
    } else {
        enabled_models[0].clone()
    };
    Arc::new(ModelRegistry {
        allowed_models: enabled_models,
        fallback_model,
        default_model,
        default_models,
    })
}

#[cfg(test)]
mod tests {
    use super::super::parse_config;
    use super::super::test_helpers::{env_from, make_providers};
    use super::*;

    fn api_providers_without_keys() -> HashMap<Provider, ProviderRuntimeConfig> {
        make_providers(&[
            (Provider::Gemini, None, Backend::Api),
            (Provider::OpenAI, None, Backend::Api),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ])
    }

    #[test]
    fn test_filter_by_availability_api_with_key() {
        let models = vec![
            "gemini-2.5-pro".into(),
            "gpt-5.2".into(),
            "deepseek-v4-pro".into(),
        ];
        let providers = make_providers(&[
            (Provider::Gemini, Some("key"), Backend::Api),
            (Provider::OpenAI, Some("key"), Backend::Api),
            (Provider::DeepSeek, Some("key"), Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
        let result = filter_by_availability(&models, &providers);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_by_availability_api_without_key() {
        let models = vec![
            "gemini-2.5-pro".into(),
            "gpt-5.2".into(),
            "deepseek-v4-pro".into(),
        ];
        let providers = api_providers_without_keys();
        let result = filter_by_availability(&models, &providers);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_by_availability_cli_no_key_needed() {
        let models = vec!["gemini-2.5-pro".into(), "gpt-5.2".into()];
        let providers = make_providers(&[
            (Provider::Gemini, None, Backend::GeminiCli),
            (Provider::OpenAI, None, Backend::CodexCli),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
        let result = filter_by_availability(&models, &providers);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_profile_cli_without_selected_profile_filters_models() {
        let models = vec!["claude-opus-4-7".into(), "gpt-5.2".into()];
        let providers = make_providers(&[
            (Provider::Anthropic, None, Backend::Profile),
            (Provider::OpenAI, Some("key"), Backend::Api),
        ]);
        let result = filter_by_availability(&models, &providers);
        assert!(
            !result.iter().any(|m| m.starts_with("claude")),
            "claude models should not be enabled without a selected CLI profile"
        );
        assert!(result.iter().any(|m| m.starts_with("gpt")));
    }

    #[test]
    fn test_filter_by_availability_unknown_prefix_rejected() {
        let models = vec!["custom-model".into()];
        let providers = api_providers_without_keys();
        let result = filter_by_availability(&models, &providers);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_by_availability_cursor_model_requires_cursor_backend() {
        let models = vec!["gemini-3.1-pro".into(), "auto".into()];
        let api_providers = make_providers(&[
            (Provider::Gemini, Some("key"), Backend::Api),
            (Provider::OpenAI, Some("key"), Backend::Api),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
        assert_eq!(
            filter_by_availability(&models, &api_providers),
            vec!["gemini-3.1-pro".to_string()]
        );

        let cursor_providers = make_providers(&[
            (Provider::Gemini, None, Backend::CursorCli),
            (Provider::OpenAI, None, Backend::CursorCli),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
        assert_eq!(filter_by_availability(&models, &cursor_providers), models);
    }

    #[test]
    fn test_append_unique_adds_new_models_once() {
        let mut models = vec!["gemini-3.1-pro-preview".to_string()];
        append_unique(
            &mut models,
            [
                "gemini-3.1-pro".to_string(),
                "gemini-3.1-pro-preview".to_string(),
            ],
        );
        assert_eq!(
            models,
            vec![
                "gemini-3.1-pro-preview".to_string(),
                "gemini-3.1-pro".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_config_with_api_keys() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "sk-test"),
            ("GEMINI_API_KEY", "gem-test"),
        ]);
        let (config, registry) = parse_config(env).unwrap();
        assert!(config.allowed_models.contains(&"gpt-5.2".to_string()));
        assert!(
            config
                .allowed_models
                .contains(&"gemini-2.5-pro".to_string())
        );
        assert_eq!(registry.fallback_model, "gpt-5.2");
    }

    #[test]
    fn test_parse_config_no_models_available() {
        let env = env_from(&[]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(err, ConfigError::NoModelsAvailable));
    }

    #[test]
    fn test_parse_config_fallback_when_no_gpt52() {
        let env = env_from(&[
            ("GEMINI_API_KEY", "key"),
            ("CONSULT_LLM_ALLOWED_MODELS", "gemini-2.5-pro"),
        ]);
        let (_, registry) = parse_config(env).unwrap();
        assert_eq!(registry.fallback_model, "gemini-2.5-pro");
    }

    #[test]
    fn test_extra_kimi_model_is_available_with_openai_api_backend() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("CONSULT_LLM_EXTRA_MODELS", "kimi-k3"),
            ("CONSULT_LLM_ALLOWED_MODELS", "kimi-k3"),
        ]);
        let (_, registry) = parse_config(env).unwrap();
        assert_eq!(registry.allowed_models, vec!["kimi-k3"]);
        assert_eq!(registry.resolve_model(Some("kimi-k3")).unwrap(), "kimi-k3");
    }

    #[test]
    fn test_all_builtin_models_order() {
        // Verify the model catalog order matches the original ALL_MODELS constant.
        // Order matters: enabled_models[0] is the fallback when gpt-5.2 is absent.
        let models = all_builtin_models();
        assert_eq!(
            models,
            vec![
                "gemini-2.5-pro",
                "gemini-3-pro-preview",
                "gemini-3.1-pro-preview",
                "deepseek-v4-pro",
                "gpt-5.2",
                "gpt-5.4",
                "gpt-5.6-sol",
                "gpt-5.5",
                "gpt-5.3-codex",
                "gpt-5.2-codex",
                "MiniMax-M2.7",
                "claude-fable-5-1",
                "claude-fable-5",
                "claude-opus-5",
                "claude-opus-4-7",
                "grok-4.6",
                "grok-4.5",
                "grok-4.3",
                "openrouter/auto",
                "glm-5.2",
            ]
        );
    }
}
