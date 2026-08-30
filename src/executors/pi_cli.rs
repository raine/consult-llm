use super::stream::{ParsedStreamEvent, StreamEvents, parse_json_line, tool_label};
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
pub struct PiOutputParser {
    prompt_tokens: u64,
    completion_tokens: u64,
    cost: Option<f64>,
    pending_error: Option<String>,
}

impl PiOutputParser {
    fn add_usage(&mut self, message: &serde_json::Value) -> Option<ParsedStreamEvent> {
        let usage = message.get("usage")?;
        self.prompt_tokens += ["input", "cacheRead", "cacheWrite"]
            .iter()
            .map(|key| usage.get(key).and_then(|v| v.as_u64()).unwrap_or(0))
            .sum::<u64>();
        self.completion_tokens += usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(cost) = usage
            .get("cost")
            .and_then(|value| value.get("total"))
            .and_then(|value| value.as_f64())
        {
            self.cost = Some(self.cost.unwrap_or(0.0) + cost);
        }
        Some(ParsedStreamEvent::Usage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            cost: self.cost,
        })
    }
}

fn tool_detail(event: &serde_json::Value) -> Option<&str> {
    let args = event.get("args")?;
    ["path", "command", "pattern", "query"]
        .iter()
        .find_map(|key| args.get(key).and_then(|value| value.as_str()))
}

fn tool_error(event: &serde_json::Value) -> Option<String> {
    event
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(|content| content.as_array())
        .and_then(|content| {
            content.iter().find_map(|item| {
                (item.get("type").and_then(|value| value.as_str()) == Some("text"))
                    .then(|| item.get("text").and_then(|value| value.as_str()))
                    .flatten()
            })
        })
        .map(str::to_string)
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
                let failed = matches!(
                    message.get("stopReason").and_then(|v| v.as_str()),
                    Some("error" | "aborted")
                );
                if failed {
                    self.pending_error = Some(
                        message
                            .get("errorMessage")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Pi request failed")
                            .to_string(),
                    );
                } else {
                    self.pending_error = None;
                }
                self.add_usage(message)
                    .map(|usage| smallvec![usage])
                    .unwrap_or_default()
            }
            Some("tool_execution_start") => {
                let call_id = event
                    .get("toolCallId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = event
                    .get("toolName")
                    .and_then(|value| value.as_str())
                    .unwrap_or("tool");
                let label = tool_label(name, tool_detail(&event));
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
                    error: (!success).then(|| {
                        tool_error(&event).unwrap_or_else(|| "tool execution failed".to_string())
                    }),
                }]
            }
            _ => smallvec![],
        };
        Ok(events)
    }

    fn finish(&mut self) -> anyhow::Result<StreamEvents> {
        if let Some(error) = self.pending_error.take() {
            anyhow::bail!(error);
        }
        Ok(smallvec::smallvec![])
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

        let mut parser = PiOutputParser::default();
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
        let mut parser = PiOutputParser::default();
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
    fn accumulates_usage_across_tool_turns() {
        let reducer = reduce(&[
            r#"{"type":"message_end","message":{"role":"assistant","stopReason":"toolUse","usage":{"input":10,"output":4,"cacheRead":20,"cacheWrite":5,"cost":{"total":0.1}}}}"#,
            r#"{"type":"message_end","message":{"role":"assistant","stopReason":"stop","usage":{"input":3,"output":6,"cacheRead":30,"cacheWrite":2,"cost":{"total":0.2}}}}"#,
        ]);

        let usage = reducer.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 70);
        assert_eq!(usage.completion_tokens, 10);
        assert!((usage.cost.unwrap() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_tool_lifecycle() {
        let reducer = reduce(&[
            r#"{"type":"tool_execution_start","toolCallId":"call-1","toolName":"read","args":{"path":"src/main.rs"}}"#,
            r#"{"type":"tool_execution_end","toolCallId":"call-1","toolName":"read","isError":false}"#,
        ]);
        assert!(reducer.response.is_empty());
    }

    #[test]
    fn reports_final_failed_assistant_message() {
        let mut parser = PiOutputParser::default();
        parser
            .on_line(
                r#"{"type":"message_end","message":{"role":"assistant","stopReason":"error","errorMessage":"auth failed"}}"#,
            )
            .unwrap();
        let error = parser.finish().unwrap_err();
        assert_eq!(error.to_string(), "auth failed");
    }

    #[test]
    fn successful_retry_clears_prior_error() {
        let mut parser = PiOutputParser::default();
        parser
            .on_line(
                r#"{"type":"message_end","message":{"role":"assistant","stopReason":"error","errorMessage":"temporary"}}"#,
            )
            .unwrap();
        parser
            .on_line(r#"{"type":"message_end","message":{"role":"assistant","stopReason":"stop"}}"#)
            .unwrap();
        assert!(parser.finish().is_ok());
    }

    #[test]
    fn extracts_tool_detail_and_error() {
        let event: serde_json::Value = serde_json::from_str(
            r#"{"args":{"path":"src/main.rs"},"result":{"content":[{"type":"text","text":"missing"}]}}"#,
        )
        .unwrap();
        assert_eq!(tool_detail(&event), Some("src/main.rs"));
        assert_eq!(tool_error(&event).as_deref(), Some("missing"));
    }

    #[test]
    fn maps_pi_thinking_levels() {
        assert_eq!(pi_reasoning_effort("none"), "off");
        assert_eq!(pi_reasoning_effort("x-high"), "xhigh");
        assert_eq!(pi_reasoning_effort("high"), "high");
    }
}
