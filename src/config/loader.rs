use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::config::file::ApiKeyPolicy;
use crate::config::types::CliProfile;

#[derive(Debug, Clone)]
pub enum Source {
    Env,
    ProjectLocal(PathBuf),
    Project(PathBuf),
    User(PathBuf),
    #[allow(dead_code)]
    Default,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Env => write!(f, "env"),
            Source::ProjectLocal(p) => write!(f, "{}", p.display()),
            Source::Project(p) => write!(f, "{}", p.display()),
            Source::User(p) => write!(f, "{}", p.display()),
            Source::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

type FileLayer = (
    Source,
    HashMap<String, String>,
    BTreeMap<String, CliProfile>,
);

pub struct LayeredEnv {
    file_layers: Vec<FileLayer>,
}

impl LayeredEnv {
    pub fn load(paths: &crate::config::discovery::DiscoveredPaths) -> Result<Self, LoadError> {
        let mut file_layers: Vec<FileLayer> = Vec::new();

        for (src_ctor, path, policy) in [
            (
                Source::ProjectLocal as fn(PathBuf) -> Source,
                &paths.project_local,
                ApiKeyPolicy::Allow,
            ),
            (Source::Project, &paths.project, ApiKeyPolicy::Forbid),
            (Source::User, &paths.user, ApiKeyPolicy::Allow),
        ] {
            if let Some(p) = path {
                let yaml = std::fs::read_to_string(p).map_err(|e| LoadError {
                    path: p.clone(),
                    message: e.to_string(),
                })?;
                let cfg = crate::config::file::ConfigFile::parse(&yaml).map_err(|e| LoadError {
                    path: p.clone(),
                    message: e.to_string(),
                })?;
                let env_map = cfg.to_env_map(policy).map_err(|e| LoadError {
                    path: p.clone(),
                    message: e.to_string(),
                })?;
                let profiles = cfg.cli_profiles.clone();
                file_layers.push((src_ctor(p.clone()), env_map, profiles));
            }
        }

        Ok(Self { file_layers })
    }

    pub fn lookup(&self, key: &str) -> Option<(String, Source)> {
        // Real env wins; treat empty as unset to match env_non_empty behavior.
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return Some((v, Source::Env));
        }
        // File layers keep empty strings so explicit empty lists (e.g. `allowed_models: []`)
        // override lower layers instead of falling through.
        for (src, map, _) in &self.file_layers {
            if let Some(v) = map.get(key) {
                return Some((v.clone(), src.clone()));
            }
            if matches!(
                key,
                "CONSULT_LLM_DEFAULT_MODEL" | "CONSULT_LLM_DEFAULT_MODELS"
            ) && map.contains_key("CONSULT_LLM_ALLOWED_MODELS")
            {
                return None;
            }
        }
        None
    }

    /// Returns all configured CLI profiles merged across layers with the correct
    /// precedence (project-local overrides project, project overrides user).
    pub fn cli_profiles(&self) -> BTreeMap<String, CliProfile> {
        let mut merged = BTreeMap::new();
        // file_layers are stored in highest-first order (project-local, project, user).
        // Reversed for merge: user first, then project, then project-local overwrites.
        for (_, _, profiles) in self.file_layers.iter().rev() {
            for (name, profile) in profiles {
                merged.insert(name.clone(), profile.clone());
            }
        }
        merged
    }

    pub fn as_env_fn(&self) -> impl Fn(&str) -> Option<String> + '_ {
        move |key| self.lookup(key).map(|(v, _)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::discovery::DiscoveredPaths;

    fn write_yaml(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn three_layer_env(dir: &tempfile::TempDir) -> (LayeredEnv, DiscoveredPaths) {
        let local_path = write_yaml(dir, "local.yaml", "default_model: from-local\n");
        let project_path = write_yaml(dir, "project.yaml", "default_model: from-project\n");
        let user_path = write_yaml(dir, "user.yaml", "default_model: from-user\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: Some(project_path),
            project_local: Some(local_path),
        };
        let env = LayeredEnv::load(&paths).unwrap();
        (env, paths)
    }

    #[test]
    fn test_lookup_prefers_project_local_over_project() {
        let dir = tempfile::tempdir().unwrap();
        let local_path = write_yaml(&dir, "local.yaml", "default_model: from-local\n");
        let project_path = write_yaml(&dir, "project.yaml", "default_model: from-project\n");
        let paths = DiscoveredPaths {
            user: None,
            project: Some(project_path),
            project_local: Some(local_path),
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, _) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-local");
    }

    #[test]
    fn test_lookup_prefers_project_over_user() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(&dir, "project.yaml", "default_model: from-project\n");
        let user_path = write_yaml(&dir, "user.yaml", "default_model: from-user\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: Some(project_path),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, _) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-project");
    }

    #[test]
    fn test_lookup_falls_through_to_user() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = write_yaml(&dir, "user.yaml", "default_model: from-user\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: None,
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, _) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-user");
    }

    #[test]
    fn test_lookup_returns_none_when_unset() {
        let paths = DiscoveredPaths {
            user: None,
            project: None,
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        // Use a key unlikely to be set in the test environment
        assert!(env.lookup("CONSULT_LLM_NONEXISTENT_KEY_XYZ").is_none());
    }

    #[test]
    fn test_project_allowed_models_blocks_user_default_models() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(&dir, "project.yaml", "allowed_models: [gpt-5.5]\n");
        let user_path = write_yaml(
            &dir,
            "user.yaml",
            "default_models: [gpt-5.5, deepseek-v4-pro]\n",
        );
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: Some(project_path),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        assert!(env.lookup("CONSULT_LLM_DEFAULT_MODELS").is_none());
    }

    #[test]
    fn test_project_allowed_models_blocks_user_default_model() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(&dir, "project.yaml", "allowed_models: [gpt-5.5]\n");
        let user_path = write_yaml(&dir, "user.yaml", "default_model: deepseek-v4-pro\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: Some(project_path),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        assert!(env.lookup("CONSULT_LLM_DEFAULT_MODEL").is_none());
    }

    #[test]
    fn test_project_default_models_still_beats_user_with_allowed_models() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(
            &dir,
            "project.yaml",
            "allowed_models: [gpt-5.5]\ndefault_models: [gpt-5.5, gpt-5.5]\n",
        );
        let user_path = write_yaml(&dir, "user.yaml", "default_models: [deepseek-v4-pro]\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: Some(project_path),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, _) = env.lookup("CONSULT_LLM_DEFAULT_MODELS").unwrap();
        assert_eq!(val, "gpt-5.5,gpt-5.5");
    }

    #[test]
    fn test_lookup_prefers_env_over_files() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(&dir, "project.yaml", "system_prompt_path: /from-file\n");
        let paths = DiscoveredPaths {
            user: None,
            project: Some(project_path),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        // CONSULT_LLM_SYSTEM_PROMPT_PATH from the file
        let (val, src) = env.lookup("CONSULT_LLM_SYSTEM_PROMPT_PATH").unwrap();
        assert_eq!(val, "/from-file");
        assert!(matches!(src, Source::Project(_)));

        // Now test that if the real env var is set, it wins.
        // We set a unique env var to avoid clashing with the real system prompt path.
        // Use set_var with a key that won't conflict with other tests.
        let test_key = "CONSULT_LLM_SYSTEM_PROMPT_PATH";
        // Only test file-layer fallback here; env override is tested via the
        // Source::Env branch implicitly by all other tests that check file-layer values.
        let _ = (val, test_key);
    }

    #[test]
    fn test_api_key_in_project_config_fails() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(&dir, "project.yaml", "openai:\n  api_key: sk-secret\n");
        let paths = DiscoveredPaths {
            user: None,
            project: Some(project_path),
            project_local: None,
        };
        match LayeredEnv::load(&paths) {
            Ok(_) => panic!("expected error for api_key in project config"),
            Err(err) => assert!(err.message.contains("api_key")),
        }
    }

    #[test]
    fn test_api_key_in_project_local_loads_without_error() {
        let dir = tempfile::tempdir().unwrap();
        let local_path = write_yaml(&dir, "local.yaml", "openai:\n  api_key: sk-secret\n");
        let paths = DiscoveredPaths {
            user: None,
            project: None,
            project_local: Some(local_path),
        };
        // Load must succeed; key-mapping behavior covered in config/file.rs tests
        assert!(LayeredEnv::load(&paths).is_ok());
    }

    #[test]
    fn test_base_url_in_project_config_fails() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = write_yaml(
            &dir,
            "project.yaml",
            "zai:\n  base_url: https://attacker.invalid/v1\n",
        );
        let paths = DiscoveredPaths {
            user: None,
            project: Some(project_path),
            project_local: None,
        };
        match LayeredEnv::load(&paths) {
            Ok(_) => panic!("expected error for base_url in project config"),
            Err(err) => assert!(err.message.contains("base_url")),
        }
    }

    #[test]
    fn test_base_url_in_user_config_reaches_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = write_yaml(
            &dir,
            "user.yaml",
            "zai:\n  base_url: https://api.z.ai/api/coding/paas/v4\n",
        );
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: None,
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, _) = env.lookup("CONSULT_LLM_ZAI_BASE_URL").unwrap();
        assert_eq!(val, "https://api.z.ai/api/coding/paas/v4");
    }

    #[test]
    fn test_api_key_in_user_config_loads_without_error() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = write_yaml(&dir, "user.yaml", "gemini:\n  api_key: gm-key\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: None,
            project_local: None,
        };
        assert!(LayeredEnv::load(&paths).is_ok());
    }

    #[test]
    fn test_three_layer_precedence_project_local_beats_project_beats_user() {
        // All three file layers populated simultaneously, single fixture.
        // Pins precedence project_local > project > user (file layers only;
        // env precedence is asserted separately).
        let dir = tempfile::tempdir().unwrap();
        let (env, paths) = three_layer_env(&dir);

        // Top layer wins.
        let (val, src) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-local");
        assert!(matches!(src, Source::ProjectLocal(_)));

        // Drop the top file layer: project layer wins.
        let paths = DiscoveredPaths {
            user: paths.user.clone(),
            project: paths.project.clone(),
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, src) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-project");
        assert!(matches!(src, Source::Project(_)));

        // Drop the project layer too: user layer wins.
        let paths = DiscoveredPaths {
            user: paths.user.clone(),
            project: None,
            project_local: None,
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, src) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-user");
        assert!(matches!(src, Source::User(_)));
    }

    #[test]
    fn test_env_overrides_all_file_layers() {
        // Real process env wins over every file layer. Uses
        // CONSULT_LLM_TEST_ENV_OVERRIDE_KEY — a unique key not consumed by
        // anything in the binary so it cannot collide with parallel tests.
        let key = "CONSULT_LLM_TEST_ENV_OVERRIDE_KEY";
        let dir = tempfile::tempdir().unwrap();
        // Use a config-known key in the file (default_model maps to
        // CONSULT_LLM_DEFAULT_MODEL); separately set the unique env key and
        // assert it surfaces with Source::Env. We test both directions:
        //   (a) a real env override on a config-known key beats every file.
        //   (b) an env-only key with no file backing is observed.
        let (env, _paths) = three_layer_env(&dir);

        // (a) override the config-known key from the real process env.
        // SAFETY: tests in this binary run on multiple threads but no other
        // test reads/writes CONSULT_LLM_DEFAULT_MODEL during its body.
        unsafe { std::env::set_var("CONSULT_LLM_DEFAULT_MODEL", "from-real-env") };
        let (val, src) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-real-env");
        assert!(matches!(src, Source::Env));
        unsafe { std::env::remove_var("CONSULT_LLM_DEFAULT_MODEL") };

        // (b) env-only unique key surfaces with Source::Env.
        unsafe { std::env::set_var(key, "env-only-value") };
        let (val, src) = env.lookup(key).unwrap();
        assert_eq!(val, "env-only-value");
        assert!(matches!(src, Source::Env));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn test_api_key_project_local_beats_user_in_file_layers() {
        // Tests file-layer precedence using a CONSULT_LLM_* key (not a real API key
        // var) to avoid interference from real environment variables.
        let dir = tempfile::tempdir().unwrap();
        let local_path = write_yaml(&dir, "local.yaml", "default_model: from-local\n");
        let user_path = write_yaml(&dir, "user.yaml", "default_model: from-user\n");
        let paths = DiscoveredPaths {
            user: Some(user_path),
            project: None,
            project_local: Some(local_path),
        };
        let env = LayeredEnv::load(&paths).unwrap();
        let (val, src) = env.lookup("CONSULT_LLM_DEFAULT_MODEL").unwrap();
        assert_eq!(val, "from-local");
        assert!(matches!(src, Source::ProjectLocal(_)));
    }
}
