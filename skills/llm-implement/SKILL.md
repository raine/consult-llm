---
name: llm-implement
description: Autonomously plan and implement a task with external LLM review. Writes a behavioral spec, runs an evidence-gated plan review (premortem + independent alternative), applies feedback through a decision ledger, implements with a triggered debug loop, and finishes with an evidence-gated red-team pass. No user interaction.
allowed-tools: Bash, Glob, Grep, Read, Edit, Write
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-implement cli_version=3.0.30 schema_version=1; do not hand-edit. -->
End-to-end autonomous workflow: spec → plan → review → implement → red-team → summary. Reviewers must produce structured findings with concrete evidence; conflicts are resolved through a written decision ledger, not silent agent judgment. Use this when you want a single command to take a task from "describe it" to "committed implementation" with external-LLM gating at the right checkpoints. For advisory-only review of an existing diff, use `/llm-review-panel` or `/llm-panel`.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn, `--run`). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Reviewer flags:** any `--<selector>` from the Models block selects a reviewer (e.g. `--gemini`, `--openai`, `--deepseek`). Repeat for multiple. With no selector flag, use **all** listed selectors in parallel. Translate each `--<selector>` into a `-m <selector>` argument.

**Rigor knob:**

- `--rigor lite|standard|deep` — default `standard`.
  - `lite` — single shared-prompt review, no premortem section, generic final review, no debug consult. Use this (not "skip review") when the task is small.
  - `standard` — shared-prompt review with structured premortem and independent-alternative sections, evidence-gated final review with attack lenses, debug consult after 2 failed hypotheses.
  - `deep` — Phase 3 and Phase 6 use `--run` with role-asymmetric prompts (security, test-strategist, data-integrity, fuzzing-strategist). Same number of reviewer calls but each model gets a focused persona.

**Mode flags:**

- `--rounds N` — repeat the review-refine cycle (Phases 3–4) N times. Default `1`. Max `3`.
- `--dry-run` — stop after Phase 4. Plan and ledger are saved; nothing is implemented or reviewed against a diff.
- `--no-review` — skip Phases 3, 4, and 6. Plan, implement, summarize. Useful for very small tasks where review overhead exceeds value.
- `--skip-final` — implement but skip the Phase 6 red-team pass.
- `--commit` — commit each completed task during Phase 5. Default is to leave changes uncommitted and report a single summary so the user can choose the final commit structure.
- `--diff-base <ref>` — override the auto-snapshot of `START_HEAD` for the Phase 6 review (rarely needed).

Strip all flags from arguments to get the task description.

## Phase 0: Load `consult-llm` skill, snapshot state

Load the skill. Then capture working-tree state so Phase 6 has a stable base:

```bash
git rev-parse HEAD                    # store as START_HEAD
git status --short                    # record dirty files
git symbolic-ref --short HEAD         # current branch
```

If `git status --short` shows changes outside the files you expect to modify, stop and report the dirty state. The user must clean or stash before `implement` runs — otherwise the Phase 6 diff will mix in unrelated work and the red-team pass becomes unreliable. (Do not stash automatically.)

If reviewer count or rigor level resolves to no available models, stop with a clear error.

## Phase 1: Gather context

Use Glob, Grep, Read to map the task surface. Make reasonable assumptions — do **not** ask the user clarifying questions.

Capture:

- Files and modules involved; existing patterns and conventions.
- Public interfaces, dependents, and migration concerns.
- The repository's validation contract: how do tests, type-checks, and lints run? Look for agent instructions, README/release documentation, `justfile`, `Makefile`, `package.json`, and `pyproject.toml`. Record the canonical command (for example `just check`) — Phase 5 must run it before declaring done.

Select the source files most relevant to the change. Keep the set small — quality over quantity. These files are passed as shared `-f <path>` to every reviewer call.

## Phase 2: Write the spec and plan

Save a single artifact to a user-requested path, or default to `<YYYY-MM-DD>-plan-<topic>.md` in the current working directory. Derive `<topic>` from the task description (kebab-case, short).

The plan **must** include both the behavioral spec and the implementation tasks. The spec is what reviewers and the final red-team pass will verify against — without it, tests rationalize whatever code got written instead of checking intent.

````markdown
# <Feature> Plan

**Goal:** <one sentence>
**Approach:** <2–3 sentences>
**Assumptions:** <list assumptions made without asking the user>
**Validation command:** `<e.g. just check>`

---

## Behavioral Spec

**In scope:**
- ...

**Out of scope (non-goals):**
- ...

**Acceptance criteria** — Given/When/Then:
- Given <precondition>, when <action>, then <observable outcome>.
- ...

**Invariants** — must always hold:
- ...

**Test matrix:**

| # | Scenario | Expected behavior | Test file/command | Required before implementation? |
| - | -------- | ----------------- | ----------------- | ------------------------------- |

**Rollback** — required when the change touches schema, on-disk format, or a public API contract; otherwise omit:
- Rollback steps:
- Data compatibility:
- Rollback window:

---

## Tasks

### Task 1 — <short description>

**Files:**
- Create: `exact/path.ext`
- Modify: `exact/path.ext` (lines 123–145)

**Steps:**
1. <specific action>
2. <specific action>

**Code:**
```language
// real code, not placeholders
```

**Verifies acceptance criteria:** #1, #3

---
````

Guidelines:

- Exact file paths; never "somewhere in src/".
- Each task small (2–5 minutes of work) and tied to one or more acceptance criteria.
- Show real code in the snippets.
- DRY, YAGNI — only what the spec demands.

## Phase 3: Plan review

Skip only if `--no-review` was passed. Reviewers receive the plan file and the relevant source files and must produce structured output — never accept free-form review.

### Standard rigor — shared prompt to all reviewers

Invoke `consult-llm` once with one `-m <selector>` per reviewer, `-f <plan path>`, and `-f <relevant source>`. Send the prompt below on stdin per the `consult-llm` invocation contract (heredoc, terminator, timeout). Capture the `[thread_id:group_xxx]` from line 1 of the response — it's needed for `--rounds` and Phase 6.

```
Review this implementation plan against the attached source context.

Output exactly these sections in this order. Do not add preamble or closing remarks.

## Spec Check
List acceptance criteria that are missing, ambiguous, or untestable. Flag invariants that the plan does not preserve. If the spec is sufficient, write "Spec sufficient."

## Independent Alternative
Before critiquing the proposed tasks, state in 3–5 sentences the approach you would choose given the spec and the source context alone. Note any material divergence from the proposed plan.

## Premortem
Assume this plan ships and fails in production within six months. List the top 3 failure modes. For each:

- failure_mode: <concrete failure with a trigger, not a category>
- impact: low | medium | high
- probability: low | medium | high
- evidence: <what in the plan or source supports this risk>
- current_mitigation: <quote the plan or "none">
- mitigation_sufficient: yes | no
- required_plan_change_or_test: <specific addition that closes the gap>

Only report failures with a concrete trigger and measurable impact.

## Plan Findings
Issues that should change the plan, spec, or test matrix. For each:

- severity: must-fix | should-fix | optional
- issue_identity: <short kebab-case label two reviewers would naturally choose>
- location_or_task: <plan section, task number, or file:line>
- rationale: <why this is a real problem>
- recommended_change: <specific edit>
```

### Lite rigor

Same single call, but drop the `## Premortem` and `## Independent Alternative` sections from the prompt. Keep `Spec Check` and `Plan Findings`.

### Deep rigor

Use `consult-llm --run` with one role prompt per reviewer (mirrors `skills/llm-panel`):

| Role | Lens |
| ---- | ---- |
| `architect` | Plan structure, coupling, scope, sequencing |
| `security` | Auth, data exposure, integrity, abuse |
| `test-strategist` | Acceptance-criteria coverage, test matrix gaps, oracle quality |
| `data-integrity` | Schema, migrations, ordering, consistency (only if the diff touches data) |
| `fuzzing-strategist` | Boundary inputs, malformed data, adversarial sequences |

Pick 3–5 roles to fit the task. Each role still produces the four sections above, biased toward its lens. Roles map positionally to selectors; each role must end up on a distinct resolved model (see `llm-panel/SKILL.md` fail-fast rules).

## Phase 4: Apply feedback through the ledger

### Verify each finding before adopting it

Reviewers hallucinate, misread the plan, and inflate corner cases. Treat every finding — must-fix included — as a *claim that needs evidence* before it earns a plan change. Reviewer severity is advisory; the agent owns the call after verification.

For each finding, pick the cheapest method that actually proves or disproves the claim:

- **Plan claims** ("acceptance criterion X is missing", "task 3 contradicts task 5") — re-read the cited plan section and confirm.
- **Source claims** ("the existing pattern uses X", "module Y already does Z") — read the cited file and confirm against current code, not memory.
- **Library/API claims** ("SDK Foo doesn't support Y", "this method throws on null") — verify against the library source or official docs. Use `gh search code` for usage-pattern lookups, `Grep` in `node_modules`/vendored deps for the actual implementation, or run a tiny throwaway script (`/tmp/verify-*.{sh,ts,py}`) that isolates the specific behavior.
- **Premortem claims** — confirm the described trigger actually occurs in the planned design, not in some adjacent shape. A failure mode that requires conditions the design rules out is not a real risk.

For each finding, classify:

- **Confirmed and worth fixing** — issue is real and the fix is proportionate. Adopt.
- **Confirmed but YAGNI** — issue is real but the trigger requires contrived inputs no caller produces, or timing windows that don't occur in practice, and the fix would add disproportionate complexity. Record as a Watched Risk with the cost-benefit reason; do not adopt.
- **Not a real issue** — reviewer misread the plan or source. Record as rejected with the evidence that disproves it.

Complexity has its own cost. A "must-fix" guarding a corner case that virtually never occurs is not worth shipping a more complicated codebase for.

### Build the ledger

After verification, **append a Feedback Ledger to the plan file**. Every conflict, every rejected suggestion, and every YAGNI dismissal goes here — this is the single biggest defense against silent judging.

```markdown
## Feedback Ledger — Round N

| id | reviewer(s) | severity | decision | rationale | plan/spec/test change |
| -- | ----------- | -------- | -------- | --------- | --------------------- |
| missing-negative-path-test | gemini, openai | must-fix | accepted | AC #2 lacks failure behavior | Added test matrix row #5 |
| replace-module-boundary | deepseek | should-fix | rejected | Larger refactor outside current scope; tracked as watched risk | None |

## Watched Risks
- **<short label>:** <why we accepted the risk; what would change the call later>

## Premortem Mitigations Applied
- <failure mode> → <plan change>
```

**Tiebreakers when reviewers conflict** (apply in order):

1. **Security wins** on safety conflicts (auth, data exposure, destructive actions, abuse).
2. **Spec coverage** — prefer the position that closes an acceptance-criterion or invariant gap.
3. **Existing patterns** — prefer the position consistent with codebase conventions.
4. **Simplicity** when the above are balanced.

Update the plan and spec with accepted changes. Any premortem finding rated `mitigation_sufficient: no` AND (`probability: high` OR `impact: high`) **must** be addressed before Phase 5 — either by changing the plan, adding a test-matrix row, or recording explicitly in Watched Risks why it's acceptable.

### Multiple rounds (`--rounds N`)

For round 2+, invoke `consult-llm` with the same `-m` flags, `-t <group_thread_id>` from the previous round, and `-f <plan path>` with the updated plan. Send:

```
Revision N of the plan. Were previous concerns addressed? Any new issues introduced by the changes? Use the same four sections as before. Focus on what changed.
```

Stop early if reviewers indicate no further changes. Append a fresh ledger section per round.

**If `--dry-run`:** stop here. Print the saved plan path.

## Phase 5: Implement

Implement tasks **in order**. The validation command from Phase 1 must pass at the end.

Rules:

1. **Spec-first per task** — for each task, write or extend the test that proves the linked acceptance criterion **before** the implementation. Confirm it fails. Then write the code. Confirm it passes. This isn't TDD-for-every-function; it's spec-first per acceptance criterion.
2. **Plan drift** — if implementation requires modifying files not in the plan, or deviating from the agreed approach, **stop**. Update the plan with the deviation and a one-line reason, then continue. Do not silently diverge.
3. **Validation** — run the recorded validation command (e.g. `just check`) after every logical unit, and again at the end. A task is **done** when (a) tests for it pass, (b) any acceptance criteria it claims are verified by tests, and (c) the validation command is green for the touched scope.
4. **Commits** — by default, leave changes uncommitted. With `--commit`, commit each completed task using the repository's documented commit conventions; if none exist, use a concise imperative subject and a body that explains why.
5. **Stop only on hard blockers** — see the debug protocol below for the path before stopping.

### Triggered debug protocol

Activate the protocol only when **the same check fails twice** OR a fix would require changing the plan/spec OR the failure cause is unclear. Do not formalize debugging for ordinary fixes.

For each blocked attempt, append a debug record to a scratch section at the bottom of the plan file (do not commit this — it's working notes):

```
1. failing command:
2. exact error:
3. recent relevant changes:
4. hypothesis:
5. evidence-gathering command (read-only):
6. result:
7. conclusion:
8. fix or plan revision:
```

Cap: **3 hypotheses**. If two have failed and rigor is `standard` or `deep`, consult external reviewers with `--task debug`, the same selectors, and the latest `[thread_id:group_xxx]` from Phase 4 so they retain plan context. Send:

```
We are blocked during Phase 5 implementation.

Task: <task ref>
Failing command and full output:
<output>

Hypotheses already tried (and why they failed):
<list>

Relevant recent changes:
<diff or summary>

Give ranked hypotheses with concrete evidence checks. For each hypothesis, state the observation that would confirm or falsify it. Do not propose code changes until the falsification step is identified.
```

If the third hypothesis fails, stop. Record the blocker, the hypotheses tried, and the unanswered evidence question in the Phase 7 summary. Do not loop.

## Phase 6: Red-team review

Skip only if `--no-review` or `--skip-final` was passed. Whether the diff is too narrow for adversarial review is the reviewer's call (it exits cleanly), not the agent's.

Resolve the diff base: use `--diff-base` if passed, otherwise the `START_HEAD` snapshot from Phase 0. Re-list changed files (mirror `llm-review-panel/SKILL.md` Phase 1):

```bash
git diff --name-only --diff-filter=d <diff-base>
git ls-files --others --exclude-standard
```

If both are empty, stop and report nothing implemented. Otherwise pass tracked files as `--diff-files <path>` and untracked files as `-f <path>`. Skip binary files and lockfiles.

Invoke `consult-llm` with `--task review`, `-t <group_thread_id>` from Phase 4 (so reviewers retain spec and plan context), `--diff-base <ref>`, the file flags above, and `-f <plan path>` so the spec is in scope. Send:

```
Adversarially review this diff against the Behavioral Spec and Test Matrix in the plan file.

Use only attack lenses relevant to the changed surface. Pick from:
- auth-bypass / authorization confusion
- injection / unsafe parsing
- race / concurrency / ordering
- data loss / migration / rollback
- fuzz / malformed or boundary input
- API contract / compatibility break
- spec violation / missing invariant enforcement

A finding counts as Verified only if it includes a concrete repro: a failing input, a curl command, a race-window description with timing, or a unit test that reproduces the bug. Speculation goes under "Unverified hypotheses" and may not be marked must-fix.

If the diff scope is too narrow for meaningful adversarial review (e.g. < 20 lines, no auth/data/input surface), say so and exit cleanly.

Output exactly:

## Verified Findings

### Finding N
severity: must-fix | should-fix
persona: <attack lens used>
location: path:line
spec_or_invariant_violated: <reference, or "none">
repro_or_poc: <concrete reproduction>
expected_failure: <what goes wrong when the repro runs>
rationale: <why the diff allows it>

## Unverified Hypotheses
- <bullet>

## Spec Coverage Gaps
- <acceptance criterion or invariant the diff does not satisfy>
```

For deep rigor, use `--run` with the same persona set as Phase 3 deep, but each persona is given a single attack lens.

### Verify each finding before fixing

The reviewer's `repro_or_poc` is a claim, not proof. Run it. Confirm the failure reproduces and the diff is actually responsible.

- **PoC reproduces, diff is responsible** → eligible to fix.
- **PoC reproduces, but a different part of the code is responsible** → re-target the fix or note the misattribution; do not silently fix the wrong place.
- **PoC does not reproduce** → drop the finding. List it in the Phase 7 summary under "rejected after verification".
- **Reproduces only via inputs no caller produces, or timing that doesn't occur in practice** → record as Watched Risk; do not fix. Same cost-benefit calculus as Phase 4 — complexity has its own cost.

### Apply fixes

Apply only `Verified Findings` that survived verification, are `must-fix`, AND localized (mirrors `llm-review-panel` Phase 5 criteria — single hunk, single right answer, no interface changes). For each fix:

1. Re-read the file and apply the smallest correct change.
2. Run the validation command for the touched scope.
3. With `--commit`, commit separately with a body that names the failure mode prevented.

Do **not** auto-fix `should-fix`, `Unverified Hypotheses`, or `Spec Coverage Gaps`. List them in the Phase 7 summary for the user.

If any `must-fix` cannot be fixed safely, stop and hand off — do not loop another review pass.

## Phase 7: Summary

Print to the user:

```
## Summary

**Implemented:** <one sentence>
**Plan:** <saved plan path>
**Diff base:** <START_HEAD short sha>
**Review phases run:** Phase 3 [yes | skipped: --no-review] · Phase 6 [yes | skipped: --no-review | skipped: --skip-final]

**Plan review (Round N):**
- accepted: <count> (must-fix: X, should-fix: Y)
- rejected: <count> — see Feedback Ledger
- watched risks: <count>

**Implementation:**
- tasks completed: X / Y
- validation: passed | failed (<command>)
- plan deviations: <list, or "none">

**Red-team:**
- verified findings auto-fixed: <count>
- verified findings handed off: <list — must-fix that couldn't be auto-applied>
- unverified hypotheses: <count>
- spec coverage gaps: <list, or "none">

**Blockers (if any):**
- <description, hypotheses tried, evidence question outstanding>

**Commits (if --commit):**
- <sha> — <subject>
```

If implementation drifted from the plan, list the deviations so the plan and the result reconcile cleanly.

## Critical rules

- **Spec is mandatory.** Phase 2 always produces a Behavioral Spec and Test Matrix. There is no flag to disable it.
- **Review phases are mandatory.** Phases 3 and 6 run unless the user passed `--no-review` or `--skip-final`. "Small / obvious / low-risk" is not a reason to skip — drop to `--rigor lite` for lighter review.
- **Reviewers always produce structured output.** Free-form review is not accepted at any rigor level.
- **Findings are claims, not facts.** Verify every finding before adopting it — re-read the cited code, run the PoC, check the library source. Findings that are real but only fire on contrived inputs or impossible timing are recorded as Watched Risks rather than fixed. Complexity has its own cost.
- **No silent judging.** Every reviewer conflict and every rejected suggestion is recorded in the Feedback Ledger with rationale and tiebreaker.
- **Evidence-gated final review.** A finding is must-fix only if it ships with a concrete repro. Speculation cannot trigger fixes.
- **Plan drift halts.** If implementation needs to leave the agreed scope, update the plan first, then continue.
- **Triggered debug, not ceremonial.** The debug protocol activates on a real signal (repeated failure or unclear cause), not by default.
- **Snapshot the diff base in Phase 0.** Use `START_HEAD` for Phase 6, not `HEAD~N` — survives commits, fixups, and rebases during implementation.
- **Dirty worktree halts.** `implement` does not auto-stash; the user cleans up first.
- **Default is no commits.** `--commit` opts in; otherwise the user chooses how to commit the result.
- **One pass through Phase 6.** If must-fix items remain after the auto-fix step, hand off — do not loop reviews.
