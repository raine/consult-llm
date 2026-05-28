//! Antigravity CLI (`agy`) executor — talks to Google's Antigravity IDE agent.
//!
//! agy v1.0.3 has **no public CLI flag for model selection**: the active model
//! is whatever was last picked in the Antigravity IDE (persisted in
//! `state.vscdb` as a protobuf blob). So this executor exposes a single `agy`
//! model and assumes the user has set the right one in the IDE.
//!
//! Other limitations: agy only emits plain text on stdout (no JSON streaming,
//! no tool/usage events, no session ID). The reducer is fed a single
//! AssistantText per stdout line; tool narration ("I will list...") is passed
//! through verbatim. Threads are limited to `--continue` (most-recent only)
//! because agy never prints conversation IDs.

use smallvec::smallvec;

use super::stream::{ParsedStreamEvent, StreamEvents};
use super::types::{ExecuteResult, ExecutionRequest, LlmExecutor, LlmExecutorCapabilities};
use super::{append_file_refs, build_extra_dir_args, run_cli_executor};

pub struct AntigravityCliExecutor {
    capabilities: LlmExecutorCapabilities,
    extra_args: Vec<String>,
}

impl AntigravityCliExecutor {
    pub fn new(extra_args: Vec<String>) -> Self {
        Self {
            capabilities: LlmExecutorCapabilities {
                is_cli: true,
                // `--continue` resumes the most-recent conversation, but agy
                // doesn't expose IDs so we can't pin a specific thread.
                supports_threads: false,
                supports_file_refs: true,
            },
            extra_args,
        }
    }
}

/// Each stdout line from agy is plain text. Pass it through as one
/// AssistantText event so the reducer accumulates the full response.
pub fn parse_agy_line(line: &str) -> StreamEvents {
    if line.is_empty() {
        return smallvec![];
    }
    smallvec![ParsedStreamEvent::AssistantText {
        text: format!("{line}\n"),
    }]
}

impl LlmExecutor for AntigravityCliExecutor {
    fn capabilities(&self) -> &LlmExecutorCapabilities {
        &self.capabilities
    }

    fn backend_name(&self) -> &'static str {
        "antigravity_cli"
    }

    fn execute(&self, req: ExecutionRequest) -> anyhow::Result<ExecuteResult> {
        let ExecutionRequest {
            prompt,
            model: _,
            system_prompt,
            file_paths,
            thread_id,
            spool,
        } = req;

        let fps = file_paths.as_deref();
        let message_with_files = append_file_refs(&prompt, fps);

        // No persistent thread store on the consult side: `--continue` only
        // resumes the most-recent conversation. When a thread_id was passed
        // in we still ask agy to continue, but we can't pin a specific id.
        let resume = thread_id.is_some();

        let message = if resume {
            message_with_files
        } else {
            format!("{system_prompt}\n\n{message_with_files}")
        };

        let mut args: Vec<String> = vec![
            "--print".to_string(),
            message,
            "--dangerously-skip-permissions".to_string(),
        ];

        args.extend(build_extra_dir_args(fps, "--add-dir"));

        if resume {
            args.push("--continue".to_string());
        }

        args.extend(self.extra_args.iter().cloned());

        run_cli_executor(
            "agy",
            &args,
            "",
            &prompt,
            &system_prompt,
            spool,
            parse_agy_line,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agy_line_passes_text_through() {
        let events = parse_agy_line("Hello world");
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ParsedStreamEvent::AssistantText { text } if text == "Hello world\n")
        );
    }

    #[test]
    fn parse_agy_line_empty_is_noop() {
        assert!(parse_agy_line("").is_empty());
    }
}
