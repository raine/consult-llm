use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config::types::CliProfile;

use consult_llm_core::monitoring::{active_dir, runs_dir, sessions_dir};

use crate::models::PROVIDERS;

// ---- terminal styling -------------------------------------------------------

fn use_color() -> bool {
    std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal()
}

fn paint(color: bool, code: &str, text: impl Display) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn bold(c: bool, t: impl Display) -> String {
    paint(c, "1", t)
}
fn muted(c: bool, t: impl Display) -> String {
    paint(c, "2", t)
}
fn accent(c: bool, t: impl Display) -> String {
    paint(c, "1;36", t)
}
fn success(c: bool, t: impl Display) -> String {
    paint(c, "1;32", t)
}
fn warning(c: bool, t: impl Display) -> String {
    paint(c, "33", t)
}
fn error_text(c: bool, t: impl Display) -> String {
    paint(c, "1;31", t)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Fail,
    Skip,
    Info,
}

fn symbol(c: bool, s: Status) -> String {
    match s {
        Status::Ok => success(c, "✓"),
        Status::Warn => warning(c, "⚠"),
        Status::Fail => error_text(c, "✗"),
        Status::Skip => muted(c, "·"),
        Status::Info => muted(c, "·"),
    }
}

fn print_section(c: bool, title: &str) {
    println!();
    println!("{}", accent(c, title));
}

fn print_row(c: bool, status: Status, label: &str, detail: Option<&str>) {
    let sym = symbol(c, status);
    let lbl = match status {
        Status::Skip => muted(c, label),
        _ => label.to_string(),
    };
    match detail {
        Some(d) => println!("  {sym} {lbl}  {}", muted(c, d)),
        None => println!("  {sym} {lbl}"),
    }
}

fn print_row_aligned(
    c: bool,
    status: Status,
    label: &str,
    label_width: usize,
    detail: Option<&str>,
) {
    let sym = symbol(c, status);
    let visible_pad = label_width.saturating_sub(label.chars().count());
    let pad: String = std::iter::repeat_n(' ', visible_pad).collect();
    let lbl = match status {
        Status::Skip => muted(c, label),
        _ => label.to_string(),
    };
    match detail {
        Some(d) => println!("  {sym} {lbl}{pad}  {}", muted(c, d)),
        None => println!("  {sym} {lbl}{pad}"),
    }
}

// ---- path helpers -----------------------------------------------------------

fn shorten(path: &Path, home: Option<&Path>) -> String {
    if let Some(h) = home
        && let Ok(rel) = path.strip_prefix(h)
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}

fn shorten_str(s: &str, home: Option<&Path>) -> String {
    if let Some(h) = home {
        let h_str = h.to_string_lossy();
        if let Some(rest) = s.strip_prefix(h_str.as_ref()) {
            return format!("~{rest}");
        }
    }
    s.to_string()
}

fn check_writable(path: &Path) -> bool {
    let test = path.join(".consult-llm-write-test");
    match std::fs::File::create(&test) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test);
            true
        }
        Err(_) => false,
    }
}

// ---- backend helpers --------------------------------------------------------

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn backend_binary(backend: &str) -> Option<&'static str> {
    match backend {
        "codex-cli" => Some("codex"),
        "gemini-cli" => Some("gemini"),
        "grok-cli" => Some("grok"),
        "cursor-cli" => Some("cursor-agent"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "claude-cli" => Some("claude"),
        _ => None,
    }
}

fn profile_command_dependency(command: &str) -> (bool, String) {
    let path_like = command.contains(std::path::MAIN_SEPARATOR);
    if path_like {
        let path = Path::new(command);
        if path.is_file() {
            (true, format!("{command} (profile command)"))
        } else {
            (false, format!("{command} not found"))
        }
    } else {
        match which(command) {
            Some(path) => (true, format!("{command} ({})", path.display())),
            None => (false, format!("{command} not found on PATH")),
        }
    }
}

fn is_profile_backend(backend: &str) -> bool {
    backend == "profile"
}

fn profile_backend_dependency(
    spec: &crate::models::ProviderSpec,
    backend: &str,
    profile_name: Option<&str>,
    cli_profiles: &BTreeMap<String, CliProfile>,
) -> Option<(bool, String)> {
    if !is_profile_backend(backend) {
        return None;
    }
    let Some(profile_name) = profile_name else {
        let allowed = cli_profiles.keys().cloned().collect::<Vec<_>>().join(", ");
        let suffix = if allowed.is_empty() {
            "no cli_profiles configured".to_string()
        } else {
            format!("available profiles: {allowed}")
        };
        return Some((false, format!("{} unset; {suffix}", spec.cli_profile_env)));
    };
    let Some(profile) = cli_profiles.get(profile_name) else {
        let allowed = cli_profiles.keys().cloned().collect::<Vec<_>>().join(", ");
        return Some((
            false,
            format!("profile '{profile_name}' not found; available profiles: {allowed}"),
        ));
    };
    let (ok, dep) = profile_command_dependency(&profile.command);
    Some((ok, format!("profile '{profile_name}' command {dep}")))
}

// ---- config key helpers -----------------------------------------------------

fn config_keys() -> Vec<&'static str> {
    let mut keys = vec![
        "CONSULT_LLM_DEFAULT_MODEL",
        "CONSULT_LLM_ALLOWED_MODELS",
        "CONSULT_LLM_EXTRA_MODELS",
        "CONSULT_LLM_SYSTEM_PROMPT_PATH",
        "CONSULT_LLM_NO_UPDATE_CHECK",
        "CONSULT_LLM_OPENCODE_PROVIDER",
    ];
    for spec in PROVIDERS {
        keys.push(spec.backend_env);
        keys.push(spec.base_url_env);
        keys.push(spec.cli_profile_env);
        keys.push(spec.opencode_env);
        if let Some(env) = spec.reasoning_effort_env {
            keys.push(env);
        }
        if let Some(env) = spec.extra_args_env {
            keys.push(env);
        }
    }
    keys
}

fn semantic_name(env_key: &str) -> String {
    match env_key {
        "CONSULT_LLM_DEFAULT_MODEL" => "default_model".into(),
        "CONSULT_LLM_ALLOWED_MODELS" => "allowed_models".into(),
        "CONSULT_LLM_EXTRA_MODELS" => "extra_models".into(),
        // Historical label predating the registry-driven view; kept stable.
        "CONSULT_LLM_CODEX_REASONING_EFFORT" => "codex.reasoning_effort".into(),
        "CONSULT_LLM_SYSTEM_PROMPT_PATH" => "system_prompt_path".into(),
        "CONSULT_LLM_NO_UPDATE_CHECK" => "no_update_check".into(),
        "CONSULT_LLM_OPENCODE_PROVIDER" => "opencode.provider".into(),
        k => {
            for spec in PROVIDERS {
                if k == spec.backend_env {
                    return format!("{}.backend", spec.id);
                }
                if k == spec.base_url_env {
                    return format!("{}.base_url", spec.id);
                }
                if k == spec.cli_profile_env {
                    return format!("{}.cli_profile", spec.id);
                }
                if k == spec.opencode_env {
                    return format!("opencode.{}.provider", spec.id);
                }
                if Some(k) == spec.reasoning_effort_env {
                    return format!("{}.reasoning_effort", spec.id);
                }
                if Some(k) == spec.extra_args_env {
                    return format!("{}.extra_args", spec.id);
                }
            }
            k.into()
        }
    }
}

// ---- data collection --------------------------------------------------------

enum ProvStatus {
    Ok,
    Err,
    Skip, // not in allowed_models scope
}

struct ProvRow {
    id: &'static str,
    model: String,
    backend: String,
    status: ProvStatus,
    detail: String,
}

struct PathRow {
    name: &'static str,
    path: PathBuf,
    exists: bool,
    writable: bool,
}

// ---- cursor model validation ------------------------------------------------

#[derive(Clone, Copy)]
enum Severity {
    Warn,
    Err,
}

fn validate_cursor_model(
    model: &str,
    effort: &str,
    cache: &mut Option<crate::executors::cursor_models::ModelList>,
) -> Option<(Severity, String)> {
    use crate::executors::cursor_cli::{map_cursor_model, model_requires_reasoning_suffix};
    use crate::executors::cursor_models::{self, ModelList};

    let candidate = map_cursor_model(model, effort);
    let base = model.replace("-preview", "");

    if cache.is_none() {
        *cache = Some(cursor_models::available_models());
    }
    let list = cache.as_ref().unwrap();

    if model_requires_reasoning_suffix(&base) {
        match cursor_models::resolve_model(&candidate, &base, list) {
            Ok(resolved) if resolved == candidate => None,
            Ok(resolved) => {
                let resolved_suffix = resolved
                    .strip_prefix(&base)
                    .and_then(|s| s.strip_prefix('-'))
                    .unwrap_or("");
                if is_effort_synonym(effort, resolved_suffix) {
                    None
                } else {
                    Some((
                        Severity::Warn,
                        format!(
                            "model '{candidate}' rewritten to '{resolved}' (effort='{effort}' isn't accepted by cursor-agent for this model)"
                        ),
                    ))
                }
            }
            Err(e) => Some((Severity::Err, e.to_string())),
        }
    } else {
        let ModelList::Fresh(models) = list else {
            return None;
        };
        if models.iter().any(|m| m == &candidate) {
            None
        } else {
            Some((
                Severity::Err,
                format!("cursor-agent does not list model '{candidate}'"),
            ))
        }
    }
}

fn is_effort_synonym(a: &str, b: &str) -> bool {
    let canon = |e: &str| {
        match e {
            "xhigh" | "extra-high" => "xhigh",
            other => match other.strip_suffix("-fast") {
                Some("xhigh") | Some("extra-high") => "xhigh",
                _ => other,
            },
        }
        .to_string()
    };
    canon(a) == canon(b)
}

// ---- main -------------------------------------------------------------------

pub fn run(verbose: bool) -> anyhow::Result<()> {
    let c = use_color();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = dirs::home_dir();
    let disc = crate::config::discovery::discover(&cwd, home.as_deref());
    let env = match crate::config::loader::LayeredEnv::load(&disc) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("config error: {}: {}", e.path.display(), e.message);
            return Ok(());
        }
    };

    let allowed: Option<Vec<String>> = env.lookup("CONSULT_LLM_ALLOWED_MODELS").map(|(v, _)| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let effort = env
        .lookup("CONSULT_LLM_CODEX_REASONING_EFFORT")
        .map(|(v, _)| v)
        .unwrap_or_else(|| "high".to_string());

    // ---- Collect provider rows
    let mut prov_rows: Vec<ProvRow> = Vec::new();
    let mut warnings: Vec<(Severity, String)> = Vec::new();
    let mut has_warn = false;
    let mut has_error = false;
    let mut cursor_list_cache: Option<crate::executors::cursor_models::ModelList> = None;
    let cli_profiles = env.cli_profiles();

    for spec in PROVIDERS {
        let backend = env
            .lookup(spec.backend_env)
            .map(|(v, _)| v)
            .unwrap_or_else(|| "api".to_string());
        let profile_name = env.lookup(spec.cli_profile_env).map(|(v, _)| v);

        let in_scope = match &allowed {
            None => true,
            Some(models) => models
                .iter()
                .any(|m| spec.model_prefixes.iter().any(|p| m.starts_with(p))),
        };

        if !in_scope {
            prov_rows.push(ProvRow {
                id: spec.id,
                model: String::new(),
                backend: String::new(),
                status: ProvStatus::Skip,
                detail: "not in allowed_models".into(),
            });
            continue;
        }

        let model = match &allowed {
            Some(models) => models
                .iter()
                .find(|m| spec.model_prefixes.iter().any(|p| m.starts_with(p)))
                .cloned()
                .unwrap_or_else(|| {
                    spec.builtin_models
                        .first()
                        .copied()
                        .unwrap_or("")
                        .to_string()
                }),
            None => spec
                .builtin_models
                .first()
                .copied()
                .unwrap_or("")
                .to_string(),
        };

        let (dep_ok, mut detail) = if backend == "api" {
            if let Some((_, src)) = env.lookup(spec.api_key_env) {
                let src_label = shorten_str(&src.to_string(), home.as_deref());
                (true, format!("{} set [{}]", spec.api_key_env, src_label))
            } else {
                (false, format!("{} unset", spec.api_key_env))
            }
        } else if let Some((ok, detail)) =
            profile_backend_dependency(spec, &backend, profile_name.as_deref(), &cli_profiles)
        {
            (ok, detail)
        } else if let Some(bin) = backend_binary(&backend) {
            match which(bin) {
                Some(path) => (true, format!("{bin} ({})", path.display())),
                None => (false, format!("{bin} not found on PATH")),
            }
        } else {
            (false, format!("unknown backend '{backend}'"))
        };

        let mut sev: Option<Severity> = if dep_ok { None } else { Some(Severity::Err) };

        if dep_ok
            && backend == "cursor-cli"
            && !model.is_empty()
            && let Some((extra_sev, extra)) =
                validate_cursor_model(&model, &effort, &mut cursor_list_cache)
        {
            sev = Some(extra_sev);
            detail = format!("{detail}; {extra}");
        }

        if let Some(s) = sev {
            match s {
                Severity::Err => has_error = true,
                Severity::Warn => has_warn = true,
            }
            warnings.push((s, format!("{}: {}", spec.id, detail)));
        }

        prov_rows.push(ProvRow {
            id: spec.id,
            model,
            backend,
            status: match sev {
                None => ProvStatus::Ok,
                Some(_) => ProvStatus::Err,
            },
            detail,
        });
    }

    // ---- Collect path rows
    let path_entries: &[(&'static str, PathBuf)] = &[
        ("sessions", sessions_dir()),
        ("active", active_dir()),
        ("runs", runs_dir()),
    ];

    let mut path_rows: Vec<PathRow> = Vec::new();
    for (name, path) in path_entries {
        let exists = path.exists();
        let writable = exists && check_writable(path);
        if !exists || !writable {
            has_warn = true;
            warnings.push((
                Severity::Warn,
                format!(
                    "{name}: {}",
                    if !exists { "not found" } else { "not writable" }
                ),
            ));
        }
        path_rows.push(PathRow {
            name,
            path: path.clone(),
            exists,
            writable,
        });
    }

    // ---- Header
    let version = env!("CARGO_PKG_VERSION");
    println!(
        "{} {}",
        bold(c, "consult-llm doctor"),
        muted(c, format!("v{version}"))
    );

    // ---- Providers
    print_section(c, "Providers");
    let id_w = prov_rows.iter().map(|r| r.id.len()).max().unwrap_or(8);
    for row in &prov_rows {
        match row.status {
            ProvStatus::Skip => {
                print_row_aligned(c, Status::Skip, row.id, id_w, Some(&row.detail));
            }
            ProvStatus::Ok => {
                let detail = format!("{} via {} - {}", row.model, row.backend, row.detail);
                print_row_aligned(c, Status::Ok, row.id, id_w, Some(&detail));
            }
            ProvStatus::Err => {
                let detail = if row.model.is_empty() && row.backend.is_empty() {
                    row.detail.clone()
                } else {
                    format!("{} via {} - {}", row.model, row.backend, row.detail)
                };
                print_row_aligned(c, Status::Fail, row.id, id_w, Some(&detail));
            }
        }
    }

    // ---- Config
    print_section(c, "Config");
    let keys = config_keys();
    if verbose {
        let key_w = keys
            .iter()
            .map(|k| semantic_name(k).len())
            .max()
            .unwrap_or(20);
        for key in &keys {
            let name = semantic_name(key);
            match env.lookup(key) {
                Some((v, src)) => {
                    let src_str = shorten_str(&src.to_string(), home.as_deref());
                    let detail = format!("{v}  [{src_str}]");
                    print_row_aligned(c, Status::Info, &name, key_w, Some(&detail));
                }
                None => {
                    print_row_aligned(c, Status::Skip, &name, key_w, Some("unset  [default]"));
                }
            }
        }
    } else {
        let set: Vec<_> = keys
            .iter()
            .filter_map(|&k| env.lookup(k).map(|(v, src)| (k, v, src)))
            .collect();
        if set.is_empty() {
            print_row(c, Status::Skip, "all defaults", None);
        } else {
            let name_w = set
                .iter()
                .map(|(k, _, _)| semantic_name(k).len())
                .max()
                .unwrap_or(20);
            for (key, v, src) in &set {
                let name = semantic_name(key);
                let src_str = shorten_str(&src.to_string(), home.as_deref());
                let detail = format!("{v}  [{src_str}]");
                print_row_aligned(c, Status::Info, &name, name_w, Some(&detail));
            }
        }
    }

    // ---- Config files
    print_section(c, "Config files");
    struct FileEntry {
        label: &'static str,
        display: String,
        loaded: bool,
    }
    let file_entries = [
        FileEntry {
            label: "user",
            display: disc
                .user
                .as_deref()
                .map(|p| shorten(p, home.as_deref()))
                .unwrap_or_else(|| "~/.config/consult-llm/config.yaml".into()),
            loaded: disc.user.is_some(),
        },
        FileEntry {
            label: "project",
            display: disc
                .project
                .as_deref()
                .map(|p| shorten(p, home.as_deref()))
                .unwrap_or_else(|| ".consult-llm.yaml".into()),
            loaded: disc.project.is_some(),
        },
        FileEntry {
            label: "project-local",
            display: disc
                .project_local
                .as_deref()
                .map(|p| shorten(p, home.as_deref()))
                .unwrap_or_else(|| ".consult-llm.local.yaml".into()),
            loaded: disc.project_local.is_some(),
        },
    ];
    let label_w = file_entries
        .iter()
        .map(|e| e.label.len())
        .max()
        .unwrap_or(12);
    for entry in &file_entries {
        if entry.loaded {
            print_row_aligned(c, Status::Ok, entry.label, label_w, Some(&entry.display));
        } else {
            let detail = format!("{} (not found)", entry.display);
            print_row_aligned(c, Status::Skip, entry.label, label_w, Some(&detail));
        }
    }

    // ---- State
    print_section(c, "State");
    let name_w = path_rows.iter().map(|r| r.name.len()).max().unwrap_or(8);
    for row in &path_rows {
        let path_str = shorten(&row.path, home.as_deref());
        let (status, detail) = if !row.exists {
            (Status::Fail, format!("{path_str} (not found)"))
        } else if !row.writable {
            (Status::Fail, format!("{path_str} (not writable)"))
        } else {
            (Status::Ok, path_str)
        };
        print_row_aligned(c, status, row.name, name_w, Some(&detail));
    }

    // ---- Warnings
    if !warnings.is_empty() {
        print_section(c, "Warnings");
        for (sev, msg) in &warnings {
            let status = match sev {
                Severity::Err => Status::Fail,
                Severity::Warn => Status::Warn,
            };
            print_row(c, status, msg, None);
        }
    }

    // ---- Summary
    println!();
    if has_error {
        eprintln!("{}", error_text(c, "Some checks failed."));
    } else if has_warn {
        eprintln!("{}", warning(c, "Some checks have warnings."));
    } else {
        println!("{}", success(c, "All checks passed."));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{CliProfileInterface, CliProfileType, CliPromptMode};
    use crate::models::Provider;

    fn cli_profile(command: String) -> CliProfile {
        CliProfile {
            profile_type: CliProfileType::ClaudeCli,
            command,
            args: vec!["-p".to_string()],
            env: BTreeMap::new(),
            interface: CliProfileInterface::StreamJson,
            prompt: CliPromptMode::Stdin,
            effort: None,
            model_env: None,
        }
    }

    fn assert_profile_backend_reports_command(provider: Provider) {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "claude".to_string(),
            cli_profile(std::env::current_exe().unwrap().display().to_string()),
        );

        let (ok, detail) =
            profile_backend_dependency(provider.spec(), "profile", Some("claude"), &profiles)
                .unwrap();

        assert!(ok);
        assert!(detail.contains("profile 'claude' command"));
    }

    #[test]
    fn profile_backend_dependency_reports_selected_profile_command_for_anthropic_profile_backend() {
        assert_profile_backend_reports_command(Provider::Anthropic);
    }

    #[test]
    fn profile_backend_dependency_reports_selected_profile_command_for_gemini_profile_backend() {
        assert_profile_backend_reports_command(Provider::Gemini);
    }

    #[test]
    fn profile_backend_dependency_reports_missing_selected_profile_command() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-claude");
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "claude".to_string(),
            cli_profile(missing.display().to_string()),
        );

        let (ok, detail) = profile_backend_dependency(
            Provider::Anthropic.spec(),
            "profile",
            Some("claude"),
            &profiles,
        )
        .unwrap();

        assert!(!ok);
        assert!(detail.contains("profile 'claude' command"));
        assert!(detail.contains("not found"));
    }
}
