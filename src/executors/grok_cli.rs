use super::stream::{ParsedStreamEvent, StreamEvents, parse_json_line};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities, Usage};
use super::{append_file_refs, prepare_cli_request, run_cli_executor_with_env};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

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

fn grok_log_path(env: &std::collections::BTreeMap<String, String>) -> Option<PathBuf> {
    env.get("GROK_HOME")
        .cloned()
        .or_else(|| std::env::var("GROK_HOME").ok())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".grok")))
        .map(|home| home.join("logs/unified.jsonl"))
}

fn grok_log_offset(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn read_grok_usage(path: &Path, offset: u64, session_id: &str) -> Option<Usage> {
    let mut file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() < offset || file.seek(std::io::SeekFrom::Start(offset)).is_err()
    {
        return None;
    }

    let mut appended = String::new();
    file.read_to_string(&mut appended).ok()?;
    let mut prompt_tokens = 0;
    let mut completion_tokens = 0;
    let mut found = false;

    for line in appended.lines() {
        let Some(event) = parse_json_line(line) else {
            continue;
        };
        if event.get("sid").and_then(|value| value.as_str()) != Some(session_id)
            || event.get("msg").and_then(|value| value.as_str())
                != Some("shell.turn.inference_done")
        {
            continue;
        }
        let Some(ctx) = event.get("ctx") else {
            continue;
        };
        let Some(input) = ctx.get("prompt_tokens").and_then(|value| value.as_u64()) else {
            continue;
        };
        let Some(output) = ctx
            .get("completion_tokens")
            .and_then(|value| value.as_u64())
        else {
            continue;
        };
        prompt_tokens += input;
        completion_tokens += output;
        found = true;
    }

    found.then_some(Usage {
        prompt_tokens,
        completion_tokens,
        cost: None,
    })
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

        let log_path = grok_log_path(&self.env);
        let log_offset = log_path.as_deref().map(grok_log_offset).unwrap_or_default();
        let mut parser = parse_grok_line;
        let mut result = run_cli_executor_with_env(
            "grok",
            &args,
            Some(&self.env),
            None,
            &prepared.prompt,
            &prepared.system_prompt,
            prepared.spool,
            &mut parser,
        )?;
        if let (Some(path), Some(session_id)) = (log_path.as_deref(), result.thread_id.as_deref()) {
            result.usage = read_grok_usage(path, log_offset, session_id);
        }
        Ok(result)
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
    fn reads_usage_appended_for_session() {
        let mut log = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(log, r#"{{"sid":"old","msg":"shell.turn.inference_done","ctx":{{"prompt_tokens":1,"completion_tokens":2}}}}"#).unwrap();
        let offset = log.as_file().metadata().unwrap().len();
        writeln!(log, r#"{{"sid":"session-1","msg":"shell.turn.inference_done","ctx":{{"prompt_tokens":100,"completion_tokens":20}}}}"#).unwrap();
        writeln!(log, r#"{{"sid":"other","msg":"shell.turn.inference_done","ctx":{{"prompt_tokens":999,"completion_tokens":999}}}}"#).unwrap();
        writeln!(log, r#"{{"sid":"session-1","msg":"shell.turn.inference_done","ctx":{{"prompt_tokens":50,"completion_tokens":10}}}}"#).unwrap();

        let usage = read_grok_usage(log.path(), offset, "session-1").unwrap();
        assert_eq!(usage.prompt_tokens, 150);
        assert_eq!(usage.completion_tokens, 30);
    }

    #[test]
    fn ignores_non_json_and_unknown_events() {
        assert!(parse_grok_line("").is_empty());
        assert!(parse_grok_line("not json").is_empty());
        assert!(parse_grok_line(r#"{"type":"unknown"}"#).is_empty());
    }
}
