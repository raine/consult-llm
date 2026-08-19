---
name: llm-consult
description: Consult an external LLM with the user's query.
allowed-tools: Bash, Glob, Grep, Read
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=llm-consult cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Consult an external LLM with the user's query via the `consult-llm` CLI.

**Load the `consult-llm` skill before proceeding** — it defines the invocation contract (stdin heredoc, flags, output format, multi-turn). Do not call the CLI without loading it first.

## Available models

Selectors resolvable in this environment (depends on configured API keys):

```
!`consult-llm models`
```

## Argument handling

**Arguments:** `$ARGUMENTS`

Check `$ARGUMENTS` for flags:

**Model flags:** any `--<selector>` from the Models block above selects that model (e.g. `--gemini`, `--openai`, `--deepseek`, `--minimax`). Repeat for multiple models — they run in parallel. With no model flag, consult **all** listed selectors in parallel.

Translate each `--<selector>` into a `-m <selector>` argument to the CLI.

**Mode flags:**
- `--browser` → use web mode (`--web`, copies prompt to clipboard)
- `--background` → run the Bash call in background mode (`run_in_background`). **Must redirect stdout to a `$(mktemp)` file** (`consult-llm … > "$out"`) and read that file — a backgrounded call's rolling `.output` scrollback drops early `## Model:` sections on a long multi-model run. See `consult-llm` skill → "Output capture".

Strip all flags from the arguments to get the user query.

## Workflow

### 0. Load `consult-llm` skill

Load it now. Follow its invocation contract for all CLI calls in this workflow.

### 1. Gather context

- Use Glob/Grep to find relevant files.
- Read key files to confirm relevance.
- Select files directly related to the question — quality over quantity.

### 2. Invoke

**One or more `--<selector>` flags** — single call with one `-m <selector>` per flag, plus `-f <path>` for each relevant file. Multiple selectors run in parallel and the CLI returns a combined response with per-model sections.

**No model flag (default)** — single call with `-m <selector>` repeated for **every** selector in the Models block, plus `-f <path>` for each relevant file.

**`--browser`** — single call with `--web` (model flags are ignored in web mode).

### 3. Present results

- **Normal mode (single model):** summarize key insights, recommendations, and considerations.
- **Normal mode (multiple models):** first **verify every model is present** — the output must contain one `## Model:` header per selector you passed (`grep -c '^## Model:'`). If any are missing the run is incomplete (see `consult-llm` skill → "Output capture"): re-run it. Do **not** synthesize from a partial panel or present one surviving model as the group verdict. Once complete, synthesize — highlight agreements, note disagreements, present a unified takeaway.
- **Web mode:** inform the user the prompt was copied to clipboard and ask them to paste it into their browser-based LLM and share the response back.

## Critical rules

- ALWAYS gather file context before consulting.
- **Pass through the user's query faithfully** — do NOT add your own theories, suspects, analysis, or suggested solutions to the prompt. The user's intent is the prompt. Rephrase as needed so the prompt reads as a direct question to the LLM, not a meta-instruction to you. You may add brief factual context (e.g. "we recently changed X to Y"), but never inject your own diagnostic opinions or hypotheses. Do not pass the user's query verbatim if it is phrased as an instruction to you rather than a question for the LLM.
- Provide focused, relevant files (quality over quantity).
