---
name: consult-llm
description: How to invoke the consult-llm CLI. Canonical reference for the invocation contract, flags, stdin/stdout format, and multi-turn. Load this before calling consult-llm from any workflow skill (/consult, /collab, /debate, /collab-vs, /debate-vs).
allowed-tools: Bash
---

Reference for invoking the `consult-llm` CLI. Workflow skills delegate here for mechanics; they focus on orchestration.

## Invocation

Run `consult-llm` with the prompt on **stdin**, using a quoted heredoc.

```bash
cat <<'__CONSULT_LLM_END__' | consult-llm -m <selector> -f src/foo.rs -f src/bar.rs
<prompt body>
__CONSULT_LLM_END__
```

Rules:

- **Run Bash in the foreground** (synchronous, no `run_in_background`). Only background the call when the caller explicitly passes `--background`. Always set `timeout: 1800000` (30 minutes) — LLM calls routinely exceed the 2-minute default.
- **ALWAYS use `<<'__CONSULT_LLM_END__'` (quoted, with this exact terminator).** The single quotes prevent shell expansion of `$var`, backticks, and escapes. The specific terminator `__CONSULT_LLM_END__` is chosen because it won't appear in model responses — never use `EOF` or `PROMPT` which commonly appear in code samples and would silently truncate the prompt.
- **Fallback to `--prompt-file <path>`** if the prompt contains `__CONSULT_LLM_END__`, or on Windows/PowerShell. Write the prompt to a temp file with `$(mktemp)`, then pass it via `consult-llm --prompt-file "$f" …`.
- **Stdout layout.** First line is `[model:<id>] [thread_id:<id>]`, then a blank line, then the response body. In `--web` mode the prefix is just `[model:<id>]` (no thread).
- **Multi-turn.** Read `[thread_id:xxx]` from line 1 and pass it back with `-t <id>` on the next call. Thread IDs are opaque strings — don't modify them. Not portable across backends.
- **Stderr** carries progress/spinner output. Ignore it.
- **Exit codes.** `0` success, `1` backend/network error (includes thread-not-found), `2` usage error, `3` configuration error (missing API key, unsupported backend).

## Models

Selectors and allowed models resolvable in this environment (availability depends on which API keys are configured):

```
!`consult-llm models`
```

Pass a selector or exact model ID to `-m` only when overriding defaults. With no `-m`, consult-llm uses configured `default_models` when that config key is present and non-empty, preserving order and duplicates. If `default_models` is unset or empty, it falls back to `default_model`, then the built-in fallback model. For same-prompt multi-model calls, omit `-m` to use those configured defaults; use repeated `-m` only for explicit overrides. For `--run`, create one `--run model=<model>,prompt-file=<path>` entry per desired run; `--run` does not consume omitted-`-m` defaults. `-m` is ignored when `--web` is used.

**Multi-model:** repeat `-m` to consult multiple model positions in parallel (e.g. `-m gemini -m openai`, max 5 total runs). You may repeat the same selector/model (e.g. `-m openai -m openai`) to get independent calls with the same prompt. The response is a group format: first line is `[thread_id:group_xxx]`, each model's answer under a `## Model: <id>` header preceded by `[model:<id>] [thread_id:<per-model-id>]`. When the same resolved model appears more than once, only those duplicate sections use `## Model: <id>#K` and `[model:<id>#K]` labels. Pass `-t group_xxx` to resume all group positions together on the next turn; pass an individual per-model thread ID with a single `-m <model>` to resume just that model outside the group context.

## Task modes

Pick a `--task` mode based on the kind of question. Omit for neutral general-purpose.

| Mode                | When to use                                                                                       |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| `general` (default) | Neutral prompt. Defers to instructions in the prompt body. Use for open questions.                |
| `review`            | Critical code reviewer — bugs, security issues, quality problems.                                 |
| `debug`             | Root-cause troubleshooter from errors/logs/stack traces. Ignores style.                           |
| `plan`              | Constructive architect — explore trade-offs, design solutions. Always ends with a recommendation. |
| `create`            | Generative writer for docs, content, or design output.                                            |

## Web mode

`--web` copies the formatted prompt (system prompt + user prompt + file context) to the clipboard and exits 0 instead of calling an LLM. **Only use when the user specifically asks for browser/web mode.** After invoking, wait for the user to paste the external LLM's response back — do not continue implementation on your own. `-m` is ignored in this mode.

## Prompt authoring

Ask neutral, open-ended questions. Do not suggest specific solutions in the prompt body — that biases the analysis. Let the LLM form its own view.

## Flags

| Flag                         | Purpose                                                         |
| ---------------------------- | --------------------------------------------------------------- |
| `-m, --model <selector\|id>` | See "Models" above. Omit for configured defaults.                   |
| `-f, --file <path>`          | Repeatable. File context — path + code block.                   |
| `-t, --thread-id <id>`       | Resume a multi-turn conversation. See "Multi-turn".             |
| `--task <mode>`              | Persona. See "Task modes" above.                                |
| `--web`                      | Clipboard mode. See "Web mode" above.                           |
| `--prompt-file <path>`       | Read prompt from file instead of stdin.                         |
| `--diff-files <path>`        | Repeatable. Include git diff for this file as context.          |
| `--diff-base <ref>`          | Base ref for diff (default `HEAD` — shows uncommitted changes). |
| `--diff-repo <path>`         | Repo path (default cwd).                                        |
| `--run <spec>`               | Per-model run. See "Per-model runs" below.                      |

Run `consult-llm --help` for the authoritative flag list.

## File context (`-f`) best practices

The consulted LLM has no access to your conversation history. Anything
it needs - source files, logs, command output, traces, timelines,
error messages - must be attached with `-f`.

- **Include conversation artifacts.** If the current session already
  produced diagnostic output relevant to the question (log excerpts,
  traces, reproduction steps, command output), attach it as a temp
  file. Prefer raw evidence over prose summaries when both exist.
- **Re-run the original command** piping to a temp file
  (`cmd > /tmp/artifact.txt`) instead of writing output from memory.
  This is cheaper, faster, and preserves the exact output.
- Source files and diagnostic artifacts are both first-class `-f`
  inputs. Do not limit context gathering to source code.

## Per-model runs

Use `--run` when a workflow needs to query multiple models in parallel with **different prompt bodies**. Do not use it for ordinary multi-model calls where the same prompt goes to every model — repeat `-m` for that.

```bash
GEMINI_PROMPT=$(mktemp)
CODEX_PROMPT=$(mktemp)

cat <<'__CONSULT_LLM_END__' >| "$GEMINI_PROMPT"
[prompt for Gemini]
__CONSULT_LLM_END__

cat <<'__CONSULT_LLM_END__' >| "$CODEX_PROMPT"
[prompt for Codex]
__CONSULT_LLM_END__

# First call — no existing threads yet
consult-llm \
  --run "model=gemini,prompt-file=$GEMINI_PROMPT" \
  --run "model=openai,prompt-file=$CODEX_PROMPT"

# Subsequent calls — continue each per-run thread
consult-llm \
  --run "model=gemini,thread=$GEMINI_THREAD,prompt-file=$GEMINI_PROMPT" \
  --run "model=openai,thread=$CODEX_THREAD,prompt-file=$CODEX_PROMPT"

# Duplicate resolved models are allowed; use distinct prompt files and distinct per-run threads.
consult-llm \
  --run "model=openai,prompt-file=$PROMPT_A" \
  --run "model=openai,prompt-file=$PROMPT_B"
```

Each `--run` value accepts `model=<selector-or-id>`, `prompt-file=<path>`, and optionally `thread=<id>`. Use `mktemp` for temporary prompt files and always use `__CONSULT_LLM_END__` as the heredoc terminator. Use `>|` to overwrite temp files in zsh (avoids `noclobber` errors).

Constraints: max 5 total runs, cannot combine with `-m`/`-t`/`--prompt-file`/`--web`, duplicate resolved models are allowed, duplicate explicit `thread=<id>` values are rejected, `thread=group_*` is rejected because `--run` uses per-run thread IDs, shared `-f` and `--diff-*` context applies to every run, prompt-file paths with commas are unsupported.

Output is the same group format as multi-model `-m` calls. Extract per-run thread IDs from each section header for subsequent `--run thread=...` turns.
