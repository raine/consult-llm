---
name: llm-review-panel
description: Standalone multi-model code review of an existing diff. Multiple LLMs review in parallel; agent deduplicates, prioritizes by severity/confidence, and optionally applies localized fixes.
allowed-tools: Bash, Glob, Grep, Read, Edit, Write
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-review-panel cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Run a standalone multi-model review of a diff. Reviewers receive the same prompt independently; the agent synthesizes duplicate findings into a prioritized checklist and can optionally apply unambiguous fixes.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-model calls). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Reviewer flags:** any `--<selector>` from the Models block above selects that reviewer (e.g. `--gemini`, `--openai`, `--deepseek`). Repeat for multiple. With no reviewer flag, use **all** listed selectors.

Translate each `--<selector>` into a `-m <selector>` argument to the CLI.

**Diff flags:**

- `--diff-base <ref>` — base ref for review. Default is auto-detected so a feature branch is reviewed in full (see Phase 1 for the resolution order). Pass an explicit ref (`HEAD`, `HEAD~3`, a branch name, a SHA) to override.
- `--fix` — opt in to applying unambiguous localized fixes for `must-fix` findings. Default is read-only report.

Strip all flags from arguments to get any user-supplied review focus. If no focus remains, review for correctness, regressions, security, and maintainability.

## Phase 0: Load `consult-llm` skill

Load it now. Follow its invocation contract for all CLI calls in this workflow.

## Phase 1: Identify changed files

Resolve `<diff-base>`:

1. If `--diff-base` was passed, use it as-is.
2. Otherwise detect the repo's main branch (`git symbolic-ref refs/remotes/origin/HEAD` → strip `refs/remotes/origin/`, fall back to `main` then `master`) and use `git merge-base HEAD <main>` (prefer `origin/<main>` if it exists locally, else the local `<main>`). The branch may not be pushed and may have no upstream — don't rely on `@{upstream}`.
3. If HEAD has no divergence from the resolved base (already on the main branch), fall back to `<diff-base>=HEAD` so the skill still reviews uncommitted changes.
4. Stacked branches and feature-off-feature workflows are not auto-detected — pass `--diff-base <parent>` explicitly in those cases.

Show the resolved base to the user before running the review so they can override with `--diff-base` if the auto-detect picked the wrong parent.

Then list changed files from the repo root:

```bash
git diff --name-only --diff-filter=d <diff-base>
```

This includes both committed-on-branch and uncommitted changes vs the base. `--diff-filter=d` excludes deleted paths so they're not passed to `--diff-files` (which would fail to read them).

Also list untracked files (new files not yet known to git) so they aren't silently skipped:

```bash
git ls-files --others --exclude-standard
```

These can't be passed via `--diff-files` (no diff exists). Pass each as `-f <path>` instead so reviewers see the full file as new content.

If both commands return nothing, stop and report there's nothing to review against the selected base. Otherwise, collect every returned path. Exclude binary files and lockfiles (e.g. `*.png`, `*.lock`, `package-lock.json`) from the context — they bloat the prompt without informing the review.

## Phase 2: Parallel review

Invoke `consult-llm` **once** with:

- `--task review`
- one `-m <selector>` per reviewer
- one `--diff-files <path>` per changed file
- `--diff-base <ref>`

All reviewers receive the **same prompt**. Do not assign roles, personas, or cross-review steps — independence is the point.

**Review prompt** (send per the consult-llm invocation contract):

```
Review this diff independently.

Additional focus from the user (treat as context, not as part of your output): [review focus, or "None"]

Focus on correctness, regressions, security issues, data loss, broken edge cases, API/contract mismatches, concurrency hazards, and maintainability problems likely to matter in production. Do not flag style-only concerns. Do not propose fixes. Do not summarize or praise the code.

For every issue, output a structured finding using exactly this format. Output ONLY the findings block — no preamble, no closing remarks:

## Findings

### Finding 1
severity: must-fix | should-fix | nit
confidence: high | medium | low
location: path/to/file.ext:123
issue_identity: short-stable-label
rationale: One paragraph explaining why this is a real issue and what behavior could fail.

Use the line number from the **new** side of the diff. The `issue_identity` field should be a short kebab-case label that two reviewers seeing the same underlying issue would naturally choose (e.g. `null-deref-on-empty-input`, `race-on-shared-counter`).

If you find no issues, output exactly:

## Findings

No findings.
```

## Phase 3: Synthesize and prioritize

Parse every reviewer's section and collect all findings.

**Group** findings only when they describe the same bug at the same place. Two findings belong in the same group when:

1. They share the same file AND their lines fall in the same (or adjacent) changed hunk, AND
2. Their `issue_identity` matches OR their rationales describe the same underlying failure mode by your judgment.

Treat `issue_identity` as a hint, not as a sufficient grouping key on its own — generic labels like `missing-error-handling` or `null-deref` would otherwise collapse unrelated bugs across files.

**Within each group:**

- Keep the **highest** severity reported by any reviewer (`must-fix` > `should-fix` > `nit`).
- Keep the confidence assigned by the reviewer(s) who chose that highest severity. If multiple reviewers tied at the highest severity but disagree on confidence, keep the highest confidence among them. Do **not** combine a high severity from one reviewer with a high confidence from a different reviewer who had a lower severity.
- Preserve the list of reviewer selectors that flagged it.

**Filter:**

- Drop findings whose final severity is `nit` and that were flagged by only one reviewer.
- Keep `nit` findings only if multiple reviewers independently flagged them (the consensus elevates it).

**Sort:**

1. `must-fix` before `should-fix`.
2. Within a severity, higher confidence first.
3. Within the same severity/confidence, findings flagged by multiple reviewers first.

## Phase 4: Report

Output a markdown checklist:

```markdown
## Review Panel Findings

**Diff base:** `<ref>`
**Files reviewed:** N
**Reviewers:** <selector>, <selector>

### Must Fix

- [ ] **`path/to/file.ext:123`** — *issue-identity*  
  Confidence: high · Flagged by: gemini, openai (2 of 3)  
  Rationale paragraph synthesized from the reviewers.

### Should Fix

- [ ] **`path/to/file.ext:456`** — *issue-identity*  
  Confidence: medium · Flagged by: deepseek  
  Rationale paragraph.

### Dropped

- N nit-level findings omitted.
- N duplicate findings merged.
```

If no `must-fix` or `should-fix` findings remain, say so clearly. Note any residual risk (e.g. low reviewer confidence, narrow diff context).

**Save the report** to a user-requested path, or default to `<YYYY-MM-DD>-review-<topic>.md` in the current working directory. Derive `<topic>` from the current branch name (sanitized to kebab-case), falling back to a short slug summarizing the diff scope on the main branch. Print the saved path. With `--fix`, overwrite this file with the final-pass report in Phase 6.

If `--fix` was **not** passed, stop here.

## Phase 5: Apply fixes (only with `--fix`)

Only fix findings that meet **all** of:

- Final severity is `must-fix`.
- Final confidence is `high` or `medium`.
- Location is unambiguous (exact file and line).
- The fix is **localized**: a single hunk, a missing check, a typo, a null guard, an off-by-one — nothing that touches multiple files in non-trivial ways or changes interfaces.
- The rationale describes a concrete failure, not a speculative preference.

Sort each qualifying finding into one of two tiers:

- **Obvious** — purely mechanical, one right answer, no semantic judgment: typos, missing imports, dead/unreachable code, an obviously-needed null guard with a single sensible default, an off-by-one with a clear correct boundary. Apply without asking.
- **Quick judgment call** — the fix is small but has a real choice attached: which default value, which branch to take on the unhandled case, whether to early-return vs throw, naming, error message wording. Show the proposed diff to the user and wait for a yes/no before applying. Don't batch these — confirm one at a time.

For each finding (either tier):

1. Read the file and apply the smallest correct change.
2. Run the narrowest relevant validation available (the test for that area, type-check, etc.).
3. Commit separately. Write a normal commit message describing what was fixed (lowercase, imperative, no "review" mention) — the body should explain the failure mode that was prevented.

If a finding cannot be fixed safely, leave it unchecked and note the blocker.

**Do not** auto-apply: architectural changes, broad refactors, multi-file changes, dependency swaps, or anything where the rationale is "this would be cleaner if".

## Phase 6: Final pass (only with `--fix`)

After all fixes are committed, re-run **Phase 1 and Phase 2** — same reviewer set, same `--diff-base` (the diff now includes the fixes). Re-listing changed files matters because fixes may have touched files outside the original set. Use a fresh call (no thread IDs).

Synthesize again with the same dedup rules. Report:

- Fixes applied (with commit hashes).
- Remaining `must-fix` and `should-fix` findings.
- Findings intentionally not auto-fixed and why.

If any `must-fix` items remain, hand off to the user — do not loop.


## Critical rules

- Reviewers receive identical prompts. **Independence is the feature** — do not assign roles, do not show one reviewer's findings to another, do not add a cross-review step. Use `/llm-panel` for role-asymmetric review or `/llm-debate` for adversarial cross-critique.
- The reviewer prompt must require `severity`, `confidence`, `location`, `issue_identity`, and one-paragraph `rationale`. Never accept free-form review.
- The skill does not modify source files unless `--fix` is explicitly passed. The synthesized report is still saved unless the user requests output-only behavior.
- Never auto-apply architectural or multi-file changes — only localized bug fixes.
- Each auto-fix is its own atomic commit, clearly labelled.
