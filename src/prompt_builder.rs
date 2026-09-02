use std::fmt::Write;

/// Pick a backtick fence longer than any run of backticks inside `content`,
/// with a minimum of three. CommonMark requires the closing fence to be at
/// least as long as the opening fence, so `n+1` backticks reliably wrap any
/// content that itself uses up to `n` consecutive backticks.
fn fence_for(content: &str) -> String {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            cur += 1;
            if cur > max_run {
                max_run = cur;
            }
        } else {
            cur = 0;
        }
    }
    "`".repeat(max_run.max(2) + 1)
}

pub fn build_git_diff_inspection_prompt(
    user_prompt: &str,
    repo_path: Option<&str>,
    base_ref: &str,
    files: &[String],
) -> String {
    let spec = serde_json::json!({
        "repo_path": repo_path.unwrap_or("."),
        "base_ref": base_ref,
        "files": files,
    });
    format!(
        "## Git Diff\nInspect the requested git diff with read-only commands before answering. Resolve relative repository paths from the current working directory. Use this scope for the comparison:\n\n```json\n{spec}\n```\n\n{user_prompt}"
    )
}

pub fn build_prompt(
    user_prompt: &str,
    files: &[(String, String)], // (path, content)
    git_diff_output: Option<&str>,
) -> String {
    let capacity = user_prompt.len()
        + git_diff_output.map_or(0, |d| d.len() + 30)
        + files
            .iter()
            .map(|(p, c)| p.len() + c.len() + 30)
            .sum::<usize>();
    let mut out = String::with_capacity(capacity);

    if let Some(diff) = git_diff_output
        && !diff.trim().is_empty()
    {
        let fence = fence_for(diff);
        let _ = write!(out, "## Git Diff\n{fence}diff\n");
        out.push_str(diff);
        let _ = write!(out, "\n{fence}\n\n");
    }

    if !files.is_empty() {
        out.push_str("## Relevant Files\n\n");
        for (path, content) in files {
            let fence = fence_for(content);
            let _ = write!(out, "### File: {path}\n{fence}\n");
            out.push_str(content);
            let _ = write!(out, "\n{fence}\n\n");
        }
    }

    out.push_str(user_prompt);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_diff_inspection_prompt_describes_diff_without_embedding_it() {
        let prompt = build_git_diff_inspection_prompt(
            "review this",
            Some("../project"),
            "origin/main",
            &["src/lib.rs".into(), "tests/review.rs".into()],
        );

        assert!(prompt.contains("Inspect the requested git diff"));
        assert!(prompt.contains(r#""repo_path":"../project""#));
        assert!(prompt.contains(r#""base_ref":"origin/main""#));
        assert!(prompt.contains(r#""files":["src/lib.rs","tests/review.rs"]"#));
        assert!(prompt.ends_with("review this"));
        assert!(!prompt.contains("diff --git"));
    }

    #[test]
    fn fence_avoids_collision_with_inner_backticks() {
        let content = "before\n```\ninner\n```\nafter";
        let prompt = build_prompt("ask", &[("f.md".into(), content.into())], None);
        // Outer fence must be 4 backticks since content has a run of 3.
        assert!(prompt.contains("````\nbefore"));
        assert!(prompt.contains("after\n````\n"));
    }

    #[test]
    fn fence_for_plain_content_uses_three_backticks() {
        assert_eq!(fence_for("no ticks here"), "```");
    }

    #[test]
    fn fence_for_handles_long_runs() {
        assert_eq!(fence_for("abc ```` def"), "`````");
    }
}
