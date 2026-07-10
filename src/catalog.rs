use crate::models::selector_priorities;

/// Single source of truth for model availability — drives both schema and validation
#[derive(Clone, Debug)]
pub struct ModelRegistry {
    pub allowed_models: Vec<String>,
    pub fallback_model: String,
    pub default_model: Option<String>,
    pub default_models: Vec<String>,
}

impl ModelRegistry {
    /// Resolve which model to use:
    /// - If model provided -> resolve as exact ID or selector
    /// - If not provided -> use default_model or fallback_model
    pub fn resolve_model(
        &self,
        requested: Option<&str>,
    ) -> Result<String, crate::errors::AppError> {
        let target = match requested {
            Some(m) => m,
            None => {
                return Ok(self
                    .default_model
                    .as_deref()
                    .unwrap_or(&self.fallback_model)
                    .to_string());
            }
        };

        resolve_selector(target, &self.allowed_models).ok_or_else(|| {
            let selectors: Vec<&str> = selector_priorities().map(|(s, _)| s).collect();
            crate::errors::AppError::InvalidParams(format!(
                "Invalid model or selector '{target}'. Selectors: {}. Available models: {}",
                selectors.join(", "),
                self.allowed_models.join(", ")
            ))
        })
    }
}

/// Resolve a model string as either an exact model ID or an abstract selector.
/// Returns the concrete model ID if found in `allowed_models`, or None.
pub fn resolve_selector(target: &str, allowed_models: &[String]) -> Option<String> {
    // 1. Exact match
    if allowed_models.iter().any(|m| m == target) {
        return Some(target.to_string());
    }

    // 2. Selector match — first available model from priority list
    if let Some((_, priorities)) = selector_priorities().find(|(s, _)| *s == target) {
        return priorities
            .iter()
            .find(|&&m| allowed_models.iter().any(|a| a == m))
            .map(|m| m.to_string());
    }

    None
}

/// Build the full model catalog from builtin models, optional extras, and an optional allowlist.
pub fn build_model_catalog(
    builtin: &[&str],
    extra_raw: Option<&str>,
    allowed_raw: Option<&str>,
) -> Vec<String> {
    let extra: Vec<String> = extra_raw
        .map(|s| {
            s.split(',')
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let mut all: Vec<String> = builtin.iter().map(|s| s.to_string()).collect();
    for m in extra {
        if !all.contains(&m) {
            all.push(m);
        }
    }

    // `None` means no allowlist was configured (every model is allowed).
    // `Some(_)` means the user explicitly set one — even if it's empty,
    // which then disables every model rather than silently re-enabling all.
    let Some(raw) = allowed_raw else {
        return all;
    };
    let allowed: Vec<String> = raw
        .split(',')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    all.into_iter().filter(|m| allowed.contains(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_model_catalog_builtin_only() {
        let result = build_model_catalog(&["a", "b", "c"], None, None);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_build_model_catalog_with_extras() {
        let result = build_model_catalog(&["a", "b"], Some("c, d, a"), None);
        assert_eq!(result, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_build_model_catalog_with_allowlist() {
        let result = build_model_catalog(&["a", "b", "c"], None, Some("b, c"));
        assert_eq!(result, vec!["b", "c"]);
    }

    #[test]
    fn test_build_model_catalog_extras_and_allowlist() {
        let result = build_model_catalog(&["a", "b"], Some("c, d"), Some("b, c"));
        assert_eq!(result, vec!["b", "c"]);
    }

    #[test]
    fn test_build_model_catalog_explicit_empty_allowlist_disables_all() {
        // `allowed_models: []` in YAML serializes to an empty env value;
        // it must be honored as "deny all", not silently treated as unset.
        let result = build_model_catalog(&["a", "b", "c"], None, Some(""));
        assert!(result.is_empty());
    }

    #[test]
    fn test_model_registry_resolve_exact() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.2".into(), "gemini-2.5-pro".into()],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
        };
        assert_eq!(
            reg.resolve_model(Some("gemini-2.5-pro")).unwrap(),
            "gemini-2.5-pro"
        );
    }

    #[test]
    fn test_model_registry_resolve_selector() {
        let reg = ModelRegistry {
            allowed_models: vec![
                "gpt-5.2".into(),
                "gemini-3.1-pro-preview".into(),
                "gemini-2.5-pro".into(),
            ],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
        };
        // "gemini" selector should resolve to best available
        assert_eq!(
            reg.resolve_model(Some("gemini")).unwrap(),
            "gemini-3.1-pro-preview"
        );
    }

    #[test]
    fn test_model_registry_resolve_selector_skips_unavailable() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.2".into(), "gemini-2.5-pro".into()],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
        };
        // "gemini" selector should skip gemini-3.1-pro-preview (not in allowed) and pick gemini-2.5-pro
        assert_eq!(reg.resolve_model(Some("gemini")).unwrap(), "gemini-2.5-pro");
    }

    #[test]
    fn test_model_registry_resolve_openai_selector() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.4".into(), "gpt-5.3-codex".into(), "gpt-5.2".into()],
            fallback_model: "gpt-5.4".into(),
            default_model: None,
            default_models: vec![],
        };
        // "openai" selector should resolve to highest priority: gpt-5.4
        assert_eq!(reg.resolve_model(Some("openai")).unwrap(), "gpt-5.4");
    }

    #[test]
    fn test_model_registry_resolve_openai_selector_prefers_gpt_5_6_sol() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.5".into(), "gpt-5.6-sol".into()],
            fallback_model: "gpt-5.5".into(),
            default_model: None,
            default_models: vec![],
        };

        assert_eq!(reg.resolve_model(Some("openai")).unwrap(), "gpt-5.6-sol");
    }

    #[test]
    fn test_model_registry_resolve_openai_selector_falls_to_codex() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.3-codex".into(), "gpt-5.2-codex".into()],
            fallback_model: "gpt-5.3-codex".into(),
            default_model: None,
            default_models: vec![],
        };
        // When only codex models available, "openai" should still resolve
        assert_eq!(reg.resolve_model(Some("openai")).unwrap(), "gpt-5.3-codex");
    }

    #[test]
    fn test_model_registry_resolve_default() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.2".into(), "gemini-2.5-pro".into()],
            fallback_model: "gpt-5.2".into(),
            default_model: Some("gemini-2.5-pro".into()),
            default_models: vec![],
        };
        assert_eq!(reg.resolve_model(None).unwrap(), "gemini-2.5-pro");
    }

    #[test]
    fn test_model_registry_resolve_fallback() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.2".into(), "gemini-2.5-pro".into()],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
        };
        assert_eq!(reg.resolve_model(None).unwrap(), "gpt-5.2");
    }

    #[test]
    fn test_model_registry_resolve_invalid() {
        let reg = ModelRegistry {
            allowed_models: vec!["gpt-5.2".into()],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
        };
        assert!(reg.resolve_model(Some("invalid")).is_err());
    }

    #[test]
    fn test_resolve_selector_exact_match() {
        let allowed = vec!["gpt-5.2".into(), "gemini-2.5-pro".into()];
        assert_eq!(
            resolve_selector("gpt-5.2", &allowed),
            Some("gpt-5.2".into())
        );
    }

    #[test]
    fn test_resolve_selector_selector_match() {
        let allowed = vec!["gemini-3.1-pro-preview".into(), "gemini-2.5-pro".into()];
        assert_eq!(
            resolve_selector("gemini", &allowed),
            Some("gemini-3.1-pro-preview".into())
        );
    }

    #[test]
    fn test_resolve_selector_no_match() {
        let allowed = vec!["gpt-5.2".into()];
        assert_eq!(resolve_selector("gemini", &allowed), None);
    }
}
