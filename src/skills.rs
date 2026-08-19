use std::cmp::Ordering;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::input::CliError;

pub const CLI_SCHEMA_VERSION: u32 = 1;
pub const SKILL_SCHEMA_VERSION: u32 = 1;
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Skill {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(skip)]
    pub content: &'static str,
    pub cli_version: &'static str,
    pub schema_version: u32,
    pub path_in_repo: &'static str,
}

macro_rules! skill {
    ($name:literal, $description:literal) => {
        Skill {
            name: $name,
            description: $description,
            content: include_str!(concat!("../skills/", $name, "/SKILL.md")),
            cli_version: CLI_VERSION,
            schema_version: SKILL_SCHEMA_VERSION,
            path_in_repo: concat!("skills/", $name, "/SKILL.md"),
        }
    };
}

pub const SKILLS: &[Skill] = &[
    skill!(
        "consult-llm",
        "Canonical reference for invoking the consult-llm CLI."
    ),
    skill!(
        "llm-collab",
        "Multiple LLMs collaboratively brainstorm solutions across rounds."
    ),
    skill!(
        "llm-collab-vs",
        "Brainstorm with one partner LLM in alternating turns."
    ),
    skill!(
        "llm-consult",
        "Consult an external LLM with the user's query."
    ),
    skill!(
        "llm-debate",
        "LLMs propose and critique approaches before synthesis."
    ),
    skill!(
        "llm-debate-vs",
        "Debate one opponent LLM through a multi-turn conversation."
    ),
    skill!(
        "llm-implement",
        "Plan and implement a task with evidence-gated external review."
    ),
    skill!(
        "llm-panel",
        "Analyze a task with a role-specialized LLM panel."
    ),
    skill!(
        "llm-review",
        "Collect critical multi-model feedback on an artifact."
    ),
    skill!(
        "llm-review-panel",
        "Run a standalone multi-model review of an existing diff."
    ),
    skill!(
        "llm-skill-review",
        "Review an AI-agent skill through skill-specific lenses."
    ),
    skill!(
        "llm-workshop",
        "Facilitate interactive design with divergent LLM proposals."
    ),
];

pub fn find(name: &str) -> Result<&'static Skill, CliError> {
    if name.is_empty() || name.trim() != name {
        return Err(CliError::domain(
            "invalid_skill_name",
            format!(
                "invalid skill name {name:?}: names must be non-empty and contain no surrounding whitespace"
            ),
        ));
    }
    SKILLS
        .iter()
        .find(|skill| skill.name == name)
        .ok_or_else(|| {
            CliError::domain(
                "skill_not_found",
                format!(
                    "unknown skill {name:?}; available skills: {}",
                    SKILLS
                        .iter()
                        .map(|skill| skill.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Pi,
    Codex,
    Opencode,
    All,
}

impl Agent {
    pub fn targets(self) -> &'static [Agent] {
        match self {
            Self::All => &[Self::Claude, Self::Pi, Self::Codex, Self::Opencode],
            Self::Claude => &[Self::Claude],
            Self::Pi => &[Self::Pi],
            Self::Codex => &[Self::Codex],
            Self::Opencode => &[Self::Opencode],
        }
    }

    fn skill_root(self, target_root: &Path) -> PathBuf {
        match self {
            Self::Claude => target_root.join(".claude/skills"),
            Self::Pi => target_root.join(".pi/agent/skills"),
            Self::Codex => target_root.join(".codex/skills"),
            Self::Opencode => target_root.join(".config/opencode/skills"),
            Self::All => unreachable!("all expands before path resolution"),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallAction {
    Create,
    UpdateManaged,
    Unchanged,
    ForceOverwrite,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub action: InstallAction,
    pub agent: Agent,
    pub skill: &'static str,
    pub path: PathBuf,
}

pub fn plan_install(
    names: &[&str],
    agent: Agent,
    target_root: &Path,
    force: bool,
) -> Result<Vec<InstallPlan>, CliError> {
    if target_root.as_os_str().is_empty() {
        return Err(CliError::domain(
            "invalid_target_root",
            "--target-root must not be empty",
        ));
    }

    let mut plans = Vec::new();
    for target in agent.targets() {
        for name in names {
            let skill = find(name)?;
            let path = target
                .skill_root(target_root)
                .join(skill.name)
                .join("SKILL.md");
            let action = classify_existing(&path, skill, force)?;
            plans.push(InstallPlan {
                action,
                agent: *target,
                skill: skill.name,
                path,
            });
        }
    }
    Ok(plans)
}

fn classify_existing(path: &Path, skill: &Skill, force: bool) -> Result<InstallAction, CliError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstallAction::Create);
        }
        Err(error) => {
            return Err(CliError::system(
                "skill_read_failed",
                format!("failed to read {}: {error}", path.display()),
            ));
        }
    };
    if bytes == skill.content.as_bytes() {
        return Ok(InstallAction::Unchanged);
    }
    if force {
        return Ok(InstallAction::ForceOverwrite);
    }

    let text = String::from_utf8_lossy(&bytes);
    let managed_prefix = format!(
        "<!-- Installed by `consult-llm skill install` — name={} ",
        skill.name
    );
    if text.contains(&managed_prefix) {
        if let Some(version) = frontmatter_value(&text, "cli_version")
            && compare_versions(version, CLI_VERSION) == Some(Ordering::Greater)
        {
            return Err(CliError::domain(
                "newer_managed_skill",
                format!(
                    "refusing to replace newer managed skill {} (installed {version}, binary {CLI_VERSION}); pass --force to overwrite",
                    path.display()
                ),
            ));
        }
        return Ok(InstallAction::UpdateManaged);
    }

    Err(CliError::domain(
        "unmanaged_skill_conflict",
        format!(
            "refusing to replace unmanaged or locally modified skill {}; pass --force to overwrite",
            path.display()
        ),
    ))
}

fn frontmatter_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let body = content.strip_prefix("---\n")?;
    let (frontmatter, _) = body.split_once("\n---\n")?;
    frontmatter.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate == key).then(|| value.trim().trim_matches('"'))
    })
}

fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    fn parse(version: &str) -> Option<Vec<u64>> {
        version.split('.').map(|part| part.parse().ok()).collect()
    }
    Some(parse(left)?.cmp(&parse(right)?))
}

pub fn apply_install(plans: &[InstallPlan]) -> Result<(), CliError> {
    for plan in plans {
        if plan.action == InstallAction::Unchanged {
            continue;
        }
        let skill = find(plan.skill)?;
        let parent = plan.path.parent().expect("skill destination has a parent");
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system(
                "skill_directory_create_failed",
                format!("failed to create {}: {error}", parent.display()),
            )
        })?;
        atomic_write(&plan.path, skill.content.as_bytes())?;
    }
    Ok(())
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), CliError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("SKILL.md");
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(CliError::system(
            "skill_write_failed",
            format!("failed to write {} atomically: {error}", path.display()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_complete_and_versioned() {
        let expected = [
            "consult-llm",
            "llm-collab",
            "llm-collab-vs",
            "llm-consult",
            "llm-debate",
            "llm-debate-vs",
            "llm-implement",
            "llm-panel",
            "llm-review",
            "llm-review-panel",
            "llm-skill-review",
            "llm-workshop",
        ];
        assert_eq!(
            SKILLS.iter().map(|skill| skill.name).collect::<Vec<_>>(),
            expected
        );
        for skill in SKILLS {
            assert!(skill.content.contains(&format!("name: {}", skill.name)));
            assert!(
                skill
                    .content
                    .contains(&format!("cli_version: \"{}\"", CLI_VERSION))
            );
            assert!(skill.content.contains("schema_version: 1"));
            assert!(
                skill
                    .content
                    .contains("Installed by `consult-llm skill install`")
            );
        }
    }

    #[test]
    fn all_agent_paths_are_exact() {
        let root = Path::new("/target");
        let plans = plan_install(&["llm-consult"], Agent::All, root, false).unwrap();
        let paths = plans
            .iter()
            .map(|plan| plan.path.as_path())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                Path::new("/target/.claude/skills/llm-consult/SKILL.md"),
                Path::new("/target/.pi/agent/skills/llm-consult/SKILL.md"),
                Path::new("/target/.codex/skills/llm-consult/SKILL.md"),
                Path::new("/target/.config/opencode/skills/llm-consult/SKILL.md"),
            ]
        );
    }

    #[test]
    fn install_is_byte_faithful_idempotent_and_protects_unmanaged_files() {
        let temp = tempfile::tempdir().unwrap();
        let plans = plan_install(&["llm-consult"], Agent::Claude, temp.path(), false).unwrap();
        apply_install(&plans).unwrap();
        let path = &plans[0].path;
        assert_eq!(
            fs::read(path).unwrap(),
            find("llm-consult").unwrap().content.as_bytes()
        );
        assert_eq!(
            plan_install(&["llm-consult"], Agent::Claude, temp.path(), false).unwrap()[0].action,
            InstallAction::Unchanged
        );

        fs::write(path, "local edit").unwrap();
        let error = plan_install(&["llm-consult"], Agent::Claude, temp.path(), false).unwrap_err();
        assert_eq!(error.code(), "unmanaged_skill_conflict");
        let forced = plan_install(&["llm-consult"], Agent::Claude, temp.path(), true).unwrap();
        assert_eq!(forced[0].action, InstallAction::ForceOverwrite);
    }

    #[test]
    fn managed_older_upgrades_but_newer_requires_force() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".pi/agent/skills/llm-consult/SKILL.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "---\ncli_version: \"3.0.1\"\n---\n<!-- Installed by `consult-llm skill install` — name=llm-consult old -->\n").unwrap();
        assert_eq!(
            plan_install(&["llm-consult"], Agent::Pi, temp.path(), false).unwrap()[0].action,
            InstallAction::UpdateManaged
        );
        fs::write(&path, "---\ncli_version: \"99.0.0\"\n---\n<!-- Installed by `consult-llm skill install` — name=llm-consult newer -->\n").unwrap();
        assert_eq!(
            plan_install(&["llm-consult"], Agent::Pi, temp.path(), false)
                .unwrap_err()
                .code(),
            "newer_managed_skill"
        );
    }
}
