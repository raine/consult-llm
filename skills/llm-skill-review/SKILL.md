---
name: llm-skill-review
description: Multi-model review of an AI-agent skill or instruction file (SKILL.md or similar). Uses skill-specific lenses, then classifies findings inline so the user can act on them.
allowed-tools: Bash, Glob, Grep, Read
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-skill-review cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Review a skill file from any supported agent runtime with multiple LLMs through skill-specific lenses, then classify findings inline so the user can act without a separate assessment workflow.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (heredocs, timeouts, `--run`, prompt files, thread IDs). Do not call the CLI without loading it first.

## When to use

- A new or modified `SKILL.md` (or any other agent-readable instruction file) that future agents will execute.
- Before relying on a skill in a high-volume or multi-agent workflow.
- When two skills now call each other and contract drift is plausible.

## When NOT to use

- Reviewing implementation code → `/llm-review` or `/llm-review-panel`.
- Open architecture / decision spanning multiple domains → `/llm-panel`.
- Tiny edits (wording fixes, typos) → not worth a multi-model round.

## Available models

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Flags:

| Flag | Default | Effect |
|---|---|---|
| `--models <list>` | `gemini,openai,anthropic,deepseek` | Comma-separated selectors / model IDs to use as reviewers |
| `--lenses <list>` | all six defaults (plus `design-fidelity` if a design doc is detected) | Subset of `executor,trigger-fit,edge-case,cross-skill,blast-radius,script-extraction,design-fidelity` |
| `--include-callees` | on | Read and pass the skills that the target skill explicitly invokes as context (so cross-skill lens has the contracts to compare against) |
| `--design-doc <path>` | auto-detect | Path to a design intent document. If set, `design-fidelity` is included. Without it, auto-detection checks only `<target-dir>/design.md`. |
| `--dry-run` | off | Run lensed reviews, skip the inline assessment phase, dump raw output |

Strip flags; the remainder is **the path to the skill file**. Reject if it does not look like a SKILL.md / instruction file (must contain agent-directed imperative prose; warn but proceed if the user insists).

## The lenses

Each lens is a single prompt the reviewer model executes against the skill text. Use the same set of lenses for every reviewer model — that's the whole point of multi-model review. There are six default lenses plus `design-fidelity`, which is only meaningful (and only included) when a design intent doc is available.

### 1. Executor lens

> "Read this skill as if you are the agent about to execute it. Walk through every step. List **every place** where you would have to guess, ask the user a question the skill does not anticipate, or make a judgment call the skill does not give you criteria for. For each, quote the exact phrase and explain what is ambiguous."

### 2. Trigger-fit lens

> "Given ONLY the skill's `name` and `description` (do not read the body), generate (a) 5 user prompts that should route to this skill, and (b) 5 plausible prompts that should NOT route to this skill but might if the description is misleading. Then read the body. For each generated prompt, state whether the skill actually handles it well, and flag any mismatch."

### 3. Edge-case lens

> "Enumerate the failure / edge cases this skill should cover: empty input, malformed input, dirty repository state, missing prerequisites, interruption mid-run, concurrent invocation, partial completion, resource exhaustion (disk, API rate limits), and unauthorized or destructive prerequisites. For each, state whether the skill addresses it, addresses it weakly, or ignores it."

### 4. Cross-skill lens

> "Identify every other skill this one explicitly invokes (look for `/<name>` references or instructions to 'run /…'). For each, read the called skill's contract — its arguments, its required preconditions, its outputs and side effects (provided as context). List any mismatch: missing required precondition, wrong argument shape, assumption about output that the called skill does not guarantee, or instruction that contradicts the called skill's own rules."

### 5. Blast-radius lens

> "List every action this skill could take that is irreversible or affects shared state outside the current working directory: deletes, force-pushes, remote API writes, mass file operations, spawning many sub-processes, financial cost (LLM token spend at scale), modifying upstream branches. For each, state whether the skill requires confirmation, exposes a dry-run, has a rollback path, or proceeds blindly. Identify the worst case if executed on a wrong input."

### 6. Script-extraction lens

> "Premise: skill files are agent-readable prose. Agents reliably follow one-line shell calls but skip multi-step prose procedures when the main task is done. So any step that is mechanical (deterministic, no judgment) should usually be a script the skill *invokes*, not prose the agent *executes*. Conversely, sometimes a skill scripts something that is actually judgment-heavy and would be better as prose.
>
> Identify candidates in BOTH directions:
>
> 1. **EXTRACT-script**: any step or block where the skill asks the agent to do a multi-step mechanical procedure that would be more reliable as a single script invocation. Look especially for: path derivation + collision handling + structured file writing; state-machine bookkeeping (counters, manifests, sequence IDs); schema enforcement / validation; parsing structured output (jq-able JSON, awk-able text); atomic file moves (mktemp + write + mv); sequenced cleanup (at least 3 ordered ops); looping over N items with bookkeeping. For each, quote the prose, propose a concrete `scripts/<name>.sh <args>` signature, and predict the agent-skip risk:
>    - **high**: late-workflow, deterministic precision required, competes with user-facing main work, OR repeated many times per run.
>    - **medium**: mid-workflow, partly mechanical with local judgment, OR a one-shot step where occasional skip is recoverable.
>    - **low**: early-workflow, single-step, judgment-tied, agent unlikely to skip.
>
> 2. **INLINE-prose** (rarer): any step the skill ALREADY externalizes to a script that is actually judgment-heavy enough that a script will get it wrong. Look for scripts taking freeform input, making ranking decisions, or branching on semantic content. For each: quote the script call, explain why judgment doesn't fit a script, and sketch the prose alternative.
>
> Where a candidate overlaps with another lens (executor ambiguity, edge-case neglect, blast-radius), note the cross-reference — a script extraction often resolves multiple lenses at once.
>
> Be specific: vague "could be scripted" is not useful; concrete "lines X–Y derive an ID, check collisions, and write markdown — extract this to `scripts/record-review.sh`" is useful.
>
> Output format:
>
> ```
> ## EXTRACT-script
> - title: <short>
>   quote: \"<verbatim from skill>\"
>   proposed_script: <path + brief signature>
>   agent_skip_risk: high|medium|low
>   cross_lens: <comma-separated other lenses also resolved, or '-'>
>   rationale: <one sentence>
>
> ## INLINE-prose
> - title: <short>
>   quote_or_script_ref: \"<verbatim or script name>\"
>   why_script_fails: <one sentence>
>   prose_alternative: <one sentence>
>
> ## Net assessment
> <2–3 sentences on prose/script balance, single highest-value change>
> ```"

### 7. Design-fidelity lens (only when a design doc is attached)

> "Read the attached design intent document and the SKILL.md side by side. List every place where the SKILL.md drifts from the stated design — scope creep (the skill does things the design did not call for), under-scoping (the skill omits something the design declared in-scope), contradiction (the skill takes a position the design explicitly ruled against), failure-mode neglect (the skill ignores a failure mode the design said it must handle), automation-level mismatch (the skill is more or less autonomous than the design specified), or feedback-handling mismatch (the skill manages user feedback differently than designed). For each finding, quote BOTH the design statement and the conflicting SKILL.md phrase. If a drift looks like a legitimate refinement (the draft genuinely discovered a better approach during writing), flag it as INTENTIONAL DRIFT rather than a violation — it needs human acknowledgement, not silent fixup."

## Workflow

### Phase 0 — Load `consult-llm`

Load it now. Follow its invocation contract.

### Phase 1 — Gather context

1. Read the target skill file. If it does not exist, abort.
2. If `--include-callees` (default): grep the skill body for `/<name>` references. For each match, first look beside the target skill, then in the installed runtime roots (`~/.claude/skills`, `~/.pi/agent/skills`, and `~/.codex/skills`). Read the first matching `SKILL.md`. Missing optional callees are reported and skipped rather than guessed. These become **context attachments** for the cross-skill lens prompt.
3. **Locate design doc.** If `--design-doc <path>` was given, use it. Otherwise check only `<target-dir>/design.md`. Add the design-fidelity lens when a design doc was found and the lens is not excluded by `--lenses`.
4. Resolve models from `--models` via `consult-llm models`. Reject duplicates after resolution.

Report to the user:
- Target skill path
- Callee skills detected (and read)
- Design doc detected (path) or "(none)"
- Lenses to run, models to use
- Approximate cost: `<N_lenses> × <N_models>` LLM calls

### Phase 2 — Run lensed reviews

For each model × lens combination, issue one `consult-llm` call with:
- The lens prompt (verbatim from the section above)
- The full skill body as primary input
- For cross-skill lens only: the callee skills' bodies as additional context
- For design-fidelity lens only: the design doc as additional context

Issue calls in parallel where the CLI supports it; otherwise sequentially. Collect raw responses indexed by `(model, lens)`.

If `--dry-run`: dump the raw responses grouped by lens, stop.

### Phase 3 — Inline skill-specific assessment

This phase is what distinguishes this skill from running `/llm-review` then `/assess-findings`. The generic assess-findings axes (confirmation, real-world likelihood, readability, architectural impact) are tuned for code findings. Skill findings need different axes.

The agent (you, executing this skill) reads all raw findings from Phase 2 and classifies each into a table.

**Deduplicate first:** the same issue may be raised by multiple models or under multiple lenses. Collapse duplicates; note "raised by N models / under M lenses" as a signal of conviction.

**Axes** (each finding scored):

| Axis | Values | Meaning |
|---|---|---|
| Concreteness | quoted / paraphrased / vague | Is the finding tied to a specific phrase in the skill, or generic concern? |
| Operational impact | silent-wrong / loud-failure / cosmetic | If a future agent hits this gap, does it silently do the wrong thing, fail loudly (recoverable), or just look ugly? |
| Reader cost | reduces / neutral / adds-noise | Does fixing it shorten the skill, leave it equal, or pad it with caveats? |
| Convergence | unique / shared-models / shared-lenses | How many independent reviewers raised it? |

**Decision** per finding, derived from the axes:

- **FIX** — silent-wrong OR (loud-failure AND quoted AND reduces-or-neutral reader cost) OR shared-models convergence
- **DISCUSS** — high impact but reader-cost adds noise, or trade-off the user should resolve
- **SKIP** — cosmetic + adds-noise, or vague + unique reviewer

Output a single decision table grouped by decision. Each row: `<short title>` · `<lens>` · `<axes summary>` · `<one-line recommendation>`. Include the verbatim quote where applicable.

**Design-fidelity findings get a separate sub-table** below the main one, with a different decision schema — they are not skill-text edits but draft-vs-design reconciliation choices:

| Decision | When |
|---|---|
| **FIX-vs-design** | SKILL.md drifted from design without justification → edit SKILL.md to match the design. |
| **UPDATE-design** | Drift is a legitimate refinement (the draft found a better path) → update the design doc to match the SKILL.md and note the rationale. |
| **DISCUSS-drift** | Drift represents a real trade-off the user should resolve — neither side is obviously right. |
| **INTENTIONAL** | The drift is acknowledged in the SKILL.md itself (e.g. "differs from design because…") — no action, just record. |

Each row in the sub-table: `<short title>` · `<design quote>` · `<skill quote>` · `<recommendation>`.

**Script-extraction findings get their own sub-table** with action classes that reflect "write a new script" or "refactor an existing script" — bigger work than ordinary skill-text edits:

| Decision | When |
|---|---|
| **EXTRACT-script** | Mechanical step, agent-skip risk ≥ medium, low script complexity → propose a new script. Automated reviewers should recommend it rather than implement it silently — script authorship is separate work. |
| **INLINE-prose** | Existing script handles judgment-heavy work poorly → move the logic back to skill prose; remove or shrink the script. |
| **DISCUSS-extraction** | Borderline — small mechanization win vs. script-maintenance cost, or two reviewers disagreed on the right direction. User decides. |
| **KEEP** | Skill's prose / script split is appropriate as-is. Often the right call when the candidate is shared with the executor lens — the script already exists in spirit, just needs better prose anchor. |

Each row in the sub-table: `<short title>` · `<direction>` · `<proposed-or-current-script>` · `<risk or judgment note>` · `<one-line recommendation>`.

### Phase 4 — Present to user

Show the decision table. For each FIX, offer to apply the recommended edit (the user can pick which ones). For DISCUSS items, summarize the trade-off so the user can decide. For SKIP, list with one-line reason so the user can object.

Do NOT auto-apply edits without user confirmation. Skill changes can have wide downstream effects.


## Notes

- **Cost discipline.** Six default lenses × four models = 24 calls per review; seven lenses (with design-fidelity) × four models = 28. For routine tweaks, pass `--lenses executor,cross-skill` to cut the call count. For a structural refactor, `--lenses script-extraction,cross-skill,blast-radius` is the highest-signal cheap bundle. For a heavy skill that will fan out 1000 spin-offs, run all of them.
- **Cross-skill lens is the highest-value lens** for skills in a workflow chain. Contract drift between callers and callees is especially costly at scale.
- **The reviewer models do not have the rest of the repo loaded.** If a finding depends on repo conventions not in the skill itself, the reviewer will miss or hallucinate. Mention repo conventions briefly in the skill when they matter.
- **Self-test:** when this skill is significantly changed, run it on itself. Recursion is fine; the skill is just text.

## Example invocations

```
/llm-skill-review ~/.claude/skills/example/SKILL.md
/llm-skill-review --lenses executor,cross-skill ~/.pi/agent/skills/example/SKILL.md
/llm-skill-review --lenses script-extraction,blast-radius ./skills/example/SKILL.md
/llm-skill-review --models gemini,deepseek --dry-run ./skills/example/SKILL.md
/llm-skill-review --design-doc ./design.md ./skills/example/SKILL.md
```
