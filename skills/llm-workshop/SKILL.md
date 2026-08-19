---
name: llm-workshop
description: Interactive design session — agent facilitates a clarifying dialogue with the user, fans out to multiple LLMs in parallel for divergent approach generation, lets the user pick one, then co-designs the chosen approach with optional multi-LLM critique before saving.
allowed-tools: Bash, Glob, Grep, Read, Write
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-workshop cli_version=3.0.30 schema_version=1; do not hand-edit. -->
A facilitated design session. The user brings a rough idea; the agent clarifies it through dialogue, then convenes external LLMs to propose distinct approaches in parallel; the user picks one; agent and user finalize the design, with an optional multi-LLM critique pass before saving. Use this when you have a vague idea and want expert divergence without losing the user-in-the-loop. For role-asymmetric advisory analysis without user interaction, use `/llm-panel`.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-model calls). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Expert flags:** any `--<selector>` from the Models block selects an expert (e.g. `--gemini`, `--openai`, `--deepseek`). Repeat for multiple. With no selector flag, use **all** listed selectors in parallel. Translate each `--<selector>` into a `-m <selector>` argument.

**Mode flags:**

- `--max-approaches N` — cap how many distinct approaches surface in Phase 2 after dedup. Default `4`. Min `2`, max `5`.
- `--no-critique` — skip the Phase 4 multi-LLM critique pass on the finalized design.
- `--no-save` — print the design at the end but do not write a report file.

Strip all flags from arguments to get the user's initial idea description. If empty, ask the user to describe their idea before continuing.

## Phase 0: Load `consult-llm` skill

Load it now. Follow its invocation contract for every CLI call.

## Phase 1: Clarify the idea (user dialogue, no LLMs)

This phase is purely user + agent. Do not call external LLMs yet — premature LLM input anchors on a half-formed framing and pollutes Phase 2.

1. If the idea references the codebase, explore briefly with Glob/Grep/Read to ground later questions.
2. Ask clarifying questions conversationally, **one at a time**:
   - One question per message — never batch.
   - Suggest 2–4 concrete options as a short numbered list to anchor the answer; keep the labels tight (1–5 words) and add a line of detail where it helps.
   - Make clear the user can answer free-form — the options are prompts, not a menu.
   - If you realize you misunderstood, acknowledge and course-correct.
3. Stop asking when you have enough clarity to write a one-paragraph problem statement that the experts could act on without further input. Common things to nail down before stopping: scope, in/out, performance and compatibility constraints, success criteria, hard constraints (existing patterns to preserve, dependencies you cannot add).

**Produce a problem statement** — internal, not shown to the user yet:

```
**Problem:** <2–3 sentences>
**Constraints:** <bulleted hard requirements>
**Success criteria:** <how we know the design is good>
**Out of scope:** <bulleted non-goals>
```

This statement is what every expert sees in Phase 2. Write it tightly — sloppy framing produces sloppy approaches.

## Phase 2: Approach divergence (parallel experts)

Fan the problem statement out to all selected experts in a single parallel call. Each expert proposes 2–3 distinct approaches independently — they do not see each other's output, which is the whole point. Solo-agent design tends to anchor on the first plausible approach and propose three slight variants; multi-expert divergence breaks that.

Write a single prompt file (or send via stdin per the consult-llm contract). Pass the relevant codebase files as `-f <path>`.

**Expert prompt:**

```
You are an expert helping a user design a solution. Below is a problem statement clarified through dialogue with the user.

Problem statement:
[problem statement block]

Propose 2–3 distinct approaches. Approaches must differ in their underlying strategy or trade-off shape — not be three flavors of the same idea. For each approach, output exactly:

### Approach <N>: <short name>
**One-line summary:** <one sentence>
**Strategy:** <how it works, 2–4 sentences>
**Trade-offs:** <what you give up to choose this; cite the constraints from the problem statement when relevant>
**Complexity:** low | medium | high — <one-line justification>
**Best when:** <conditions under which this approach wins>
**Worst when:** <conditions under which this approach loses>

Do not propose implementations or pseudo-code. Do not pick a winner. Do not soften trade-offs.
```

Invoke `consult-llm` with one `-m <selector>` per expert and `-f` for relevant files. Capture the `[thread_id:group_xxx]` from line 1 — needed for Phase 4 critique continuation.

### Synthesize approaches

Collect every approach across experts. **Group** approaches that describe the same underlying strategy (different surface labels, same trade-off shape). For each group, keep the clearest summary; preserve the union of best-when/worst-when conditions; record which experts proposed it.

Filter and rank:

- Drop approaches that violate a hard constraint from the problem statement (cite the violation in the dropped list).
- Sort by distinctness — favor approaches that occupy different points on the trade-off space, not the most popular one.
- Cap at `--max-approaches N` (default 4).

### Present to the user

Show the surviving approaches conversationally as a numbered list — one entry per approach plus a final "Other / hybrid" choice — and ask the user which one they want to take forward (they can answer free-form):

```
**Approach A: <name>** — <one-line summary>
Trade-offs: <one line>
Best when: <conditions>
Proposed by: <experts>

**Approach B: <name>** — ...

[2–4 total]
```

If the user picks "Other / hybrid", ask a conversational follow-up to pin down which elements they want and continue Phase 3 from the synthesis. Do **not** start a new Phase 2 round unless the user explicitly rejects all surviving approaches as off-target — in that case, restate the problem and rerun Phase 2 once.

## Phase 3: Co-design the chosen approach (user + agent)

Back to user dialogue. Break the design into sections sized 200–300 words each. After each section, ask the user conversationally to validate before continuing. Cover whichever apply:

- Architecture and structure
- Key components and responsibilities
- Data flow and state
- Error handling strategy
- Testing approach
- Migration path (if changing existing code)
- Roll-out and validation

Apply YAGNI — cut anything not justified by the problem statement's success criteria. Acknowledge unknowns explicitly rather than papering over them.

Do not call external LLMs in Phase 3. The user is the human-in-the-loop; mid-phase LLM interruptions break conversational flow and re-anchor on stale framing. If you genuinely need a focused second opinion on one section, finish Phase 3 first and let Phase 4 catch it.

By the end of Phase 3, you have a complete design document in your head or on screen. Lay it out in markdown for Phase 4.

## Phase 4: Expert critique (skip with `--no-critique`)

Send the finalized design back to the same experts on their existing thread (using `-t <group_thread_id>` from Phase 2). They have the problem statement and the approach choice in context — only the new design needs to go in the prompt.

**Critique prompt:**

```
The user picked Approach <name> from your earlier proposals. Here is the finalized design.

Design:
[full design document]

Critique it. Output exactly these sections:

## Blind spots
What did the design miss that your proposal would have caught? Be specific — cite sections of the design.

## Constraint violations
Does the design violate any constraint from the problem statement? Quote the constraint and the violating section.

## Risk register
Top 3 things most likely to go wrong in implementation. For each:
- risk: <concrete failure with a trigger>
- likelihood: low | medium | high
- mitigation_in_design: <quote the design or "none">
- recommended_addition: <specific section or change to add>

## Verdict
ship | revise | rethink — one sentence justification.

Do not rewrite the design. Do not propose alternative approaches at this stage — that ship has sailed. Focus on what would change if this design proceeded as written.
```

Synthesize the critiques:

- Group identical findings across experts.
- Flag any unanimous "rethink" verdict to the user — that's a strong signal the chosen approach has a fatal flaw.
- For each unique finding: present it to the user conversationally and ask whether to `Adopt into design`, `Note as watched risk`, or `Ignore` (offer those as a numbered list; they can answer free-form). Don't batch — one finding at a time, like Phase 1 questions. Skip findings the user clearly already addressed in Phase 3.

Update the design with adopted findings. Append a "Watched Risks" section for noted-but-not-adopted ones.

## Phase 5: Save and report

Unless `--no-save`, write the design to a user-requested path, or default to `<YYYY-MM-DD>-design-<topic>.md` in the current working directory. Derive `<topic>` from the user's idea (kebab-case, short).

**Artifact template:**

```markdown
# Workshop: <topic>

**Problem:** <one paragraph>
**Constraints:** <bullets>
**Success criteria:** <bullets>
**Out of scope:** <bullets>

## Approach chosen

**<name>** — <one-line summary>
Trade-offs accepted: <bullets>
Proposed by: <experts>

### Approaches considered and rejected

- **<name>** (<experts>) — rejected because <one-line reason>
- ...

## Design

<full design from Phase 3, incorporating Phase 4 adoptions>

## Watched risks

- **<short label>:** <what could go wrong; what would change the call later>

## Expert thread

- group thread: `<group_thread_id>`
- per-expert: `<selector>` / `<thread_id>`, ...
```

The thread map lets a follow-up `consult-llm -t <id>` continue any expert's conversation later — useful if implementation surfaces a question the experts could answer in context.

Print the saved path and a one-paragraph recap of the chosen approach to the user.


## Critical rules

- **Phase 1 is LLM-free.** No external calls until the problem statement is tight. Premature LLM input anchors on bad framing.
- **Phase 2 experts are independent.** A single parallel `consult-llm` call with one `-m` per expert; never show one expert's proposals to another in this phase. Anchoring defeat is the whole point.
- **Phase 3 is LLM-free.** The user is the human-in-the-loop. No mid-phase LLM interruptions.
- **One question at a time.** Every user question follows the brainstorm rule — a single conversational question, 2–4 suggested options, free-form answers always welcome.
- **Reuse Phase 2 threads in Phase 4.** Pass `-t <group_thread_id>` so experts retain problem-statement context without resending it.
- **Advisory critique, not rewrite.** Phase 4 surfaces blind spots and risks; the user (with the agent) decides what to adopt. Do not let an expert's critique silently overwrite the user's chosen design.
- **YAGNI ruthlessly.** Cut features not justified by the success criteria. Acknowledge unknowns explicitly instead of inventing plausible answers.
- **The skill produces a design document, not code.** Do not modify source files. To take a design forward, hand it to `/llm-implement`.
