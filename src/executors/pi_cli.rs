use super::stream::{ParsedStreamEvent, StreamEvents, parse_json_line};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{CliOutputParser, append_file_refs, prepare_cli_request, run_cli_executor_with_env};

pub struct PiCliExecutor {
    capabilities: LlmExecutorCapabilities,
    provider: String,
    reasoning_effort: Option<String>,
    env: std::collections::BTreeMap<String, String>,
}

impl PiCliExecutor {
    pub fn new(
        provider: String,
        reasoning_effort: Option<String>,
        env: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                supports_threads: true,
                supports_file_refs: true,
            },
            provider,
            reasoning_effort,
            env,
        }
    }
}

#[derive(Default)]
pub struct PiOutputParser;

fn usage_event(value: &serde_json::Value) -> Option<ParsedStreamEvent> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("message")?.get("usage"))?;
    let prompt_tokens = ["input", "cacheRead", "cacheWrite"]
        .iter()
        .map(|key| usage.get(key).and_then(|v| v.as_u64()).unwrap_or(0))
        .sum();
    Some(ParsedStreamEvent::Usage {
        prompt_tokens,
        completion_tokens: usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
        cost: usage
            .get("cost")
            .and_then(|cost| cost.get("total"))
            .and_then(|v| v.as_f64()),
    })
}

impl CliOutputParser for PiOutputParser {
    fn on_line(&mut self, line: &str) -> anyhow::Result<StreamEvents> {
        use smallvec::smallvec;

        let Some(event) = parse_json_line(line) else {
            return Ok(smallvec![]);
        };

        let events = match event.get("type").and_then(|value| value.as_str()) {
            Some("session") => event
                .get("id")
                .and_then(|value| value.as_str())
                .map(|id| smallvec![ParsedStreamEvent::SessionStarted { id: id.to_string() }])
                .unwrap_or_default(),
            Some("message_update") => {
                let assistant_event = &event["assistantMessageEvent"];
                let mut events = smallvec![];
                match assistant_event.get("type").and_then(|value| value.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = assistant_event.get("delta").and_then(|v| v.as_str()) {
                            events.push(ParsedStreamEvent::AssistantText {
                                text: text.to_string(),
                            });
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(text) = assistant_event.get("delta").and_then(|v| v.as_str()) {
                            events.push(ParsedStreamEvent::Thinking {
                                text: text.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
                if let Some(usage) = usage_event(&event) {
                    events.push(usage);
                }
                events
            }
            Some("message_end")
                if event
                    .get("message")
                    .and_then(|m| m.get("role"))
                    .and_then(|v| v.as_str())
                    == Some("assistant") =>
            {
                let message = &event["message"];
                if matches!(
                    message.get("stopReason").and_then(|v| v.as_str()),
                    Some("error" | "aborted")
                ) {
                    let detail = message
                        .get("errorMessage")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Pi request failed");
                    anyhow::bail!(detail.to_string());
                }
                usage_event(&event)
                    .map(|usage| smallvec![usage])
                    .unwrap_or_default()
            }
            Some("tool_execution_start") => {
                let call_id = event
                    .get("toolCallId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let label = event
                    .get("toolName")
                    .and_then(|value| value.as_str())
                    .unwrap_or("tool")
                    .to_string();
                smallvec![ParsedStreamEvent::ToolStarted { call_id, label }]
            }
            Some("tool_execution_end") => {
                let call_id = event
                    .get("toolCallId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let success = event.get("isError").and_then(|value| value.as_bool()) != Some(true);
                smallvec![ParsedStreamEvent::ToolFinished {
                    call_id,
                    success,
                    error: (!success).then(|| "tool execution failed".to_string()),
                }]
            }
            _ => smallvec![],
        };
        Ok(events)
    }
}

fn pi_reasoning_effort(value: &str) -> &str {
    match value {
        "none" => "off",
        "x-high" | "extra-high" => "xhigh",
        other => other,
    }
}

impl LlmExecutor for PiCliExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "pi_cli"
    }

    fn reasoning_effort(&self, _model: &str) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let prepared = prepare_cli_request(req, append_file_refs);
        let mut args = vec![
            "--mode".to_string(),
            "json".to_string(),
            "--provider".to_string(),
            self.provider.clone(),
            "--model".to_string(),
            prepared.model.clone(),
        ];

        if let Some(effort) = self.reasoning_effort.as_deref() {
            args.push("--thinking".to_string());
            args.push(pi_reasoning_effort(effort).to_string());
        }
        if let Some(thread_id) = prepared.thread_id.as_deref() {
            args.push("--session".to_string());
            args.push(thread_id.to_string());
        }

        let mut parser = PiOutputParser;
        run_cli_executor_with_env(
            "pi",
            &args,
            Some(&self.env),
            Some(&prepared.stdin_prompt),
            &prepared.prompt,
            &prepared.system_prompt,
            prepared.spool,
            &mut parser,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::stream::StreamReducer;
    use consult_llm_core::monitoring::RunSpool;
    use std::sync::{Arc, Mutex};

    fn reduce(lines: &[&str]) -> StreamReducer {
        let mut reducer =
            StreamReducer::new(Arc::new(Mutex::new(RunSpool::disabled())), None, None);
        let mut parser = PiOutputParser;
        for line in lines {
            reducer.process(parser.on_line(line).unwrap());
        }
        reducer
    }

    #[test]
    fn parses_session_text_and_final_usage() {
        let reducer = reduce(&[
            r#"{"type":"session","id":"session-1"}"#,
            r#"{"type":"message_update","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"cost":{"total":0}},"assistantMessageEvent":{"type":"text_delta","delta":"hello"}}"#,
            r#"{"type":"message_end","message":{"role":"assistant","stopReason":"stop","usage":{"input":100,"output":20,"cacheRead":30,"cacheWrite":5,"cost":{"total":0.25}}}}"#,
        ]);

        assert_eq!(reducer.thread_id.as_deref(), Some("session-1"));
        assert_eq!(reducer.response, "hello");
        let usage = reducer.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 135);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.cost, Some(0.25));
    }

    #[test]
    fn parses_tool_lifecycle() {
        let reducer = reduce(&[
            r#"{"type":"tool_execution_start","toolCallId":"call-1","toolName":"read"}"#,
            r#"{"type":"tool_execution_end","toolCallId":"call-1","toolName":"read","isError":false}"#,
        ]);
        assert!(reducer.response.is_empty());
    }

    #[test]
    fn rejects_failed_assistant_message() {
        let mut parser = PiOutputParser;
        let error = parser
            .on_line(
                r#"{"type":"message_end","message":{"role":"assistant","stopReason":"error","errorMessage":"auth failed"}}"#,
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "auth failed");
    }

    #[test]
    fn maps_pi_thinking_levels() {
        assert_eq!(pi_reasoning_effort("none"), "off");
        assert_eq!(pi_reasoning_effort("x-high"), "xhigh");
        assert_eq!(pi_reasoning_effort("high"), "high");
    }
}
