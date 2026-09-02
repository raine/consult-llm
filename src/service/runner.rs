use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::executors::types::{ExecutionRequest, LlmExecutor, LlmExecutorCapabilities, Usage};
use crate::llm_query::query_llm;
use crate::logger::{log_prompt, log_response};
use crate::schema::TaskMode;
use crate::system_prompt::get_system_prompt;
use consult_llm_core::monitoring::RunSpool;

use super::SharedInputs;

pub struct SingleResult {
    pub model: String,
    pub body: String,
    pub usage: Option<Usage>,
    pub thread_id: Option<String>,
    pub entry_index: Option<usize>,
    pub failed: bool,
}

fn build_model_prompt(
    prompt: &str,
    shared: &SharedInputs,
    capabilities: &LlmExecutorCapabilities,
) -> String {
    if capabilities.is_cli
        && let Some(args) = &shared.git_diff_args
    {
        crate::prompt_builder::build_git_diff_inspection_prompt(
            prompt,
            args.repo_path.as_deref(),
            &args.base_ref,
            &args.files,
        )
    } else if capabilities.supports_file_refs {
        crate::prompt_builder::build_prompt(prompt, &[], shared.git_diff.as_deref())
    } else {
        crate::prompt_builder::build_prompt(
            prompt,
            &shared.context_files,
            shared.git_diff.as_deref(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_single_model(
    config: &Config,
    shared: &SharedInputs,
    model: String,
    executor: Arc<dyn LlmExecutor>,
    thread_id: Option<String>,
    entry_index: Option<usize>,
    prompt: String,
    task_mode: TaskMode,
) -> anyhow::Result<SingleResult> {
    if thread_id.is_some() && !executor.capabilities().supports_threads {
        anyhow::bail!("thread_id is not supported by the configured backend for model: {model}");
    }

    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let backend_name = executor.backend_name().to_string();

    let task_mode_str = match task_mode {
        crate::schema::TaskMode::General => None,
        other => Some(format!("{other:?}").to_lowercase()),
    };
    let reasoning_effort = executor.reasoning_effort(&model).map(|s| s.to_string());

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let project = std::env::current_dir()
        .ok()
        .as_deref()
        .map(crate::git_worktree::resolve_project_name)
        .unwrap_or_else(|| "unknown".to_string());

    let meta = consult_llm_core::monitoring::RunMeta {
        v: 1,
        run_id: run_id.clone(),
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        project: project.clone(),
        cwd,
        model: model.clone(),
        backend: backend_name.clone(),
        thread_id: thread_id.clone(),
        task_mode: task_mode_str.clone(),
        reasoning_effort: reasoning_effort.clone(),
    };
    let spool = Arc::new(Mutex::new(RunSpool::new(meta)));

    let capabilities = executor.capabilities();
    let final_prompt = build_model_prompt(&prompt, shared, capabilities);
    let file_paths = if capabilities.supports_file_refs {
        shared.abs_file_paths.clone()
    } else {
        None
    };

    log_prompt(&model, &final_prompt);

    if !shared.raw_files.is_empty() {
        spool.lock().unwrap().stream_event(
            consult_llm_core::stream_events::ParsedStreamEvent::FilesContext {
                files: shared.raw_files.clone(),
            },
        );
    }

    let system_prompt = get_system_prompt(config, executor.capabilities().is_cli, task_mode);

    let start = std::time::Instant::now();
    let run = query_llm(
        ExecutionRequest {
            prompt: final_prompt,
            model: model.clone(),
            system_prompt,
            file_paths,
            thread_id: thread_id.clone(),
            spool: Arc::clone(&spool),
        },
        &executor,
    );
    let duration_ms = start.elapsed().as_millis() as u64;

    let (success, error, usage, body, result_thread_id) = match &run {
        Ok(r) => {
            log_response(&model, &r.response, &r.cost_info);
            (
                true,
                None,
                r.usage.clone(),
                r.response.clone(),
                r.thread_id.clone().or_else(|| thread_id.clone()),
            )
        }
        Err(e) => (false, Some(e.to_string()), None, String::new(), None),
    };

    let history = consult_llm_core::monitoring::HistoryRecord {
        ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        run_id: Some(run_id),
        project,
        model: model.clone(),
        backend: backend_name,
        duration_ms,
        success,
        error: error.clone(),
        tokens_in: usage.as_ref().map(|u| u.prompt_tokens),
        tokens_out: usage.as_ref().map(|u| u.completion_tokens),
        cost: usage.as_ref().and_then(|u| u.cost),
        parsed_ts: None,
        thread_id: result_thread_id.clone(),
        reasoning_effort,
        task_mode: task_mode_str,
    };
    spool
        .lock()
        .unwrap()
        .finish(duration_ms, success, error, &history);

    run.map(|_| SingleResult {
        model,
        body,
        usage,
        thread_id: result_thread_id,
        entry_index,
        failed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::GitDiffArgs;

    fn shared_diff(repo_path: Option<&str>) -> SharedInputs {
        SharedInputs {
            context_files: vec![],
            abs_file_paths: None,
            git_diff: Some("diff --git a/src/lib.rs b/src/lib.rs\n+change".into()),
            git_diff_args: Some(GitDiffArgs {
                repo_path: repo_path.map(str::to_string),
                files: vec!["src/lib.rs".into()],
                base_ref: "origin/main".into(),
            }),
            raw_files: vec![],
        }
    }

    #[test]
    fn cli_inspects_diff_while_api_embeds_it() {
        let cli = LlmExecutorCapabilities {
            is_cli: true,
            supports_threads: true,
            supports_file_refs: true,
        };
        let api = LlmExecutorCapabilities {
            is_cli: false,
            supports_threads: true,
            supports_file_refs: false,
        };

        let cli_prompt = build_model_prompt("review", &shared_diff(None), &cli);
        assert!(cli_prompt.contains("Inspect the requested git diff"));
        assert!(!cli_prompt.contains("diff --git"));

        let api_prompt = build_model_prompt("review", &shared_diff(None), &api);
        assert!(api_prompt.contains("diff --git"));

        let external_repo_prompt =
            build_model_prompt("review", &shared_diff(Some("/tmp/repo")), &cli);
        assert!(external_repo_prompt.contains(r#""repo_path":"/tmp/repo""#));
        assert!(!external_repo_prompt.contains("diff --git"));
    }
}
