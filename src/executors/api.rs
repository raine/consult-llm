use std::time::Duration;

use serde::Serialize;

use super::api_chat::ChatStreamHandler;
use super::api_common::{ApiTextMessage, prepare_api_turn};
use super::api_transport::{StreamLabels, StreamRequest, run_stream};
use super::tag_splitter::TagSplitter;
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use crate::models::{OpenAiCompatRuntime, OpenAiExtraBody};

const LABELS: StreamLabels = StreamLabels {
    request: "API request",
    stream: "API stream",
};

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ApiTextMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_body: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct ReasoningOptions {
    effort: String,
}

pub struct ApiExecutor {
    agent: ureq::Agent,
    api_key: String,
    base_url: String,
    idle_timeout: Duration,
    runtime: OpenAiCompatRuntime,
    reasoning_effort: Option<String>,
    capabilities: LlmExecutorCapabilities,
}

impl ApiExecutor {
    pub fn new(
        agent: ureq::Agent,
        api_key: String,
        base_url: Option<String>,
        idle_timeout: Duration,
        runtime: OpenAiCompatRuntime,
        reasoning_effort: Option<String>,
    ) -> Self {
        Self {
            agent,
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1/".to_string()),
            idle_timeout,
            runtime,
            reasoning_effort,
            capabilities: LlmExecutorCapabilities {
                is_cli: false,
                supports_threads: true,
                supports_file_refs: false,
            },
        }
    }
}

fn extra_body(runtime: OpenAiCompatRuntime, model: &str) -> Option<serde_json::Value> {
    runtime.extra_body.and_then(|extra_body| {
        extra_body
            .applies_to_model(model)
            .then(|| match extra_body {
                OpenAiExtraBody::GoogleThinkingConfig => serde_json::json!({
                    "google": {
                        "thinking_config": {
                            "thinking_level": "high",
                            "include_thoughts": true
                        }
                    }
                }),
            })
    })
}

impl LlmExecutor for ApiExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "api"
    }

    fn reasoning_effort(&self, _model: &str) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let turn = prepare_api_turn(req)?;

        let base = if self.base_url.ends_with('/') {
            self.base_url.clone()
        } else {
            format!("{}/", self.base_url)
        };
        let url = format!("{base}chat/completions");

        let mut messages = vec![ApiTextMessage::system(turn.system_prompt().to_string())];
        messages.extend(turn.transcript_messages());

        let extra_body = extra_body(self.runtime, turn.model());
        let reasoning = self
            .reasoning_effort
            .as_ref()
            .map(|effort| ReasoningOptions {
                effort: effort.to_string(),
            });

        let request = ChatRequest {
            model: turn.model().to_string(),
            messages,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            reasoning,
            extra_body,
        };
        let body = serde_json::to_vec(&request)?;

        let splitter = self
            .runtime
            .think_tags
            .map(|tags| TagSplitter::new(tags.start, tags.end));
        let handler = ChatStreamHandler::new(splitter, turn.spool());

        let outcome = run_stream(
            StreamRequest {
                agent: &self.agent,
                url,
                headers: vec![
                    (
                        "Authorization".to_string(),
                        format!("Bearer {}", self.api_key),
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
