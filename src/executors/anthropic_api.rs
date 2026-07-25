use std::time::Duration;

use serde::Serialize;

use super::anthropic_events::AnthropicStreamHandler;
use super::api_common::{ApiTextMessage, prepare_api_turn};
use super::api_transport::{StreamLabels, StreamRequest, run_stream};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 32_000;
const LABELS: StreamLabels = StreamLabels {
    request: "Anthropic API request",
    stream: "Anthropic stream",
};

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    system: String,
    messages: Vec<ApiTextMessage>,
    max_tokens: u32,
    stream: bool,
}

pub struct AnthropicApiExecutor {
    agent: ureq::Agent,
    api_key: String,
    base_url: String,
    idle_timeout: Duration,
    capabilities: LlmExecutorCapabilities,
}

impl AnthropicApiExecutor {
    pub fn new(
        agent: ureq::Agent,
        api_key: String,
        base_url: Option<String>,
        idle_timeout: Duration,
    ) -> Self {
        Self {
            agent,
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
            idle_timeout,
            capabilities: LlmExecutorCapabilities {
                is_cli: false,
                supports_threads: true,
                supports_file_refs: false,
            },
        }
    }
}

impl LlmExecutor for AnthropicApiExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "api"
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let turn = prepare_api_turn(req)?;

        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/v1/messages");

        let messages = turn.transcript_messages();

        let request = MessagesRequest {
            model: turn.model().to_string(),
            system: turn.system_prompt().to_string(),
            messages,
            max_tokens: DEFAULT_MAX_TOKENS,
            stream: true,
        };
        let body = serde_json::to_vec(&request)?;

        let handler = AnthropicStreamHandler::new(turn.spool(), DEFAULT_MAX_TOKENS);
        let outcome = run_stream(
            StreamRequest {
                agent: &self.agent,
                url,
                headers: vec![
                    ("x-api-key".to_string(), self.api_key.clone()),
                    (
                        "anthropic-version".to_string(),
                        ANTHROPIC_VERSION.to_string(),
                    ),
                    ("Content-Type".to_string(), "application/json".to_string()),
                ],
                body,
                idle_timeout: self.idle_timeout,
                model: turn.model().to_string(),
                labels: LABELS,
            },
            handler,
        )?;

        turn.commit(outcome.response, outcome.usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_omits_empty_system() {
        let req = MessagesRequest {
            model: "claude-opus-5".into(),
            system: String::new(),
            messages: vec![ApiTextMessage::user("hi".into())],
            max_tokens: 1024,
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("\"system\""), "empty system must be omitted");
        assert!(json.contains("\"stream\":true"));
    }
}
