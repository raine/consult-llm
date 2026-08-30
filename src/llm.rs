use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::types::CliProfileType;
use crate::config::{Backend, Config};
use crate::executors::anthropic_api::AnthropicApiExecutor;
use crate::executors::api::ApiExecutor;
use crate::executors::claude_cli::{ClaudeCliConfig, ClaudeCliExecutor};
use crate::executors::codex_cli::CodexCliExecutor;
use crate::executors::cursor_cli::CursorCliExecutor;
use crate::executors::gemini_cli::GeminiCliExecutor;
use crate::executors::grok_cli::GrokCliExecutor;
use crate::executors::opencode_cli::OpenCodeCliExecutor;
use crate::executors::pi_cli::PiCliExecutor;
use crate::executors::types::LlmExecutor;
use crate::models::{ApiProtocol, Provider};

pub struct ExecutorProvider {
    cache: Mutex<HashMap<String, Arc<dyn LlmExecutor>>>,
    agent: ureq::Agent,
    idle_timeout: std::time::Duration,
    config: Arc<Config>,
}

impl ExecutorProvider {
    pub fn new(config: Arc<Config>) -> Self {
        // Socket read-idle: ureq applies this as a per-read deadline (each
        // blocking read gets a fresh budget), so it's the right knob for
        // "the connection went silent", heartbeat bytes count as liveness
        // and reset the timer naturally. Set per-request in the executors.
        let idle_timeout = config.api_idle_timeout;

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(std::time::Duration::from_secs(30)))
            // Bound body upload so a provider that accepts the connection
            // but never reads can't hang `.send()` forever.
            .timeout_send_body(Some(std::time::Duration::from_secs(120)))
            // Absolute lifetime cap on any single request, backstop for
            // pathological cases the per-read socket idle can't catch
            // (server trickling a single byte every <idle interval).
            //
            // Note: do NOT also set timeout_recv_response. ureq's
            // next_timeout(RecvBody) takes the min over RecvBody,
            // RecvResponse, and Global. RecvResponse's deadline is fixed
            // at `headers_time + recv_response`, which sits in the past
            // once the body has been streaming a while; that pins every
            // subsequent body read to a 1-second timeout and the stream
            // dies on the first ~1s gap between tokens.
            .timeout_global(Some(std::time::Duration::from_secs(30 * 60)))
            .build()
            .into();

        Self {
            cache: Mutex::new(HashMap::new()),
            agent,
            idle_timeout,
            config,
        }
    }

    pub fn get_executor(&self, model: &str) -> anyhow::Result<Arc<dyn LlmExecutor>> {
        let cfg = &*self.config;
        let provider = Provider::from_model(model).ok_or_else(|| {
            anyhow::anyhow!("Unable to determine LLM provider for model: {model}")
        })?;

        let backend = cfg.backend_for(provider);
        let cache_key = format!("{model}-{backend:?}");

        let mut cache = self.cache.lock().unwrap();
        if let Some(exec) = cache.get(&cache_key) {
            return Ok(exec.clone());
        }

        let executor: Arc<dyn LlmExecutor> = match backend {
            Backend::Api => {
                let key = cfg.api_key_for(provider).ok_or_else(|| {
                    anyhow::anyhow!("API key is required for {provider:?} models in API mode")
                })?;
                let base = cfg.api_base_url_for(provider).map(|s| s.to_string());
                let idle_timeout = self.idle_timeout;
                match provider.api_protocol() {
                    ApiProtocol::OpenAiCompat(runtime) => Arc::new(ApiExecutor::new(
                        self.agent.clone(),
                        key.to_string(),
                        base,
                        idle_timeout,
                        runtime,
                        cfg.reasoning_effort_for(provider).map(str::to_string),
                    )),
                    ApiProtocol::AnthropicMessages => Arc::new(AnthropicApiExecutor::new(
                        self.agent.clone(),
                        key.to_string(),
                        base,
                        idle_timeout,
                    )),
                }
            }
            Backend::CodexCli => Arc::new(CodexCliExecutor::new(
                cfg.codex_reasoning_effort.clone(),
                cfg.codex_extra_args.clone(),
                cfg.env_for(provider).clone(),
            )),
            Backend::GeminiCli => Arc::new(GeminiCliExecutor::new(
                cfg.gemini_extra_args.clone(),
                cfg.env_for(provider).clone(),
            )),
            Backend::GrokCli => Arc::new(GrokCliExecutor::new(
                cfg.reasoning_effort_for(provider).map(str::to_string),
                cfg.grok_extra_args.clone(),
                cfg.env_for(provider).clone(),
            )),
            Backend::ClaudeCli => {
                use crate::config::types::{CliProfileInterface, CliPromptMode};
                let config = ClaudeCliConfig {
                    command: "claude".to_string(),
                    args: cfg.claude_extra_args.clone(),
                    env: cfg.env_for(provider).clone(),
                    interface: CliProfileInterface::StreamJson,
                    prompt: CliPromptMode::Stdin,
                    effort: cfg.claude_reasoning_effort,
                    model_env: Some("ANTHROPIC_MODEL".to_string()),
                };
                Arc::new(ClaudeCliExecutor::new(config))
            }
            Backend::CursorCli => Arc::new(CursorCliExecutor::new(
                cfg.codex_reasoning_effort.clone(),
                cfg.env_for(provider).clone(),
            )),
            Backend::OpenCodeCli => {
                let prefix = cfg.opencode_provider_for(provider).to_string();
                let effort = cfg.reasoning_effort_for(provider).map(str::to_string);
                Arc::new(OpenCodeCliExecutor::new(
                    prefix,
                    effort,
                    cfg.env_for(provider).clone(),
                ))
            }
            Backend::PiCli => {
                let effort = match provider {
                    Provider::OpenAI => Some(cfg.codex_reasoning_effort.clone()),
                    Provider::Anthropic => {
                        cfg.claude_reasoning_effort.map(|value| value.to_string())
                    }
                    _ => cfg.reasoning_effort_for(provider).map(str::to_string),
                };
                Arc::new(PiCliExecutor::new(
                    cfg.pi_provider_for(provider).to_string(),
                    effort,
                    cfg.env_for(provider).clone(),
                ))
            }
            Backend::Profile => {
                let selected = cfg.selected_cli_profile_for(provider).ok_or_else(|| {
                    anyhow::anyhow!(
                        "backend 'profile' has no selected CLI profile for {provider:?}"
                    )
                })?;
                match selected.profile.profile_type {
                    CliProfileType::ClaudeCli => {
                        let config = ClaudeCliConfig {
                            command: selected.profile.command.clone(),
                            args: selected.profile.args.clone(),
                            env: {
                                let mut env = selected.profile.env.clone();
                                env.extend(cfg.env_for(provider).clone());
                                env
                            },
                            interface: selected.profile.interface.clone(),
                            prompt: selected.profile.prompt.clone(),
                            effort: selected.profile.effort,
                            model_env: selected.profile.model_env.clone(),
                        };
                        Arc::new(ClaudeCliExecutor::new(config))
                    }
                }
            }
        };

        cache.insert(cache_key, executor.clone());
        Ok(executor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse::parse_config_with_cli_profiles;
    use crate::config::types::{CliProfile, CliProfileInterface, CliProfileType, CliPromptMode};
    use std::collections::BTreeMap;

    fn test_cli_profiles() -> BTreeMap<String, CliProfile> {
        let mut map = BTreeMap::new();
        map.insert(
            "claude".to_string(),
            CliProfile {
                profile_type: CliProfileType::ClaudeCli,
                command: "sh".to_string(),
                args: vec!["-c".to_string(), "echo ok".to_string()],
                env: BTreeMap::new(),
                interface: CliProfileInterface::Text,
                prompt: CliPromptMode::Stdin,
                effort: None,
                model_env: None,
            },
        );
        map
    }

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn test_claude_cli_executor_is_created() {
        let env = env_from(&[("CONSULT_LLM_ANTHROPIC_BACKEND", "claude-cli")]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("claude-opus-5")
            .expect("should create claude cli executor");
        assert_eq!(executor.backend_name(), "claude_cli");
    }

    #[test]
    fn test_claude_cli_executor_uses_configured_effort() {
        let env = env_from(&[
            ("CONSULT_LLM_ANTHROPIC_BACKEND", "claude-cli"),
            ("CONSULT_LLM_CLAUDE_REASONING_EFFORT", "x-high"),
        ]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("claude-opus-5")
            .expect("should create claude cli executor");
        assert_eq!(executor.reasoning_effort("claude-opus-5"), Some("xhigh"));
    }

    #[test]
    fn test_grok_cli_executor_is_created() {
        let env = env_from(&[("CONSULT_LLM_GROK_BACKEND", "grok-cli")]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("grok-4.5")
            .expect("should create grok cli executor");
        assert_eq!(executor.backend_name(), "grok_cli");
    }

    #[test]
    fn test_grok_cli_executor_uses_configured_effort() {
        let env = env_from(&[
            ("CONSULT_LLM_GROK_BACKEND", "grok-cli"),
            ("CONSULT_LLM_GROK_REASONING_EFFORT", "high"),
        ]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("grok-4.5")
            .expect("should create grok cli executor");
        assert_eq!(executor.reasoning_effort("grok-4.5"), Some("high"));
    }

    #[test]
    fn test_pi_executor_is_created_with_reasoning_effort() {
        let env = env_from(&[
            ("CONSULT_LLM_OPENAI_BACKEND", "pi"),
            ("CONSULT_LLM_CODEX_REASONING_EFFORT", "xhigh"),
        ]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("gpt-5.6-sol")
            .expect("should create Pi executor");

        assert_eq!(executor.backend_name(), "pi_cli");
        assert_eq!(executor.reasoning_effort("gpt-5.6-sol"), Some("xhigh"));
    }

    #[test]
    fn test_gemini_profile_executor_is_created() {
        let env = env_from(&[
            ("CONSULT_LLM_GEMINI_BACKEND", "profile"),
            ("CONSULT_LLM_GEMINI_CLI_PROFILE", "claude"),
        ]);
        let (config, _) = parse_config_with_cli_profiles(env, test_cli_profiles()).unwrap();
        let provider = ExecutorProvider::new(Arc::new(config));
        let executor = provider
            .get_executor("gemini-3.1-pro-preview")
            .expect("should create profile executor for gemini model");
        assert_eq!(executor.backend_name(), "claude_cli");
    }
}
