use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::models::Provider;
use crate::models::selector_priorities;

#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    Api,
    CodexCli,
    GeminiCli,
    CursorCli,
    OpenCodeCli,
    AntigravityCli,
}

impl Backend {
    pub fn from_str(s: &str) -> Option<Backend> {
        match s {
            "api" => Some(Backend::Api),
            "codex-cli" => Some(Backend::CodexCli),
            "gemini-cli" => Some(Backend::GeminiCli),
            "cursor-cli" => Some(Backend::CursorCli),
            "opencode" => Some(Backend::OpenCodeCli),
            "antigravity-cli" => Some(Backend::AntigravityCli),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Api => "api",
            Backend::CodexCli => "codex-cli",
            Backend::GeminiCli => "gemini-cli",
            Backend::CursorCli => "cursor-cli",
            Backend::OpenCodeCli => "opencode",
            Backend::AntigravityCli => "antigravity-cli",
        }
    }
}

/// Per-provider runtime configuration parsed from environment variables.
#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub api_key: Option<String>,
    pub backend: Backend,
    pub opencode_provider: String,
}

#[derive(Debug)]
pub struct Config {
    pub(crate) providers: HashMap<Provider, ProviderRuntimeConfig>,
    #[allow(dead_code)]
    pub default_model: Option<String>,
    pub default_models: Vec<String>,
    pub codex_reasoning_effort: String,
    pub codex_extra_args: Vec<String>,
    pub gemini_extra_args: Vec<String>,
    pub agy_extra_args: Vec<String>,
    pub api_idle_timeout: Duration,
    pub system_prompt_path: Option<String>,
    pub allowed_models: Vec<String>,
}

impl Config {
    /// Get the configured backend for a provider.
    pub fn backend_for(&self, provider: Provider) -> &Backend {
        &self.providers[&provider].backend
    }

    /// Get the API key for a provider (when using API backend).
    pub fn api_key_for(&self, provider: Provider) -> Option<&str> {
        self.providers[&provider].api_key.as_deref()
    }

    /// Get the OpenCode provider prefix for a provider family.
    pub fn opencode_provider_for(&self, provider: Provider) -> &str {
        &self.providers[&provider].opencode_provider
    }

    #[allow(dead_code)]
    /// Iterate over all provider runtime configs.
    pub fn iter_providers(&self) -> impl Iterator<Item = (&Provider, &ProviderRuntimeConfig)> {
        self.providers.iter()
    }
}

#[derive(Debug)]
pub enum ConfigError {
    NoModelsAvailable,
    InvalidBackend {
        env_var: String,
        raw: String,
        allowed: Vec<String>,
    },
    InvalidDefaultModel {
        model: String,
        allowed: Vec<String>,
    },
    InvalidDefaultModels {
        model: String,
        allowed: Vec<String>,
    },
    TooManyDefaultModels {
        count: usize,
    },
    InvalidCodexReasoningEffort(String),
    InvalidExtraArgs {
        env_var: String,
        raw: String,
        message: String,
    },
    ConfigFile {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NoModelsAvailable => write!(
                f,
                "Invalid environment variables:\n  No models available. Set API keys or configure CLI backends."
            ),
            ConfigError::InvalidBackend {
                env_var,
                raw,
                allowed,
            } => {
                let opts = allowed
                    .iter()
                    .map(|v| format!("'{v}'"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                write!(
                    f,
                    "Invalid environment variables:\n  {env_var}: Invalid enum value. Expected {opts}, received '{raw}'"
                )
            }
            ConfigError::InvalidDefaultModel { model, allowed } => {
                let selectors: Vec<&str> = selector_priorities().map(|(s, _)| s).collect();
                let opts = allowed
                    .iter()
                    .map(|m| format!("'{m}'"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                write!(
                    f,
                    "Invalid environment variables:\n  defaultModel: Invalid value '{model}'. Expected a selector ({}) or exact model ({opts})",
                    selectors.join(", ")
                )
            }
            ConfigError::InvalidDefaultModels { model, allowed } => {
                let selectors: Vec<&str> = selector_priorities().map(|(s, _)| s).collect();
                let opts = allowed
                    .iter()
                    .map(|m| format!("'{m}'"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                write!(
                    f,
                    "Invalid environment variables:\n  defaultModels: Invalid value '{model}'. Expected a selector ({}) or exact model ({opts})",
                    selectors.join(", ")
                )
            }
            ConfigError::TooManyDefaultModels { count } => write!(
                f,
                "Invalid environment variables:\n  defaultModels: max 5 total runs, including duplicates (got {count})"
            ),
            ConfigError::InvalidCodexReasoningEffort(effort) => write!(
                f,
                "Invalid environment variables:\n  codexReasoningEffort: Invalid enum value. Expected 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh', received '{effort}'"
            ),
            ConfigError::InvalidExtraArgs {
                env_var,
                raw,
                message,
            } => write!(
                f,
                "Invalid environment variables:\n  {env_var}: {message} (received '{raw}')"
            ),
            ConfigError::ConfigFile { path, message } => write!(
                f,
                "Configuration file error:\n  {}: {}",
                path.display(),
                message,
            ),
        }
    }
}
