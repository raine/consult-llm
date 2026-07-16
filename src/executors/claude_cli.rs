use std::collections::BTreeMap;

use smallvec::smallvec;

use super::stream::{ParsedStreamEvent, StreamEvents, first_non_empty_string, tool_label};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{
    CliOutputParser, append_file_refs, build_extra_dir_args, prepare_cli_request,
    run_cli_executor_with_env,
};

use crate::config::types::{ClaudeEffort, CliProfileInterface, CliPromptMode};

/// Configuration for a claude-cli executor, decoupled from the profile system.
/// When the backend is `ClaudeCli` (native), this is constructed with defaults.
/// When the backend is `Profile` with `CliProfileType::ClaudeCli`, it's converted
/// from the selected profile.
#[derive(Debug, Clone)]
pub struct ClaudeCliConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub interface: CliProfileInterface,
    pub prompt: CliPromptMode,
    pub effort: Option<ClaudeEffort>,
    pub model_env: Option<String>,
}

pub struct ClaudeCliExecutor {
    capabilities: LlmExecutorCapabilities,
    config: ClaudeCliConfig,
}

impl ClaudeCliExecutor {
    pub fn new(config: ClaudeCliConfig) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                supports_threads: true,
                supports_file_refs: true,
            },
            config,
        }
    }
}

impl LlmExecutor for ClaudeCliExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "claude_cli"
    }

    fn reasoning_effort(&self, _model: &str) -> Option<&str> {
        self.config.effort.as_ref().map(ClaudeEffort::as_str)
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let prepared = prepare_cli_request(req, append_file_refs);
        let fps = prepared.file_paths.as_deref();
        let tid = prepared.thread_id.as_deref();

        let config = &self.config;

        let mut args: Vec<String> = config.args.clone();
        let mut env = config.env.clone();

        // Auto-inject boilerplate flags, skipping any already present in
        // profile args to avoid duplication.
        let output_format = match config.interface {
            CliProfileInterface::StreamJson => "stream-json",
            CliProfileInterface::Json => "json",
            CliProfileInterface::Text => "text",
        };
        let boilerplate_flags: &[(&str, Option<&str>)] = &[
            ("-p", None),
            ("--output-format", Some(output_format)),
            ("--verbose", None),
        ];
        for (flag, value) in boilerplate_flags {
            // Always inject --output-format to ensure the parser and CLI agree
            // on the output format, regardless of user-supplied extra args.
            if flag != &"--output-format" && args.iter().any(|a| a == flag) {
                continue;
            }
            args.push(flag.to_string());
            if let Some(v) = value {
                args.push(v.to_string());
            }
        }

        if let Some(t) = tid {
            args.push("--resume".into());
            args.push(t.to_string());
        } else {
            args.extend(build_extra_dir_args(fps, "--add-dir"));
        }

        if let Some(effort) = &config.effort
            && !args.iter().any(|a| a == "--effort")
        {
            args.push("--effort".into());
            args.push(effort.to_string());
        }

        // Route the requested model to the CLI.
        if let Some(model_env) = &config.model_env {
            env.insert(model_env.clone(), prepared.model.clone());
        } else if !args.iter().any(|a| a == "--model") {
            args.push("--model".into());
            args.push(prepared.model.clone());
        }

        // Default env vars for programmatic use. Profile-level env overrides these.
        env.entry("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into())
            .or_insert_with(|| "1".into());
        env.entry("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into())
            .or_insert_with(|| "1".into());
        env.entry("CLAUDE_CODE_DISABLE_UPDATE_CHECK".into())
            .or_insert_with(|| "1".into());
        env.entry("NO_COLOR".into()).or_insert_with(|| "1".into());

        let prompt_arg = prepared.stdin_prompt.clone();
        let stdin_prompt = match config.prompt {
            CliPromptMode::Stdin => Some(prepared.stdin_prompt),
            CliPromptMode::Argument => {
                args.push(prompt_arg);
                None
            }
        };

        let mut parser = ClaudeCliParser::new(config.interface.clone());

        run_cli_executor_with_env(
            &config.command,
            &args,
            Some(&env),
            stdin_prompt.as_deref(),
            &prepared.prompt,
            &prepared.system_prompt,
            prepared.spool,
            &mut parser,
        )
    }
}

// --- Parser ---

pub struct ClaudeCliParser {
    interface: CliProfileInterface,
    json_lines: Vec<String>,
    has_assistant_text: bool,
}

impl ClaudeCliParser {
    pub fn new(interface: CliProfileInterface) -> Self {
        Self {
            interface,
            json_lines: Vec::new(),
            has_assistant_text: false,
        }
    }

    fn parse_stream_json_line(&mut self, line: &str) -> StreamEvents {
        if line.is_empty() {
            return smallvec![];
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return smallvec![];
        };

        self.parse_stream_json_from_value(&event)
    }

    fn parse_json_document(&mut self, text: &str) -> StreamEvents {
        if text.trim().is_empty() {
            return smallvec![];
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(text) else {
            return smallvec![];
        };
        self.parse_stream_json_from_value(&event)
    }

    fn parse_stream_json_from_value(&mut self, event: &serde_json::Value) -> StreamEvents {
        match event.get("type").and_then(|t| t.as_str()) {
            Some("system") => self.parse_system_event(event),
            Some("assistant") => self.parse_assistant_event(event),
            Some("user") => self.parse_user_event(event),
            Some("result") => self.parse_result_event(event),
            Some("error") => smallvec![ParsedStreamEvent::AssistantText {
                text: format!("Error: {}", extract_error_message(event)),
            }],
            _ => smallvec![],
        }
    }

    fn parse_system_event(&self, event: &serde_json::Value) -> StreamEvents {
        match event.get("subtype").and_then(|s| s.as_str()) {
            Some("init") => event
                .get("session_id")
                .and_then(|id| id.as_str())
                .map(|id| smallvec![ParsedStreamEvent::SessionStarted { id: id.to_string() }])
                .unwrap_or_else(|| smallvec![]),
            // `thinking_tokens` are incremental token-count heartbeats emitted
            // during the model's thinking phase. They carry no content and
            // would flood the stream with empty Thinking events, keeping the
            // TUI stuck on "Thinking...". Skip them.
            Some("thinking_tokens") => smallvec![],
            _ => smallvec![],
        }
    }

    fn parse_assistant_event(&mut self, event: &serde_json::Value) -> StreamEvents {
        let mut events: StreamEvents = smallvec![];
        let Some(content) = event
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return events;
        };

        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("thinking") => {
                    events.push(ParsedStreamEvent::Thinking {
                        text: block
                            .get("thinking")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                Some("text") => {
                    self.has_assistant_text = true;
                    events.push(ParsedStreamEvent::AssistantText {
                        text: block
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                Some("tool_use") => {
                    let call_id = block
                        .get("id")
                        .and_then(|id| id.as_str())
                        .or_else(|| event.get("uuid").and_then(|id| id.as_str()))
                        .unwrap_or("tool")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|name| name.as_str())
                        .unwrap_or("tool");
                    events.push(ParsedStreamEvent::ToolStarted {
                        call_id,
                        label: tool_label(name, tool_detail(block).as_deref()),
                    });
                }
                _ => {}
            }
        }

        if let Some(u) = event.get("message").and_then(|m| m.get("usage"))
            && let Some(usage) = extract_usage(u)
        {
            events.push(usage);
        }
        events
    }

    fn parse_user_event(&self, event: &serde_json::Value) -> StreamEvents {
        let Some(content) = event
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return smallvec![];
        };

        let mut events: StreamEvents = smallvec![];
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                let call_id = block
                    .get("tool_use_id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let success = !block
                    .get("is_error")
                    .and_then(|is_error| is_error.as_bool())
                    .unwrap_or(false);
                let error = (!success).then(|| extract_tool_result_text(block));
                events.push(ParsedStreamEvent::ToolFinished {
                    call_id,
                    success,
                    error,
                });
            }
        }
        events
    }

    fn parse_result_event(&mut self, event: &serde_json::Value) -> StreamEvents {
        let mut events: StreamEvents = smallvec![];
        if !self.has_assistant_text
            && let Some(text) = event.get("result").and_then(|r| r.as_str())
        {
            events.push(ParsedStreamEvent::AssistantText {
                text: text.to_string(),
            });
        }
        if let Some(u) = event.get("usage")
            && let Some(usage) = extract_usage(u)
        {
            events.push(usage);
        }
        events
    }
}

fn tool_detail(block: &serde_json::Value) -> Option<String> {
    block.get("input").and_then(|input| {
        first_non_empty_string(
            input,
            &["file_path", "path", "pattern", "command", "cmd", "url"],
        )
    })
}

fn extract_tool_result_text(block: &serde_json::Value) -> String {
    match block.get("content") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn extract_error_message(event: &serde_json::Value) -> &str {
    event
        .get("error")
        .and_then(|e| e.as_str())
        .or_else(|| {
            event
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
        })
        .unwrap_or("unknown error")
}

fn extract_usage(u: &serde_json::Value) -> Option<ParsedStreamEvent> {
    let input_tokens = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_creation = u
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = u
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let prompt_tokens = input_tokens + cache_creation + cache_read;
    if prompt_tokens > 0 || completion_tokens > 0 {
        Some(ParsedStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            cost: None,
        })
    } else {
        None
    }
}

impl CliOutputParser for ClaudeCliParser {
    fn on_line(&mut self, line: &str) -> anyhow::Result<StreamEvents> {
        match self.interface {
            CliProfileInterface::Text => Ok(smallvec![ParsedStreamEvent::AssistantText {
                text: format!("{line}\n"),
            }]),
            CliProfileInterface::Json => {
                self.json_lines.push(line.to_string());
                Ok(smallvec![])
            }
            CliProfileInterface::StreamJson => Ok(self.parse_stream_json_line(line)),
        }
    }

    fn finish(&mut self) -> anyhow::Result<StreamEvents> {
        match self.interface {
            CliProfileInterface::Json => {
                let combined = self.json_lines.join("\n");
                Ok(self.parse_json_document(&combined))
            }
            _ => Ok(smallvec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::stream::StreamReducer;
    use consult_llm_core::monitoring::RunSpool;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn make_parser(interface: CliProfileInterface) -> ClaudeCliParser {
        ClaudeCliParser::new(interface)
    }

    fn reducer() -> StreamReducer {
        StreamReducer::new(Arc::new(Mutex::new(RunSpool::disabled())), None, None)
    }

    // --- Text interface ---

    #[test]
    fn test_text_interface_each_line_is_assistant_text() {
        let mut p = make_parser(CliProfileInterface::Text);
        let ev = p.on_line("hello").unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "hello\n"));
    }

    #[test]
    fn test_text_interface_blank_lines_preserved() {
        let mut p = make_parser(CliProfileInterface::Text);
        let ev = p.on_line("").unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "\n"));
    }

    #[test]
    fn test_text_interface_multiple_lines() {
        let mut p = make_parser(CliProfileInterface::Text);
        let ev1 = p.on_line("line1").unwrap();
        let ev2 = p.on_line("line2").unwrap();
        assert!(matches!(&ev1[0], ParsedStreamEvent::AssistantText { text } if text == "line1\n"));
        assert!(matches!(&ev2[0], ParsedStreamEvent::AssistantText { text } if text == "line2\n"));
    }

    // --- StreamJson interface ---

    #[test]
    fn test_stream_json_assistant_text() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "Hello"));
    }

    #[test]
    fn test_stream_json_assistant_tool_use_content() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Answer: "},{"type":"tool_use","id":"toolu_1","name":"bash","input":{"cmd":"ls"}}]}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 2);
        assert!(matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "Answer: "));
        assert!(
            matches!(&ev[1], ParsedStreamEvent::ToolStarted { call_id, label } if call_id == "toolu_1" && label == "bash ls")
        );
    }

    #[test]
    fn test_stream_json_assistant_with_empty_content() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[]}}"#;
        let ev = p.on_line(line).unwrap();
        assert!(ev.is_empty());
    }

    #[test]
    fn test_stream_json_thinking_content() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"checking"}]}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::Thinking { text } if text == "checking"));
    }

    #[test]
    fn test_stream_json_system_init_session() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"system","subtype":"init","session_id":"sess_123"}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::SessionStarted { id } if id == "sess_123"));
    }

    #[test]
    fn test_stream_json_result_fallback() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"result","result":"Final answer","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 2);
        assert!(
            matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "Final answer")
        );
        assert!(matches!(
            &ev[1],
            ParsedStreamEvent::Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                ..
            }
        ));
    }

    #[test]
    fn test_stream_json_result_not_used_when_assistant_text_seen() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let assistant = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        let result = r#"{"type":"result","result":"Ignored","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let _ = p.on_line(assistant).unwrap();
        let ev = p.on_line(result).unwrap();
        // Usage emitted but no assistant text from result fallback
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::Usage { .. }));
    }

    #[test]
    fn test_stream_json_error_event() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"error","error":{"message":"Rate limit exceeded"}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(
            matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text.contains("Rate limit exceeded"))
        );
    }

    #[test]
    fn test_stream_json_error_string() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"error","error":"Internal error"}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(
            matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "Error: Internal error")
        );
    }

    #[test]
    fn test_stream_json_usage_with_cache_tokens() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let line = r#"{"type":"result","result":"Done","usage":{"input_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5,"output_tokens":30}}"#;
        let ev = p.on_line(line).unwrap();
        assert_eq!(ev.len(), 2);
        // prompt = 50 + 10 + 5 = 65
        assert!(matches!(
            &ev[1],
            ParsedStreamEvent::Usage {
                prompt_tokens: 65,
                completion_tokens: 30,
                ..
            }
        ));
    }

    #[test]
    fn test_stream_json_invalid_line_skipped() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let ev = p.on_line("not json").unwrap();
        assert!(ev.is_empty());
    }

    #[test]
    fn test_stream_json_empty_line_skipped() {
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let ev = p.on_line("").unwrap();
        assert!(ev.is_empty());
    }

    // --- Json interface ---

    #[test]
    fn test_json_interface_buffers_then_finish_parses() {
        let mut p = make_parser(CliProfileInterface::Json);
        let ev = p.on_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#);
        assert!(ev.unwrap().is_empty());
        let ev = p.finish().unwrap();
        assert_eq!(ev.len(), 1);
        assert!(matches!(&ev[0], ParsedStreamEvent::AssistantText { text } if text == "Hello"));
    }

    #[test]
    fn test_json_interface_finish_empty() {
        let mut p = make_parser(CliProfileInterface::Json);
        let ev = p.finish().unwrap();
        assert!(ev.is_empty());
    }

    // --- StreamReducer integration ---

    #[test]
    fn test_reducer_with_stream_json_sequence() {
        let mut r = reducer();
        let mut p = make_parser(CliProfileInterface::StreamJson);
        let lines = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#,
            r#"{"type":"result","usage":{"input_tokens":50,"output_tokens":10}}"#,
        ];
        for line in &lines {
            let events = p.on_line(line).unwrap();
            r.process(events);
        }
        assert_eq!(r.response, "Hello");
        let u = r.usage.expect("usage captured");
        assert_eq!(u.prompt_tokens, 50);
        assert_eq!(u.completion_tokens, 10);
    }

    #[test]
    fn test_reducer_with_text_sequence() {
        let mut r = reducer();
        let mut p = make_parser(CliProfileInterface::Text);
        for line in &["Hello", "World"] {
            let events = p.on_line(line).unwrap();
            r.process(events);
        }
        assert_eq!(r.response, "Hello\nWorld\n");
    }

    #[test]
    fn test_reducer_with_json_interface() {
        let mut r = reducer();
        let mut p = make_parser(CliProfileInterface::Json);
        p.on_line(r#"{"type":"result","result":"Final answer","usage":{"input_tokens":50,"output_tokens":10}}"#)
            .unwrap();
        let events = p.finish().unwrap();
        r.process(events);
        assert_eq!(r.response, "Final answer");
        let u = r.usage.expect("usage captured");
        assert_eq!(u.prompt_tokens, 50);
        assert_eq!(u.completion_tokens, 10);
    }

    // --- Executor tests ---

    fn request(prompt: &str, system_prompt: &str) -> ExecutionRequest {
        request_with_files(prompt, system_prompt, None)
    }

    fn request_with_files(
        prompt: &str,
        system_prompt: &str,
        file_paths: Option<Vec<PathBuf>>,
    ) -> ExecutionRequest {
        ExecutionRequest {
            prompt: prompt.to_string(),
            model: "claude-opus-4-7".to_string(),
            system_prompt: system_prompt.to_string(),
            file_paths,
            thread_id: None,
            spool: Arc::new(Mutex::new(RunSpool::disabled())),
        }
    }

    fn make_stdin_profile(command: &str, args: Vec<String>) -> ClaudeCliConfig {
        make_config(
            command,
            args,
            std::collections::BTreeMap::new(),
            CliPromptMode::Stdin,
            None,
        )
    }

    fn make_config(
        command: &str,
        args: Vec<String>,
        env: std::collections::BTreeMap<String, String>,
        prompt: CliPromptMode,
        model_env: Option<String>,
    ) -> ClaudeCliConfig {
        ClaudeCliConfig {
            command: command.to_string(),
            args,
            env,
            interface: CliProfileInterface::Text,
            prompt,
            effort: None,
            model_env,
        }
    }

    #[test]
    fn executor_delivers_prompt_via_stdin() {
        let profile = make_stdin_profile("sh", vec!["-c".to_string(), "cat".to_string()]);
        let executor = ClaudeCliExecutor::new(profile);
        let result = executor
            .execute(request("hello from stdin", "system: you are a test"))
            .expect("execute");
        assert!(
            result.response.contains("hello from stdin"),
            "stdin prompt should appear in output"
        );
    }

    #[test]
    fn executor_delivers_prompt_via_argument() {
        let profile = make_config(
            "bash",
            vec![
                "-c".to_string(),
                // Injected boilerplate args come before the prompt, so read the last arg.
                r#"printf '%s\n' "${@: -1}""#.to_string(),
                "bash".to_string(),
            ],
            std::collections::BTreeMap::new(),
            CliPromptMode::Argument,
            None,
        );
        let executor = ClaudeCliExecutor::new(profile);
        let result = executor
            .execute(request("hello as arg", "system: test"))
            .expect("execute");
        assert!(
            result.response.contains("hello as arg"),
            "argument prompt should appear in output"
        );
    }

    #[test]
    fn executor_uses_file_refs_in_prompt() {
        let profile = make_stdin_profile("sh", vec!["-c".to_string(), "cat".to_string()]);
        let executor = ClaudeCliExecutor::new(profile);
        let result = executor
            .execute(request_with_files(
                "read the files",
                "system: test",
                Some(vec![
                    PathBuf::from("src/lib.rs"),
                    PathBuf::from("README.md"),
                ]),
            ))
            .expect("execute");

        assert!(result.response.contains("Files: @src/lib.rs @README.md"));
    }

    #[test]
    fn executor_passes_configured_args_and_env() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("CLAUDE_CLI_TEST_VAR".to_string(), "env-value".to_string());
        let profile = make_config(
            "sh",
            vec![
                "-c".to_string(),
                "printf '%s\\n' \"$CLAUDE_CLI_TEST_VAR\"".to_string(),
            ],
            env,
            CliPromptMode::Stdin,
            None,
        );
        let executor = ClaudeCliExecutor::new(profile);
        let result = executor
            .execute(request("", "system: test"))
            .expect("execute");
        assert_eq!(result.response.trim(), "env-value");
    }

    #[test]
    fn executor_does_not_inject_bare_mode() {
        let profile = make_config(
            "sh",
            vec![
                "-c".to_string(),
                "printf '%s\\n' \"$@\"".to_string(),
                "sh".to_string(),
            ],
            std::collections::BTreeMap::new(),
            CliPromptMode::Stdin,
            None,
        );
        let executor = ClaudeCliExecutor::new(profile);
        let result = executor
            .execute(request("", "system: test"))
            .expect("execute");
        assert!(result.response.contains("--verbose"));
        assert!(!result.response.contains("--bare"));
    }

    #[test]
    fn executor_non_zero_exit_surfaces_error() {
        let profile = make_stdin_profile(
            "sh",
            vec!["-c".to_string(), "echo oops 1>&2; exit 7".to_string()],
        );
        let executor = ClaudeCliExecutor::new(profile);
        let err = executor
            .execute(request("prompt", "system: test"))
            .unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("exited with code 7"),
            "should include exit code: {msg}"
        );
        assert!(msg.contains("oops"), "should include stderr: {msg}");
    }

    #[test]
    fn executor_supports_thread_resume() {
        let profile = make_stdin_profile("sh", vec!["-c".to_string(), "echo ok".to_string()]);
        let executor = ClaudeCliExecutor::new(profile);
        let req = ExecutionRequest {
            thread_id: Some("thr_abc".to_string()),
            ..request("prompt", "system: test")
        };
        let result = executor.execute(req).expect("should support thread resume");
        assert_eq!(result.response.trim(), "ok");
    }

    #[test]
    fn executor_injects_model_env() {
        let profile = make_config(
            "sh",
            vec![
                "-c".to_string(),
                "printf '%s\\n' \"$ANTHROPIC_MODEL\"".to_string(),
            ],
            std::collections::BTreeMap::new(),
            CliPromptMode::Stdin,
            Some("ANTHROPIC_MODEL".to_string()),
        );
        let executor = ClaudeCliExecutor::new(profile);
        let req = ExecutionRequest {
            model: "gemini-3.1-pro-preview".to_string(),
            ..request("", "system: test")
        };
        let result = executor.execute(req).expect("execute");
        assert_eq!(result.response.trim(), "gemini-3.1-pro-preview");
    }

    #[test]
    fn executor_model_env_overrides_profile_env() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("ANTHROPIC_MODEL".to_string(), "stale-model".to_string());
        let profile = make_config(
            "sh",
            vec![
                "-c".to_string(),
                "printf '%s\\n' \"$ANTHROPIC_MODEL\"".to_string(),
            ],
            env,
            CliPromptMode::Stdin,
            Some("ANTHROPIC_MODEL".to_string()),
        );
        let executor = ClaudeCliExecutor::new(profile);
        let req = ExecutionRequest {
            model: "gemini-3.1-pro-preview".to_string(),
            ..request("", "system: test")
        };
        let result = executor.execute(req).expect("execute");
        assert_eq!(result.response.trim(), "gemini-3.1-pro-preview");
    }
}
