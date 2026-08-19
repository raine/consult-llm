---
name: llm-debate-vs
description: The agent debates an opponent LLM through a multi-turn conversation, then synthesizes the best approach and implements.
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-debate-vs cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Debate an opponent LLM on the best implementation approach using multi-turn
conversations, then synthesize and implement.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Phase 0: Load `consult-llm` Skill

Load it now. Follow its invocation contract for all CLI calls in this workflow.

## Configuration

**Arguments:** `$ARGUMENTS`

Check the arguments for flags:

**Opponent flag** (exactly one required): any `--<selector>` from the Models block above (e.g. `--gemini`, `--openai`, `--deepseek`). Translates to `-m <selector>` for the CLI.

**Mode flags:**

- `--dry-run` → debate and plan only, skip implementation
- `--skip-final` → skip the final review phase
- `--rounds N` → number of debate rounds (default: 1, max: 3). Each round =
  agent argues + opponent responds.

Strip all flags from arguments to get the task description.

**Set variables from the opponent flag:**

- `MODEL`: the selector (e.g. `gemini`, `openai`)
- `OPPONENT`: the same selector, used as the display label

If no `--<selector>` flag is provided, ask the user which opponent to use,
listing the selectors from the Models block.

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

## Phase 2: Opening Arguments

Both debaters propose their approach independently.

### Step 1: Agent's Opening Argument

You ARE the debater. Form your own implementation approach based on what you
found in Phase 1. Write it out in full:

```
## Agent's Opening Argument

1. **Approach**: [2-3 sentences]
2. **Key decisions**: [architectural/design decisions]
3. **Files**: [files to create or modify]
4. **Steps**: [implementation steps]
5. **Trade-offs**: [pros and cons]
```

Present this to the user so they can see your position.

### Step 2: Opponent's Opening Argument

Invoke `consult-llm` per the `consult-llm` skill with `-m <MODEL>` and `-f <path>` for each relevant source file discovered in Phase 1. Send the opening prompt below per the consult-llm invocation contract.

**Opening prompt:**

```
I need to implement the following task:

[Task description]

Here's what I found in the codebase:
[Context summary - relevant files, patterns, conventions]

Propose your implementation approach:
1. **Approach**: Describe your recommended approach in 2-3 sentences
2. **Key decisions**: List the main architectural/design decisions
3. **Files**: What files to create or modify
4. **Steps**: High-level implementation steps
5. **Trade-offs**: What are the pros and cons of this approach?

Be specific and opinionated. Defend your choices.
```

**Save the returned thread_id** for subsequent rounds (see consult-llm's multi-turn section).

Present the opponent's opening argument to the user as
`## OPPONENT's Opening Argument`.

## Phase 3: Debate Rounds

For each round (default 1, configurable with `--rounds N`):

### Agent's Turn

Analyze the opponent's latest argument. Write your rebuttal:

```
## Agent's Rebuttal (Round N)

1. **Critique**: [weaknesses in opponent's approach]
2. **Defense**: [address weaknesses opponent identified in your approach]
3. **Concessions**: [good ideas from opponent worth adopting]
4. **Updated position**: [your refined recommendation]
```

Present this to the user.

### Opponent's Turn

Invoke `consult-llm` per the `consult-llm` skill with `-m <MODEL>` and `-t <thread_id>` (from the previous response), sending the rebuttal prompt below.

**Rebuttal prompt:**

```
Your opponent has responded with this rebuttal:

[Opponent's rebuttal from above]

Provide your counter-argument:
1. **Critique**: What are the weaknesses in your opponent's approach and rebuttal?
2. **Defense**: Address the weaknesses your opponent identified in your approach
3. **Concessions**: Are there any good ideas from your opponent worth adopting?
4. **Updated position**: State your refined recommendation

Be constructive but thorough in your critique.
```

**Update the thread_id** if a new one is returned.

Present the opponent's rebuttal to the user as
`## OPPONENT's Rebuttal (Round N)`.

## Phase 4: Synthesis and Plan

As both debater and moderator, synthesize the final approach:

1. **Score the arguments**:
   - Which approach is simpler?
   - Which approach better fits existing patterns?
   - Which critiques were valid?
   - What concessions were made?

2. **Identify consensus**: Where did you and the opponent agree?

3. **Resolve disagreements**: For each point of contention:
   - Evaluate the arguments from both sides
   - Be honest about where the opponent had the stronger argument
   - Prefer simpler solutions when arguments are equally strong

4. **Write the verdict** as part of the plan:

````markdown
# [Feature Name] Implementation Plan

**Goal:** [One sentence describing what this builds]

## Debate Summary

**Agent's position:** [1-2 sentence summary] **OPPONENT's position:** [1-2
sentence summary]

**Points of agreement:**

- [Consensus point 1]
- [Consensus point 2]

**Resolved disagreements:**

- [Issue]: Agent said X, OPPONENT said Y → **Verdict:** [Decision and why]

**Verdict:** [2-3 sentences on the final synthesized approach]

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
- **Be honest** - credit the opponent when its ideas won

Save the plan to a user-requested path, or default to `plan-<feature-name>.md` in the current working directory.

## Phase 5: Implement

**If `--dry-run`:** Skip to Phase 7 (Summary) - report the debate and plan
without implementing.

Implement the plan without further interaction:

1. **Follow the plan exactly** - implement each task in order
2. **Commit after each logical unit** - keep commits small and focused
3. **If something is unclear** - make a reasonable decision and note it in the
   commit message
4. **If a task fails** - attempt to fix it before moving on
5. **Only stop if there's a blocking error** that cannot be resolved

Implementation rules:

- Work through tasks sequentially
- Test changes when possible
- Keep commits atomic and well-documented
- Use commit messages that explain the "why"

## Phase 6: Final Review

**If `--skip-final`:** Skip to Phase 7 (Summary).

After implementation, have the opponent review using the existing thread (full
debate context):

Invoke `consult-llm` per the `consult-llm` skill with `-m <MODEL>`, `--task review`, `-t <thread_id>` (from the debate), `--diff-files <path>` for each changed file, and `--diff-base HEAD~N`, sending the final review prompt below.

**Final review prompt:**

```
Forget which side you argued during the debate. Review the implementation purely on its merits:
- Any bugs or edge cases missed?
- Code quality issues?
- Security concerns?

Be concise. Only flag issues worth fixing.
```

**Apply fixes** if the opponent identifies clearly valid concerns:

- Fix bugs and edge cases
- Commit each fix separately with clear messages

**Skip** minor style suggestions.

## Phase 7: Summary

Present a final summary to the user:

```
## Summary

**Implemented:** [One sentence describing what was built]

**Debate outcome (Agent vs OPPONENT):**
- Agent advocated: [key position]
- OPPONENT advocated: [key position]
- Final verdict: [who won which points, synthesized approach]

**Key decisions from debate:**
- [Decision 1 - who proposed it and why it won]
- [Decision 2 - who proposed it and why it won]

**Post-implementation fixes:**
- [Fix applied after final review, if any]

**Commits:**
- `abc1234` - [commit message]
- `def5678` - [commit message]
```
