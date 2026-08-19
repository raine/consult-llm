#[cfg(test)]
mod test_util;

mod catalog;
mod cli;
mod clipboard;
mod config;
mod errors;
mod executors;
mod external_dirs;
mod file;
mod git;
mod git_worktree;
mod group_thread_store;
mod llm;
mod llm_query;
mod logger;
mod models;
mod paths;
mod prompt_builder;
mod schema;
mod service;
mod skills;
mod system_prompt;
mod update;

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        logger::log_to_file(&format!("PANIC: {info}"));
        default_hook(info);
    }));

    executors::child_guard::install_signal_handler();

    consult_llm_core::path_migrate::migrate_if_needed();
    paths::migrate_to_xdg_if_needed();

    use clap::Parser;
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = match error.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => 0,
                _ => 1,
            };
            let _ = error.print();
            std::process::exit(exit_code);
        }
    };
    let json_output = cli.json;
    if !matches!(
        cli.cmd,
        Some(
            cli::Command::CheckUpdate
                | cli::Command::Update
                | cli::Command::Docs
                | cli::Command::Skill(_)
                | cli::Command::Version
                | cli::Command::InstallSkills(_)
        )
    ) {
        update::check_and_notify();
    }

    let result: Result<(), cli::input::CliError> = match cli.cmd {
        None => cli::run::run_ask(cli),
        Some(cli::Command::Models) => cli::commands::models::run().map_err(Into::into),
        Some(cli::Command::Doctor { verbose }) => {
            cli::commands::doctor::run(verbose).map_err(Into::into)
        }
        Some(cli::Command::InitPrompt) => cli::commands::init_prompt::run().map_err(Into::into),
        Some(cli::Command::InitConfig) => cli::commands::init_config::run().map_err(Into::into),
        Some(cli::Command::Config(args)) => cli::commands::config::run(args).map_err(Into::into),
        Some(cli::Command::Skill(args)) => cli::commands::skill::run(args, json_output),
        Some(cli::Command::Version) => {
            cli::commands::version::run(json_output);
            Ok(())
        }
        Some(cli::Command::InstallSkills(args)) => {
            cli::commands::install_skills::run(args, json_output)
        }
        Some(cli::Command::Update) => cli::commands::update::run().map_err(Into::into),
        Some(cli::Command::Docs) => cli::commands::docs::run().map_err(Into::into),
        Some(cli::Command::CheckUpdate) => update::run_background_check().map_err(Into::into),
    };

    if let Err(e) = result {
        if json_output {
            eprintln!(
                "{}",
                serde_json::json!({
                    "schema_version": skills::CLI_SCHEMA_VERSION,
                    "error": {
                        "code": e.code(),
                        "message": e.message(),
                    }
                })
            );
        } else {
            eprintln!("error: {}", e.message());
        }
        std::process::exit(e.exit_code());
    }
}
