---
name: llm-collab
description: Multiple LLMs collaboratively brainstorm solutions, building on each other's ideas across rounds. Agent synthesizes the best ideas into a plan.
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-collab cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Have multiple LLMs collaboratively brainstorm solutions, then synthesize the best ideas into a plan. The LLMs build on each other's ideas across rounds rather than critiquing positions.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

**Arguments:** `$ARGUMENTS`

**Model flags:** any `--<selector>` from the Models block above selects a collaborator (e.g. `--gemini`, `--openai`, `--deepseek`). Repeat for multiple. Need at least **two**. With no model flag, brainstorm uses **all** listed selectors.

Translate each `--<selector>` into a `-m <selector>` argument to the CLI.

**Logging flags:**

Strip all flags from arguments to get the task description. Use the selector name as the label when presenting per-model output.

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

## Phase 2: Initial Ideas

Have all selected LLMs independently brainstorm approaches (in parallel).

**Seed prompt:**
```
I need to implement the following task:

[Task description]

Here's what I found in the codebase:
[Context summary - relevant files, patterns, conventions]

Brainstorm implementation ideas:
1. **Ideas**: List 2-3 possible approaches with brief descriptions
2. **Favorite**: Which approach do you lean toward and why?
3. **Open questions**: What aspects are you unsure about or would benefit from another perspective?
4. **Risks**: What could go wrong or be tricky?

Think creatively. Share rough ideas — we're exploring, not committing.
```

Invoke `consult-llm` with one `-m <selector>` per collaborator and `-f <path>` for each relevant source file, sending the seed prompt per the consult-llm invocation contract. All models are queried in parallel in a single call.

**Extract per-model thread IDs** from the response — needed for Phase 3 since each model receives a different prompt.

Present each set of ideas to the user, labeled by selector.

## Phase 3: Build On Each Other

Each round, share every other LLM's ideas with each model and ask them to build on them (in parallel). Pass each LLM's thread ID via `-t <id>` to continue its conversation. Continue until the ideas converge into a clear approach — typically 2-3 rounds, but use as many as needed.

**Build-on prompt (same template for each model; embed every other model's previous-round response, labeled by selector):**
```
Your collaborator(s) shared these ideas:

[Other LLMs' responses from the previous round, each labeled with the selector name]

Build on their thinking:
1. **What resonates**: Which ideas are strong? Why?
2. **Combinations**: Can any ideas be combined into something better?
3. **New ideas**: Did their thinking spark any new approaches?
4. **Refinements**: How would you improve the most promising ideas so far?
5. **Concerns resolved**: Did their ideas address any open questions?

Keep building — don't tear down. Refine toward the best solution.
```

Each model receives a different prompt (the other models' responses embedded). Invoke `consult-llm` once with one `--run` flag per collaborator, continuing each model's thread.

Present every response to the user after each round, labeled by selector.

**When to stop:** All collaborators are refining details rather than introducing new ideas, and a clear approach has emerged. Don't stop while there are still unresolved open questions or competing directions.

## Phase 4: Synthesize

After all rounds, synthesize the brainstorm into a plan:

1. **Identify the strongest ideas** — which approaches gained momentum across rounds?

2. **Note convergence** — where did the LLMs naturally align?

3. **Pick the best combination** — merge the strongest elements into one coherent approach

4. **Write the plan**:

````markdown
# [Feature Name] Implementation Plan

**Goal:** [One sentence describing what this builds]

## Brainstorm Summary

**Key ideas** (one block per collaborator, labeled with the selector):
- **<selector>:** [2-3 bullet points]

**Convergence:** [Where they naturally agreed]
**Synthesis:** [How the final approach combines the best ideas]

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

Save the plan to a user-requested path, or default to `plan-<feature-name>.md` in the current working directory.

