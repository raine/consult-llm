---
name: implement
description: One-unit implementation workflow using presets. It writes a compact note or rich code-bearing plan, optionally consults external LLMs, optionally delegates execution to sideagent, verifies, commits, and summarizes.
allowed-tools: Bash, Glob, Grep, Read, Edit, Write
---

Implement one bounded unit of work. Optimize for cheap execution by doing enough reasoning before edits that the executor can follow concrete instructions instead of inventing design, APIs, tests, or control flow.

## Argument handling

Arguments are `$ARGUMENTS`.

Parse these flags before starting:

- `--preset light|standard|design|strict`: workflow preset. Default: `standard`.
- `--planning note|rich|consult-first`: override compiled planning mode.
- `--plan-review none|narrow|full`: override compiled plan review mode.
- `--executor self|sideagent`: override compiled executor.
- `--verification light|full`: override compiled verification.
- `--parent-plan <path>`: path to a master plan or phase brief.
- `--reviewer <selector>`: reviewer selector for consult-llm. Repeatable.
- `--reviewers <selector,selector>`: comma-separated reviewer selectors.
- `--validation <command>`: expected validation command.

Everything else is the implementation request. Preserve it as the task statement.

If no preset is provided, use `standard`. Apply explicit compiled-field overrides after resolving the preset.

Preset compilation:

```yaml
light:
  planning: note
  plan_review: none
  executor: self
  verification: light
standard:
  planning: rich
  plan_review: narrow
  executor: sideagent
  verification: light
design:
  planning: consult-first
  plan_review: full
  executor: sideagent
  verification: light
strict:
  planning: rich
  plan_review: full
  executor: sideagent
  verification: full
```

Do not ask the user during the workflow unless there is no safe way to continue. Use best judgment.

## Required artifacts

Write artifacts under `history/` using today's date prefix.

Required:

- implementation note or rich implementation plan
- final summary

Required only when `--parent-plan` is provided or the caller explicitly requests one:

- result sentinel

Optional:

- external proposal capture
- external review capture
- debug notes

Artifacts under `history/` are workflow records, not repository changes. Do not stage or commit them, even when they are required for the workflow or requested as sentinels. Leave them as uncommitted files unless the user explicitly asks to commit history artifacts.

Do not create a feedback ledger. Before implementation starts, when review changes a plan, update the plan directly. After implementation starts, or when the worktree already contains source changes for this unit, do not rewrite the plan for minor review feedback. Append a short `Review feedback` section to the plan or note instead, then apply the needed amendments manually.

## Phase A: snapshot and context

1. Record the start commit:

   ```bash
   git rev-parse HEAD
   ```

2. Check the working tree before editing:

   ```bash
   git status --short
   ```

   Stop if unrelated uncommitted changes would make the work unsafe. Existing user changes may be present. Do not overwrite them.

3. Gather context:

   - Use Glob and Grep to find relevant files, callers, tests, fixtures, generated code, and configuration.
   - Read enough code to understand current behavior and project idioms.
   - Identify validation commands. Prefer project-local commands from docs, justfiles, package scripts, or existing test patterns.
   - If a parent plan path is provided, read it and treat it as authoritative for scope.
   - Keep scope to one implementation unit.

4. Choose the validation command. If `--validation` was provided, use it unless it is plainly wrong. Otherwise choose the narrowest useful command plus any required project check.

## Phase B: plan

### Light planning

For `planning: note`, write a compact implementation note:

```markdown
# <Feature> Implementation Note

**Goal:** <one sentence>
**Preset:** light
**Compiled fields:** planning=note, plan_review=none, executor=self, verification=light
**Parent plan:** <path or n/a>
**Start commit:** <sha>
**Validation:** `<command>`

## Checklist

- <concrete step>

## Acceptance criteria

- <criterion>
```

The note must include enough accountability to verify the result: checklist, acceptance criteria, and validation command.

### Rich planning

For `planning: rich`, research first and write a rich code-bearing plan:

````markdown
# <Feature> Implementation Plan

**Goal:** <one sentence>
**Preset:** standard | design | strict
**Compiled fields:** planning=<mode>, plan_review=<mode>, executor=<mode>, verification=<mode>
**Parent plan:** <path or n/a>
**Start commit:** <sha>
**Approach:** <2-3 sentences>
**Validation:** `<command>`

## Requirements

- In scope:
  - <item>
- Out of scope:
  - <item>
- Acceptance criteria:
  - <criterion>

## Context

- `<file>`: <relevant behavior or pattern>

## Tasks

### Task 1: <short description>

**Files:**

- Create: `path/to/file`
- Modify: `path/to/file` lines 10-40

**Steps:**

1. Write or update the failing test.
2. Run `<focused test>` and confirm the expected failure.
3. Apply the source change.
4. Run `<focused test>` and confirm it passes.

**Code:**

```language
actual code, not placeholders
```

**Tests:**

```language
actual test code, not placeholders
```

**Validates:** <acceptance criterion>

## Validation

1. `<focused command>`
2. `<final command>`
````

Rules for rich plans:

- Include exact files and paths.
- Break work into small ordered tasks.
- Include test-first steps whenever a test can be written.
- Include actual code blocks for non-trivial source edits.
- Include actual code blocks for non-trivial test edits.
- Omit code only for mechanical replacements or generated content.
- Prefer existing project patterns over new abstractions.
- Include constraints from the parent plan or phase brief.
- Do not include speculative future work.
- Do not include rollout notes or transient comments in code.

### Consult-first planning

For `planning: consult-first`, load the `consult-llm` skill before any consult-llm CLI call. Then gather factual context and ask external LLMs for plan proposals before synthesizing the rich plan.

Use this prompt shape:

```text
We need an implementation plan for one bounded code change.

Task:
<task statement>

Scope constraints:
<parent plan constraints, paths, acceptance criteria, out-of-scope items>

Relevant facts from source:
<brief factual summary with file paths>

Please propose a concrete implementation plan. Include exact files, ordered tasks, tests, validation commands, compatibility concerns, and edge cases. Include code snippets where they are important. Do not assume access to files beyond the attached context.
```

Call consult-llm with file context, a quoted heredoc terminator, and a 10 minute timeout:

```bash
cat <<'__CONSULT_LLM_END__' | consult-llm --task plan -f <file> -f <file>
<prompt body>
__CONSULT_LLM_END__
```

If reviewer selectors were supplied, pass one `-m <selector>` per selector. Otherwise omit `-m` so consult-llm applies configured `default_models` when present, then `default_model` or fallback. Always set Bash `timeout` to `600000`.

Synthesize one rich implementation plan from the proposals and source evidence. Do not paste proposals blindly. The plan is the contract for execution.

## Phase C: review the plan

Skip this phase for `plan_review: none`.

For `plan_review: narrow` or `full`, load the `consult-llm` skill before calling consult-llm. Attach the plan, relevant source files, tests, and parent plan if any. Use `--task review`. Use supplied reviewer selectors when present. Use the quoted heredoc terminator `__CONSULT_LLM_END__` and Bash timeout `600000`.

### Narrow plan review prompt

```text
Review this implementation plan only for handoff quality.

Check whether a cheaper execution agent can implement it correctly without inventing missing details. Focus on exact files, task order, code snippets, test-first steps, acceptance coverage, validation commands, phase scope boundaries, and vague instructions.

Do not redesign the architecture unless the plan is plainly inconsistent or impossible.

If something is missing, provide concrete replacement text, missing code blocks, missing tests, or task edits. Return only must-fix and should-fix feedback.
```

### Full plan review prompt

```text
Review this implementation plan for correctness, risk, and handoff quality.

Include the narrow handoff checks: exact files, task order, code snippets, test-first steps, acceptance coverage, validation commands, phase scope boundaries, and vague instructions.

Also check architecture, module boundaries, compatibility, public contracts, regression risk, edge cases, invariants, test adequacy, and security where relevant.

If something is missing or wrong, provide concrete replacement text, missing code blocks, missing tests, or task edits. Return only actionable feedback.
```

Apply accepted feedback according to implementation state:

- Before implementation starts, edit the plan directly when review feedback changes the execution contract.
- After implementation starts, or when the worktree already contains source changes for this unit, do not churn the plan for minor amendments. Append a compact `Review feedback` section to the plan or note with the actionable items, then apply them manually during validation.
- If feedback is wrong, ignore it.
- If review materially changes the design before implementation starts, add this optional section:

```markdown
## Review changes applied

- <short bullet>
```

Do not create a large feedback ledger.

## Phase D: execute

### Self execution

For `executor: self`, implement directly from the note or plan.

Rules:

- Implement tasks in order.
- Follow test-first steps when present.
- Stop if the note or plan is wrong or underspecified in a way that changes scope.
- Make small obvious corrections directly and update the note or plan.
- Do not overwrite user changes.

### Sideagent execution

For `executor: sideagent`, pass the execution prompt directly to `sideagent` on stdin. Do not write the sideagent prompt to `history/` or any other file. If the exact sideagent profile is unspecified, use the configured default profile. Do not invent a profile.

Prompt format:

```markdown
# Sideagent Execution Prompt

You are executing one bounded implementation plan in this repository.

## Instructions

- Follow the plan exactly unless it is wrong or unsafe.
- Implement tasks in order.
- Follow test-first steps when present.
- Keep scope to this phase only.
- If a small correction is obvious, update the plan directly and continue.
- If a correction changes design or scope, stop and report the blocker.
- Do not ask the user questions.
- Do not overwrite user changes.
- Run the validation commands.
- Do not stage or commit `history/` artifacts.
- Write the result sentinel described below only when `--parent-plan` is provided or the caller explicitly requests one.

## Task

<task statement>

## Plan

<plan path>

## Parent context

<parent plan path or n/a>

## Result sentinel

When `--parent-plan` is provided or the caller explicitly requests one, write the requested result sentinel with the sentinel format from the plan. For standalone `/implement` runs, do not write a result sentinel.
```

Load the `sideagent` skill before invoking sideagent. Follow that skill's current invocation contract. Use the configured default profile unless the user or local configuration names a specific profile. Do not invent a profile.

Use sideagent for the initial implementation pass only. If the worktree already contains a substantial implementation diff for this unit, do not invoke sideagent again for review feedback, validation fixes, or minor plan amendments. The host agent owns those follow-up edits manually. Inspect the existing diff, apply localized fixes, and validate.

If sideagent fails, inspect its output. Fix only local and obvious issues. Otherwise stop and summarize the blocker.

## Phase E: validate and verify

Run the selected validation command. Also run any focused tests from the plan.

For `verification: light`, check:

- validation passed
- when a result sentinel is required, it exists and reports success
- diff follows the note or plan
- acceptance criteria are plausibly covered
- no obvious scope drift, regression, or unsafe overwrite occurred

For `verification: full`, check all light verification items plus:

- compatibility with existing callers
- public contract consistency
- edge cases and error paths
- test adequacy against acceptance criteria
- regression risk across touched modules
- security issues when relevant

Only auto-fix localized must-fix issues with one clear answer. If a fix requires redesign or scope changes, stop and report the blocker.

## Result sentinel

Standalone `/implement` runs do not write a result sentinel. When `--parent-plan` is provided or the caller explicitly requests one, write or confirm a result sentinel under `history/` before committing:

```markdown
# Implementation Result: <feature>

status: success | blocked | failed
preset: light | standard | design | strict
planning: note | rich | consult-first
plan_review: none | narrow | full
executor: self | sideagent
verification: light | full
start_commit: <sha>
end_commit: <sha or pending>
commit: <sha or pending>
plan_or_note: <path>
validation: <command>
validation_status: passed | failed | skipped

## Summary

- <what changed>

## Acceptance

- <criterion>: met | not met | unknown

## Blockers

- <blocker or none>
```

Update `end_commit` and `commit` after committing. This update still stays uncommitted when the sentinel is under `history/`.

When no result sentinel is required, include the same status, validation, acceptance, and blocker facts in the final summary instead.

## Phase F: commit and summarize

Commit when validation passes, verification passes, no blockers remain, and you are confident the change is done.

Commit rules:

- Do not stage or commit `history/` artifacts.

After committing, print the final summary:

```markdown
## Result

- Preset: <preset>
- Plan or note: `<path>`
- Review: <none | narrow passed | full passed | blocker>
- Executor: <self | sideagent>
- Validation: `<command>` <passed | failed | skipped>
- Verification: <light | full> <passed | failed>
- Commit: `<sha>`
- Sentinel: `<path or n/a>`
- Blockers: <none or list>
```
