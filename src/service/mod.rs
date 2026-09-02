use std::path::PathBuf;
use std::sync::Arc;

use crate::catalog::ModelRegistry;
use crate::config::Config;
use crate::executors::types::{LlmExecutor, Usage};
use crate::file::process_files;
use crate::git::generate_git_diff;
use crate::group_thread_store::{self, StoredGroup, is_group_id};
use crate::llm::ExecutorProvider;
use crate::prompt_builder::build_prompt;
use crate::schema::{ConsultLlmArgs, GitDiffArgs, TaskMode};
use crate::system_prompt::get_system_prompt;

mod group;
pub mod plan;
pub mod runner;

use plan::{OutputShape, normalize_models, plan_resume};
use runner::SingleResult;

fn resolve_git_diff(git_diff: Option<&GitDiffArgs>) -> Option<String> {
    let gd = git_diff?;
    match generate_git_diff(gd.repo_path.as_deref(), &gd.files, &gd.base_ref) {
        Ok(diff) => Some(diff),
        Err(e) => {
            eprintln!("Warning: git diff failed: {e}");
            None
        }
    }
}

pub enum ConsultOutcome {
    Response {
        body: String,
        #[allow(dead_code)]
        usage: Option<Usage>,
        model: String,
        thread_id: Option<String>,
    },
    GroupResponse {
        body: String,
        #[allow(dead_code)]
        usage: Option<Usage>,
    },
    WebPrompt {
        clipboard_text: String,
        file_count: usize,
    },
}

#[derive(Clone)]
pub struct ConsultJob {
    pub model: String,
    pub prompt: String,
    pub thread_id: Option<String>,
    pub entry_index: Option<usize>,
}

/// A fully-resolved set of model jobs together with the output shape that
/// determines how their results are rendered. Both `consult` and
/// `consult_jobs` build a `RunPlan` and hand it to `execute`.
pub struct RunPlan {
    pub jobs: Vec<ConsultJob>,
    pub output: OutputShape,
}

pub struct ConsultService {
    config: Arc<Config>,
    registry: Arc<ModelRegistry>,
    executor_provider: Arc<ExecutorProvider>,
}

/// Pre-built inputs shared across all parallel model runs.
pub struct SharedInputs {
    /// Parsed file contents for building API-mode prompts.
    pub context_files: Vec<(String, String)>,
    /// Absolute file paths for CLI-mode executors.
    pub abs_file_paths: Option<Vec<PathBuf>>,
    pub git_diff: Option<String>,
    pub git_diff_args: Option<GitDiffArgs>,
    pub raw_files: Vec<String>,
}

fn build_shared_inputs(args: &ConsultLlmArgs) -> anyhow::Result<SharedInputs> {
    build_shared_inputs_from_files(
        args.files.as_deref().unwrap_or_default(),
        args.git_diff.as_ref(),
    )
}

fn build_shared_inputs_from_files(
    files: &[String],
    git_diff_args: Option<&GitDiffArgs>,
) -> anyhow::Result<SharedInputs> {
    let git_diff = resolve_git_diff(git_diff_args);

    let context_files = if !files.is_empty() {
        process_files(files)?
    } else {
        vec![]
    };

    let abs_file_paths = if !files.is_empty() {
        let cwd = std::env::current_dir()?;
        Some(files.iter().map(|f| cwd.join(f)).collect())
    } else {
        None
    };

    Ok(SharedInputs {
        context_files,
        abs_file_paths,
        git_diff,
        git_diff_args: git_diff_args.cloned(),
        raw_files: files.to_vec(),
    })
}

/// Single-model output assembly: surface the worker error directly.
fn single_outcome(outcomes: Vec<anyhow::Result<SingleResult>>) -> anyhow::Result<ConsultOutcome> {
    let r = outcomes
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("single-model run produced no outcome"))??;
    Ok(ConsultOutcome::Response {
        body: r.body,
        usage: r.usage,
        model: r.model,
        thread_id: r.thread_id,
    })
}

impl ConsultService {
    pub fn new(
        config: Arc<Config>,
        registry: Arc<ModelRegistry>,
        executor_provider: Arc<ExecutorProvider>,
    ) -> Self {
        Self {
            config,
            registry,
            executor_provider,
        }
    }

    #[cfg(test)]
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub fn consult(&self, args: ConsultLlmArgs) -> anyhow::Result<ConsultOutcome> {
        if args.web_mode {
            return self.handle_web_mode(args);
        }

        let loaded_group = match args.thread_id.as_deref() {
            Some(tid) if is_group_id(tid) => group_thread_store::load(tid)?,
            _ => None,
        };

        let models = normalize_models(&self.registry, args.model.clone(), loaded_group.as_ref())?;
        let resume = plan_resume(args.thread_id.as_deref(), &models, loaded_group)?;
        let shared = build_shared_inputs(&args)?;

        let jobs: Vec<ConsultJob> = models
            .into_iter()
            .zip(resume.threads)
            .zip(resume.matched_entry_indices)
            .map(|((model, thread_id), entry_index)| ConsultJob {
                model,
                prompt: args.prompt.clone(),
                thread_id,
                entry_index,
            })
            .collect();

        self.execute(
            RunPlan {
                jobs,
                output: resume.output,
            },
            shared,
            args.task_mode,
        )
    }

    /// Run a set of pre-built jobs with per-job prompts in parallel. Always
    /// renders as a group document — the `--run` surface never unwraps to a
    /// plain Response.
    pub fn consult_jobs(
        &self,
        jobs: Vec<ConsultJob>,
        files: &[String],
        git_diff_args: Option<&GitDiffArgs>,
        task_mode: TaskMode,
        existing_group_id: Option<String>,
    ) -> anyhow::Result<ConsultOutcome> {
        let shared = build_shared_inputs_from_files(files, git_diff_args)?;
        self.execute(
            RunPlan {
                jobs,
                output: OutputShape::Group {
                    existing_id: existing_group_id,
                },
            },
            shared,
            task_mode,
        )
    }

    fn execute(
        &self,
        plan: RunPlan,
        shared: SharedInputs,
        task_mode: TaskMode,
    ) -> anyhow::Result<ConsultOutcome> {
        let RunPlan { jobs, output } = plan;
        let outcomes = self.run_workers(&jobs, &shared, task_mode)?;

        match output {
            OutputShape::Single => single_outcome(outcomes),
            OutputShape::Group { existing_id } => {
                let results = group::collect_group_results(&jobs, outcomes)?;
                self.persist_group_and_render(existing_id, results)
            }
        }
    }

    fn run_workers(
        &self,
        jobs: &[ConsultJob],
        shared: &SharedInputs,
        task_mode: TaskMode,
    ) -> anyhow::Result<Vec<anyhow::Result<SingleResult>>> {
        let executors: Vec<Arc<dyn LlmExecutor>> = jobs
            .iter()
            .map(|j| self.executor_provider.get_executor(&j.model))
            .collect::<anyhow::Result<Vec<_>>>()?;

        // Fan out one OS thread per job. `std::thread::scope` joins them
        // deterministically and lets each thread borrow `shared`.
        Ok(std::thread::scope(|s| {
            let handles: Vec<_> = jobs
                .iter()
                .cloned()
                .zip(executors)
                .map(|(job, exec)| {
                    let config = Arc::clone(&self.config);
                    s.spawn(move || {
                        runner::run_single_model(
                            &config,
                            shared,
                            job.model,
                            exec,
                            job.thread_id,
                            job.entry_index,
                            job.prompt,
                            task_mode,
                        )
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| match h.join() {
                    Ok(r) => r,
                    Err(_) => Err(anyhow::anyhow!("worker thread panicked")),
                })
                .collect()
        }))
    }

    fn persist_group_and_render(
        &self,
        existing_group_id: Option<String>,
        results: Vec<SingleResult>,
    ) -> anyhow::Result<ConsultOutcome> {
        let group_id = existing_group_id
            .clone()
            .unwrap_or_else(group_thread_store::generate_group_id);

        // Take an exclusive lock around load → merge → save so concurrent
        // runs against the same group can't both read the old state and
        // clobber each other on write.
        group_thread_store::with_lock(&group_id, || {
            let existing_group = existing_group_id
                .as_deref()
                .map(group_thread_store::load)
                .transpose()?
                .flatten();

            // Resume case where the group file vanished between when we picked
            // it up and when we got the lock (e.g. cleanup_expired raced).
            // Saving fresh would drop every prior member; bail instead.
            if let Some(id) = existing_group_id.as_deref()
                && existing_group.is_none()
            {
                anyhow::bail!(
                    "Group '{id}' disappeared during the call (likely cleaned up); refusing to recreate with only the new turn"
                );
            }

            group_thread_store::save(&StoredGroup {
                id: group_id.clone(),
                entries: group::merge_group_entries(existing_group.as_ref(), &results)?,
            })
        })?;

        if existing_group_id.is_none() {
            // Synchronous: directory scan is fast (<1ms typical) and detached
            // threads aren't reliable to run before process exit. See plan
            // ledger entry `untestable-cleanup-background`.
            let _ = group_thread_store::cleanup_expired(7);
        }

        Ok(ConsultOutcome::GroupResponse {
            body: group::assemble_group_markdown(&group_id, &results),
            usage: None,
        })
    }

    fn handle_web_mode(&self, args: ConsultLlmArgs) -> anyhow::Result<ConsultOutcome> {
        let context_files = args
            .files
            .as_ref()
            .map(|f| process_files(f))
            .transpose()?
            .unwrap_or_default();

        let git_diff = resolve_git_diff(args.git_diff.as_ref());

        let prompt = build_prompt(&args.prompt, &context_files, git_diff.as_deref());
        let system_prompt = get_system_prompt(&self.config, false, args.task_mode);
        let clipboard_text =
            format!("# System Prompt\n\n{system_prompt}\n\n# User Prompt\n\n{prompt}");
        let file_count = context_files.len();

        Ok(ConsultOutcome::WebPrompt {
            clipboard_text,
            file_count,
        })
    }
}

#[cfg(test)]
mod tests;
