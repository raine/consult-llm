use crate::cli::commands::skill::{InstallArgs, SkillArgs, SkillCommand};
use crate::cli::input::CliError;
use crate::skills::Agent;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformArg {
    Claude,
    Opencode,
    Codex,
}

#[derive(clap::Args, Debug)]
pub struct InstallSkillsArgs {
    /// Target one legacy platform; omit to target all supported agents
    #[arg(long = "platform", value_enum)]
    pub platform: Option<PlatformArg>,
}

/// Backward-compatible adapter for the pre-3.x installer spelling.
pub fn run(args: InstallSkillsArgs, json_output: bool) -> Result<(), CliError> {
    let agent = match args.platform {
        Some(PlatformArg::Claude) => Agent::Claude,
        Some(PlatformArg::Opencode) => Agent::Opencode,
        Some(PlatformArg::Codex) => Agent::Codex,
        None => Agent::All,
    };
    super::skill::run(
        SkillArgs {
            command: SkillCommand::Install(InstallArgs {
                name: None,
                agent,
                target_root: None,
                dry_run: false,
                force: false,
            }),
        },
        json_output,
    )
}
