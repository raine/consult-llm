use serde_json::json;

use crate::skills::{CLI_SCHEMA_VERSION, CLI_VERSION, SKILLS};

pub fn run(json_output: bool) {
    let commit = option_env!("GIT_HASH").filter(|hash| hash.len() == 40);
    let provenance_kind = option_env!("BUILD_PROVENANCE_KIND").unwrap_or("tarball");
    let provenance_note =
        option_env!("BUILD_PROVENANCE_NOTE").unwrap_or("no .git in source archive");

    if json_output {
        let skills = SKILLS
            .iter()
            .map(|skill| {
                json!({
                    "name": skill.name,
                    "cli_version": skill.cli_version,
                    "schema_version": skill.schema_version,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": CLI_SCHEMA_VERSION,
                "version": CLI_VERSION,
                "commit": commit,
                "build_provenance": {
                    "kind": if commit.is_some() { "git" } else { provenance_kind },
                    "note": if commit.is_some() { "git commit embedded at build time" } else { provenance_note },
                },
                "supported_schemas": [CLI_SCHEMA_VERSION],
                "skills": skills,
            }))
            .expect("version JSON is serializable")
        );
    } else {
        println!("consult-llm {CLI_VERSION}");
        println!("schema_version: {CLI_SCHEMA_VERSION}");
        println!("commit: {}", commit.unwrap_or("unavailable"));
        println!("bundled skills ({}):", SKILLS.len());
        for skill in SKILLS {
            println!(
                "  {} (cli {}, schema {})",
                skill.name, skill.cli_version, skill.schema_version
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_metadata_is_synchronized() {
        assert_eq!(SKILLS.len(), 12);
        assert!(SKILLS.iter().all(|skill| skill.cli_version == CLI_VERSION));
        assert!(SKILLS.iter().all(|skill| skill.schema_version == 1));
        if let Some(commit) = option_env!("GIT_HASH") {
            assert_eq!(commit.len(), 40);
            assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }
}
