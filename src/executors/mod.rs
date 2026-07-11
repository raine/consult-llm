pub mod anthropic_api;
pub mod anthropic_events;
pub mod api;
pub mod api_chat;
pub mod api_common;
pub mod api_transport;
pub mod child_guard;
pub mod claude_cli;
pub mod cli_runner;
pub mod codex_cli;
pub mod cursor_cli;
pub mod cursor_models;
pub mod gemini_cli;
pub mod grok_cli;
pub mod opencode_cli;
mod opencode_db;
pub mod sse;
pub mod stream;
pub mod tag_splitter;
pub mod thread_store;
pub mod types;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cli_runner::run_cli_streaming_with_env;
use consult_llm_core::monitoring::{ProgressStage, RunSpool};
use stream::{StreamEvents, StreamReducer};
use types::ExecuteResult;

/// Parser trait for CLI output. Implementations produce stream events from
/// each stdout line and can emit additional events after the process exits
/// via `finish` (e.g. to parse a complete JSON buffer).
pub trait CliOutputParser {
    fn on_line(&mut self, line: &str) -> anyhow::Result<StreamEvents>;
    fn finish(&mut self) -> anyhow::Result<StreamEvents> {
        Ok(smallvec::smallvec![])
    }
}

impl<F> CliOutputParser for F
where
    F: FnMut(&str) -> StreamEvents,
{
    fn on_line(&mut self, line: &str) -> anyhow::Result<StreamEvents> {
        Ok(self(line))
    }
}

use crate::external_dirs::get_external_directories;
use crate::git_worktree::get_main_worktree_path;

/// Format file paths as relative `@path` references appended to the prompt.
/// Used by Codex and Gemini CLI executors.
pub fn append_file_refs(text: &str, file_paths: Option<&[PathBuf]>) -> String {
    match file_paths {
        Some(fps) if !fps.is_empty() => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let file_refs: Vec<String> = fps
                .iter()
                .map(|p| {
                    let rel = pathdiff::diff_paths(p, &cwd).unwrap_or_else(|| p.clone());
                    format!("@{}", rel.display())
                })
                .collect();
            format!("{text}\n\nFiles: {}", file_refs.join(" "))
        }
        _ => text.to_string(),
    }
}

/// Prepared stdin prompt and original request fields for CLI executors.
pub struct PreparedCliRequest {
    pub model: String,
    pub prompt: String,
    pub system_prompt: String,
    pub stdin_prompt: String,
    pub file_paths: Option<Vec<PathBuf>>,
    pub thread_id: Option<String>,
    pub spool: Arc<Mutex<RunSpool>>,
}

/// Build the stdin prompt from an execution request.
///
/// Appends file context via `append_file_context`, then prepends the system
/// prompt only when there is no thread id (resume sessions keep prior context).
pub fn prepare_cli_request(
    req: types::ExecutionRequest,
    append_file_context: fn(&str, Option<&[PathBuf]>) -> String,
) -> PreparedCliRequest {
    let types::ExecutionRequest {
        prompt,
        model,
        system_prompt,
        file_paths,
        thread_id,
        spool,
    } = req;
    let fps = file_paths.as_deref();
    let tid = thread_id.as_deref();

    let message = append_file_context(&prompt, fps);
    let stdin_prompt = if tid.is_some() {
        message
    } else {
        format!("{system_prompt}\n\n{message}")
    };

    PreparedCliRequest {
        model,
        prompt,
        system_prompt,
        stdin_prompt,
        file_paths,
        thread_id,
        spool,
    }
}

/// Build CLI args for extra directories (worktree + external file paths).
pub fn build_extra_dir_args(file_paths: Option<&[PathBuf]>, flag: &str) -> Vec<String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut args = Vec::new();
    if let Some(wt) = get_main_worktree_path() {
        args.push(flag.to_string());
        args.push(wt.to_string());
    }
    let resolved: Option<Vec<PathBuf>> = file_paths.map(|fps| fps.to_vec());
    for dir in get_external_directories(resolved.as_deref(), &cwd) {
        args.push(flag.to_string());
        args.push(dir);
    }
    args
}

/// Run a CLI tool with streaming, profile-backed env, and a parser trait.
/// Processes `parser.finish()` after the child exits. The prompt is delivered
/// via stdin when `stdin_prompt` is `Some`, or as part of `args` when `None`.
#[allow(clippy::too_many_arguments)]
pub fn run_cli_executor_with_env(
    command: &str,
    args: &[String],
    extra_env: Option<&std::collections::BTreeMap<String, String>>,
    stdin_prompt: Option<&str>,
    prompt: &str,
    system_prompt: &str,
    spool: Arc<Mutex<RunSpool>>,
    parser: &mut (impl CliOutputParser + Send),
) -> anyhow::Result<ExecuteResult> {
    let mut reducer = StreamReducer::new(Arc::clone(&spool), Some(prompt), Some(system_prompt));
    let spawn_spool = Arc::clone(&spool);
    let on_spawn: Option<Box<dyn FnOnce(u32) + Send>> = Some(Box::new(move |pid| {
        if let Ok(mut s) = spawn_spool.lock() {
            s.set_stage(ProgressStage::CliSpawned { pid });
        }
    }));
    let mut parser_error: Option<anyhow::Error> = None;
    let result =
        run_cli_streaming_with_env(command, args, extra_env, stdin_prompt, on_spawn, |line| {
            if parser_error.is_some() {
                return;
            }
            match parser.on_line(line) {
                Ok(events) => reducer.process(events),
                Err(err) => parser_error = Some(err),
            }
        })?;
    if let Some(err) = parser_error {
        return Err(err);
    }

    // Allow the parser to emit final events (e.g. buffered JSON deserialization).
    let events = parser.finish()?;
    reducer.process(events);

    if result.code == Some(0) {
        let response = reducer.response.trim_end().to_string();
        if response.is_empty() {
            anyhow::bail!("No response found in {command} stream output");
        }
        Ok(ExecuteResult {
            response,
            usage: reducer.usage,
            thread_id: reducer.thread_id,
        })
    } else {
        anyhow::bail!(
            "{command} exited with code {}. Error: {}",
            result.code.unwrap_or(-1),
            result.stderr.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use consult_llm_core::monitoring::RunSpool;

    use super::*;
    use types::ExecutionRequest;

    fn request(file_paths: Option<Vec<PathBuf>>, thread_id: Option<&str>) -> ExecutionRequest {
        ExecutionRequest {
            prompt: "explain this".to_string(),
            model: "test-model".to_string(),
            system_prompt: "use concise answers".to_string(),
            file_paths,
            thread_id: thread_id.map(str::to_string),
            spool: Arc::new(Mutex::new(RunSpool::disabled())),
        }
    }

    fn custom_file_context(text: &str, file_paths: Option<&[PathBuf]>) -> String {
        let files = file_paths
            .unwrap_or_default()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("{text}\nfiles={files}")
    }

    #[test]
    fn prepare_cli_request_prepends_system_prompt_for_new_threads() {
        let prepared = prepare_cli_request(request(None, None), custom_file_context);

        assert_eq!(
            prepared.stdin_prompt,
            "use concise answers\n\nexplain this\nfiles="
        );
        assert_eq!(prepared.thread_id, None);
    }

    #[test]
    fn prepare_cli_request_omits_system_prompt_for_resumed_threads() {
        let prepared = prepare_cli_request(request(None, Some("thread-1")), custom_file_context);

        assert_eq!(prepared.stdin_prompt, "explain this\nfiles=");
        assert_eq!(prepared.thread_id, Some("thread-1".to_string()));
    }

    #[test]
    fn prepare_cli_request_passes_file_paths_to_formatter() {
        let file_paths = vec![PathBuf::from("src/lib.rs"), PathBuf::from("README.md")];
        let prepared =
            prepare_cli_request(request(Some(file_paths.clone()), None), custom_file_context);

        assert_eq!(
            prepared.stdin_prompt,
            "use concise answers\n\nexplain this\nfiles=src/lib.rs,README.md"
        );
        assert_eq!(prepared.file_paths, Some(file_paths));
    }
}
