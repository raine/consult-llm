use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NAMES: &[&str] = &[
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

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_consult-llm"));
    command.env("CONSULT_LLM_NO_UPDATE_CHECK", "1");
    command
}

fn run(args: &[&str]) -> Output {
    binary().args(args).output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn installed(root: &Path, agent_path: &str, name: &str) -> PathBuf {
    root.join(agent_path).join(name).join("SKILL.md")
}

#[test]
fn list_and_version_discover_the_exact_catalog() {
    let list = run(&["--json", "skill", "list"]);
    assert_success(&list);
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    let listed = value["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(listed, NAMES);

    let version = run(&["version", "--json"]);
    assert_success(&version);
    let value: serde_json::Value = serde_json::from_slice(&version.stdout).unwrap();
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["supported_schemas"], serde_json::json!([1]));
    assert_eq!(value["skills"].as_array().unwrap().len(), NAMES.len());
    assert_eq!(value["commit"].as_str().unwrap().len(), 40);

    let compatible = run(&["--version"]);
    assert_success(&compatible);
    assert_eq!(
        String::from_utf8(compatible.stdout).unwrap(),
        format!("consult-llm {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn print_show_and_install_are_byte_faithful_for_every_skill() {
    let temp = tempfile::tempdir().unwrap();
    for name in NAMES {
        let source = fs::read(Path::new("skills").join(name).join("SKILL.md")).unwrap();
        let printed = run(&["skill", "print", name]);
        assert_success(&printed);
        assert_eq!(printed.stdout, source, "print mismatch for {name}");
        let shown = run(&["skill", "show", name]);
        assert_success(&shown);
        assert_eq!(shown.stdout, source, "show mismatch for {name}");
    }

    let root = temp.path().to_str().unwrap();
    let install = run(&[
        "skill",
        "install",
        "--agent",
        "claude",
        "--target-root",
        root,
    ]);
    assert_success(&install);
    for name in NAMES {
        assert_eq!(
            fs::read(installed(temp.path(), ".claude/skills", name)).unwrap(),
            fs::read(Path::new("skills").join(name).join("SKILL.md")).unwrap()
        );
    }
}

#[test]
fn all_target_uses_exact_supported_agent_paths() {
    let temp = tempfile::tempdir().unwrap();
    let output = run(&[
        "skill",
        "install",
        "llm-consult",
        "--agent",
        "all",
        "--target-root",
        temp.path().to_str().unwrap(),
        "--json",
    ]);
    assert_success(&output);
    for path in [
        ".claude/skills",
        ".pi/agent/skills",
        ".codex/skills",
        ".config/opencode/skills",
    ] {
        assert!(installed(temp.path(), path, "llm-consult").is_file());
    }
}

#[test]
fn dry_run_validates_but_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let output = run(&[
        "skill",
        "install",
        "--agent",
        "all",
        "--target-root",
        temp.path().to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["would"].as_array().unwrap().len(), NAMES.len() * 4);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn unknown_names_agents_and_clobbers_fail_safely() {
    let unknown = run(&["--json", "skill", "print", "missing"]);
    assert_eq!(unknown.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&unknown.stderr).unwrap();
    assert_eq!(error["error"]["code"], "skill_not_found");

    let agent = run(&["skill", "install", "--agent", "other"]);
    assert_eq!(agent.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&agent.stderr).contains("invalid value 'other'"));

    let temp = tempfile::tempdir().unwrap();
    let path = installed(temp.path(), ".claude/skills", "llm-consult");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "local instructions\n").unwrap();
    let root = temp.path().to_str().unwrap();
    let refused = run(&[
        "--json",
        "skill",
        "install",
        "llm-consult",
        "--target-root",
        root,
    ]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), "local instructions\n");

    let forced = run(&[
        "skill",
        "install",
        "llm-consult",
        "--target-root",
        root,
        "--force",
    ]);
    assert_success(&forced);
    assert_eq!(
        fs::read(&path).unwrap(),
        fs::read("skills/llm-consult/SKILL.md").unwrap()
    );
}
