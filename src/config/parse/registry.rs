use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::{ModelRegistry, build_model_catalog};
use crate::logger::log_to_file;
use crate::models::{Provider, all_builtin_models};

use super::super::types::{Backend, ConfigError, ProviderRuntimeConfig};

pub fn filter_by_availability(
    models: &[String],
    providers: &HashMap<Provider, ProviderRuntimeConfig>,
) -> Vec<String> {
    models
        .iter()
        .filter(|model| match Provider::from_model(model) {
            Some(provider) => {
                let cfg = &providers[&provider];
                // CLI backends don't need API keys
                cfg.backend != Backend::Api || cfg.api_key.is_some()
            }
            None => {
                log_to_file(&format!(
                    "WARNING: dropping model '{model}' — unrecognized provider prefix"
                ));
                false
            }
        })
        .cloned()
        .collect()
}

pub fn resolve_enabled_models(
    env: &impl Fn(&str) -> Option<String>,
    providers: &HashMap<Provider, ProviderRuntimeConfig>,
) -> Result<Vec<String>, ConfigError> {
    let builtin = all_builtin_models();
    let catalog_models = build_model_catalog(
        &builtin,
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
    })
}

#[cfg(test)]
mod tests {
    use super::super::parse_config;
    use super::super::test_helpers::{env_from, make_providers};
    use super::*;

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
        let providers = make_providers(&[
            (Provider::Gemini, None, Backend::Api),
            (Provider::OpenAI, None, Backend::Api),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
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
    fn test_filter_by_availability_unknown_prefix_rejected() {
        let models = vec!["custom-model".into()];
        let providers = make_providers(&[
            (Provider::Gemini, None, Backend::Api),
            (Provider::OpenAI, None, Backend::Api),
            (Provider::DeepSeek, None, Backend::Api),
            (Provider::MiniMax, None, Backend::Api),
        ]);
        let result = filter_by_availability(&models, &providers);
        assert!(result.is_empty());
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
                "gpt-5.5",
                "gpt-5.3-codex",
                "gpt-5.2-codex",
                "MiniMax-M2.7",
                "claude-opus-4-7",
                "grok-4.3",
                "agy",
            ]
        );
    }
}
