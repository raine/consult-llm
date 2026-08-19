---
name: consult-llm
description: How to invoke the consult-llm CLI. Canonical reference for the invocation contract, flags, stdin/stdout format, and multi-turn. Load this before calling consult-llm from an llm-* workflow skill.
allowed-tools: Bash
cli_version: "3.0.30"
schema_version: 1
---

<!-- Installed by `consult-llm skill install` — name=consult-llm cli_version=3.0.30 schema_version=1; do not hand-edit. -->
Reference for invoking the `consult-llm` CLI. Workflow skills delegate here for mechanics; they focus on orchestration.

## Invocation

Run `consult-llm` with the prompt on **stdin**, using a quoted heredoc.

```bash
cat <<'__CONSULT_LLM_END__' | consult-llm -m <selector> -f src/foo.rs -f src/bar.rs
<prompt body>
__CONSULT_LLM_END__
```

Rules:

- **Run Bash in the foreground** (synchronous, no `run_in_background`). Only background the call when the caller explicitly passes `--background`. Always set `timeout: 600000` (10 minutes) — LLM calls routinely exceed the 2-minute default.
- **ALWAYS use `<<'__CONSULT_LLM_END__'` (quoted, with this exact terminator).** The single quotes prevent shell expansion of `$var`, backticks, and escapes. The specific terminator `__CONSULT_LLM_END__` is chosen because it won't appear in model responses — never use `EOF` or `PROMPT` which commonly appear in code samples and would silently truncate the prompt.
- **Fallback to `--prompt-file <path>`** if the prompt contains `__CONSULT_LLM_END__`, or on Windows/PowerShell. Write the prompt to a temp file with `$(mktemp)`, then pass it via `consult-llm --prompt-file "$f" …`.
- **Stdout layout.** First line is `[model:<id>] [thread_id:<id>]`, then a blank line, then the response body. In `--web` mode the prefix is just `[model:<id>]` (no thread).
- **Multi-turn.** Read `[thread_id:xxx]` from line 1 and pass it back with `-t <id>` on the next call. Thread IDs are opaque strings — don't modify them. Not portable across backends.
- **Stderr** carries progress/spinner output. Ignore it.
- **Exit codes.** `0` success, `1` backend/network error (includes thread-not-found), `2` usage error, `3` configuration error (missing API key, unsupported backend).

## Output capture (never truncate a multi-model run)

The **entire** response goes to **stdout** — there is no `--output` flag. Multi-model (`-m` repeated) and `--run` responses, especially over multiple turns, are **long**, and two capture paths silently drop content:

- A **backgrounded** Bash call (`run_in_background`) captures stdout to a task `.output` file that is a **rolling terminal scrollback** — early `## Model:` sections scroll off the top and are lost for good (this is not a `tail` you can undo; the bytes are gone).
- Even a **foreground** call can exceed the Bash tool's inline-output cap for a big panel.

So for **any run you will parse structurally** (multi-model, `--run`, or multi-turn reviews) **redirect stdout to a durable file and read that file** — never rely on the inline Bash capture or a background task's `.output`:

```bash
out="$(mktemp)"
cat <<'__CONSULT_LLM_END__' | consult-llm -m gemini -m openai -m deepseek -f src/foo.rs > "$out"
<prompt body>
__CONSULT_LLM_END__
```

Then read `$out` with Read/grep. **Verify completeness before using the result:** an N-model call must contain N `## Model:` headers — assert it (`grep -c '^## Model:' "$out"`). If fewer, the run is **incomplete**: re-run it; do **not** proceed on a partial panel and never present one surviving model's findings as the panel's verdict.

## Models

Selectors and allowed models resolvable in this environment (availability depends on which API keys are configured):

```
!`consult-llm models`
```

Pass a selector or an exact model ID to `-m`. Only enabled selectors are listed — anything not shown has no available model. **Usually omit `-m`** to use the configured default; pass it explicitly only when the user names a specific model. `-m` is ignored when `--web` is used.

**Multi-model:** repeat `-m` to consult multiple models in parallel (e.g. `-m gemini -m openai`, max 5). The response is a group format: first line is `[thread_id:group_xxx]`, each model's answer under a `## Model: <id>` header preceded by `[model:<id>] [thread_id:<per-model-id>]`. Pass `-t group_xxx` to resume all models together on the next turn; pass an individual per-model thread ID with a single `-m <model>` to resume just that model while keeping the group context.

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
| `-m, --model <selector\|id>` | See "Models" above. Usually omit.                               |
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

# Subsequent calls — continue each model's thread
consult-llm \
  --run "model=gemini,thread=$GEMINI_THREAD,prompt-file=$GEMINI_PROMPT" \
  --run "model=openai,thread=$CODEX_THREAD,prompt-file=$CODEX_PROMPT"
```

Each `--run` value accepts `model=<selector-or-id>`, `prompt-file=<path>`, and optionally `thread=<id>`. Use `mktemp` for temporary prompt files and always use `__CONSULT_LLM_END__` as the heredoc terminator. Use `>|` to overwrite temp files in zsh (avoids `noclobber` errors).

Constraints: max 5 runs, cannot combine with `-m`/`-t`/`--prompt-file`/`--web`, duplicate resolved models are rejected, shared `-f` and `--diff-*` context applies to every run, prompt-file paths with commas are unsupported.

Output is the same group format as multi-model `-m` calls. Extract per-model thread IDs from `[thread_id:xxx]` in each model's section header for subsequent turns.
