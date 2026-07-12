use super::stream::{ParsedStreamEvent, StreamEvents, parse_json_line};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{append_file_refs, prepare_cli_request, run_cli_executor_with_env};

pub struct GrokCliExecutor {
    capabilities: LlmExecutorCapabilities,
    reasoning_effort: Option<String>,
    extra_args: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
}

impl GrokCliExecutor {
    pub fn new(
        reasoning_effort: Option<String>,
        extra_args: Vec<String>,
        env: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                supports_threads: true,
                supports_file_refs: true,
            },
            reasoning_effort,
            extra_args,
            env,
        }
    }
}

pub fn parse_grok_line(line: &str) -> StreamEvents {
    use smallvec::smallvec;

    let Some(event) = parse_json_line(line) else {
        return smallvec![];
    };

    match event.get("type").and_then(|value| value.as_str()) {
        Some("thought") => event
            .get("data")
            .and_then(|value| value.as_str())
            .map(|text| {
                smallvec![ParsedStreamEvent::Thinking {
                    text: text.to_string(),
                }]
            })
            .unwrap_or_default(),
        Some("text") => event
            .get("data")
            .and_then(|value| value.as_str())
            .map(|text| {
                smallvec![ParsedStreamEvent::AssistantText {
                    text: text.to_string(),
                }]
            })
            .unwrap_or_default(),
        Some("end") => event
            .get("sessionId")
            .and_then(|value| value.as_str())
            .map(|id| smallvec![ParsedStreamEvent::SessionStarted { id: id.to_string() }])
            .unwrap_or_default(),
        _ => smallvec![],
    }
}

impl LlmExecutor for GrokCliExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "grok_cli"
    }

    fn reasoning_effort(&self, _model: &str) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let prepared = prepare_cli_request(req, append_file_refs);
        let mut args = vec![
            "--model".to_string(),
            prepared.model.clone(),
            "--output-format".to_string(),
            "streaming-json".to_string(),
            "--no-memory".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
        ];

        if let Some(thread_id) = prepared.thread_id.as_deref() {
            args.push("--resume".to_string());
            args.push(thread_id.to_string());
        }

        if let Some(effort) = &self.reasoning_effort {
            args.push("--reasoning-effort".to_string());
            args.push(effort.clone());
        }

        args.extend(self.extra_args.iter().cloned());
        args.push("--single".to_string());
        args.push(prepared.stdin_prompt.clone());

        let mut parser = parse_grok_line;
        run_cli_executor_with_env(
            "grok",
            &args,
            Some(&self.env),
            None,
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

    #[test]
    fn parses_thought_text_and_session_events() {
        assert!(matches!(
            &parse_grok_line(r#"{"type":"thought","data":"Checking"}"#)[0],
            ParsedStreamEvent::Thinking { text } if text == "Checking"
        ));
        assert!(matches!(
            &parse_grok_line(r#"{"type":"text","data":"Done"}"#)[0],
            ParsedStreamEvent::AssistantText { text } if text == "Done"
        ));
        assert!(matches!(
            &parse_grok_line(r#"{"type":"end","stopReason":"EndTurn","sessionId":"session-1"}"#)[0],
            ParsedStreamEvent::SessionStarted { id } if id == "session-1"
        ));
    }

    #[test]
    fn reducer_concatenates_streamed_text() {
        let mut reducer = StreamReducer::new(
            std::sync::Arc::new(std::sync::Mutex::new(
                consult_llm_core::monitoring::RunSpool::disabled(),
            )),
            None,
            None,
        );
        reducer.process(parse_grok_line(r#"{"type":"text","data":"GRO"}"#));
        reducer.process(parse_grok_line(r#"{"type":"text","data":"K_OK"}"#));
        reducer.process(parse_grok_line(
            r#"{"type":"end","stopReason":"EndTurn","sessionId":"session-1"}"#,
        ));

        assert_eq!(reducer.response, "GROK_OK");
        assert_eq!(reducer.thread_id.as_deref(), Some("session-1"));
    }

    #[test]
    fn ignores_non_json_and_unknown_events() {
        assert!(parse_grok_line("").is_empty());
        assert!(parse_grok_line("not json").is_empty());
        assert!(parse_grok_line(r#"{"type":"unknown"}"#).is_empty());
    }
}
