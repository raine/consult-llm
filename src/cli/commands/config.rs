use anyhow::Context;
use serde_yaml::Value;
use std::{fs, path::PathBuf};

#[derive(clap::Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigCommand {
    /// Set a config value
    Set {
        /// Config key in dot notation (e.g. gemini.backend, default_model, default_models)
        key: String,
        /// Value to set (parsed as YAML: true/false, numbers, [a,b] lists)
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Write to project config (.consult-llm.yaml)
        #[arg(long)]
        project: bool,
        /// Write to local project config (.consult-llm.local.yaml)
        #[arg(long, conflicts_with = "project")]
        local: bool,
    },
}

pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command {
        ConfigCommand::Set {
            key,
            value,
            project,
            local,
        } => run_set(key, value, project, local),
    }
}

fn run_set(key: String, value: String, project: bool, local: bool) -> anyhow::Result<()> {
    validate_key_path(&key)?;

    let path = target_path(project, local)?;
    let (mut doc, from_legacy) = if path.exists() {
        let src = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let v: Value =
            serde_yaml::from_str(&src).with_context(|| format!("parse {}", path.display()))?;
        // An empty file deserializes as Null, treat it as an empty mapping
        let doc = match v {
            Value::Null => Value::Mapping(serde_yaml::Mapping::new()),
            other => other,
        };
        (doc, false)
    } else {
        let legacy = (!project && !local)
            .then(|| crate::paths::legacy_config_dir().map(|d| d.join("config.yaml")))
            .flatten()
            .filter(|p| p.exists());
        if let Some(l) = legacy {
            let src = fs::read_to_string(&l).with_context(|| format!("read {}", l.display()))?;
            let v: Value =
                serde_yaml::from_str(&src).with_context(|| format!("parse {}", l.display()))?;
            let doc = match v {
                Value::Null => Value::Mapping(serde_yaml::Mapping::new()),
                other => other,
            };
            (doc, true)
        } else {
            (Value::Mapping(serde_yaml::Mapping::new()), false)
        }
    };

    let parsed = serde_yaml::from_str::<Value>(&value)
        .with_context(|| format!("parse value {:?}", value))?;

    set_nested_key(&mut doc, &key, parsed)?;

    // Validate the resulting document against the config schema before persisting.
    // This catches typos like "gemini.bakcend" before they silently corrupt the file.
    let out = serde_yaml::to_string(&doc)?;
    let cfg = crate::config::file::ConfigFile::parse(&out)
        .with_context(|| format!("invalid config key {:?} - check spelling", key))?;

    // Reject API keys in the committed project config.
    if project {
        cfg.to_env_map(crate::config::file::ApiKeyPolicy::Forbid)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &out)?;
    if from_legacy {
        eprintln!("Migrated config from legacy path to: {}", path.display());
    }
    println!("set {} in {}", key, path.display());
    Ok(())
}

fn target_path(project: bool, local: bool) -> anyhow::Result<PathBuf> {
    if local {
        return Ok(PathBuf::from(".consult-llm.local.yaml"));
    }
    if project {
        return Ok(PathBuf::from(".consult-llm.yaml"));
    }
    crate::paths::user_config_file()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))
}

fn validate_key_path(key: &str) -> anyhow::Result<()> {
    for segment in key.split('.') {
        if segment.is_empty() {
            anyhow::bail!(
                "invalid key {:?}: segments must be non-empty (got empty segment)",
                key
            );
        }
    }
    Ok(())
}

fn set_nested_key(node: &mut Value, key: &str, value: Value) -> anyhow::Result<()> {
    let (head, tail) = match key.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (key, None),
    };

    let map = match node {
        Value::Mapping(m) => m,
        other => anyhow::bail!("expected a mapping, got {:?}", other),
    };

    let head_key = Value::String(head.to_owned());

    if let Some(rest) = tail {
        if !map.contains_key(&head_key) {
            map.insert(head_key.clone(), Value::Mapping(serde_yaml::Mapping::new()));
        }
        let child = map.get_mut(&head_key).unwrap();
        set_nested_key(child, rest, value)
    } else {
        map.insert(head_key, value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::file::ConfigFile;
    use crate::config::types::{CliProfileInterface, CliPromptMode};
    use serde_yaml::Value;

    fn mapping() -> Value {
        Value::Mapping(serde_yaml::Mapping::new())
    }

    #[test]
    fn test_set_top_level() {
        let mut doc = mapping();
        set_nested_key(&mut doc, "default_model", Value::String("gemini".into())).unwrap();
        let m = doc.as_mapping().unwrap();
        assert_eq!(
            m.get("default_model").unwrap(),
            &Value::String("gemini".into())
        );
    }

    #[test]
    fn test_set_nested() {
        let mut doc = mapping();
        set_nested_key(
            &mut doc,
            "gemini.backend",
            Value::String("gemini-cli".into()),
        )
        .unwrap();
        let gemini = doc.as_mapping().unwrap().get("gemini").unwrap();
        let backend = gemini.as_mapping().unwrap().get("backend").unwrap();
        assert_eq!(backend, &Value::String("gemini-cli".into()));
    }

    #[test]
    fn test_set_preserves_existing_keys() {
        let mut doc = mapping();
        set_nested_key(&mut doc, "default_model", Value::String("gemini".into())).unwrap();
        set_nested_key(
            &mut doc,
            "gemini.backend",
            Value::String("gemini-cli".into()),
        )
        .unwrap();
        // Both keys present
        let m = doc.as_mapping().unwrap();
        assert!(m.contains_key("default_model"));
        assert!(m.contains_key("gemini"));
    }

    #[test]
    fn test_set_base_url_round_trips() {
        let mut doc = mapping();
        set_nested_key(
            &mut doc,
            "zai.base_url",
            Value::String("https://api.z.ai/api/coding/paas/v4".into()),
        )
        .unwrap();
        let out = serde_yaml::to_string(&doc).unwrap();
        let cfg = ConfigFile::parse(&out).unwrap();
        let m = cfg
            .to_env_map(crate::config::file::ApiKeyPolicy::Allow)
            .unwrap();
        assert_eq!(
            m["CONSULT_LLM_ZAI_BASE_URL"],
            "https://api.z.ai/api/coding/paas/v4"
        );
    }

    #[test]
    fn test_validate_key_rejects_empty_segments() {
        assert!(validate_key_path("").is_err());
        assert!(validate_key_path("gemini..backend").is_err());
        assert!(validate_key_path(".gemini").is_err());
    }

    #[test]
    fn test_validate_key_accepts_valid() {
        assert!(validate_key_path("default_model").is_ok());
        assert!(validate_key_path("gemini.backend").is_ok());
        assert!(validate_key_path("opencode.default_provider").is_ok());
    }

    #[test]
    fn test_incomplete_profile_gets_defaults() {
        // A profile with only command set should get defaults for the rest.
        let mut doc = mapping();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.command",
            Value::String("claude".into()),
        )
        .unwrap();
        let out = serde_yaml::to_string(&doc).unwrap();
        let cfg = ConfigFile::parse(&out).unwrap();
        let profile = &cfg.cli_profiles["claude"];
        assert_eq!(profile.command, "claude");
        assert_eq!(profile.interface, CliProfileInterface::StreamJson);
        assert_eq!(profile.prompt, CliPromptMode::Stdin);
        assert_eq!(profile.effort, None);
    }

    #[test]
    fn test_set_cli_profile_nested_keys_valid() {
        // Setting all required fields for a profile should produce valid config.
        let mut doc = mapping();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.command",
            Value::String("claude".into()),
        )
        .unwrap();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.type",
            Value::String("claude-cli".into()),
        )
        .unwrap();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.args",
            Value::Sequence(vec![Value::String("-p".into())]),
        )
        .unwrap();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.interface",
            Value::String("stream-json".into()),
        )
        .unwrap();
        set_nested_key(
            &mut doc,
            "cli_profiles.claude.prompt",
            Value::String("stdin".into()),
        )
        .unwrap();
        let out = serde_yaml::to_string(&doc).unwrap();
        let parsed = ConfigFile::parse(&out).unwrap();
        assert!(parsed.cli_profiles.contains_key("claude"));
    }
}
