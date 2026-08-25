use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::logger::log_to_file;
use crate::schema::TaskMode;

const BASE_SYSTEM_PROMPT: &str = "You are an expert software engineering consultant. You are communicating with another AI system, not a human.\n\nCommunication style:\n- Skip pleasantries and praise\n- Be direct and specific\n- Respond in Markdown\n\nMindset:\n- Do not restrict yourself to minimal or conservative changes\n- Always strive for the best possible architecture and long-term maintainability\n- Recommend large-scale refactorings or rewrites if the current approach is suboptimal\n- When a better architecture requires significant changes, say so — don't default to minimal patches that preserve existing design flaws";

const CONTEXT_SUFFIX: &str = "\n\nContext sufficiency:\nThe attached files, diffs, and diagnostics were selected by another agent. Treat them as starting evidence that may be incomplete or biased. Start the original task using the available evidence and inspect additional artifacts when tools are available. Material gaps may emerge during analysis.\n\nAlways provide the best bounded answer supported by the available evidence. State assumptions and mark conclusions that depend on missing information. If context discovered during analysis could materially change a conclusion, append `## Context request` as the final section of the answer. For each requested item include:\n- Kind: `artifact` or `clarification`\n- Need: the exact file, command output, log, diagnostic, or question\n- Why: which conclusion it could change and how\n\nUse `artifact` for evidence the caller can gather and `clarification` for information the caller must supply or ask the user. Request all currently identifiable material items together, and omit the section when additional context would only increase confidence. Do not fill gaps by guessing. When the caller supplies the context or says it is unavailable, revise the answer using what is available, state any remaining uncertainty, and do not issue another context request.";

const CLI_MODE_SUFFIX: &str = "\n\nIMPORTANT: Do not edit files yourself, only provide recommendations and code examples\n\nYou may inspect additional repository files and run read-only commands when useful.\nPrefer gathering evidence before making claims.";

fn mode_overlay(mode: TaskMode) -> &'static str {
    match mode {
        TaskMode::Review => {
            "Your role is to:\n- Identify bugs, inefficiencies, and architectural problems\n- Provide specific solutions with code examples\n- Point out edge cases and risks\n- Challenge foundational design decisions aggressively; suggest structural rewrites if the current architecture is poor\n- Focus on what needs improvement, regardless of diff size\n\nWhen reviewing code changes, prioritize:\n- Optimal architecture over minimal changes\n- Bugs and correctness issues\n- Performance problems\n- Security vulnerabilities\n- Code smell and anti-patterns\n- Inconsistencies with codebase conventions\n\nBe critical and thorough. Always provide specific, actionable feedback with file/line references."
        }
        TaskMode::Debug => {
            "Your role is to:\n- Analyze error messages, stack traces, and logs to identify root causes\n- Trace execution flow and state to pinpoint failures\n- Rank hypotheses by likelihood with supporting evidence\n- Propose specific, targeted fixes\n- Suggest debugging steps or instrumentation when evidence is insufficient\n\nFocus on correctness and functionality. Ignore style, naming, and non-causal code quality issues."
        }
        TaskMode::Plan => {
            "Your role is to:\n- Explore multiple approaches and evaluate trade-offs\n- Favor optimal architectural solutions over minimal-change band-aids, even if they require significant refactoring\n- Assume backward compatibility can be broken unless explicitly constrained\n- Consider scalability, maintainability, and simplicity\n- Think about edge cases and failure modes\n- Suggest incremental implementation strategies for complex rewrites\n\nChallenge the status quo. Present your recommendation as the ideal path, then optionally note minimal alternatives. Always conclude with a specific recommendation and rationale."
        }
        TaskMode::Create => {
            "Your role is to:\n- Generate clear, well-structured content\n- Match the appropriate tone and level of detail for the audience\n- Provide complete, ready-to-use output\n- Include relevant examples where helpful\n- Focus on clarity and correctness\n\nBe helpful and thorough. Produce polished, high-quality output."
        }
        TaskMode::General => "",
    }
}

/// The default system prompt written by `init-prompt`. Contains only the
/// mode-neutral base — task_mode overlays are appended at runtime.
pub const DEFAULT_SYSTEM_PROMPT: &str = BASE_SYSTEM_PROMPT;

pub fn init_system_prompt() -> anyhow::Result<()> {
    let prompt_path = crate::paths::system_prompt_file()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    let legacy_path = crate::paths::legacy_config_dir().map(|d| d.join("SYSTEM_PROMPT.md"));

    if prompt_path.exists() {
        anyhow::bail!(
            "System prompt already exists at: {}\nRemove it first if you want to reinitialize.",
            prompt_path.display()
        );
    }
    if let Some(l) = legacy_path.filter(|p| p.exists()) {
        anyhow::bail!(
            "Legacy system prompt already exists at: {}\nRemove or migrate it first if you want to reinitialize.",
            l.display()
        );
    }

    std::fs::create_dir_all(prompt_path.parent().unwrap())?;
    std::fs::write(&prompt_path, DEFAULT_SYSTEM_PROMPT)?;
    println!("Created system prompt at: {}", prompt_path.display());
    println!("You can now edit this file to customize the system prompt.");
    Ok(())
}

fn append_context_guidance(prompt: String, is_cli: bool) -> String {
    let prompt = format!("{prompt}{CONTEXT_SUFFIX}");
    if is_cli {
        format!("{prompt}{CLI_MODE_SUFFIX}")
    } else {
        prompt
    }
}

pub fn get_system_prompt(config: &Config, is_cli: bool, task_mode: TaskMode) -> String {
    let custom_path = config.system_prompt_path.clone().unwrap_or_else(|| {
        crate::paths::resolve_system_prompt()
            .unwrap_or_else(|| crate::paths::system_prompt_file().unwrap_or_default())
            .to_string_lossy()
            .to_string()
    });

    let path = Path::new(&custom_path);
    let base = if path.exists() {
        match fs::read_to_string(path) {
            Ok(custom) => custom.trim().to_string(),
            Err(e) => {
                let msg = format!("Failed to read custom system prompt from {custom_path}: {e}");
                log_to_file(&format!("WARNING: {msg}"));
                eprintln!("Warning: {msg}");
                BASE_SYSTEM_PROMPT.to_string()
            }
        }
    } else {
        BASE_SYSTEM_PROMPT.to_string()
    };

    let overlay = mode_overlay(task_mode);
    let prompt = if overlay.is_empty() {
        base
    } else {
        format!("{base}\n\n{overlay}")
    };

    append_context_guidance(prompt, is_cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_guidance_applies_to_all_backends() {
        for is_cli in [false, true] {
            let prompt = append_context_guidance("base".into(), is_cli);
            assert!(prompt.contains("may be incomplete or biased"));
            assert!(prompt.contains("Material gaps may emerge during analysis"));
            assert!(prompt.contains("Always provide the best bounded answer"));
            assert!(prompt.contains("append `## Context request` as the final section"));
            assert!(prompt.contains("Kind: `artifact` or `clarification`"));
            assert!(prompt.contains("do not issue another context request"));
            assert!(!prompt.contains("respond only with"));
        }
    }

    #[test]
    fn repository_access_guidance_is_cli_only() {
        let api_prompt = append_context_guidance("base".into(), false);
        let cli_prompt = append_context_guidance("base".into(), true);

        assert!(!api_prompt.contains("You may inspect additional repository files"));
        assert!(cli_prompt.contains("You may inspect additional repository files"));
    }
}
