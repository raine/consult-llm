---
name: implement
description: Explicit workflow for one bounded implementation using source-grounded discovery, a walking slice, evidence-gated review, validation, and commit. Use only when the user invokes `/implement` or another skill explicitly delegates to it. Do not trigger for ordinary coding requests, follow-up edits, fixes with an established design, or requests to amend an existing commit.
allowed-tools: Bash, Glob, Grep, Read, Edit, Write
---

# Implement workflow

Implement one bounded unit in the current worktree. Prefer the smallest complete
change that satisfies the requested behavior. The current agent researches and
implements the unit directly, so do not write the implementation twice as a
code-bearing plan.

Presets increase required evidence, not architecture, abstractions, tests, or
documentation.

## Arguments

Arguments are `$ARGUMENTS`.

Parse these flags before starting:

- `--preset light|standard|design|strict`: default `standard`
- `--parent-plan <path>`: authoritative scope or phase brief
- `--reviewer <selector>`: consult-llm reviewer selector, repeatable
- `--reviewers <selector,selector>`: comma-separated reviewer selectors
- `--validation <command>`: expected validation command

Everything else is the implementation request.

Presets:

- `light`: an established local pattern, focused validation, no external review
- `standard`: the default, acceptance evidence and runtime exercise where
  applicable, no routine external review
- `design`: a meaningful API, ownership, or architecture choice, with one
  source-grounded technical-shape review before edits
- `strict`: authentication, secrets, untrusted input, protocols, migrations,
  persistence, destructive behavior, concurrency, or FFI, with boundary-focused
  evidence and one final diff review

Treat a component with a strict boundary as strict even when the selected preset
is lighter. Strictness adds relevant proof. It does not justify speculative
hardening.

Do not ask the user during the workflow unless continuing safely requires a
material product choice, public or irreversible contract, dependency,
durable-state change, trust-model change, unsafe overwrite, or major scope
expansion.

## Working record

Keep at most one evolving implementation brief under `history/` with today's
date prefix. A brief is required only when:

- the preset is `design` or `strict`
- the caller explicitly requests one
- discoveries are complex enough that a durable execution record prevents
  mistakes

A sufficient parent plan replaces the brief. Add a local note only for material
facts or deviations absent from the parent plan.

Use this compact shape when a brief is needed:

```markdown
# Implement: <topic>

**Goal:** <one observable outcome>
**Preset:** <preset>
**Parent plan:** <path or n/a>
**Validation:** <commands>

## Scope

- In:
- Out:

## Source facts

- `<path>:<symbol>`: <ownership, contract, or convention>

## Technical shape

- Existing mechanisms to reuse:
- Smallest complete slice:
- Acceptance evidence:
- Real trust or compatibility boundaries:
- Stop conditions:

## Accepted review findings

- <only independently reproduced findings>

## Result

- Acceptance evidence:
- Validation:
- Commit:
- Blockers:
```

Do not pre-write source or test code in the brief. Do not create separate plans,
proposal captures, feedback ledgers, or review transcripts. Update the brief only
when scope, technical ownership, accepted evidence, or the result changes. Briefs
are workflow records. Do not stage or commit them.

A result sentinel is a separate artifact only when a parent plan or caller
requires one.

## 1. Establish facts

1. Read active repository instructions and relevant architecture documentation.
2. Record `git rev-parse HEAD` and inspect `git status --short`.
3. Stop before overwriting or unsafely entangling unrelated user changes.
4. Read a supplied parent plan and treat its scope and acceptance criteria as
   authoritative.
5. Find the nearest existing implementation, callers, tests, configuration, and
   runtime path. Prefer repository mechanisms over new ones.
6. Define the observable outcome, explicit non-goals, smallest complete slice,
   actual trust or compatibility boundaries, and acceptance evidence.
7. Select the narrowest useful focused checks plus every repository-required
   check. Use `--validation` unless it is plainly wrong.

Do not invent future consumers, extension points, defensive layers, or tests for
unchanged framework behavior.

## 2. Review the technical shape when required

Skip this section unless the preset is `design`. A run receives at most one
external review. Do not run a second review to approve corrections.

Before calling consult-llm, load the `consult-llm` skill and follow its invocation
contract. Attach the brief or parent plan and focused source files. Use
`--task review`, supplied reviewer selectors when present, the quoted heredoc
terminator `__CONSULT_LLM_END__`, and Bash timeout `600000`.

Ask whether the technical shape demonstrably conflicts with source-established
ownership, behavior, contracts, or trust boundaries. Do not ask for a replacement
plan or general hardening advice.

Every reported finding must include:

- **Claim:** the specific incorrect behavior, contradiction, or execution blocker
- **Trigger:** the concrete input, state, caller, or plan step that exposes it
- **Expected:** the acceptance criterion, existing contract, repository
  convention, or real boundary requirement
- **Actual:** the behavior the proposed shape would produce
- **Evidence:** exact files and symbols plus a manual procedure, command, or
  complete reachable source trace
- **Smallest correction:** the minimum change that fixes the issue
- **Verification:** the check that proves the correction

Tell the reviewer to return no findings when no issue meets this standard.
General hardening, hypothetical edge cases, style preferences, alternative valid
designs, and speculative callers are not findings.

Independently confirm each finding before accepting it. For a pre-implementation
finding, compare the named plan step with the complete current source path and
identify the exact contract or acceptance criterion that would fail. Reviewer
assertions alone are not evidence.

Ignore findings that cannot be reproduced, have no reachable trigger, protect no
accepted behavior or real boundary, duplicate lower-layer guarantees, or ask for
more hardening than the demonstrated issue requires. Record only accepted
findings and their smallest corrections in the brief. Do not preserve rejected
feedback in a ledger.

## 3. Implement a walking slice

Implement serially in the current worktree:

1. Build the thinnest complete path through existing owners.
2. Reuse repository conventions before adding state, types, helpers, or error
   layers.
3. Add an early test only when it captures a demonstrated regression, pure
   contract, parser, protocol invariant, or trust-boundary invariant needed to
   make the slice safe.
4. Compile or run the changed path as soon as it works.
5. Add remaining tests only when they prove accepted behavior, platform behavior,
   compatibility, a demonstrated regression, or a real trust boundary.
6. Keep provisional APIs local and easy to reshape until a real second consumer
   proves a broader boundary.

Continue with best judgment when a correction preserves accepted behavior,
reduces complexity, follows an existing convention, and stays within scope. Stop
only when a stop condition from the opening section fires.

Revisit the technical shape when implementation introduces a generic mechanism,
pass-through layer, one-consumer abstraction, unexplained convention deviation,
or material scope growth. Prefer deletion or a smaller local form.

## 4. Exercise, simplify, and validate

For every acceptance criterion, obtain concrete evidence from a focused test,
manual reproduction, real process or application exercise, or source proof for a
static contract.

For `standard` and above, exercise the changed runtime path and one representative
failure or constrained state when applicable. For strict components, verify only
the relevant input, authorization, persistence, compatibility, error,
cancellation, allocation, and destructive-state boundaries.

Audit the actual diff for:

- unexplained scope growth or ownership changes
- accidental overwrite of user changes
- duplicated logic or tests
- pass-through layers and single-consumer abstractions
- tests of unchanged framework behavior
- protections that must remain at real trust or compatibility boundaries

Run all focused validation and repository-required checks.

### Conditional final diff review

Run one final diff review only for `strict`, or when the implementation materially
diverges from the technical shape or changes a public or generic framework
contract. If a `design` run already used its review, verify the diff directly
instead. Never create a review cycle.

Load the `consult-llm` skill and use the same invocation requirements from
section 2. Attach the diff, acceptance criteria, and focused source context. Ask
for deletion-first review and concrete correctness findings. Require every
finding to use the Claim, Trigger, Expected, Actual, Evidence, Smallest
correction, and Verification fields above.

The reviewer must not report hypothetical failures, speculative compatibility,
defense-in-depth without a real boundary, new extensibility, unchanged framework
behavior, style preferences, or alternative valid designs. A minimality finding
must identify exact code that serves no accepted behavior or real boundary.

Before changing code, independently reproduce each finding through the real entry
point, a safe command, or a complete source trace from a real boundary to the
failure. For unsafe or destructive triggers, source proof must demonstrate the
reachable path and violated invariant without executing harm. For a minimality
finding, confirm the existing owner or single consumer, apply the smallest safe
deletion, and rerun the relevant evidence.

Apply only the smallest correction that resolves a reproduced issue. Ignore
unreproduced suggestions. Do not request follow-up review. If a reproduced fix
requires redesign or scope expansion, stop and report the blocker.

## 5. Commit and report

Commit when acceptance evidence and required validation pass, no blocker remains,
and repository instructions permit committing. Never commit workflow records.

When a result sentinel is required, follow the caller's exact path and format.
Write it after committing so its commit fields are final. If the caller supplies
no format, use:

```markdown
# Implementation Result: <topic>

status: success | blocked | failed
head_commit: <sha or pending>
commit: <sha or pending>
validation: <commands>
validation_status: passed | failed | skipped

## Summary

- <what changed>

## Acceptance

- <criterion>: met | not met | unknown, with evidence

## Blockers

- <blocker or none>
```

Report concisely:

```markdown
## Result

- Outcome: <observable result>
- Main implementation: <owners and mechanisms reused>
- Acceptance evidence: <tests or runtime proof>
- Review: <none | accepted reproduced findings | blocker>
- Validation: <commands and results>
- Commit: <sha or reason absent>
- Remaining risks: <none or bounded demonstrated risks>
- Sentinel: <path or n/a>
```
