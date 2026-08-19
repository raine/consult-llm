---
name: llm-debate
description: LLMs propose and critique approaches, agent moderates the debate and synthesizes the best solution, then implements.
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-debate cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Have multiple LLMs debate the best approach, then synthesize and implement.

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

**Model flags:** any `--<selector>` from the Models block above selects a debater (e.g. `--gemini`, `--openai`, `--deepseek`). Repeat for multiple. Need at least **two** debaters. With no model flag, debate uses **all** listed selectors.

Translate each `--<selector>` into a `-m <selector>` argument to the CLI.

**Mode flags:**
- `--dry-run` → debate and plan only, skip implementation
- `--skip-final` → skip the final review phase
- `--rounds N` → number of debate rounds (default: 2, max: 3)

Strip all flags from arguments to get the task description.

Throughout this skill, references to "each LLM"/"each debater" mean every selected model. Use the selector name (`gemini`, `openai`, etc.) as the label when presenting per-model output.

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

Have both LLMs propose their approach independently (in parallel).

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

Invoke `consult-llm` with one `-m <selector>` per debater and `-f <path>` for each relevant source file, sending the opening prompt per the consult-llm invocation contract. All models are queried in parallel in a single call.

**Extract per-model thread IDs** from the response — needed for Phase 3 since each model receives the others' rebuttals.

## Phase 3: Debate Rounds

For each round (default 2, configurable with `--rounds N`, max 3):

Have each LLM critique the others' latest arguments (in parallel). Pass each LLM's thread ID via `-t <id>` to continue its conversation — they already have full context of the task and their own prior arguments, so you only need to send the opponents' latest responses.

**Round 1 rebuttal prompt (same template for each debater; embed every other debater's opening argument, labeled by selector):**
```
Your opponent(s) proposed these alternative approaches:
[Opponents' opening arguments, each labeled with the selector name]

Provide a rebuttal:
1. **Critique**: What are the weaknesses in each opponent's approach?
2. **Defense**: Address any weaknesses in your own approach
3. **Concessions**: Are there any good ideas worth adopting?
4. **Updated position**: State your refined recommendation

Be constructive but thorough in your critique.
```

**Subsequent round prompt (same template; embed every other debater's latest rebuttal):**
```
Your opponent(s) have responded to your critique:
[Opponents' latest rebuttals, each labeled with the selector name]

Continue the debate:
1. **Critique**: What weaknesses remain in their updated positions?
2. **Defense**: Address any new points raised against your approach
3. **Concessions**: Any new ideas worth adopting?
4. **Updated position**: State your refined recommendation

Focus on unresolved disagreements. Don't repeat settled points.
```

Each model receives every other model's latest response. Invoke `consult-llm` once with one `--run` per debater, continuing each model's thread.

Present both responses to the user after each round.

## Phase 4: Moderator's Verdict

As the moderator, analyze the debate and synthesize the best approach:

1. **Score the arguments**:
   - Which approach is simpler?
   - Which approach better fits existing patterns?
   - Which critiques were valid?
   - What concessions were made?

2. **Identify consensus**: Where did all the debaters agree?

3. **Resolve disagreements**: For each point of contention:
   - Evaluate the arguments from each side
   - Pick the strongest position or find a middle ground
   - Prefer simpler solutions when arguments are equally strong

4. **Write the verdict** as part of the plan:

````markdown
# [Feature Name] Implementation Plan

**Goal:** [One sentence describing what this builds]

## Debate Summary

**Positions** (one bullet per debater, labeled with the selector name):
- **<selector>:** [1-2 sentence summary]

**Points of agreement:**
- [Consensus point 1]
- [Consensus point 2]

**Resolved disagreements:**
- [Issue]: <selector-A> said X, <selector-B> said Y → **Verdict:** [Your decision and why]

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

Save the plan to a user-requested path, or default to `plan-<feature-name>.md` in the current working directory.


## Phase 5: Implement

**If `--dry-run`:** Skip to Phase 7 (Summary) - report the debate and plan without implementing.

Implement the plan without further interaction:

1. **Follow the plan exactly** - implement each task in order
2. **Commit after each logical unit** - keep commits small and focused
3. **If something is unclear** - make a reasonable decision and note it in the commit message
4. **If a task fails** - attempt to fix it before moving on
5. **Only stop if there's a blocking error** that cannot be resolved

Implementation rules:
- Work through tasks sequentially
- Test changes when possible
- Keep commits atomic and well-documented
- Use commit messages that explain the "why"

## Phase 6: Final Review

**If `--skip-final`:** Skip to Phase 7 (Summary).

After implementation, have every debater LLM review the result (in parallel). Pass each LLM's thread ID via `-t <id>` to continue its conversation — they already have full context of the task and the debate, so you only need to send the review prompt and the diff.

**Final review prompt:**
```
Forget which side you argued during the debate. Review the implementation purely on its merits:
- Any obvious bugs or edge cases missed?
- Code quality issues (error handling, naming, structure)?
- Deviations from best practices?
- Security concerns?

Be concise. Only flag issues worth fixing.
```

Invoke `consult-llm --task review` once with one `--run` per debater, passing `--diff-files` and `--diff-base` as shared context, continuing each model's thread.

**Apply fixes** if multiple reviewers identify the same issue, or if one raises a clearly valid concern:
- Fix bugs and edge cases
- Commit each fix separately with clear messages

**Skip** minor style suggestions or conflicting opinions.

## Phase 7: Summary

Present a final summary to the user:

```
## Summary

**Implemented:** [One sentence describing what was built]

**Debate outcome:**
- One bullet per debater, labeled with the selector: `<selector>` advocated: [key position]
- Final verdict: [synthesized approach]

**Key decisions from debate:**
- [Decision 1 and why]
- [Decision 2 and why]

**Post-implementation fixes:**
- [Fix applied after final review, if any]

**Commits:**
- `abc1234` - [commit message]
- `def5678` - [commit message]
```
