---
name: llm-panel
description: Role-specialized LLM panel analyzes a task from asymmetric expert lenses (architect, security, maintainability, test-strategist by default). Agent synthesizes a trade-off resolution.
allowed-tools: Bash, Glob, Grep, Read
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-panel cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Run a role-asymmetric advisory panel. Each role analyzes the same task from a single expert lens; the agent synthesizes a PM-style trade-off resolution. Use this when a decision spans multiple domains that pull in different directions and one of them shouldn't silently win. For peer-style brainstorming with no role separation, use `/llm-collab`. For multi-model review of an existing diff with identical prompts, use `/llm-review-panel`.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (heredocs, timeouts, `--run`, prompt files, thread IDs). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Role selection:**

- `--roles <comma-separated>` — explicit kebab-case role list (e.g. `--roles security,api-design,ops-readiness`). Trim whitespace; reject duplicates and empty entries.
- (none) — **the agent picks the roles** based on the task focus and the Phase 1 context. Default starting point is `architect, security, maintainability, test-strategist`; swap individual roles to fit the actual work. See "Example role bundles" near the bottom of this file as a reference. Pick 3–5 roles; do not exceed 5.

After picking, **show the chosen roles to the user before Round 1** so they can override with `--roles` if the inference is off (e.g. "Roles: migration-architect, security, data-integrity, rollback-operator. Override with --roles if needed.").

**Mode flags:**

- `--mode design|review` — default `design`.
  - `design` is a forward-looking proposal panel. Uses `--task plan` internally.
  - `review` is a critical pass on an existing diff. Uses `--task review` internally and adds `--diff-files`/`--diff-base`.
- `--diff-base <ref>` — review mode only. Default is auto-detected (see Phase 1). Pass an explicit ref to override.
- `--react` — opt in to a second round where each role responds to the agent's draft synthesis on its own thread.

**Model assignment:** any `--<selector>` from the Models block selects a backing model. Repeat for multiple. With no selectors, use all listed selectors in their listed order. Roles map to selectors positionally — first role to first selector, etc. Each role must end up on a **distinct** resolved model (`--run` rejects duplicates).

Strip all flags; the remainder is the panel focus. In `design` mode, treat it as the proposal/decision under analysis. In `review` mode, treat it as optional review focus.

### Fail-fast preconditions

Stop before Round 1 with a clear error if any of these fail:

- Role count > 5 (`--run` max).
- Role count exceeds the number of distinct selected/available models.
- Selectors resolve to duplicate underlying models.

Error format:

```
panel: <N> roles requested, only <M> distinct models available.
Roles:     <role list>
Selectors: <selector list>
Either pick fewer roles (--roles ...) or pass distinct --<selector> flags.
```

## Phase 0: Load `consult-llm` skill

Load it now. Follow its invocation contract for every CLI call.

## Phase 1: Gather shared context

Build a context summary that every role sees verbatim. Reasonable assumptions only — do not ask the user clarifying questions.

**Both modes:**

- Use Glob/Grep/Read to find files, patterns, conventions, constraints related to the focus.
- Note compatibility requirements, security boundaries, deployment concerns, prior decisions, known unknowns.
- Exclude generated files, lockfiles, vendored dependencies unless central.

**Design mode** — pass relevant source files as shared `-f <path>` to the panel call.

**Review mode** — resolve `<diff-base>` using the same logic as `llm-review-panel/SKILL.md` Phase 1 (prefer `@{upstream}`, fall back to merge-base with the detected main branch, fall back to `HEAD`). Show the resolved base to the user before running the panel.

List changed files:

```bash
git diff --name-only --diff-filter=d <diff-base>
git ls-files --others --exclude-standard
```

Pass tracked changed files as shared `--diff-files <path>` with `--diff-base <ref>`. Pass untracked files as `-f <path>` so roles see full contents. Exclude binary files and lockfiles. If there is no diff, stop and report nothing to review.

Prepare the **shared context summary** as plain markdown — focus, mode, key files, constraints, known unknowns.

## Phase 2: Round 1 — parallel panel

Write one prompt file per role with `mktemp`. Each file contains the persona framing, the panel focus, and the shared context summary.

**Persona prompt — design mode:**

```
You are the {role} specialist on an advisory panel. Speak only from your assigned perspective. Do not assume other roles' responsibilities.

Mode: design — analyze this proposed work before implementation.

Panel focus:
[focus]

Shared context:
[context summary]

Output exactly these sections:
1) Key observations — what stands out from your domain's viewpoint?
2) Risks / opportunities — what could go wrong or be improved?
3) Non-negotiable requirements — what must be true (or must not be true) for the solution to be acceptable from your perspective?
4) Open questions for other roles — what do you need from other specialists?

Acknowledge conflicts with other perspectives but do not resolve them — that is the agent's job.
```

**Persona prompt — review mode:**

Same structure, but framing changes to "critical pass on an existing diff". Replace section 1 with "what stands out when reading this diff (cite file:line where possible)" and section 2 with "concrete defects, regressions, safety issues, compatibility breaks, missing tests from your domain". Add: "Do not propose fixes. Do not soften findings."

Invoke `consult-llm` **once** with one `--run` per role. For each run pass `model=<selector>,prompt-file=<tempfile>`. Add `--task plan` for design mode or `--task review --diff-base <ref>` for review mode. Do not pass `-m` alongside `--run`.

Save the per-role thread IDs from each section header (`[thread_id:<id>]`) keyed by role name — needed for `--react` and the final artifact.

## Phase 3: React round (only with `--react`)

Draft a provisional synthesis using the Phase 4 structure. This draft is internal; do not show the user yet.

Write one continuation prompt file per role and invoke `consult-llm` again with one `--run` per role, passing `model=<selector>,thread=<role-thread-id>,prompt-file=<tempfile>`. Keep the same shared `-f`/`--diff-*` context.

**Reaction prompt template:**

```
The agent has drafted this synthesis from the Round 1 panel:

[draft synthesis]

From your assigned {role} perspective:
1) Are your domain concerns adequately addressed? If not, what is missing?
2) What is truly non-negotiable — a hard requirement you would block on?
3) Which trade-offs do you reject? Be specific about why.
4) Any disagreement that should remain unresolved?

Do not resolve cross-role conflicts. Flag them for the agent.
```

Update the synthesis using the reactions. New non-negotiables and rejected trade-offs go into the final artifact; if a role rejects on a domain-critical point, either resolve it explicitly or preserve it under unresolved disagreements.

## Phase 4: Synthesize trade-offs

The agent owns the resolution. The roles advise; you decide.

**Resolution rules:**

- **Defer to security** on safety conflicts: auth, data exposure, integrity, destructive actions, privacy, abuse, supply chain risk. If `security` (or any equivalent role) flags a concrete risk, the resolution must neutralize it.
- **Prefer simpler** when trade-offs balance.
- **Preserve unresolved disagreements explicitly** — do not fake consensus. If you cannot resolve a conflict with high confidence, name it as Unresolved and state what input would settle it.
- If the final recommendation accepts a risk, name the risk and the reason it's acceptable.
- If the recommendation depends on later validation, make that dependency explicit.

## Phase 5: Save and report

Save the synthesis to a user-requested path, or default to `<YYYY-MM-DD>-panel-<topic>.md` in the current working directory. Derive `<topic>` from the current branch name (sanitized to kebab-case), falling back to a short slug from the panel focus on the main branch or detached HEAD. Print the saved path.

**Artifact template:**

```markdown
# Panel: <topic>

**Mode:** design|review
**Roles:** <role> (<selector>), <role> (<selector>)
**Diff base:** <ref>   _(review mode only)_

## Panel summary

- **<role>:** one-bullet summary of this role's position.
- **<role>:** one-bullet summary.

## Areas of agreement

- <point of multi-role consensus>

## Conflicts and resolutions

### <short conflict title>
- **<role A>** said: <concise paraphrase or quote>
- **<role B>** said: <position>

**Resolution:** <agent decision>. **Reasoning:** <why this trade-off wins, citing the resolution rules>.

## Unresolved disagreements

- **<issue>:** <roles involved>. <what remains unresolved; what decision or input would resolve it>.
- If none, write `None`.

## Final recommendation

<One paragraph: chosen direction, required constraints, rejected alternatives, validation needed before implementation or merge.>

## Thread map

- **<role>:** `<selector>` / `<thread_id>`
```

The thread map lets a follow-up `--react` run or manual `consult-llm -t <id>` continue any role's conversation later.

Print the saved path and a short final-recommendation summary to the user.


## Critical rules

- **Strict asymmetry.** Each role gets only its own persona prompt. Never leak other roles' prompts. Roles drifting into general commentary weaken the panel — enforce focus through the persona framing.
- **Distinct models per role.** `--run` rejects duplicates. Fail fast with the error format above.
- **Mode → task mapping.** `design` ⇒ `--task plan`. `review` ⇒ `--task review` plus `--diff-files`/`--diff-base`.
- **`--react` continues threads.** Do not start fresh threads for the reaction round.
- **Defer to security on safety conflicts. Prefer simpler when trade-offs balance.**
- **Preserve unresolved disagreements.** No fake consensus.
- **Advisory only.** The panel produces a report. Do not modify source files or commit as part of this skill — hand the artifact back to the user.

## Example role bundles

Reference starting points when the agent is picking roles. Use them as inspiration, not canonical sets — swap individual roles to fit the actual task. Each bundle is 4 roles; trim or extend within the 3–5 range.

| Domain | Roles |
| --- | --- |
| General (default) | `architect, security, maintainability, test-strategist` |
| Frontend | `frontend-architect, accessibility, performance, design-system-maintainer` |
| Backend | `backend-architect, security, reliability, data-integrity` |
| Migration / data move | `migration-architect, data-integrity, rollback-operator, compatibility` |
| Library / public API | `api-designer, backward-compatibility, documentation, test-strategist` |
| Infra / deploy | `platform-architect, reliability, observability, cost` |
| ML / data pipeline | `data-engineer, ml-architect, reproducibility, evaluation` |

A role label is just a kebab-case persona handed to the LLM — no validation, no special casing. Coin new ones freely (e.g. `i18n`, `bundle-size`, `fuzzing-strategist`) when the task warrants.
