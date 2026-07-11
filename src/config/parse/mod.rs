use std::collections::BTreeMap;
use std::sync::Arc;

use crate::catalog::ModelRegistry;

use super::types::{CliProfile, Config, ConfigError};

mod defaults;
mod env;
mod provider;
mod registry;

pub use env::parse_extra_args;

/// Pure config parsing: takes an env-lookup function, returns Config + ModelRegistry or an error.
/// Does not read real env vars, call process::exit, or set globals.
#[allow(dead_code)]
pub fn parse_config(
    env: impl Fn(&str) -> Option<String>,
) -> Result<(Config, Arc<ModelRegistry>), ConfigError> {
    parse_config_with_cli_profiles(env, BTreeMap::new())
}

/// Pure config parsing with CLI profiles: takes an env-lookup function and a profile map.
/// Returns Config + ModelRegistry or an error.
pub fn parse_config_with_cli_profiles(
    env: impl Fn(&str) -> Option<String>,
    cli_profiles: BTreeMap<String, CliProfile>,
) -> Result<(Config, Arc<ModelRegistry>), ConfigError> {
    let providers = provider::parse_all_providers(&env, &cli_profiles)?;
    let enabled_models = registry::resolve_enabled_models(&env, &providers)?;

    let resolved_default = defaults::resolve_default_model(&env, &enabled_models)?;
    let resolved_default_models = defaults::resolve_default_models(&env, &enabled_models)?;

    let api_idle_timeout = env::resolve_api_idle_timeout(&env);
    let codex_reasoning_effort = env::resolve_codex_reasoning_effort(&env)?;
    let claude_reasoning_effort = env::resolve_claude_reasoning_effort(&env)?;
    let codex_extra_args = parse_extra_args(
        env("CONSULT_LLM_CODEX_EXTRA_ARGS").as_deref(),
        "CONSULT_LLM_CODEX_EXTRA_ARGS",
    )?;
    let gemini_extra_args = parse_extra_args(
        env("CONSULT_LLM_GEMINI_EXTRA_ARGS").as_deref(),
        "CONSULT_LLM_GEMINI_EXTRA_ARGS",
    )?;
    let grok_extra_args = parse_extra_args(
        env("CONSULT_LLM_GROK_EXTRA_ARGS").as_deref(),
        "CONSULT_LLM_GROK_EXTRA_ARGS",
    )?;
    let claude_extra_args = parse_extra_args(
        env("CONSULT_LLM_CLAUDE_EXTRA_ARGS").as_deref(),
        "CONSULT_LLM_CLAUDE_EXTRA_ARGS",
    )?;

    let config = Config {
        providers,
        default_model: resolved_default.clone(),
        default_models: resolved_default_models.clone(),
        codex_reasoning_effort,
        claude_reasoning_effort,
        codex_extra_args,
        gemini_extra_args,
        grok_extra_args,
        claude_extra_args,
        api_idle_timeout,
        system_prompt_path: env("CONSULT_LLM_SYSTEM_PROMPT_PATH"),
        allowed_models: enabled_models.clone(),
        cli_profiles,
    };

    let registry =
        registry::build_registry(enabled_models, resolved_default, resolved_default_models);

    Ok((config, registry))
}

#[cfg(test)]
pub(super) mod test_helpers {
    use std::collections::HashMap;

    use crate::models::Provider;

    use super::super::types::{Backend, ProviderRuntimeConfig};

    pub fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    pub fn make_providers(
        entries: &[(Provider, Option<&str>, Backend)],
    ) -> HashMap<Provider, ProviderRuntimeConfig> {
        entries
            .iter()
            .map(|(p, key, backend)| {
                (
                    *p,
                    ProviderRuntimeConfig {
                        api_key: key.map(|k| k.to_string()),
                        backend: backend.clone(),
                        opencode_provider: String::new(),
                        reasoning_effort: None,
                        env: std::collections::BTreeMap::new(),
                        selected_cli_profile: None,
                    },
                )
            })
            .collect()
    }
}
