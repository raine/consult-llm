use crate::catalog::resolve_selector;

use super::super::types::ConfigError;

pub fn resolve_default_model(
    env: &impl Fn(&str) -> Option<String>,
    enabled_models: &[String],
) -> Result<Option<String>, ConfigError> {
    let Some(dm) = env("CONSULT_LLM_DEFAULT_MODEL") else {
        return Ok(None);
    };
    let resolved =
        resolve_selector(&dm, enabled_models).ok_or_else(|| ConfigError::InvalidDefaultModel {
            model: dm.clone(),
            allowed: enabled_models.to_vec(),
        })?;
    Ok(Some(resolved))
}

pub fn resolve_default_models(
    env: &impl Fn(&str) -> Option<String>,
    enabled_models: &[String],
) -> Result<Vec<String>, ConfigError> {
    let Some(raw) = env("CONSULT_LLM_DEFAULT_MODELS") else {
        return Ok(Vec::new());
    };
    let items: Vec<String> = raw
        .split(',')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    if items.len() > 5 {
        return Err(ConfigError::TooManyDefaultModels { count: items.len() });
    }
    let mut resolved = Vec::with_capacity(items.len());
    for item in items {
        let model = resolve_selector(&item, enabled_models).ok_or_else(|| {
            ConfigError::InvalidDefaultModels {
                model: item.clone(),
                allowed: enabled_models.to_vec(),
            }
        })?;
        resolved.push(model);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::super::parse_config;
    use super::super::test_helpers::env_from;
    use super::*;

    #[test]
    fn test_parse_config_invalid_default_model() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODEL", "nonexistent"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidDefaultModel { ref model, .. } if model == "nonexistent")
        );
    }

    #[test]
    fn test_parse_config_valid_default_model() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODEL", "gpt-5.2"),
        ]);
        let (config, registry) = parse_config(env).unwrap();
        assert_eq!(config.default_model, Some("gpt-5.2".to_string()));
        assert_eq!(registry.default_model, Some("gpt-5.2".to_string()));
    }

    #[test]
    fn test_parse_config_selector_default_model() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODEL", "openai"),
        ]);
        let (config, registry) = parse_config(env).unwrap();
        assert_eq!(config.default_model, Some("gpt-5.6-sol".to_string()));
        assert_eq!(registry.default_model, Some("gpt-5.6-sol".to_string()));
    }

    #[test]
    fn test_parse_config_default_models_preserve_duplicates() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("GEMINI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODELS", "openai,gemini,openai"),
        ]);
        let (config, _) = parse_config(env).unwrap();
        assert_eq!(
            config.default_models,
            vec!["gpt-5.6-sol", "gemini-3.1-pro-preview", "gpt-5.6-sol"]
        );
    }

    #[test]
    fn test_parse_config_default_models_cap_counts_duplicates() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            (
                "CONSULT_LLM_DEFAULT_MODELS",
                "openai,openai,openai,openai,openai,openai",
            ),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::TooManyDefaultModels { count: 6 }
        ));
    }

    #[test]
    fn test_parse_config_default_models_invalid_model() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODELS", "openai,missing"),
        ]);
        let err = parse_config(env).unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidDefaultModels { ref model, .. } if model == "missing")
        );
    }

    #[test]
    fn test_parse_config_default_models_propagate_to_registry() {
        let env = env_from(&[
            ("OPENAI_API_KEY", "key"),
            ("GEMINI_API_KEY", "key"),
            ("CONSULT_LLM_DEFAULT_MODELS", "openai,gemini,openai"),
        ]);

        let (config, registry) = parse_config(env).unwrap();

        assert_eq!(registry.default_models, config.default_models);
        assert_eq!(
            registry.default_models,
            vec!["gpt-5.6-sol", "gemini-3.1-pro-preview", "gpt-5.6-sol"]
        );
    }
}
