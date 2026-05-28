use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::{Backend, Config};
use crate::executors::anthropic_api::AnthropicApiExecutor;
use crate::executors::antigravity_cli::AntigravityCliExecutor;
use crate::executors::api::ApiExecutor;
use crate::executors::codex_cli::CodexCliExecutor;
use crate::executors::cursor_cli::CursorCliExecutor;
use crate::executors::gemini_cli::GeminiCliExecutor;
use crate::executors::opencode_cli::OpenCodeCliExecutor;
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
        // "the connection went silent" — heartbeat bytes count as liveness
        // and reset the timer naturally. Set per-request in the executors.
        let idle_timeout = config.api_idle_timeout;

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(std::time::Duration::from_secs(30)))
            // Bound body upload so a provider that accepts the connection
            // but never reads can't hang `.send()` forever.
            .timeout_send_body(Some(std::time::Duration::from_secs(120)))
            // Absolute lifetime cap on any single request — backstop for
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
                let base = provider.api_base_url().map(|s| s.to_string());
                let idle_timeout = self.idle_timeout;
                match provider.api_protocol() {
                    ApiProtocol::OpenAiCompat(runtime) => Arc::new(ApiExecutor::new(
                        self.agent.clone(),
                        key.to_string(),
                        base,
                        idle_timeout,
                        runtime,
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
            )),
            Backend::GeminiCli => Arc::new(GeminiCliExecutor::new(cfg.gemini_extra_args.clone())),
            Backend::CursorCli => {
                Arc::new(CursorCliExecutor::new(cfg.codex_reasoning_effort.clone()))
            }
            Backend::OpenCodeCli => {
                let prefix = cfg.opencode_provider_for(provider).to_string();
                Arc::new(OpenCodeCliExecutor::new(prefix))
            }
            Backend::AntigravityCli => {
                Arc::new(AntigravityCliExecutor::new(cfg.agy_extra_args.clone()))
            }
        };

        cache.insert(cache_key, executor.clone());
        Ok(executor)
    }
}
