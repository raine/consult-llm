---
name: llm-collab-vs
description: The agent brainstorms with a partner LLM in alternating turns, building on each other's ideas. Synthesizes the best ideas into a plan.
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-collab-vs cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Brainstorm collaboratively with a partner LLM, building on each other's ideas in alternating turns, then synthesize the best ideas into a plan.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

**Arguments:** `$ARGUMENTS`

Check the arguments for flags:

**Partner flag** (exactly one required): any `--<selector>` from the Models block above (e.g. `--gemini`, `--openai`, `--deepseek`). Translates to `-m <selector>` for the CLI.

Strip all flags from arguments to get the task description.

**Set variables from the partner flag:**

- `MODEL`: the selector (e.g. `gemini`, `openai`)
- `PARTNER`: the same selector, used as the display label

If no `--<selector>` flag is provided, ask the user which partner to use,
listing the selectors from the Models block.

## Phase 0: Load `consult-llm` Skill

Load it now. Follow its invocation contract for all CLI calls in this workflow.

## Phase 1: Understand the Task (No Questions)

1. **Explore the codebase** - use Glob, Grep, Read to understand:
   - Relevant files and their structure
   - Existing patterns and conventions
   - Dependencies and interfaces

2. **Make reasonable assumptions** - do NOT ask clarifying questions
   - Use best judgment based on codebase context
   - Prefer simpler solutions when ambiguous
   - Follow existing patterns in the codebase

3. **Prepare context summary** - create a brief summary of:
   - The task to be implemented
   - Relevant files discovered
   - Key patterns and conventions in the codebase
   - Any constraints or considerations

## Phase 2: Seed (agent starts)

You kick off the brainstorm with initial ideas based on what you found in
Phase 1. Write them out in full:

```
## Agent's Ideas

1. **Ideas**: [2-3 possible approaches with brief descriptions]
2. **Favorite**: [which approach you lean toward and why]
3. **Open questions**: [aspects you're unsure about or would benefit from another perspective]
4. **Risks**: [what could go wrong or be tricky]
```

Present this to the user.

## Phase 3: Back and Forth

Alternate between the partner LLM and the agent. Each turn builds on the previous
response. Continue until the ideas converge into a clear approach — typically
2-3 rounds, but use as many as needed.

### Round 1

**Step 1 — PARTNER responds** to the agent's seed:

Invoke `consult-llm` per the `consult-llm` skill with `-m <MODEL>` and `-f <path>` for each relevant source file discovered in Phase 1. Send the build-on prompt below (with the agent's seed ideas embedded) per the consult-llm invocation contract.

**Build-on prompt:**
```
A collaborator shared these ideas:

[Agent's ideas from above]

Build on their thinking:
1. **What resonates**: Which ideas are strong? Why?
2. **Combinations**: Can any ideas be combined into something better?
3. **New ideas**: Did their thinking spark any new approaches?
4. **Refinements**: How would you improve the most promising ideas so far?
5. **Concerns resolved**: Did their ideas address any open questions?

Keep building — don't tear down. Refine toward the best solution.
```

Save the returned thread_id as `partner_thread_id` (see consult-llm's multi-turn section).

Present PARTNER's response to the user as `## PARTNER's Ideas (Round 1)`.

**Step 2 — agent responds** to PARTNER:

Analyze the partner's response and build on it:

```
## Agent's Ideas (Round 1)

1. **What resonates**: [which of PARTNER's ideas are strong and why]
2. **Combinations**: [ideas that can be merged into something better]
3. **New ideas**: [anything their thinking sparked]
4. **Refinements**: [improvements to the most promising ideas so far]
5. **Concerns resolved**: [open questions addressed]
```

Present this to the user.

### Subsequent rounds

Continue alternating (PARTNER → agent). On each PARTNER turn, invoke `consult-llm` with `-m <MODEL>` and `-t <partner_thread_id>` to continue the conversation, sending the build-on prompt (with the agent's latest response embedded).

**When to stop:** Both sides are refining details rather than introducing new
ideas, and a clear approach has emerged. Don't stop while there are still
unresolved open questions or competing directions.

## Phase 4: Synthesize

After all rounds, synthesize the brainstorm into a plan:

1. **Identify the strongest ideas** — which approaches gained momentum across
   rounds?

2. **Note convergence** — where did you and the partner naturally align?

3. **Pick the best combination** — merge the strongest elements into one
   coherent approach. Be honest about where the partner's ideas won.

4. **Write the plan**:

````markdown
# [Feature Name] Implementation Plan

**Goal:** [One sentence describing what this builds]

## Brainstorm Summary

**Key ideas from agent:** [2-3 bullet points]
**Key ideas from PARTNER:** [2-3 bullet points]
**Convergence:** [Where they naturally agreed]
**Synthesis:** [How the final approach combines the best of both]

---

### Task 1: [Short description]

**Files:**
- Create: `exact/path/to/file.py`
- Modify: `exact/path/to/existing.py` (lines 123-145)

**Steps:**
1. [Specific action]
2. [Specific action]

**Code:**
```language
// Include actual code, not placeholders
```

---
````

Guidelines:
- **Exact file paths** - never "somewhere in src/"
- **Complete code** - show the actual code
- **Small tasks** - 2-5 minutes of work each
- **DRY, YAGNI** - only what's needed
- **Be honest** - credit the partner when its ideas were better
