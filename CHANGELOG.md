# Changelog

## v3.0.27 (2026-07-15)

- API providers now support custom base URLs for proxies, gateways, and provider-specific endpoints, with shared project config protected from redirecting API credentials. ([#15](https://github.com/raine/consult-llm/pull/15))
- The `/implement` workflow now runs only when explicitly invoked and executes plans directly without a separate sideagent executor.

## v3.0.26 (2026-07-12)

- Grok CLI consultations now report accurate prompt and completion token usage.

## v3.0.25 (2026-07-12)

- Grok CLI and API backends now support configurable `low`, `medium`, or `high` reasoning effort.

## v3.0.24 (2026-07-12)

- Added Grok CLI backend with multi-turn conversations, file context, streamed reasoning, and configurable extra arguments.

## v3.0.23 (2026-07-10)

- Added GPT-5.6 Sol support, with the `openai` selector preferring it when available.

## v3.0.22 (2026-06-27)

- Claude CLI runs now pass attached files as `@file` references instead of inlining file contents, avoiding prompt-size failures for large file context.

## v3.0.21 (2026-06-25)

- Added z.ai (GLM) provider support, including the `zai` selector, `glm-5.2`, and `ZAI_API_TOKEN` API configuration.
- OpenRouter OpenCode runs now support configured reasoning effort variants.
- OpenCode sessions now keep separate session databases per conversation, avoiding cross-session history interference.
- OpenCode cost reporting now uses the cost reported by OpenCode when available.
- Provider backends can now receive provider-specific environment variables from user or local config.

## v3.0.20 (2026-06-12)

- Added a native Claude CLI backend for Anthropic models, so `anthropic.backend: claude-cli` can use the stock `claude` command without defining a custom profile.
- Claude CLI runs now support configurable reasoning effort and extra arguments through config files or `CONSULT_LLM_CLAUDE_*` environment variables.
- `consult-llm doctor` now checks for the `claude` command when the native Claude CLI backend is selected.
- Monitor detail views now clear the in-progress stage once a run finishes.

## v3.0.19 (2026-06-12)

- OpenCode CLI runs now show tool calls and tool errors in the monitor detail view.

## v3.0.18 (2026-06-10)

- Claude CLI profiles now support thread resume, letting you continue conversations across multiple turns through the `--resume` session mechanism. Previously thread resume was unsupported for Claude CLI; now it works like the other CLI backends

## v3.0.17 (2026-06-10)

- New OpenRouter provider: `openrouter/*` models are now supported through both API (OpenAI-compatible) and OpenCode backends. Use `-m openrouter/auto` or any specific OpenRouter model ID
- Usage summary now renders as a single token line at the end of the detail view instead of appearing inline between content blocks

## v3.0.16 (2026-06-10)

- Claude CLI profiles are now much simpler to configure: `command` defaults to `claude`, `interface` to `stream-json`, and `prompt` to `stdin`. Required boilerplate flags and env vars are auto-injected, so profiles only need the values that differ from defaults
- Added `effort` field to CLI profiles, letting you set the reasoning effort level per profile
- Removed the `headless` field from CLI profiles — all profiles are now treated as non-interactive
- CLI backends now prompt agents to inspect repository files and gather evidence before answering, improving response quality

## v3.0.15 (2026-06-08)

- Added reusable CLI backend profiles, so any model family can route through a configured headless CLI process while keeping the normal model selector
- Added Claude CLI profile support for Anthropic and profile-backed models, including streamed thinking, tool, session, and usage events in monitor output
- `consult-llm` now uses configured `default_models` when `-m` is omitted, making multi-model defaults work automatically in CLI and skill workflows
- `consult-llm doctor` now validates profile-backed backends and reports missing, non-headless, or unavailable profile commands before a run starts

## v3.0.14 (2026-06-06)

- Cursor-specific model names (`auto`, `composer`, `kimi`) are now accepted in the enabled model catalog when using the `cursor-cli` backend, letting you select them directly as model selectors
- The `consult-llm models` command now reports `cursor-cli` as the backend for dynamically accepted cursor model IDs

## v3.0.13 (2026-05-26)

- `-f` file context guidance now tells agents to attach conversation artifacts (logs, traces, command output) as temp files, not just source code. Agents that already have diagnostic output in their session will forward the raw evidence instead of paraphrasing it in the prompt

## v3.0.12 (2026-05-15)

- Gemini 2.5 API calls no longer receive Gemini 3 thinking configuration

## v3.0.10 (2026-05-01)

- Added Grok 4.3 as a first-class API model, including configuration docs and cost estimates for xAI runs
- The same model can now be invoked multiple times in a single call (e.g. `-m gemini -m gemini`)
- Project `allowed_models` settings now prevent user-level defaults from selecting models outside the project allowlist
- `consult-llm models` now shows default model information

## v3.0.9 (2026-04-29)

- `consult-llm-monitor` TUI now supports mouse input: click rows to select or open them, and use the scroll wheel to navigate
- New `phased-implement` workflow skill for coordinating multi-phase implementations across worktree branches, with support for sequential, parallel, and DAG dependency ordering
- `implement` skill: added `--consult-first` flag for unbiased planning by gathering independent external proposals before writing a behavioral spec
- `implement` skill: simplified by removing `--rigor`, `--commit`, `--skip-final`, `--dry-run`, and `--diff-base` flags. Reviews now follow a single standardized path, and auto-commit at the end of implementation is now the default behavior

## v3.0.8 (2026-04-27)

- `install-skills` interactive prompt now installs to a single platform per run; re-run the command to install to another platform
- Workflow skills are now discoverable by model invocation again (removed `disable-model-invocation` from `collab`, `collab-vs`, `debate`, `debate-vs`, `implement`, `panel`, `review-panel`, and `workshop`)

## v3.0.7 (2026-04-27)

- `doctor` now validates the configured cursor model and reasoning effort against `cursor-agent --list-models` and reports a hard error when the combination would be rejected (e.g. `gpt-5.5` only accepts `medium`/`high`/`extra-high`)
- Cursor backend now maps `low` → `medium` and `xhigh` → `extra-high` when targeting `gpt-5.5`, so existing `codex_reasoning_effort` configs keep working
- `implement` skill now verifies reviewer findings empirically before adopting them, reducing churn from hallucinated or contrived issues

## v3.0.5 (2026-04-26)

- Added new skills: `implement`, `workshop`, `panel`, and `review-panel`
- Config discovery now walks past nested `.git` directories so configs in parent repos are found from inside submodules or worktrees
- Workflow skills set `disable-model-invocation` so they only run when explicitly invoked

## v3.0.4 (2026-04-26)

- Added support for passing extra CLI arguments to the `codex` and `gemini` backends
- `XDG_CONFIG_HOME` is now honored when resolving the user config file
- Monitor: detail view now wraps text based on terminal width, and the Model column is sized to fit the longest model name
- When all consultations fail, per-model error messages are now included in the output

## v3.0.3 (2026-04-26)

- API executors now stream responses via SSE for faster, incremental output
- Reasoning/thinking output is now surfaced for Gemini, DeepSeek, and other reasoning models when using API backends
- `install-skills` now has an interactive multi-select UI for choosing which platforms to install to, with no platforms pre-selected

## v3.0.1 (2026-04-26)

- `consult-llm` is now published to crates.io and can be installed with `cargo install consult-llm`

## v3.0.0 (2026-04-26)

- **Migrated from MCP server to standalone CLI.** `consult-llm` is now a
  native binary invoked directly in the terminal or from agent skills via
  Bash. The MCP server, rmcp runtime, and npm packaging have been removed.
  Skills have been rewritten to invoke the CLI instead of the MCP tool.
- Package renamed from `consult-llm-mcp` to `consult-llm`
- Added Homebrew tap: `brew install raine/consult-llm/consult-llm`
- Added layered YAML config file support. Settings can now be declared in
  `~/.config/consult-llm/config.yaml` (user), `.consult-llm.yaml`
  (project, committed), or `.consult-llm.local.yaml` (local overrides, not
  committed). Environment variables still take highest precedence
- API keys can now be stored in `.consult-llm.local.yaml` or the user
  config file. Writing `api_key` to the committed project config is blocked
  to prevent accidental secret exposure
- Added `consult-llm config set <key> <value>` subcommand for editing YAML
  config from the CLI using dot-notation keys (e.g.
  `consult-llm config set gemini.backend gemini-cli`)
- Added `consult-llm install-skills` subcommand. Replaces the curl-piped
  shell script; skill files are bundled at compile time and written to the
  detected platform directories (`~/.claude/skills/`,
  `~/.config/opencode/skills/`, `~/.codex/skills/`)
- Added `consult-llm docs` subcommand that prints the bundled README to
  stdout
- Added parallel multi-model support: pass `-m` multiple times
  (e.g. `-m gemini -m openai`) to query models concurrently in a single
  call. Multi-turn group threads are supported via the returned
  `group_<uuid>` thread ID
- Added `--run <spec>` flag for per-model runs, enabling different prompt
  bodies to be sent to different models in a single invocation
- Moved user config directory from `~/.consult-llm/` to
  `~/.config/consult-llm/` for XDG Base Directory compliance. Supports
  `$XDG_CONFIG_HOME`. The legacy `~/.consult-llm/` path is still read for
  backward compatibility, and existing configs are auto-migrated on startup
- Monitor: use full worktree name as project identifier in the TUI

## v2.13.4 (2026-04-24)

- Updated DeepSeek model from `deepseek-reasoner` to `deepseek-v4-pro`. The
  `deepseek` selector now resolves to the new model

## v2.13.3 (2026-04-23)

- Added gpt-5.5 model support ($5/$30 per million tokens). The `openai` selector
  now resolves to gpt-5.5, and the Cursor CLI backend automatically appends the
  reasoning effort suffix when routing through cursor-agent

## v2.13.2 (2026-04-22)

- Added Anthropic provider support with the `claude-opus-4-7` model. Configure
  with `ANTHROPIC_API_KEY`; select via the `anthropic` selector or the exact
  model ID. API backend only (no CLI backend).
- Monitor: press `K` on an active consultation to kill a stuck agent process
  after confirming
- Fixed cursor-cli backend failing with "Cannot use this model: gpt-5.4" when
  using the `openai` selector, by automatically appending the reasoning effort
  suffix

## v2.13.1 (2026-04-08)

- Monitor now shows the full error message in the detail view when a
  consultation fails, instead of only indicating failure
- Improved startup logging: available models are now logged with their backends,
  and the working directory is included in the server start log
- System prompts for review and plan modes now encourage bolder architectural
  recommendations rather than defaulting to minimal changes

## v2.13.0 (2026-03-29)

- Monitor: press `r` in detail view to jump directly to the response section,
  available in both single consultation and thread detail views

## v2.12.1 (2026-03-28)

- CLI backends now receive prompts via stdin instead of command-line arguments,
  avoiding exposure in `ps` output and ARG_MAX limits with large prompts

## v2.12.0 (2026-03-28)

- Added OpenCode CLI as a new backend option, routing models through OpenCode's
  75+ providers (Copilot, OpenRouter, Ollama, etc.) without needing direct API
  keys. Configure per-provider with `CONSULT_LLM_OPENCODE_<FAMILY>_PROVIDER` env
  vars. Supports multi-turn via `--session` and file refs via `--file`.
- Added MiniMax M2.7 provider support
- DeepSeek and MiniMax backends are now configurable (previously hardcoded to
  API-only), enabling e.g. `CONSULT_LLM_MINIMAX_BACKEND=opencode`

## v2.11.0 (2026-03-27)

- Added multi-turn thread support for API backends. Threads are stored as JSON
  files under `$XDG_STATE_HOME/consult-llm-mcp/threads/` and replayed as the
  messages array on each call. Expired threads (>7 days) are cleaned up
  automatically. All backends now support `thread_id`.
- Fixed API cost tracking undercounting tokens for thinking models (e.g.
  gemini-3.1-pro-preview). Thinking tokens excluded from `completion_tokens` are
  now derived from `total_tokens`.
- Monitor: show cost information in history table, detail view header, usage
  separator lines, and thread detail header. Cost is only shown for API backend
  consultations.
- Monitor: show files as compact path list in detail view instead of inlined
  file contents
- Fixed `reasoning_effort` incorrectly showing for non-codex models on
  cursor_cli backend

## v2.10.0 (2026-03-15)

- Monitor: cycle between sibling consultations (started around the same time)
  with Tab/Shift+Tab in detail view
- Monitor: sort projects by most recent consultation activity
- Monitor: add Esc key to quit from table view

## v2.9.0 (2026-03-14)

- Added `consult-llm-mcp update` self-update command. Updates both
  `consult-llm-mcp` and `consult-llm-monitor` if present.

## v2.8.0 (2026-03-13)

- Replaced hardcoded model enum with abstract selectors (`gemini`, `openai`,
  `deepseek`) that resolve to the best available model at query time. This
  avoids the need to hardcode a specific model in the caller side.
- Responses now include a `[model:xxx]` prefix showing which concrete model was
  used
- Default Codex reasoning effort to "high" (was previously unset)
- Monitor: added Task column to active and history tables
- Monitor: show task mode and reasoning effort in detail view header
- Monitor: press `s` in detail view to toggle system prompt display
- Monitor: system prompt is now recorded in sidecar event files for viewing in
  the TUI

## v2.7.4 (2026-03-13)

- Fixed Linux prebuilt binaries failing on older distros due to glibc version
  mismatch by switching to musl static linking

## v2.7.1 (2026-03-09)

- Monitor: show "Thinking..." spinner when thinking events are streaming
- Monitor: auto-enable follow mode when scrolled to bottom in detail view
- Monitor: sort servers with active consultations above idle ones
- Monitor: show tool error messages in detail view
- Fixed cursor-agent thinking deltas containing literal `\n` instead of newlines
- Fixed cursor-agent crash on unknown tool types
- Fixed monitor event flushing for real-time detail view updates

## v2.7.0 (2026-03-08)

- Fixed cursor-agent tool success detection
- Show cursor thinking text content in monitor detail view instead of a static
  "Thinking..." label
- Added PageUp/PageDown support in monitor detail view

## v2.6.0 (2026-03-08)

- Added `consult-llm-monitor` TUI dashboard for real-time monitoring of active
  consultations, with history tracking, detail view, and keyboard navigation

## v2.5.6 (2026-03-07)

- Added bash installer (`curl | bash`) for installing without Node.js

## v2.5.4 (2026-03-07)

- Rewrote server from TypeScript to Rust
- Distributed as cross-compiled native binaries (macOS arm64/x64, Linux
  x64/arm64) via npm with a POSIX sh launcher
- Added `MCP_DEBUG_STDIN` env var for raw stdin transport logging

## v2.5.3 (2026-03-06)

- Added `CONSULT_LLM_` prefix to backend and reasoning effort env vars:
  `CONSULT_LLM_GEMINI_BACKEND`, `CONSULT_LLM_OPENAI_BACKEND`,
  `CONSULT_LLM_CODEX_REASONING_EFFORT`. Old unprefixed names still work with a
  deprecation warning.

## v2.5.2 (2026-03-06)

- Fixed Codex CLI thread resumption failing due to unsupported `--add-dir` flag
  in `codex exec resume`

## v2.5.1 (2026-03-06)

- Consult skill now queries both Gemini and Codex in parallel by default, with
  `--gemini` and `--codex` flags for single-model consultation
- CLI backends now receive main worktree path as additional context when running
  inside a git worktree
- CLI backends now detect external file directories (outside workspace) and pass
  them to Gemini/Codex so referenced files are accessible
- Debate skill now supports multi-round debates via `--rounds` flag (default 2,
  max 3)
- Reduced anchoring bias in debate/debate-vs final review phase
- Added `install-skills` script for easy skill installation

## v2.5.0 (2026-03-05)

- Added gpt-5.4 model support ($2.50/$15 per million tokens)

## v2.4.2 (2026-02-28)

- Added gemini-3.1-pro-preview model support
- Filter unavailable models from the tool schema based on configured API keys
  and CLI backends, preventing errors when selecting unconfigured models

## v2.4.1 (2026-02-25)

- Updated tool description to tell callers not to inline file contents in the
  prompt field, since the server reads files automatically via the `files`
  parameter

## v2.3.0 (2026-02-25)

- Added Cursor CLI (`cursor-agent`) as a new executor backend
- Replaced `GEMINI_MODE`/`OPENAI_MODE` with `GEMINI_BACKEND`/`OPENAI_BACKEND`
  for backend routing (legacy env vars still work with deprecation warnings)
- Added `CONSULT_LLM_EXTRA_MODELS` environment variable for adding models
  without code changes
- Removed gpt-5.1 and Claude models from built-in model list

## v2.2.0 (2026-02-14)

- Added `task_mode` parameter for adaptive system prompts with five modes:
  `review`, `plan`, `create`, `debug`, and `general` (default)

## v2.1.0 (2026-02-12)

- Added multi-turn conversation support for CLI modes via `thread_id` parameter
  - Codex CLI: uses `--json` output and `exec resume` for session continuity
  - Gemini CLI: uses `-o json` output and `-r` flag for session resume
  - Responses include a `[thread_id:xxx]` prefix for follow-up requests
- Replaced generic CLI executor with dedicated Codex and Gemini executors
- Added debate skill example (`skills/debate/SKILL.md`) showcasing multi-turn
  conversations

## v2.0.1 (2026-02-05)

- Added gpt-5.3-codex model support

## v2.0.0 (2026-02-04)

- Log files now stored in XDG state directory
  (`~/.local/state/consult-llm-mcp/`) instead of `~/.consult-llm-mcp/logs/`,
  following the XDG Base Directory Specification

## v1.7.2 (2026-02-04)

- Extracted model definitions to dedicated module, resolving a circular
  dependency between config and schema

## v1.7.1 (2026-02-04)

- Removed o3 model, succeeded by gpt-5.2 which is now the default OpenAI model
- Documented model selection behavior and `CONSULT_LLM_ALLOWED_MODELS` usage

## v1.7.0 (2026-01-29)

- Added configurable system prompt path via `CONSULT_LLM_SYSTEM_PROMPT_PATH`
  environment variable

## v1.5.0 (2026-01-12)

- Added gpt-5.2-codex model support
- Added `CONSULT_LLM_ALLOWED_MODELS` environment variable to filter which models
  are available in the tool schema

## v1.4.7 (2025-12-12)

- Added gpt-5.2 model support

## v1.4.6 (2025-12-03)

- Removed 5-minute timeout from CLI executors

## v1.4.5 (2025-12-02)

- Improved system prompt by removing redundant "no critical issues found"
  statement
- Fixed `init-prompt` command to use npx
- Moved skill and slash command examples to separate files

## v1.4.4 (2025-11-26)

- Added support for gemini-3-pro-preview model

## v1.4.3 (2025-11-22)

- Added `--skip-git-repo-check` to Codex CLI args, fixing issues when running
  outside git repositories

## v1.4.2 (2025-11-21)

- Added gpt-5.1-codex-max model support
- Made Codex CLI reasoning effort configurable
- Fixed Codex CLI execution

## v1.4.1 (2025-11-19)

- Added test suite with vitest
- Fixed web mode file handling
- Split server entry point for better modularity

## v1.4.0 (2025-11-18)

- Added Codex CLI support as a new executor backend
- Refactored LLM execution to a functional executor pattern
- Migrated to Zod v4 with native JSON schema generation
- Added example Claude Code skill to README

## v1.3.0 (2025-11-16)

- Added web mode: copies the formatted prompt to clipboard for pasting into
  browser-based LLMs instead of querying an API directly

## v1.2.0 (2025-10-25)

- Added custom system prompt support, configurable via file
- Added environment variable documentation
- Set up ESLint and code quality checks

## v1.1.2 (2025-10-25)

- Added `--version` flag to display the server version

## v1.1.1 (2025-10-25)

- Added Gemini CLI mode support with a dedicated system prompt and instructions
- Enforced code file context requirement in the tool description
- Improved code review guidance in prompts

## v1.1.0 (2025-07-28)

- Added Gemini CLI mode as an alternative to the Gemini API, with free quota
  support
- Added debug logging and configuration logging with sensitive data redaction

## v1.0.5 (2025-06-29)

- Improved system prompt for more concise and critical analysis
- Updated tool description to emphasize neutral, unbiased questioning
- Moved main prompt to the `prompt` field instead of embedding in markdown

## v1.0.4 (2025-06-25)

- Added system prompt to LLM queries for enhanced analysis and recommendations
- Added `prompt` parameter to the tool

## v1.0.3 (2025-06-23)

- Added deepseek-reasoner and gemini-2.5-pro model options
- Added API key validation checks for OpenAI, Gemini, and DeepSeek models

## v1.0.2 (2025-06-23)

- Added server version logging and version in server metadata

## v1.0.1 (2025-06-23)

Initial release.
