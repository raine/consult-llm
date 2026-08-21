use super::stream::{
    ParsedStreamEvent, StreamEvents, first_non_empty_string, parse_json_line, tool_label,
};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{CliOutputParser, prepare_cli_request, run_cli_executor_with_env};

pub struct PiCliExecutor {
    capabilities: LlmExecutorCapabilities,
    /// pi provider name (e.g. "anthropic", "google", "xai")
    provider_prefix: String,
    thinking: Option<String>,
    env: std::collections::BTreeMap<String, String>,
}

impl PiCliExecutor {
    pub fn new(
        provider_prefix: String,
        thinking: Option<String>,
        env: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                supports_threads: true,
                supports_file_refs: true,
            },
            provider_prefix,
            thinking,
            env,
        }
    }
}

/// Stateful parser for `pi --mode json` output (one JSON event per line).
///
/// Usage is reported per assistant message (`message_end`); the parser
/// accumulates it and emits running totals so the reducer ends up with the
/// sum across all turns instead of the last message's slice.
pub struct PiJsonParser {
    prompt_tokens: u64,
    completion_tokens: u64,
    cost: f64,
    seen_usage: bool,
}

impl PiJsonParser {
    pub fn new() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            cost: 0.0,
            seen_usage: false,
        }
    }
}

impl Default for PiJsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CliOutputParser for PiJsonParser {
    fn on_line(&mut self, line: &str) -> anyhow::Result<StreamEvents> {
        use smallvec::smallvec;

        let Some(event) = parse_json_line(line) else {
            return Ok(smallvec![]);
        };

        let events = match event.get("type").and_then(|t| t.as_str()) {
            Some("session") => {
                if let Some(id) = event.get("id").and_then(|v| v.as_str()) {
                    smallvec![ParsedStreamEvent::SessionStarted { id: id.to_string() }]
                } else {
                    smallvec![]
                }
            }
            Some("message_update") => {
                if let Some(delta) = event
                    .get("assistantMessageEvent")
                    .filter(|e| e.get("type").and_then(|t| t.as_str()) == Some("text_delta"))
                    .and_then(|e| e.get("delta"))
                    .and_then(|d| d.as_str())
                {
                    smallvec![ParsedStreamEvent::AssistantText {
                        text: delta.to_string(),
                    }]
                } else {
                    smallvec![]
                }
            }
            Some("tool_execution_start") => {
                let call_id = event
                    .get("toolCallId")
                    .and_then(|id| id.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let name = event
                    .get("toolName")
                    .and_then(|name| name.as_str())
                    .unwrap_or("tool");
                let detail = event.get("args").and_then(tool_detail);
                smallvec![ParsedStreamEvent::ToolStarted {
                    call_id,
                    label: tool_label(name, detail.as_deref()),
                }]
            }
            Some("tool_execution_end") => {
                let call_id = event
                    .get("toolCallId")
                    .and_then(|id| id.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let is_error = event
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                smallvec![ParsedStreamEvent::ToolFinished {
                    call_id,
                    success: !is_error,
                    error: if is_error {
                        event.get("result").and_then(error_text)
                    } else {
                        None
                    },
                }]
            }
            Some("message_end") => {
                let usage = event
                    .get("message")
                    .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
                    .and_then(|m| m.get("usage"));
                if let Some(usage) = usage {
                    self.prompt_tokens += usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
                    self.completion_tokens +=
                        usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                    self.cost += usage
                        .get("cost")
                        .and_then(|c| c.get("total"))
                        .and_then(|t| t.as_f64())
                        .unwrap_or(0.0);
                    self.seen_usage = true;
                    smallvec![ParsedStreamEvent::Usage {
                        prompt_tokens: self.prompt_tokens,
                        completion_tokens: self.completion_tokens,
                        cost: Some(self.cost),
                    }]
                } else {
                    smallvec![]
                }
            }
            _ => smallvec![],
        };
        Ok(events)
    }
}

fn error_text(result: &serde_json::Value) -> Option<String> {
    result
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            result
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .or_else(|| {
            result
                .get("content")
                .and_then(|c| c.as_str())
                .map(str::to_string)
        })
        .or_else(|| Some(result.to_string()).filter(|r| r != "null"))
}

fn tool_detail(args: &serde_json::Value) -> Option<String> {
    first_non_empty_string(
        args,
        &[
            "path",
            "filePath",
            "file_path",
            "pattern",
            "command",
            "cmd",
            "url",
            "query",
            "description",
        ],
    )
}

/// Map a configured reasoning effort to a `pi --thinking` level.
/// pi accepts off|minimal|low|medium|high|xhigh|max; consult-llm's
/// codex-style "none" maps to "off".
fn thinking_level(effort: &str) -> String {
    match effort {
        "none" => "off".to_string(),
        other => other.to_string(),
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
        self.thinking.as_deref()
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        // Files are passed as `@path` positional args (pi loads them into
        // context), so the prompt text itself needs no file references.
        let prepared = prepare_cli_request(req, |text, _| text.to_string());
        let tid = prepared.thread_id.as_deref();

        let pi_model = if prepared
            .model
            .starts_with(&format!("{}/", self.provider_prefix))
        {
            // Model already includes the provider prefix
            // (e.g. "openrouter/xiaomi/mimo-v2.5-pro" with prefix "openrouter").
            prepared.model.to_string()
        } else {
            format!("{}/{}", self.provider_prefix, prepared.model)
        };

        let mut args: Vec<String> = vec![
            "--mode".to_string(),
            "json".to_string(),
            "--model".to_string(),
            pi_model,
        ];

        if let Some(effort) = &self.thinking {
            args.push("--thinking".to_string());
            args.push(thinking_level(effort));
        }

        if let Some(t) = tid {
            args.push("--session".to_string());
            args.push(t.to_string());
        }

        if let Some(fps) = prepared.file_paths.as_deref() {
            for fp in fps {
                args.push(format!("@{}", fp.display()));
            }
        }

        let mut parser = PiJsonParser::new();
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

    fn parse_line(parser: &mut PiJsonParser, line: &str) -> StreamEvents {
        parser.on_line(line).unwrap()
    }

    #[test]
    fn test_parse_session_header() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"session","version":3,"id":"1b0d1c7e-1234-4abc-9def-0123456789ab","timestamp":"2026-08-11T10:00:00.000Z","cwd":"/repo"}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::SessionStarted { id } if id == "1b0d1c7e-1234-4abc-9def-0123456789ab")
        );
    }

    #[test]
    fn test_parse_text_delta() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello"}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ParsedStreamEvent::AssistantText { text } if text == "Hello"));
    }

    #[test]
    fn test_parse_ignores_thinking_delta() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","contentIndex":0,"delta":"hmm"}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_tool_execution_start() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"read","args":{"path":"src/main.rs"}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::ToolStarted { call_id, label } if call_id == "call_1" && label == "read src/main.rs")
        );
    }

    #[test]
    fn test_parse_tool_execution_end_success() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"tool_execution_end","toolCallId":"call_1","toolName":"bash","result":{"content":[{"type":"text","text":"ok"}],"details":{}},"isError":false}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::ToolFinished { call_id, success, error } if call_id == "call_1" && *success && error.is_none())
        );
    }

    #[test]
    fn test_parse_tool_execution_end_error() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"tool_execution_end","toolCallId":"call_1","toolName":"bash","result":{"message":"exit 1"},"isError":true}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::ToolFinished { call_id, success, error } if call_id == "call_1" && !*success && error.as_deref() == Some("exit 1"))
        );
    }

    #[test]
    fn test_usage_accumulates_across_messages() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"message_end","message":{"role":"assistant","content":[],"usage":{"input":1000,"output":50,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}}}}"#,
        );
        assert!(matches!(
            &events[0],
            ParsedStreamEvent::Usage { prompt_tokens: 1000, completion_tokens: 50, cost } if *cost == Some(0.01)
        ));
        let events = parse_line(
            &mut p,
            r#"{"type":"message_end","message":{"role":"assistant","content":[],"usage":{"input":2000,"output":70,"cacheRead":500,"cacheWrite":0,"cost":{"total":0.02}}}}"#,
        );
        assert!(matches!(
            &events[0],
            ParsedStreamEvent::Usage { prompt_tokens: 3000, completion_tokens: 120, cost } if *cost == Some(0.03)
        ));
    }

    #[test]
    fn test_parse_ignores_user_message_end() {
        let mut p = PiJsonParser::new();
        let events = parse_line(
            &mut p,
            r#"{"type":"message_end","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_ignores_lifecycle_events() {
        let mut p = PiJsonParser::new();
        for line in [
            r#"{"type":"agent_start"}"#,
            r#"{"type":"turn_start"}"#,
            r#"{"type":"message_start","message":{"role":"assistant","content":[]}}"#,
            r#"{"type":"turn_end","message":{},"toolResults":[]}"#,
            r#"{"type":"agent_end","messages":[]}"#,
        ] {
            assert!(parse_line(&mut p, line).is_empty(), "line: {line}");
        }
    }

    #[test]
    fn test_parse_empty_and_malformed() {
        let mut p = PiJsonParser::new();
        assert!(parse_line(&mut p, "").is_empty());
        assert!(parse_line(&mut p, "  ").is_empty());
        assert!(parse_line(&mut p, "not json").is_empty());
    }

    #[test]
    fn test_thinking_level_maps_none_to_off() {
        assert_eq!(thinking_level("none"), "off");
        assert_eq!(thinking_level("high"), "high");
        assert_eq!(thinking_level("xhigh"), "xhigh");
    }

    #[test]
    fn test_pi_reasoning_effort_reports_thinking() {
        let executor = PiCliExecutor::new(
            "anthropic".into(),
            Some("high".into()),
            std::collections::BTreeMap::new(),
        );
        assert_eq!(executor.reasoning_effort("claude-opus-5"), Some("high"));
    }

    #[test]
    fn test_reducer_full_sequence() {
        let mut parser = PiJsonParser::new();
        let mut reducer = StreamReducer::new(
            std::sync::Arc::new(std::sync::Mutex::new(
                consult_llm_core::monitoring::RunSpool::disabled(),
            )),
            None,
            None,
        );
        let lines = vec![
            r#"{"type":"session","version":3,"id":"abc-123","timestamp":"t","cwd":"/repo"}"#,
            r#"{"type":"agent_start"}"#,
            r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"read","args":{"path":"src/main.rs"}}"#,
            r#"{"type":"tool_execution_end","toolCallId":"call_1","toolName":"read","result":{"content":[]},"isError":false}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"4"}}"#,
            r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"4"}],"usage":{"input":15000,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.05}}}}"#,
            r#"{"type":"agent_end","messages":[]}"#,
        ];
        for line in &lines {
            reducer.process(parser.on_line(line).unwrap());
        }
        assert_eq!(reducer.thread_id, Some("abc-123".to_string()));
        assert_eq!(reducer.response, "4");
        let usage = reducer.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 15000);
        assert_eq!(usage.completion_tokens, 1);
        assert_eq!(usage.cost, Some(0.05));
    }
}
