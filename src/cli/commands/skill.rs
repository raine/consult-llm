use std::io::Write;
use std::path::PathBuf;

use serde_json::json;

use crate::cli::input::CliError;
use crate::skills::{self, Agent, SKILLS};

#[derive(clap::Args, Debug)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum SkillCommand {
    /// List skills bundled with this binary
    List,
    /// Print a bundled SKILL.md without writing files
    Print { name: String },
    /// Show one bundled skill (equivalent to print)
    Show { name: String },
    /// Install one skill, or all skills when NAME is omitted
    Install(InstallArgs),
}

#[derive(clap::Args, Debug)]
pub struct InstallArgs {
    /// Skill to install; omit to install the complete catalog
    pub name: Option<String>,
    /// Agent runtime to target
    #[arg(long, value_enum, default_value = "claude")]
    pub agent: Agent,
    /// Home-like root under which agent directories are created
    #[arg(long)]
    pub target_root: Option<PathBuf>,
    /// Validate and print the complete write plan without changing files
    #[arg(long)]
    pub dry_run: bool,
    /// Replace unmanaged, locally modified, or newer managed copies
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: SkillArgs, json_output: bool) -> Result<(), CliError> {
    match args.command {
        SkillCommand::List => list(json_output),
        SkillCommand::Print { name } | SkillCommand::Show { name } => print(&name, json_output),
        SkillCommand::Install(args) => install(args, json_output),
    }
}

fn list(json_output: bool) -> Result<(), CliError> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": skills::CLI_SCHEMA_VERSION,
                "cli_version": skills::CLI_VERSION,
                "skills": SKILLS,
            }))
            .expect("skill list JSON is serializable")
        );
    } else {
        for skill in SKILLS {
            println!("{}\t{}", skill.name, skill.description);
        }
    }
    Ok(())
}

fn print(name: &str, json_output: bool) -> Result<(), CliError> {
    let skill = skills::find(name)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": skills::CLI_SCHEMA_VERSION,
                "name": skill.name,
                "cli_version": skill.cli_version,
                "schema_version_skill": skill.schema_version,
                "content": skill.content,
                "path_in_repo": skill.path_in_repo,
            }))
            .expect("skill print JSON is serializable")
        );
    } else {
        std::io::stdout()
            .lock()
            .write_all(skill.content.as_bytes())
            .map_err(|error| {
                CliError::system(
                    "stdout_write_failed",
                    format!("failed to write skill to stdout: {error}"),
                )
            })?;
    }
    Ok(())
}

fn install(args: InstallArgs, json_output: bool) -> Result<(), CliError> {
    let target_root = match args.target_root {
        Some(root) => root,
        None => dirs::home_dir().ok_or_else(|| {
            CliError::system(
                "home_directory_unresolved",
                "cannot determine the home directory; pass --target-root <PATH>",
            )
        })?,
    };
    let names = match args.name.as_deref() {
        Some(name) => {
            skills::find(name)?;
            vec![name]
        }
        None => SKILLS.iter().map(|skill| skill.name).collect(),
    };
    let plans = skills::plan_install(&names, args.agent, &target_root, args.force)?;
    if !args.dry_run {
        skills::apply_install(&plans)?;
    }

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": skills::CLI_SCHEMA_VERSION,
                "dry_run": args.dry_run,
                "target_root": target_root,
                "installed": !args.dry_run,
                "would": plans,
            }))
            .expect("skill install JSON is serializable")
        );
    } else {
        for plan in &plans {
            let prefix = if args.dry_run { "would " } else { "" };
            println!("{prefix}{:?}\t{}", plan.action, plan.path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_does_not_write() {
        let temp = tempfile::tempdir().unwrap();
        install(
            InstallArgs {
                name: Some("llm-consult".into()),
                agent: Agent::Pi,
                target_root: Some(temp.path().to_path_buf()),
                dry_run: true,
                force: false,
            },
            false,
        )
        .unwrap();
        assert!(!temp.path().join(".pi").exists());
    }

    #[test]
    fn unknown_name_is_domain_error() {
        assert_eq!(
            skills::find("not-a-skill").unwrap_err().code(),
            "skill_not_found"
        );
    }
}
