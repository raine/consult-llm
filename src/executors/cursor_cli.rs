use smallvec::SmallVec;
use std::path::PathBuf;

use super::cursor_models;
use super::stream::{
    ParsedStreamEvent, StreamEvents, parse_json_line, tool_label, usage_event_from_keys,
};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{prepare_cli_request, run_cli_executor_with_env};
pub struct CursorCliExecutor {
    capabilities: LlmExecutorCapabilities,
    codex_reasoning_effort: String,
    env: std::collections::BTreeMap<String, String>,
}

impl CursorCliExecutor {
    pub fn new(
        codex_reasoning_effort: String,
        env: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                supports_threads: true,
                supports_file_refs: true,
            },
            codex_reasoning_effort,
            env,
        }
    }
}

pub fn parse_cursor_line(line: &str) -> StreamEvents {
    use smallvec::smallvec;

    let Some(event) = parse_json_line(line) else {
        return smallvec![];
    };
    let event_type = event.get("type").and_then(|t| t.as_str());
    let subtype = event.get("subtype").and_then(|t| t.as_str());

    match event_type {
        Some("system") if subtype == Some("init") => {
            if let Some(sid) = event.get("session_id").and_then(|v| v.as_str()) {
                smallvec![ParsedStreamEvent::SessionStarted {
                    id: sid.to_string(),
                }]
            } else {
                smallvec![]
            }
        }
        Some("thinking") if subtype == Some("delta") => {
            let text = event.get("text").and_then(|t| t.as_str()).unwrap_or("");
            // Cursor thinking deltas sometimes contain literal "\n" sequences
            // as paragraph separators; replace with actual newlines.
            let text = text.replace("\\n", "\n");
            smallvec![ParsedStreamEvent::Thinking { text }]
        }
        Some("tool_call") => {
            let tc = event.get("tool_call");
            let call_id = event
                .get("call_id")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            match subtype {
                Some("started") => {
                    let label = tc
                        .map(extract_cursor_tool_name)
                        .unwrap_or_else(|| "tool".to_string());
                    smallvec![ParsedStreamEvent::ToolStarted { call_id, label }]
                }
                Some("completed") => {
                    let (success, error) = tc
                        .map(|tc| {
                            let ok = is_cursor_tool_success(tc);
                            let err = if ok {
                                None
                            } else {
                                extract_cursor_tool_error(tc)
                            };
                            (ok, err)
                        })
                        .unwrap_or((false, None));
                    smallvec![ParsedStreamEvent::ToolFinished {
                        call_id,
                        success,
                        error,
                    }]
                }
                _ => smallvec![],
            }
        }
        Some("assistant") => {
            // Emit Responding progress without accumulating text —
            // full response comes from the result event
            smallvec![ParsedStreamEvent::AssistantText {
                text: String::new(),
            }]
        }
        Some("result") => {
            let mut events = SmallVec::new();
            if let Some(text) = event.get("result").and_then(|v| v.as_str()) {
                events.push(ParsedStreamEvent::AssistantText {
                    text: text.to_string(),
                });
            }
            if let Some(u) = event.get("usage") {
                events.push(usage_event_from_keys(u, "inputTokens", "outputTokens"));
            }
            events
        }
        _ => smallvec![],
    }
}

fn extract_cursor_tool_name(tool_call: &serde_json::Value) -> String {
    if let Some(shell) = tool_call.get("shellToolCall") {
        if let Some(desc) = shell.get("description").and_then(|d| d.as_str()) {
            return desc.to_string();
        }
        if let Some(args) = shell.get("args") {
            if let Some(desc) = args.get("description").and_then(|d| d.as_str()) {
                return desc.to_string();
            }
            if let Some(cmds) = args.get("simpleCommands").and_then(|c| c.as_array())
                && let Some(first) = cmds.first().and_then(|c| c.as_str())
            {
                return first.to_string();
            }
        }
    }
    if let Some(read) = tool_call.get("readToolCall") {
        let path = read
            .get("args")
            .and_then(|a| a.get("path"))
            .or_else(|| read.get("path"))
            .and_then(|v| v.as_str());
        return tool_label("read", path);
    }
    if let Some(glob) = tool_call.get("globToolCall") {
        let pattern = glob
            .get("args")
            .and_then(|a| a.get("globPattern"))
            .or_else(|| glob.get("pattern"))
            .and_then(|v| v.as_str());
        return tool_label("glob", pattern);
    }
    // Unknown tool type — use the key name (e.g. "editToolCall" → "edit")
    if let Some(key) = tool_call
        .as_object()
        .and_then(|o| o.keys().find(|k| k.ends_with("ToolCall")))
    {
        return key.trim_end_matches("ToolCall").to_string();
    }
    "tool".to_string()
}

/// Find the `result` object inside a cursor tool call by looking for any
/// key ending in "ToolCall" (e.g. readToolCall, shellToolCall, editToolCall).
fn cursor_tool_result(tool_call: &serde_json::Value) -> Option<&serde_json::Value> {
    tool_call.as_object().and_then(|obj| {
        obj.iter()
            .find(|(k, _)| k.ends_with("ToolCall"))
            .and_then(|(_, v)| v.get("result"))
    })
}

fn is_cursor_tool_success(tool_call: &serde_json::Value) -> bool {
    if let Some(result) = cursor_tool_result(tool_call) {
        // cursor-agent uses result.success as either a bool (true/false)
        // or an object (containing content) for successful tool calls
        return match result.get("success") {
            Some(v) if v.is_boolean() => v.as_bool().unwrap_or(false),
            Some(v) if v.is_object() => true,
            _ => false,
        };
    }
    false
}

/// Extract error message from a failed cursor tool call.
/// Handles `result.error.errorMessage` and `result.rejected.reason`.
fn extract_cursor_tool_error(tool_call: &serde_json::Value) -> Option<String> {
    if let Some(result) = cursor_tool_result(tool_call) {
        if let Some(err) = result.get("error") {
            return err
                .get("errorMessage")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());
        }
        if let Some(rejected) = result.get("rejected") {
            let reason = rejected
                .get("reason")
                .and_then(|r| r.as_str())
                .filter(|s| !s.is_empty());
            let command = rejected
                .get("command")
                .and_then(|c| c.as_str())
                .filter(|s| !s.is_empty());
            return Some(match (reason, command) {
                (Some(r), Some(c)) => format!("rejected: {r} ({c})"),
                (Some(r), None) => format!("rejected: {r}"),
                (None, Some(c)) => format!("rejected ({c})"),
                (None, None) => "rejected".to_string(),
            });
        }
    }
    None
}

/// Map model IDs to cursor-agent model names
pub(crate) fn map_cursor_model(model: &str, codex_reasoning_effort: &str) -> String {
    let mut cursor_model = model.replace("-preview", "");

    // cursor-agent encodes reasoning effort in the model name for models
    // that require it. e.g. gpt-5.3-codex + high → gpt-5.3-codex-high,
    // gpt-5.6-sol + high → gpt-5.6-sol-high.
    // The effort suffix may not match cursor-agent's actual ladder for the
    // model (e.g. gpt-5.5 wants `extra-high`, not `xhigh`); the resolver in
    // `cursor_models` rewrites the candidate against the live model list.
    if model_requires_reasoning_suffix(&cursor_model) {
        cursor_model = format!("{cursor_model}-{codex_reasoning_effort}");
    }

    cursor_model
}

pub(crate) fn model_requires_reasoning_suffix(model: &str) -> bool {
    model.contains("-codex") || matches!(model, "gpt-5.4" | "gpt-5.5" | "gpt-5.6-sol")
}

fn append_files(text: &str, file_paths: Option<&[PathBuf]>) -> String {
    match file_paths {
        Some(fps) if !fps.is_empty() => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let file_list: Vec<String> = fps
                .iter()
                .map(|p| {
                    let rel = pathdiff::diff_paths(p, &cwd).unwrap_or_else(|| p.clone());
                    format!("- {}", rel.display())
                })
                .collect();
            format!(
                "{text}\n\nPlease read the following files for context:\n{}",
                file_list.join("\n")
            )
        }
        _ => text.to_string(),
    }
}

impl LlmExecutor for CursorCliExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "cursor_cli"
    }

    fn reasoning_effort(&self, model: &str) -> Option<&str> {
        if model_requires_reasoning_suffix(model) {
            Some(&self.codex_reasoning_effort)
        } else {
            None
        }
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let prepared = prepare_cli_request(req, append_files);
        let tid = prepared.thread_id.as_deref();

        let candidate = map_cursor_model(&prepared.model, &self.codex_reasoning_effort);
        let base = prepared.model.replace("-preview", "");
        let cursor_model = if model_requires_reasoning_suffix(&base) {
            let list = cursor_models::available_models();
            cursor_models::resolve_model(&candidate, &base, &list)?
        } else {
            candidate
        };

        // --trust is required for headless (--print) mode to skip the interactive
        // workspace trust prompt. --mode ask restricts to read-only operations.
        let mut args: Vec<String> = vec![
            "--print".to_string(),
            "--trust".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--mode".to_string(),
            "ask".to_string(),
            "--model".to_string(),
            cursor_model,
        ];
        if let Some(t) = tid {
            args.push("--resume".to_string());
            args.push(t.to_string());
        }

        let mut parser = parse_cursor_line;
        let mut result = run_cli_executor_with_env(
            "cursor-agent",
            &args,
            Some(&self.env),
            Some(&prepared.stdin_prompt),
            &prepared.prompt,
            &prepared.system_prompt,
            prepared.spool,
            &mut parser,
        )?;
        // Cursor doesn't always emit a session ID; preserve the input thread_id
        if result.thread_id.is_none() {
            result.thread_id = tid.map(|s| s.to_string());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_cursor_model_appends_effort_to_sol() {
        assert_eq!(map_cursor_model("gpt-5.6-sol", "high"), "gpt-5.6-sol-high");
    }

    #[test]
    fn test_map_cursor_model_preserves_canonical_non_effort_model() {
        assert_eq!(
            map_cursor_model("gemini-3.1-pro-preview", "high"),
            "gemini-3.1-pro"
        );
    }

    #[test]
    fn test_parse_cursor_line_init() {
        let events =
            parse_cursor_line(r#"{"type":"system","subtype":"init","session_id":"sess_456"}"#);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ParsedStreamEvent::SessionStarted { id } if id == "sess_456"));
    }

    #[test]
    fn test_parse_cursor_line_thinking() {
        let events =
            parse_cursor_line(r#"{"type":"thinking","subtype":"delta","text":"**Starting"}"#);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ParsedStreamEvent::Thinking { text } if text == "**Starting"));
    }

    #[test]
    fn test_parse_cursor_line_tool_started() {
        let events = parse_cursor_line(
            r#"{"type":"tool_call","subtype":"started","call_id":"c1","tool_call":{"readToolCall":{"path":"src/lib.rs"}}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::ToolStarted { call_id, label } if call_id == "c1" && label == "read src/lib.rs")
        );
    }

    #[test]
    fn test_parse_cursor_line_assistant() {
        let events = parse_cursor_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]},"session_id":"...","timestamp_ms":123}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ParsedStreamEvent::AssistantText { text } if text.is_empty()));
    }

    #[test]
    fn test_parse_cursor_line_result() {
        let events = parse_cursor_line(
            r#"{"type":"result","result":"Final answer","usage":{"inputTokens":500,"outputTokens":100}}"#,
        );
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], ParsedStreamEvent::AssistantText { text } if text == "Final answer")
        );
        assert!(matches!(
            &events[1],
            ParsedStreamEvent::Usage {
                prompt_tokens: 500,
                completion_tokens: 100,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_cursor_line_empty() {
        assert!(parse_cursor_line("").is_empty());
        assert!(parse_cursor_line("not json").is_empty());
    }

    #[test]
    fn test_extract_cursor_tool_name_shell() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"shellToolCall":{"args":{"simpleCommands":["ls -la"]}}}"#)
                .unwrap();
        assert_eq!(extract_cursor_tool_name(&tc), "ls -la");
    }

    #[test]
    fn test_extract_cursor_tool_name_read() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"readToolCall":{"path":"src/lib.rs"}}"#).unwrap();
        assert_eq!(extract_cursor_tool_name(&tc), "read src/lib.rs");
    }

    #[test]
    fn test_extract_cursor_tool_name_glob() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"globToolCall":{"pattern":"**/*.rs"}}"#).unwrap();
        assert_eq!(extract_cursor_tool_name(&tc), "glob **/*.rs");
    }

    #[test]
    fn test_is_cursor_tool_success_false() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"readToolCall":{"result":{"success":false}}}"#).unwrap();
        assert!(!is_cursor_tool_success(&tc));
    }

    #[test]
    fn test_is_cursor_tool_success_true() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"readToolCall":{"result":{"success":true}}}"#).unwrap();
        assert!(is_cursor_tool_success(&tc));
    }

    #[test]
    fn test_is_cursor_tool_success_missing() {
        let tc: serde_json::Value =
            serde_json::from_str(r#"{"readToolCall":{"result":{}}}"#).unwrap();
        assert!(!is_cursor_tool_success(&tc));
    }

    #[test]
    fn test_is_cursor_tool_success_object() {
        let tc: serde_json::Value = serde_json::from_str(
            r#"{"readToolCall":{"result":{"success":{"content":"hello","isEmpty":false}}}}"#,
        )
        .unwrap();
        assert!(is_cursor_tool_success(&tc));
    }
}
