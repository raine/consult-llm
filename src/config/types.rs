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
    GrokCli,
    CursorCli,
    OpenCodeCli,
    ClaudeCli,
    Profile,
}

impl Backend {
    pub fn from_builtin_str(s: &str) -> Option<Backend> {
        match s {
            "api" => Some(Backend::Api),
            "codex-cli" => Some(Backend::CodexCli),
            "gemini-cli" => Some(Backend::GeminiCli),
            "grok-cli" => Some(Backend::GrokCli),
            "cursor-cli" => Some(Backend::CursorCli),
            "opencode" => Some(Backend::OpenCodeCli),
            "claude-cli" => Some(Backend::ClaudeCli),
            "profile" => Some(Backend::Profile),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Backend::Api => "api",
            Backend::CodexCli => "codex-cli",
            Backend::GeminiCli => "gemini-cli",
            Backend::GrokCli => "grok-cli",
            Backend::CursorCli => "cursor-cli",
            Backend::OpenCodeCli => "opencode",
            Backend::ClaudeCli => "claude-cli",
            Backend::Profile => "profile",
        }
    }
}

/// Per-provider runtime configuration parsed from environment variables.
#[derive(Debug, Clone)]
pub struct ProviderRuntimeConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub backend: Backend,
    pub opencode_provider: String,
    pub reasoning_effort: Option<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub selected_cli_profile: Option<SelectedCliProfile>,
}

impl ProviderRuntimeConfig {
    /// Returns true if this provider has an available executor for its backend.
    /// For API backends this means an API key is set. For profile-backed backends
    /// it means a matching CLI profile is selected. Built-in CLI backends always
    /// have an executor.
    pub fn has_executable_backend(&self) -> bool {
        match &self.backend {
            Backend::Api => self.api_key.is_some(),
            Backend::Profile => self.selected_cli_profile.is_some(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CliProfileInterface {
    Text,
    Json,
    StreamJson,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CliPromptMode {
    Stdin,
    Argument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum CliProfileType {
    #[default]
    ClaudeCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaudeEffort {
    Low,
    Medium,
    High,
    #[serde(alias = "xhigh", alias = "extra-high")]
    XHigh,
    Max,
}

impl ClaudeEffort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaudeEffort::Low => "low",
            ClaudeEffort::Medium => "medium",
            ClaudeEffort::High => "high",
            ClaudeEffort::XHigh => "xhigh",
            ClaudeEffort::Max => "max",
        }
    }
}

impl std::fmt::Display for ClaudeEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliProfile {
    #[serde(rename = "type", default)]
    pub profile_type: CliProfileType,
    #[serde(default = "default_command")]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_interface")]
    pub interface: CliProfileInterface,
    #[serde(default = "default_prompt")]
    pub prompt: CliPromptMode,
    pub effort: Option<ClaudeEffort>,
    pub model_env: Option<String>,
}

fn default_command() -> String {
    "claude".to_string()
}

fn default_interface() -> CliProfileInterface {
    CliProfileInterface::StreamJson
}

fn default_prompt() -> CliPromptMode {
    CliPromptMode::Stdin
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCliProfile {
    pub name: String,
    pub profile: CliProfile,
}

#[derive(Debug)]
pub struct Config {
    pub(crate) providers: HashMap<Provider, ProviderRuntimeConfig>,
    #[allow(dead_code)]
    pub default_model: Option<String>,
    pub default_models: Vec<String>,
    pub codex_reasoning_effort: String,
    pub claude_reasoning_effort: Option<ClaudeEffort>,
    pub codex_extra_args: Vec<String>,
    pub gemini_extra_args: Vec<String>,
    pub grok_extra_args: Vec<String>,
    pub claude_extra_args: Vec<String>,
    pub api_idle_timeout: Duration,
    pub system_prompt_path: Option<String>,
    pub allowed_models: Vec<String>,
    #[allow(dead_code)]
    pub cli_profiles: std::collections::BTreeMap<String, CliProfile>,
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

    /// API base URL for a provider: the configured override when set,
    /// otherwise the registry default (when using API backend).
    pub fn api_base_url_for(&self, provider: Provider) -> Option<&str> {
        self.providers[&provider]
            .base_url
            .as_deref()
            .or_else(|| provider.api_base_url())
    }

    /// Get the OpenCode provider prefix for a provider family.
    pub fn opencode_provider_for(&self, provider: Provider) -> &str {
        &self.providers[&provider].opencode_provider
    }

    /// Get the configured reasoning effort for a provider, if any.
    pub fn reasoning_effort_for(&self, provider: Provider) -> Option<&str> {
        self.providers[&provider].reasoning_effort.as_deref()
    }

    /// Get configured environment variables for a provider backend.
    pub fn env_for(&self, provider: Provider) -> &std::collections::BTreeMap<String, String> {
        &self.providers[&provider].env
    }

    #[allow(dead_code)]
    /// Iterate over all provider runtime configs.
    pub fn iter_providers(&self) -> impl Iterator<Item = (&Provider, &ProviderRuntimeConfig)> {
        self.providers.iter()
    }

    #[allow(dead_code)]
    /// Get the selected CLI profile for a provider, if any.
    pub fn selected_cli_profile_for(&self, provider: Provider) -> Option<&SelectedCliProfile> {
        self.providers[&provider].selected_cli_profile.as_ref()
    }

    #[allow(dead_code)]
    /// Get all configured CLI profiles.
    pub fn all_cli_profiles(&self) -> &std::collections::BTreeMap<String, CliProfile> {
        &self.cli_profiles
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
    InvalidClaudeReasoningEffort(String),
    InvalidExtraArgs {
        env_var: String,
        raw: String,
        message: String,
    },
    ConfigFile {
        path: PathBuf,
        message: String,
    },
    MissingCliProfile {
        key: String,
        backend: String,
        allowed: Vec<String>,
    },
    InvalidCliProfileReference {
        key: String,
        raw: String,
        allowed: Vec<String>,
    },
}

struct QuotedOptions<'a>(&'a [String]);

impl fmt::Display for QuotedOptions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opts = self
            .0
            .iter()
            .map(|v| format!("'{v}'"))
            .collect::<Vec<_>>()
            .join(" | ");
        write!(f, "{opts}")
    }
}

fn fmt_invalid_default_model(
    f: &mut fmt::Formatter<'_>,
    field: &str,
    model: &str,
    allowed: &[String],
) -> fmt::Result {
    let selectors: Vec<&str> = selector_priorities().map(|(s, _)| s).collect();
    write!(
        f,
        "Invalid environment variables:\n  {field}: Invalid value '{model}'. Expected a selector ({}) or exact model ({})",
        selectors.join(", "),
        QuotedOptions(allowed)
    )
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
            } => write!(
                f,
                "Invalid environment variables:\n  {env_var}: Invalid enum value. Expected {}, received '{raw}'",
                QuotedOptions(allowed)
            ),
            ConfigError::InvalidDefaultModel { model, allowed } => {
                fmt_invalid_default_model(f, "defaultModel", model, allowed)
            }
            ConfigError::InvalidDefaultModels { model, allowed } => {
                fmt_invalid_default_model(f, "defaultModels", model, allowed)
            }
            ConfigError::TooManyDefaultModels { count } => write!(
                f,
                "Invalid environment variables:\n  defaultModels: max 5 total runs, including duplicates (got {count})"
            ),
            ConfigError::InvalidCodexReasoningEffort(effort) => write!(
                f,
                "Invalid environment variables:\n  codexReasoningEffort: Invalid enum value. Expected 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh', received '{effort}'"
            ),
            ConfigError::InvalidClaudeReasoningEffort(effort) => write!(
                f,
                "Invalid environment variables:\n  claudeReasoningEffort: Invalid enum value. Expected 'low' | 'medium' | 'high' | 'x-high' | 'xhigh' | 'extra-high' | 'max', received '{effort}'"
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
            ConfigError::MissingCliProfile {
                key,
                backend,
                allowed,
            } => write!(
                f,
                "Invalid environment variables:\n  {key}: CLI profile required for backend '{backend}' but no profile set. Expected one of: {}",
                QuotedOptions(allowed)
            ),
            ConfigError::InvalidCliProfileReference { key, raw, allowed } => write!(
                f,
                "Invalid environment variables:\n  {key}: Invalid CLI profile '{raw}'. Expected one of: {}",
                QuotedOptions(allowed)
            ),
        }
    }
}
