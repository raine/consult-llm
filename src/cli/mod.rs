use clap::{Parser, Subcommand};

pub mod commands;
pub mod input;
pub mod output;
pub mod run;
pub mod run_spec;

#[cfg(test)]
mod tests;

#[derive(Parser, Debug)]
#[command(name = "consult-llm", version, about = "Consult an external LLM")]
pub struct Cli {
    /// Emit schema-versioned JSON for commands that return structured data
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub cmd: Option<Command>,

    /// Model selector (repeatable: -m gemini -m openai). Use a provider family name or exact model ID.
    #[arg(short = 'm', long = "model")]
    pub model: Vec<String>,

    /// File context (repeatable)
    #[arg(short = 'f', long = "file")]
    pub files: Vec<String>,

    /// Resume a multi-turn conversation
    #[arg(short = 't', long = "thread-id")]
    pub thread_id: Option<String>,

    /// Task persona: general (default), review, debug, plan, create
    #[arg(long = "task", default_value = "general")]
    pub task: TaskArg,

    /// Copy formatted prompt to clipboard, exit 0
    #[arg(long = "web")]
    pub web: bool,

    /// Read prompt from this file (alternative to stdin)
    #[arg(long = "prompt-file")]
    pub prompt_file: Option<String>,

    /// Include git diff for these files (repeatable)
    #[arg(long = "diff-files")]
    pub diff_files: Vec<String>,

    /// Base ref for git diff (default HEAD)
    #[arg(long = "diff-base")]
    pub diff_base: Option<String>,

    /// Repo path for git diff (default cwd)
    #[arg(long = "diff-repo")]
    pub diff_repo: Option<String>,

    /// Per-model run spec (repeatable). Format: model=<selector>,thread=<id>,prompt-file=<path>
    /// Cannot be combined with -m, -t, --prompt-file, or --web.
    /// Note: prompt-file paths containing commas are not supported.
    #[arg(long = "run")]
    pub runs: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List available models and resolved selectors
    Models,
    /// Self-update the binary
    Update,
    /// Diagnose backend auth, paths, and env vars
    Doctor {
        /// Show all config keys including unset defaults
        #[arg(long = "verbose")]
        verbose: bool,
    },
    /// Scaffold ~/.config/consult-llm/SYSTEM_PROMPT.md
    InitPrompt,
    /// Scaffold ~/.config/consult-llm/config.yaml
    InitConfig,
    /// Set a config value
    Config(commands::config::ConfigArgs),
    /// List, print, or install bundled agent skills
    Skill(commands::skill::SkillArgs),
    /// Show CLI, schema, build, and bundled-skill versions
    Version,
    /// Install all bundled skills (legacy alias for `skill install --agent all`)
    #[command(hide = true)]
    InstallSkills(commands::install_skills::InstallSkillsArgs),
    /// Show bundled documentation
    Docs,
    /// Internal: background update check (self-spawned by update.rs).
    #[command(hide = true)]
    CheckUpdate,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum TaskArg {
    General,
    Review,
    Debug,
    Plan,
    Create,
}

impl From<TaskArg> for crate::schema::TaskMode {
    fn from(t: TaskArg) -> Self {
        match t {
            TaskArg::General => Self::General,
            TaskArg::Review => Self::Review,
            TaskArg::Debug => Self::Debug,
            TaskArg::Plan => Self::Plan,
            TaskArg::Create => Self::Create,
        }
    }
}
