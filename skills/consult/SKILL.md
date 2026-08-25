---
name: consult
description: Consult an external LLM with the user's query.
allowed-tools: Bash, Glob, Grep, Read
---

Consult an external LLM with the user's query via the `consult-llm` CLI.

**Load the `consult-llm` skill before invoking** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Model flags:** any `--<selector>` from the Models block above selects that model (e.g. `--gemini`, `--openai`, `--deepseek`, `--minimax`). Repeat for multiple models — they run in parallel.

Translate model flags according to the loaded `consult-llm` skill's model-selection rules.

**Mode flags:**
- `--browser` → use web mode (`--web`, copies prompt to clipboard)
- `--background` → run the Bash call in background mode (`run_in_background`)

Strip all flags from the arguments to get the user query.

## Workflow

### 0. Load `consult-llm` skill

Load it now. Follow its invocation contract for all CLI calls in this workflow.

### 1. Gather context

- Use Glob/Grep to find relevant files.
- Read key files to confirm relevance.
- Do enough research to understand how the requested behavior actually works. Before starting, think about what resources would be useful to obtain first: relevant source files, tests, logs, generated files, config, examples, command output, external docs, or authoritative upstream source. Gather the cheapest useful evidence before consulting.
- Do not stop at the first plausible file, definition, setting, or example. Follow references, callers, related tests, and runtime usage until you can explain the current behavior and the likely impact of changing it.
- Select files that let reviewers understand the behavior being discussed and verify your assumptions.

### 2. Invoke

**One or more `--<selector>` flags** — single call with one `-m <selector>` per flag, plus `-f <path>` for each relevant file. Multiple selectors run in parallel and the CLI returns a combined response with per-model sections.

**No model flag (default)** — use the loaded `consult-llm` skill's default model-selection rules, plus `-f <path>` for each relevant file.

**`--browser`** — single call with `--web` (model flags are ignored in web mode).

### 3. Resolve context requests

Before using the response, apply the loaded `consult-llm` skill's context request loop. If an answer ends with a context request, resume that model independently with the requested evidence and treat its revised answer as the consultation result.

### 4. Present results

- **Normal mode (single model):** summarize key insights, recommendations, and considerations.
- **Normal mode (multiple models):** the CLI output already contains labeled per-model sections. Synthesize by highlighting agreements, noting disagreements, and presenting a unified takeaway.
- **Web mode:** inform the user the prompt was copied to clipboard and ask them to paste it into their browser-based LLM and share the response back.

## Critical rules

- ALWAYS gather file context before consulting.
- **Pass through the user's query faithfully** — do NOT add your own theories, suspects, analysis, or suggested solutions to the prompt. The user's intent is the prompt. Rephrase as needed so the prompt reads as a direct question to the LLM, not a meta-instruction to you. You may add brief factual context (e.g. "we recently changed X to Y"), but never inject your own diagnostic opinions or hypotheses. Do not pass the user's query verbatim if it is phrased as an instruction to you rather than a question for the LLM.
- Provide focused, relevant files (quality over quantity).
