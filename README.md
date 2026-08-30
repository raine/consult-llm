<br>

<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="meta/logo-dark.svg">
    <img src="meta/logo-light.svg" alt="consult-llm" height="120">
  </picture>
</div>

<br>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#providers--configuration">Providers & Configuration</a> ·
  <a href="#skills">Skills</a> ·
  <a href="#logging">Logging</a> ·
  <a href="#monitor">Monitor</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

`consult-llm` is a tool for getting a second opinion from another AI model,
right inside your existing agent workflow. Use it to plan architecture,
review changes, debate approaches, or get unstuck on tricky bugs. It supports GPT-5.6 Sol, Gemini 3.1 Pro, Claude Opus 4.7,
DeepSeek V4 Pro, MiniMax M2.7, Grok 4.6, and GLM 5.2, along with any `openrouter/*` model, with API and local CLI backends,
multi-turn threads, git diff context, web-mode clipboard export, and a live monitor TUI.

## Why a second opinion?

A model reviewing its own work isn't an independent check. Even in a fresh
context, it shares the same training, priors, and many of the same failure
modes. A different model was trained differently and makes different
mistakes, so it's more likely to push back, challenge weak reasoning, or
expose a blind spot.

Model strengths are uneven and hard to predict per task. A quick cross-check
is cheap, and sometimes the other model is the one that finds the path forward.

## How it works

1. Install skills into your agent (Claude Code, Codex, OpenCode, Pi)
2. Trigger with `/consult`, `/debate`, or `/collab`. In Pi, use `/skill:consult`, `/skill:debate`, or `/skill:collab`
3. The skill pipes your prompt into `consult-llm`, which calls the backend and streams the response back inline

For the more detailed flow, [continue below](#how-it-really-works).

```
> The hint unlocking is still broken after your fix. Ask gemini

⏺ Skill(consult-llm)
  ⎿  Successfully loaded skill · 1 tool allowed

⏺ Let me consult Gemini to understand this segmentation mismatch:

⏺ Bash(cat issue.md | consult-llm -m gemini -f HintsText.tsx -f hints.ts)
  ⎿ [model:gemini-3.1-pro-preview]

     The root cause is a race condition in `handleConfirmGenerateHints`.
     After `await generateHints()`, the component state updates in the
     background, but the function continues with a stale `activeSegment`
     variable, requesting a hint for "isn't" instead of "isn't it".

⏺ Gemini identified the real issue: a race condition with stale state
  after async operations. I was looking at the wrong thing entirely.
```

> [!NOTE]
> This project previously used MCP as its transport layer. If you were using that version, see the [migration guide](#migrating-from-mcp) below.

## What you can do

- **Get a second opinion from another model** from your agent with relevant file context (`/consult`)
- **Have models debate the best approach** and synthesize a consensus (`/debate`)
- **Use existing subscriptions** via CLI backends without API keys ([Gemini CLI](#gemini-cli), [Codex CLI](#codex-cli), [Cursor CLI](#cursor-cli), [Claude CLI](#profile-backend), [OpenCode](#opencode))
- **Continue conversations across requests** with `thread_id`
- **Copy prompts to clipboard** for browser-based LLMs (`--web`)
- **[Monitor](#monitor) active and past runs** in a real-time TUI

<img src="meta/monitor-screenshot.webp" alt="consult-llm-monitor screenshot" width="600">

## Quick Start

1. Install the binaries:

```bash
# Homebrew
brew install raine/consult-llm/consult-llm

# Shell script (macOS/Linux)
curl -fsSL https://raw.githubusercontent.com/raine/consult-llm/main/scripts/install.sh | bash
```

2. Configure a backend. CLI backends are the easiest if you have an existing subscription (no API key needed):

```bash
consult-llm config set gemini.backend gemini-cli   # requires: gemini login
consult-llm config set openai.backend codex-cli    # requires: codex login
```

Or set API keys:

```bash
consult-llm config set openai.api_key your_openai_key
consult-llm config set gemini.api_key your_gemini_key
consult-llm config set grok.api_key your_xai_key
consult-llm config set zai.api_key your_zai_key
# or via environment variables: OPENAI_API_KEY, GEMINI_API_KEY, XAI_API_KEY, ZAI_API_TOKEN, etc.
```

3. Verify your setup:

```bash
consult-llm models                    # see available models and resolved selectors
echo "hello" | consult-llm -m gemini  # quick smoke test
```

4. Install the skills so your agent can call `consult-llm` for you:

```bash
consult-llm install-skills
```

Then invoke skills from inside your agent (see [Usage](#usage) right below).

## Example workflows

Some real-world examples. Click to expand.

<details>
<summary>Explain the problem, and tell CC to consult a smarter LLM</summary>

```
  <Some long explanation of the issue here>

  Consult LLM for assistance, use gemini

⏺ I see the issue! The problem is that the frontend is trying to use the hint
  for "isn't" but the server generated a hint for "isn't it" (the full phrase).
  This is a mismatch between how the frontend segments text and how the server
  generates hints.

  Let me consult the LLM to get assistance on this segmentation mismatch issue.

⏺ Write(hint-segmentation-issue.md)
  ⎿  Wrote 53 lines to hint-segmentation-issue.md
     # Hint Segmentation Mismatch Issue
     ## Problem Description
     There's a mismatch between how the frontend segments text for clickable
     words and how the backend generates hints. This causes hint unlocking to
     fail when the segments don't match exactly.
     ## Example Case
     **Original Text**: "Hello! Nice weather, isn't it?"
     … +43 lines (ctrl+r to expand)

⏺ Bash(cat hint-segmentation-issue.md | consult-llm -m gemini -f hint-segmentation-issue.md -f web/src/components/HintsText.tsx -f web/src/utils/hints.ts -f server/src/routes/public/hints.ts -f server/src/llm/hints.ts)
  ⎿  [model:gemini-3.1-pro-preview] [thread_id:a3f7c...]

     An excellent analysis of the problem. The root cause has been correctly
     identified: a race condition within the `handleConfirmGenerateHints`
     function in `HintsText.tsx`. The component state updates in the background
     after `await generateHints()`, but the function continues executing with a
     stale `activeSegment` variable, causing it to request a hint for a smaller
     text segment ( e.g., "isn't") instead of the full, newly-available hint
     phrase (e.g., "isn't it").
     … +607 lines (ctrl+r to expand)

⏺ Perfect! The LLM identified the core issue - there's a race condition in
  handleConfirmGenerateHints where the function uses stale state after async
  operations. The solution is to re-target the hint key after generation to
  find the correct matching hint.

  Let me implement this solution:

⏺ Update Todos
  ⎿  ☐ Fix hint segmentation mismatch by implementing smarter hint key
       targeting in handleConfirmGenerateHints
```

</details>

<details>
<summary>Using web mode to copy prompts to clipboard for browser-based LLMs</summary>

```
> I need help implementing a -b/--background flag for my Rust CLI. Use consult LLM with web mode

⏺ I'll use web mode to copy the formatted prompt to your clipboard so you can
  paste it into any browser-based LLM service.

⏺ Bash(cat <<'EOF' | consult-llm -m gemini --web -f src/cli.rs -f src/workflow.rs -f src/tmux.rs
  What is the best approach to implement a -b/--background flag for the
  "workmux add" command that creates a tmux window in the background without
  switching to it?

  The current implementation automatically switches to the newly created window
  via `tmux::select_window()` in the `setup_environment()` function. I need to
  make this behavior conditional based on a flag passed from the CLI.

  Consider:
  1. Where should the background flag be threaded through the call chain?
  2. Should I modify the existing SetupOptions struct or add a parameter?
  3. What's the most maintainable approach given the codebase structure?
  4. Are there any edge cases to consider (e.g., should select_pane also be
     conditional)?
  EOF)
  ⎿  ✓ Prompt copied to clipboard!

     Please paste it into your browser-based LLM service and share the response
     here before I proceed with any implementation.

     Note: File paths were included:
       - src/cli.rs
       - src/workflow.rs
       - src/tmux.rs

> [User pastes the response from the external LLM]

⏺ Thank you! Based on the recommendations, I'll now implement the solution...
```

This is useful when:

- You want to use a free browser-based LLM service instead of API credits
- You prefer a specific LLM's web interface
- You want to review the full prompt before submitting it

</details>

<details>
<summary>Debugging Neovim plugin treesitter API breakage</summary>

````
⏺ Fixed! Added nil checks for both node and child and updated the iter_matches call to handle the newer API properly.

> Still not working:

  E5108: Error executing lua /Users/raine/code/testonly.nvim/lua/testonly.lua:92: attempt to call method 'child' (a nil value)
  stack traceback:
          /Users/raine/code/testonly.nvim/lua/testonly.lua:92: in function 'reset_all_exclusive'
          /Users/raine/code/testonly.nvim/lua/testonly.lua:110: in function 'reset_all_exclusive'
          [string ":lua"]:1: in main chunk

  Ask gemini

⏺ Let me consult with a more powerful AI to understand the current Neovim treesitter API changes:

⏺ Bash(cat <<'EOF' | consult-llm -m gemini -f lua/testonly.lua
  I'm debugging a Neovim plugin that's broken due to treesitter API changes.
  The error is "attempt to call method 'child' (a nil value)" on line 92. The
  code is trying to access child nodes from treesitter matches but the API
  seems to have changed in newer Neovim versions.

  Here's the problematic code:
  ```lua
  for _, match, _ in parsed_query:iter_matches(root, bufnr, start_row, end_row) do
      for id, node in pairs(match) do
          local name = parsed_query.captures[id]
          if name == "call" and node then
              local child = node:child(0)  -- ERROR HERE
              if child then
                  local text = vim.treesitter.get_node_text(child, bufnr)
                  -- ... rest of logic
              end
          end
      end
  end
  ```

  What's the correct way to iterate over treesitter query matches and access
  child nodes in current Neovim versions? What API changes should I be aware of?
  EOF)
  ⎿  [model:gemini-3.1-pro-preview] [thread_id:b9e2a...]

     Problem
     -------

     … +93 lines (ctrl+r to expand)

⏺ Perfect! The issue is that iter_matches() can return nil nodes in the match table. Let me fix this by switching to the recommended iter_captures() approach:

⏺ Update(lua/testonly.lua)
  ⎿  Updated lua/testonly.lua with 7 additions and 10 removals
       85      local root = parser:parse()[1]:root()
       86      local start_row, _, end_row, _ = root:range()
       87
       88 -    for _, match, _ in parsed_query:iter_matches(root, bufnr, start_row, end_row) do
       89 -      for id, node in pairs(match) do
       90 -        local name = parsed_query.captures[id]
       91 -        if name == "call" and node then
       92 -          local child = node:child(0)
       93 -          if child then
       94 -            local text = vim.treesitter.get_node_text(child, bufnr)
       88 +    for id, node, _ in parsed_query:iter_captures(root, bufnr, start_row, end_row) do
       89 +      if parsed_query.captures[id] == "call" then
       90 +        local first_child = node:named_child(0)
       91 +        if first_child then
       92 +          local text = vim.treesitter.get_node_text(first_child, bufnr)
       93
       94 -            if text == "it.only" or text == "describe.only" then
       95 -              toggle_test_exclusive(child)
       96 -            end
       94 +          if text == "it.only" or text == "describe.only" then
       95 +            toggle_test_exclusive(first_child)
       96            end
       97          end
       98        end
````

</details>

<details>
<summary>Using slash command to consult multiple LLMs for comparison</summary>

```
> /consult The branch completions should dynamic. Git should be called only when
using tab to autocomplete. Ask gemini and codex

⏺ I'll consult both Gemini and Codex about making the branch completions dynamic
  (only calling git when tab-completing). Let me gather the relevant context first.

⏺ Bash(cat <<'EOF' | consult-llm -m gemini -f src/cli.rs -f src/git.rs
  I have a Rust CLI tool using clap that needs dynamic shell completions.
  Currently, the branch completions are generated at completion script generation
  time via `possible_values()`. Instead, I need the git operations to only run
  when the user presses TAB to autocomplete.

  Current setup:
  1. `WorktreeBranchParser` implements `TypedValueParser` with `possible_values()`
     that calls git
  2. Shell completions are generated via clap_complete's `generate()` function
  3. The completions are sourced at shell startup: `source <(workmux completions zsh)`

  The problem: Running `workmux completions zsh` takes ~210ms because it runs git
  operations during completion generation, not during actual tab completion.

  How can I implement truly dynamic completions where git is only called when the
  user presses TAB? What's the best approach with clap and clap_complete for zsh?
  EOF)
  ⎿  [model:gemini-3.1-pro-preview] [thread_id:c1d4e...]

     The root cause of the slow startup (~210ms) is that `clap_complete::generate()`
     executes your `possible_values()` logic (git operations) *while generating the
     script*. This bakes the *current* branches into the static script, which is both
     slow (happens every time you `source` it) and incorrect (branches become stale).
     … +116 lines (ctrl+o to expand)


⏺ Bash(cat <<'EOF' | consult-llm -m codex -f src/cli.rs -f src/git.rs
  I have a Rust CLI tool using clap that needs dynamic shell completions.
  Currently, the branch completions are generated at completion script generation
  time via `possible_values()`. Instead, I need the git operations to only run
  when the user presses TAB to autocomplete.

  Current setup:
  1. `WorktreeBranchParser` implements `TypedValueParser` with `possible_values()`
     that calls git
  2. Shell completions are generated via clap_complete's `generate()` function
  3. The completions are sourced at shell startup: `source <(workmux completions zsh)`

  The problem: Running `workmux completions zsh` takes ~210ms because it runs git
  operations during completion generation, not during actual tab completion.

  How can I implement truly dynamic completions where git is only called when the
  user presses TAB? What's the best approach with clap and clap_complete for zsh?
  EOF)
  ⎿  [model:gpt-5.1-codex-max] [thread_id:d2f5b...]

     **Findings**
     - `WorktreeBranchParser::possible_values` (`cli.rs:41-72`) runs git during script
       generation, so `workmux completions zsh` always pays the git cost (~210ms) even
       though the result is only needed at tab time.
     … +77 lines (ctrl+o to expand)


⏺ Summary from Gemini and Codex

  Both agree on the same solution:

  Root Cause

  clap_complete::generate() calls possible_values() during script generation, not
  at tab-completion time. This means git runs every shell startup.

...
```

This example shows using the `/consult` slash command to ask multiple LLMs
(Gemini and Codex) about the same problem **in parallel** and compare their
responses. Both LLMs independently arrived at the same solution, providing
confidence in the approach.

</details>

## Usage

The CLI is invoked by your agent via the installed skills; you don't call it directly. From inside Claude Code, OpenCode, or Codex:

```
/consult what's the best way to model this state machine?
/consult --gemini review this design for edge cases
/debate should this be a separate service or stay in the monolith?
```

Pi exposes skills with the `skill:` prefix:

```
/skill:consult what's the best way to model this state machine?
/skill:consult --gemini review this design for edge cases
/skill:debate should this be a separate service or stay in the monolith?
```

### CLI utilities

```bash
consult-llm models                    # list available models and resolved selectors
consult-llm doctor                    # diagnose backend auth and config
consult-llm config set <key> <value>  # set a config value (user config by default)
consult-llm init-config               # scaffold ~/.config/consult-llm/config.yaml
consult-llm init-prompt               # scaffold ~/.config/consult-llm/SYSTEM_PROMPT.md
consult-llm install-skills            # install bundled skills to platform skill dirs
consult-llm update                    # self-update the binary
```

`consult-llm models` shows which models are active based on the configuration loaded for the current directory and prints `Default models:`, the ordered list used when `-m` is omitted. The `Default -m args:` line is a convenience for same-prompt calls; `--run` workflows use the model list to build one `--run model=...` entry per prompt.

`consult-llm doctor` checks that each provider's backend dependency (API key or CLI binary) is satisfied, shows which config files were loaded, and validates session storage. Pass `--verbose` to see all config keys including unset defaults.

## Providers & Configuration

`consult-llm` separates **model families** from **backends**.

A **model family** is what you ask for: `gemini`, `openai`, `deepseek`, `minimax`, `anthropic`, `grok`, `zai`, or `openrouter`.

A **backend** is how `consult-llm` reaches that model family:

- **`api`**: direct HTTP calls using an API key
- **CLI backends**: shell out to a local CLI tool already installed and logged in

| Model family | `api` backend | CLI backends available                            | API key env var      |
| ------------ | ------------- | ------------------------------------------------- | -------------------- |
| Gemini       | yes           | `gemini-cli`, `cursor-cli`, `opencode`, `profile` | `GEMINI_API_KEY`     |
| OpenAI       | yes           | `codex-cli`, `cursor-cli`, `opencode`, `profile`  | `OPENAI_API_KEY`     |
| DeepSeek     | yes           | `opencode`, `profile`                             | `DEEPSEEK_API_KEY`   |
| MiniMax      | yes           | `opencode`, `profile`                             | `MINIMAX_API_KEY`    |
| Anthropic    | yes           | `profile`, `claude-cli`, `cursor-cli`             | `ANTHROPIC_API_KEY`  |
| Grok         | yes           | `grok-cli`, `cursor-cli`, `profile`               | `XAI_API_KEY`        |
| Zai (GLM)    | yes           | `profile`                                         | `ZAI_API_TOKEN`      |
| OpenRouter   | yes           | `opencode`, `profile`                             | `OPENROUTER_API_KEY` |

### API backend

Direct HTTP calls to the provider. Requires an API key. Set it in your user config or as an environment variable:

```bash
# User config (recommended, persists across sessions)
consult-llm config set openai.api_key your_openai_key
consult-llm config set gemini.api_key your_gemini_key
consult-llm config set grok.api_key your_xai_key

# Or as environment variables
export OPENAI_API_KEY=your_openai_key
export GEMINI_API_KEY=your_gemini_key
export XAI_API_KEY=your_xai_key
```

The `api` backend is the default. To set it explicitly:

```bash
consult-llm config set gemini.backend api
consult-llm config set openai.backend api
consult-llm config set grok.backend api
```

#### Custom base URL

Any provider's API endpoint can be overridden with `base_url` (or the
provider's `CONSULT_LLM_<PROVIDER>_BASE_URL` environment variable). Useful for
proxies, gateways, and plan-specific endpoints. Not allowed in the committed
project config (`.consult-llm.yaml`) because it controls where API credentials
are sent.

For example, Z.AI coding-plan subscription keys only work against the coding
endpoint:

```bash
consult-llm config set zai.api_key your_zai_coding_plan_key
consult-llm config set zai.base_url https://api.z.ai/api/coding/paas/v4
```

Kimi models served by an OpenAI-compatible gateway can use the OpenAI provider
with an extra model entry and a custom endpoint:

```yaml
extra_models: [kimi-k3]
allowed_models: [kimi-k3]

openai:
  backend: api
  api_key: your_gateway_key
  base_url: https://your-gateway.example/v1
```

### CLI backends

Shell out to an already-installed local CLI. No API keys needed in `consult-llm`; authentication is handled by the CLI tool.

A key advantage over the API backend: CLI agents can browse your codebase, run commands, and do their own research before responding. The API backend receives only the prompt and files you explicitly include.

#### Gemini CLI

Requires the [Gemini CLI](https://github.com/google-gemini/gemini-cli) and `gemini login`:

```bash
consult-llm config set gemini.backend gemini-cli
```

#### Codex CLI

Requires Codex CLI and `codex login`. GPT-5.6 Sol is the preferred built-in
OpenAI model and works with the Codex CLI backend:

```bash
consult-llm config set openai.backend codex-cli
consult-llm config set allowed_models '[gpt-5.6-sol]'
consult-llm config set default_model gpt-5.6-sol
consult-llm config set openai.reasoning_effort high  # none | minimal | low | medium | high | xhigh

# Optional: append extra args to every `codex exec` invocation. Shell-quoted.
# Useful e.g. to skip the sandbox in environments that already isolate Codex:
consult-llm config set openai.extra_args '--dangerously-bypass-approvals-and-sandbox'
```

The same `extra_args` field is supported on `gemini:` and `grok:` for their native CLI backends.

#### Grok CLI

Requires the Grok CLI on `PATH` and an authenticated Grok account. Grok 4.6 is
the preferred built-in Grok model:

```bash
consult-llm config set grok.backend grok-cli
consult-llm config set allowed_models '[grok-4.6]'
consult-llm config set default_model grok-4.6
consult-llm config set grok.reasoning_effort high
```

The backend uses the CLI's headless streaming JSON mode and preserves session
IDs for multi-turn consultations. Authentication and subscription access are
managed by the Grok CLI, so `XAI_API_KEY` is unnecessary.

Provider blocks support `env` for values that should be set only while launching that provider backend:

```yaml
openai:
  backend: opencode
  env:
    OPENCODE_PERMISSION: '{"glob":"allow","read":"allow"}'
```

Use provider `env` from `.consult-llm.local.yaml` or the user config for machine-local values. The shared project config rejects literal provider env values.

#### Cursor CLI

Routes through `cursor-agent`:

```bash
consult-llm config set openai.backend cursor-cli
consult-llm config set gemini.backend cursor-cli
```

If your prompts need shell commands in Cursor CLI ask mode, allow them in `~/.cursor/cli-config.json`.

#### Claude CLI

Routes Anthropic models through the stock `claude` executable on `PATH`:

```bash
consult-llm config set anthropic.backend claude-cli
```

The native backend defaults to `claude`, stream-json output, stdin prompt delivery, and the non-interactive flags needed by consult-llm. Optional native settings:

```bash
consult-llm config set anthropic.reasoning_effort high  # low | medium | high | xhigh | max
consult-llm config set anthropic.extra_args '--permission-mode acceptEdits'
```

Use the `profile` backend instead when you need a custom Claude command, model env var, prompt mode, or wrapper. Provider `env` is merged into profile env and takes precedence for matching keys.

#### Profile backend

Routes any model family through a named CLI profile. This is useful when a Claude Code CLI process proxies another provider, for example routing Gemini models through a local [claude-code-proxy](https://github.com/raine/claude-code-proxy):

```yaml
cli_profiles:
  claude-gemini-proxy:
    command: /Users/you/.local/bin/claude
    env:
      ANTHROPIC_BASE_URL: http://localhost:18765
      ANTHROPIC_AUTH_TOKEN: anything
      ANTHROPIC_SMALL_FAST_MODEL: gemini-3.1-pro-preview
    model_env: ANTHROPIC_MODEL

gemini:
  backend: profile
  cli_profile: claude-gemini-proxy
```

`model_env` sets the named environment variable to the requested model ID before launching the profile command. For Anthropic models, use `anthropic.backend: profile` when selecting a named CLI profile. Existing configs that combine `anthropic.backend: claude-cli` with `anthropic.cli_profile` are treated as profile-backed for backward compatibility, but new configs should use `profile` explicitly.

> Fields like `type: claude-cli`, `command: claude`, `interface: stream-json`, `prompt: stdin` and flags like `-p`, `--output-format stream-json`, `--verbose` are defaulted or auto-injected for `claude-cli` profiles. Only non-default choices need to be written. See [CLI backend profiles](#cli-backend-profiles) below.

The example passes literal environment values and arguments to the CLI process. Prefer a user or project-local config for profiles with `env` values; committed project config rejects `cli_profiles.*.env` so secrets and machine-local paths do not leak.

Run `consult-llm doctor` after configuring it. The provider row should show `via profile` and the selected profile command, for example `profile 'claude-gemini-proxy' command claude (...)`.

#### OpenCode

Routes through `opencode` to Copilot, OpenRouter, or other providers:

```bash
consult-llm config set openai.backend opencode
consult-llm config set gemini.backend opencode
consult-llm config set deepseek.backend opencode
consult-llm config set minimax.backend opencode

# Optional: configure OpenCode provider routing
consult-llm config set opencode.default_provider copilot
consult-llm config set openai.opencode_provider openai
```

#### OpenRouter

Routes `openrouter/*` models through OpenCode:

```bash
consult-llm config set openrouter.backend opencode
```

No API key needed -- authentication is handled by your OpenCode installation.
Any `openrouter/*` model ID available in OpenCode works automatically. Add it
to your config:

```yaml
openrouter:
  backend: opencode
  reasoning_effort: high

extra_models:
  - openrouter/xiaomi/mimo-v2.5-pro

allowed_models:
  - gpt-5.6-sol
  - gemini-3.1-pro-preview
  - openrouter/xiaomi/mimo-v2.5-pro
```

The `extra_models` entry adds the model to the catalog; `allowed_models` must
also list it since it acts as an allowlist. Any `openrouter/*` model ID from
OpenCode works -- add them to both lists.

Set `openrouter.reasoning_effort` to `low`, `medium`, or `high` for reasoning
models. With the OpenCode backend this is passed as `--variant <level>`; with
the API backend it is sent as OpenRouter's `reasoning.effort` field.

The `openrouter` selector resolves to the first available enabled model (e.g.
`openrouter/auto` when no specific model is configured directly).

### CLI backend profiles

The `profile` backend selects a named entry from the top-level `cli_profiles` map. Each profile defines how consult-llm launches the CLI process:

- `type`: profile executor type (defaults to `claude-cli`, the only supported value today)
- `command`: executable name or path (defaults to `claude`)
- `args`: literal argv entries before the prompt
- `env`: literal environment variables passed to the CLI process
- `effort`: optional effort level (`low`, `medium`, `high`, `xhigh`, `max`); passed as `--effort <level>` to the CLI
- `model_env`: optional env var name set to the requested model ID at launch time
- `interface`: output parsing strategy (`text`, `json`, or `stream-json`; defaults to `stream-json`)
- `prompt`: how the prompt is delivered (`stdin` or `argument`; defaults to `stdin`)

For `claude-cli` profiles, the executor auto-injects `-p`, `--output-format <interface>`, `--verbose` and the env vars `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `CLAUDE_CODE_DISABLE_UPDATE_CHECK=1`, and `NO_COLOR=1`.

Provider blocks reference a profile by name. For example, `gemini.backend: profile` with `gemini.cli_profile: claude-gemini-proxy` uses the profile shown above. For Anthropic, prefer `anthropic.backend: claude-cli` for the stock native backend and `anthropic.backend: profile` when selecting a named profile.

## Multi-turn conversations

CLI backends support multi-turn conversations. The first response includes a
`[thread_id:xxx]` prefix; pass that ID back with `--thread-id` to continue
the conversation with full context from prior turns.

```
> Ask codex what's the best caching strategy for our read-heavy API

⏺ Bash(cat <<'EOF' | consult-llm -m codex
  What's the best caching strategy for a read-heavy API?
  EOF)
  ⎿  [model:gpt-5.6-sol] [thread_id:thread_b1ff711...]

     Redis with TTL-based invalidation. Set short TTLs for volatile data ...

⏺ Codex recommends Redis with TTL-based invalidation, but our payment
  endpoints need strong consistency. Let me push back on that.

⏺ Bash(cat <<'EOF' | consult-llm -m codex --thread-id thread_b1ff711...
  What if we need strong consistency guarantees? Does that change your
  recommendation?
  EOF)
  ⎿  [model:gpt-5.6-sol] [thread_id:thread_b1ff711...]

     Yes. With strong consistency, the TTL approach breaks down. Consider
     write-through caching or cache invalidation on writes instead ...

⏺ Codex revised its recommendation based on the consistency constraint. I'll
  use write-through caching for the payment endpoints.
```

This works with Gemini CLI, Codex CLI, Cursor CLI, OpenCode, and Claude CLI. OpenCode sessions use a consult-llm-managed SQLite database per thread unless `OPENCODE_DB` is set in provider `env`; this avoids contention between unrelated parallel OpenCode runs while preserving resume for returned thread IDs. See the [debate skills](#skills) for multi-LLM workflows that use thread IDs to maintain context across debate rounds.

### Config files

consult-llm reads layered YAML config files. Resolution order (highest to lowest precedence):

1. Environment variables
2. `.consult-llm.local.yaml` (project-local overrides, not committed to git)
3. `.consult-llm.yaml` (committed project config)
4. `~/.config/consult-llm/config.yaml` (user config)

   Supports `$XDG_CONFIG_HOME`. The legacy path `~/.consult-llm/config.yaml` is still read for backward compatibility.

Project files are discovered by walking up from the current directory to the nearest `.git` root or `$HOME`.

`.consult-llm.local.yaml` is useful for personal backend or model preferences that you don't want committed. Add it to your [global gitignore](https://docs.github.com/en/get-started/getting-started-with-git/ignoring-files#configuring-ignored-files-for-all-repositories-on-your-computer) so it's excluded from all projects:

```bash
echo '.consult-llm.local.yaml' >> ~/.gitignore_global
```

If you use [workmux](https://github.com/raine/workmux) worktrees, symlink it into new worktrees automatically by adding it to your `.workmux.yaml`:

```yaml
files:
  symlink:
    - .consult-llm.local.yaml
```

Scaffold the user config and set values:

```bash
consult-llm init-config
consult-llm config set default_model gemini
consult-llm config set default_models '[gemini, openai, openai]'
consult-llm config set gemini.backend gemini-cli
# Write to project config instead of user config:
consult-llm config set --project default_model openai
# Write to local project overrides (not committed):
consult-llm config set --local openai.backend codex-cli
```

Values are parsed as YAML, so booleans and lists work naturally:

```bash
consult-llm config set no_update_check true
consult-llm config set allowed_models '[gemini, openai]'
```

Model selection has three layers:

- `allowed_models` is the allowlist: it restricts which exact model IDs are enabled and which selectors can resolve. It also validates `default_model`, `default_models`, and explicit `--<selector>` skill flags.
- `default_model` controls single-response CLI calls where `-m` is omitted and `default_models` is empty or unset.
- `default_models` controls multi-model calls where `-m` is omitted; it preserves order and duplicates, so `[openai, openai]` intentionally samples OpenAI twice. When `default_models` is empty or unset, the CLI falls back to `default_model`, then the built-in fallback model.

If `default_models` names a model excluded by `allowed_models`, config loading fails instead of silently using it.

Example `~/.config/consult-llm/config.yaml`:

```yaml
allowed_models: [gemini-3.1-pro-preview, gpt-5.6-sol, grok-4.6]
default_model: gpt-5.6-sol
default_models: [gpt-5.6-sol, gpt-5.6-sol]

gemini:
  backend: gemini-cli

openai:
  backend: codex-cli
  reasoning_effort: high

anthropic:
  backend: claude-cli
  reasoning_effort: high

grok:
  api_key: your_xai_key

opencode:
  default_provider: copilot
```

### API keys

API keys can be set in your user config, a project-local config file, or as environment variables. Environment variables take highest precedence.

**User config** (`~/.config/consult-llm/config.yaml`), applies everywhere:

```yaml
openai:
  api_key: your_openai_key
gemini:
  api_key: your_gemini_key
grok:
  api_key: your_xai_key
```

**Project-local config** (`.consult-llm.local.yaml` in the repo root, gitignored), overrides the user config for that project:

```yaml
openai:
  api_key: your_project_specific_key
```

API keys are **not** allowed in `.consult-llm.yaml` (the committed project config). The tool will refuse to load it and tell you to move the key to `.consult-llm.local.yaml`.

**Environment variables** (highest precedence, useful for CI):

- `OPENAI_API_KEY`
- `GEMINI_API_KEY`
- `ANTHROPIC_API_KEY`
- `DEEPSEEK_API_KEY`
- `MINIMAX_API_KEY`
- `XAI_API_KEY`
- `ZAI_API_TOKEN`

**[direnv](https://direnv.net/)** is an alternative to `.consult-llm.local.yaml` for project-specific keys via environment variables. Add a `.envrc` in the repo root and `direnv allow` it, then put keys in a `.env` file (both gitignored):

```bash
# .envrc
dotenv
```

```bash
# .env
OPENAI_API_KEY=your_project_specific_key
```

direnv loads the variables automatically when you enter the directory and unloads them when you leave.

### Known models with pricing

Cost estimates are displayed for known models. Any model name is accepted; cost
estimates show as zero for models without pricing data.

<details>
<summary>Pricing table</summary>

| Model                    | Input   | Output   |
| ------------------------ | ------- | -------- |
| **OpenAI models**        |
| `gpt-5.5`                | $5.00/M | $30.00/M |
| `gpt-5.4`                | $2.50/M | $15.00/M |
| `gpt-5.3-codex`          | $2.50/M | $10.00/M |
| `gpt-5.2`                | $1.75/M | $14.00/M |
| `gpt-5.2-codex`          | $1.75/M | $7.00/M  |
| **Google Gemini models** |
| `gemini-2.5-pro`         | $1.25/M | $10.00/M |
| `gemini-3-pro-preview`   | $2.00/M | $12.00/M |
| `gemini-3.1-pro-preview` | $2.00/M | $12.00/M |
| **DeepSeek models**      |
| `deepseek-v4-pro`        | $0.55/M | $2.19/M  |
| **MiniMax models**       |
| `MiniMax-M2.7`           | $0.30/M | $1.20/M  |
| **Anthropic models**     |
| `claude-opus-4-7`        | $5.00/M | $25.00/M |
| **Grok models**          |
| `grok-4.3`               | $1.25/M | $2.50/M  |

Pricing is per million tokens (M). Check the provider's current pricing page
before relying on estimates for billing decisions.

</details>

### Custom system prompt

```bash
consult-llm init-prompt   # scaffold ~/.config/consult-llm/SYSTEM_PROMPT.md
```

Override the path in config:

```yaml
system_prompt_path: /path/to/project/.consult-llm/SYSTEM_PROMPT.md
```

<details>
<summary>All environment variables</summary>

Environment variables override config file values.

| Variable                                   | Description                                                                             | Allowed values                                       | Default                                              |
| ------------------------------------------ | --------------------------------------------------------------------------------------- | ---------------------------------------------------- | ---------------------------------------------------- |
| `OPENAI_API_KEY`                           | OpenAI API key                                                                          |                                                      |                                                      |
| `GEMINI_API_KEY`                           | Gemini API key                                                                          |                                                      |                                                      |
| `ANTHROPIC_API_KEY`                        | Anthropic API key                                                                       |                                                      |                                                      |
| `DEEPSEEK_API_KEY`                         | DeepSeek API key                                                                        |                                                      |                                                      |
| `MINIMAX_API_KEY`                          | MiniMax API key                                                                         |                                                      |                                                      |
| `OPENROUTER_API_KEY`                       | OpenRouter API key                                                                      |                                                      |                                                      |
| `XAI_API_KEY`                              | xAI API key for Grok models                                                             |                                                      |                                                      |
| `ZAI_API_TOKEN`                            | Z.AI API key for GLM models                                                             |                                                      |                                                      |
| `CONSULT_LLM_<PROVIDER>_BASE_URL`          | Override the API base URL for a provider's `api` backend (e.g. `CONSULT_LLM_ZAI_BASE_URL` for the Z.AI coding-plan endpoint) | URL                                                  | registry default                                     |
| `CONSULT_LLM_DEFAULT_MODEL`                | Model or selector to use for single-response calls when `-m` is omitted                 | selector or exact model ID                           | first available                                      |
| `CONSULT_LLM_DEFAULT_MODELS`               | Comma-separated ordered multi-model defaults when `-m` is omitted; duplicates preserved | selectors or exact model IDs                         | empty (falls through to default_model then fallback) |
| `CONSULT_LLM_GEMINI_BACKEND`               | Backend for Gemini models                                                               | `api` `gemini-cli` `cursor-cli` `opencode` `profile` | `api`                                                |
| `CONSULT_LLM_OPENAI_BACKEND`               | Backend for OpenAI models                                                               | `api` `codex-cli` `cursor-cli` `opencode` `profile`  | `api`                                                |
| `CONSULT_LLM_DEEPSEEK_BACKEND`             | Backend for DeepSeek models                                                             | `api` `opencode` `profile`                           | `api`                                                |
| `CONSULT_LLM_MINIMAX_BACKEND`              | Backend for MiniMax models                                                              | `api` `opencode` `profile`                           | `api`                                                |
| `CONSULT_LLM_ANTHROPIC_BACKEND`            | Backend for Anthropic models                                                            | `api` `profile` `claude-cli` `cursor-cli`            | `api`                                                |
| `CONSULT_LLM_GROK_BACKEND`                 | Backend for Grok models                                                                 | `api` `grok-cli` `cursor-cli` `profile`              | `api`                                                |
| `CONSULT_LLM_ZAI_BACKEND`                  | Backend for Z.AI (GLM) models                                                           | `api` `profile`                                      | `api`                                                |
| `CONSULT_LLM_OPENROUTER_BACKEND`           | Backend for OpenRouter models                                                           | `api` `opencode` `profile`                           | `api`                                                |
| `CONSULT_LLM_ALLOWED_MODELS`               | Comma-separated allowlist; restricts which models are enabled                           | model IDs                                            | all                                                  |
| `CONSULT_LLM_EXTRA_MODELS`                 | Comma-separated extra model IDs to add to the catalog                                   | model IDs                                            |                                                      |
| `CONSULT_LLM_CODEX_REASONING_EFFORT`       | Reasoning effort for Codex CLI backend                                                  | `none` `minimal` `low` `medium` `high` `xhigh`       | `high`                                               |
| `CONSULT_LLM_CODEX_EXTRA_ARGS`             | Extra CLI args appended to `codex exec` (shell-quoted)                                  | e.g. `--dangerously-bypass-approvals-and-sandbox`    |                                                      |
| `CONSULT_LLM_GEMINI_EXTRA_ARGS`            | Extra CLI args appended to `gemini` (shell-quoted)                                      | shell-quoted args                                    |                                                      |
| `CONSULT_LLM_GROK_REASONING_EFFORT`        | Reasoning effort for Grok CLI and API backends                                          | `low` `medium` `high`                                | unset                                                |
| `CONSULT_LLM_GROK_EXTRA_ARGS`              | Extra CLI args appended to `grok` (shell-quoted)                                        | shell-quoted args                                    |                                                      |
| `CONSULT_LLM_CLAUDE_REASONING_EFFORT`      | Reasoning effort for native Claude CLI backend                                          | `low` `medium` `high` `xhigh` `max`                  | unset                                                |
| `CONSULT_LLM_OPENROUTER_REASONING_EFFORT`  | Reasoning effort for OpenRouter reasoning models                                        | `low` `medium` `high`                                | unset                                                |
| `CONSULT_LLM_CLAUDE_EXTRA_ARGS`            | Extra CLI args appended to `claude` (shell-quoted)                                      | shell-quoted args                                    |                                                      |
| `CONSULT_LLM_OPENCODE_PROVIDER`            | Default OpenCode provider prefix for all models                                         | provider name                                        | per-model default                                    |
| `CONSULT_LLM_ANTHROPIC_CLI_PROFILE`        | CLI profile name when `anthropic.backend` is `profile`                                  | profile name                                         |                                                      |
| `CONSULT_LLM_GEMINI_CLI_PROFILE`           | CLI profile name when `gemini.backend` is `profile`                                     | profile name                                         |                                                      |
| `CONSULT_LLM_OPENAI_CLI_PROFILE`           | CLI profile name when `openai.backend` is `profile`                                     | profile name                                         |                                                      |
| `CONSULT_LLM_DEEPSEEK_CLI_PROFILE`         | CLI profile name when `deepseek.backend` is `profile`                                   | profile name                                         |                                                      |
| `CONSULT_LLM_MINIMAX_CLI_PROFILE`          | CLI profile name when `minimax.backend` is `profile`                                    | profile name                                         |                                                      |
| `CONSULT_LLM_GROK_CLI_PROFILE`             | CLI profile name when `grok.backend` is `profile`                                       | profile name                                         |                                                      |
| `CONSULT_LLM_ZAI_CLI_PROFILE`              | CLI profile name when `zai.backend` is `profile`                                        | profile name                                         |                                                      |
| `CONSULT_LLM_OPENROUTER_CLI_PROFILE`       | CLI profile name when `openrouter.backend` is `profile`                                 | profile name                                         |                                                      |
| `CONSULT_LLM_OPENCODE_OPENAI_PROVIDER`     | OpenCode provider for OpenAI models                                                     | provider name                                        | `openai`                                             |
| `CONSULT_LLM_OPENCODE_GEMINI_PROVIDER`     | OpenCode provider for Gemini models                                                     | provider name                                        | `google`                                             |
| `CONSULT_LLM_OPENCODE_DEEPSEEK_PROVIDER`   | OpenCode provider for DeepSeek models                                                   | provider name                                        | `deepseek`                                           |
| `CONSULT_LLM_OPENCODE_MINIMAX_PROVIDER`    | OpenCode provider for MiniMax models                                                    | provider name                                        | `minimax`                                            |
| `CONSULT_LLM_OPENCODE_OPENROUTER_PROVIDER` | OpenCode provider for OpenRouter models                                                 | provider name                                        | `openrouter`                                         |
| `CONSULT_LLM_SYSTEM_PROMPT_PATH`           | Path to a custom system prompt file                                                     | file path                                            | `~/.config/consult-llm/SYSTEM_PROMPT.md`             |
| `CONSULT_LLM_NO_UPDATE_CHECK`              | Disable background update checks                                                        | `1` `true` `yes`                                     |                                                      |

</details>

## Logging

All prompts and responses are logged to:

```text
$XDG_STATE_HOME/consult-llm/consult-llm.log
```

Default: `~/.local/state/consult-llm/consult-llm.log`

Each entry includes tool call arguments, the full prompt, the full response,
and token usage with cost estimates.

<details>
<summary>Example log entry</summary>

```
[2025-06-22T20:16:04.675Z] PROMPT (model: deepseek-v4-pro):
## Relevant Files

### File: src/main.ts

...

Please provide specific suggestions for refactoring with example code structure
where helpful.
================================================================================
[2025-06-22T20:19:20.632Z] RESPONSE (model: deepseek-v4-pro):
Based on the analysis, here are the key refactoring suggestions to improve
separation of concerns and maintainability:

...

This refactoring maintains all existing functionality while significantly
improving maintainability and separation of concerns.

Tokens: 3440 input, 5880 output | Cost: $0.014769 (input: $0.001892, output: $0.012877)
================================================================================
```

</details>

## Monitor

`consult-llm-monitor` is a real-time TUI for active runs and history.

<p align="center">
  <img src="meta/monitor-demo.gif" alt="consult-llm-monitor demo" width="800">
</p>

```bash
consult-llm-monitor
```

It reads the per-run spool written by `consult-llm`, including active snapshots,
run metadata, event streams, and shared history.

## How it really works

`consult-llm` keeps orchestration in the host agent and uses the CLI as a
small transport boundary. Instead of manually copying context into a browser LLM
or juggling another agent TUI, your current agent can hand off a focused prompt,
stream the answer back inline, and continue the conversation from there.

That boundary also lets the host agent and external model talk to each other in
multi-turn workflows. `/consult` can ask for a second opinion, `/debate` can
have models critique each other, and threaded CLI backends can continue the same
conversation without leaving the agent session.

The installed skills are reusable workflow definitions; the backend is just
configuration. You can use Codex CLI for personal projects, Cursor CLI at work,
direct APIs in CI, or different default model lists per repo while keeping the
same `/consult`, `/debate`, and `/review-panel` habits.

At runtime, the installed skill decides what context to include, formats the
prompt, and invokes `consult-llm` with stdin plus `-f` file attachments. API
backends receive only that explicit prompt and file context. CLI-agent backends
such as Gemini CLI, Codex CLI etc. can also inspect the working tree themselves,
depending on their own tools and permissions. The CLI resolves layered
configuration, selects the requested backend, streams the model response to
stdout, and records run metadata for logging and monitoring.

If you like sequence diagrams, here's one for you:

```mermaid
sequenceDiagram
    participant User
    participant Agent as Host agent<br/>(Claude Code, Codex, OpenCode, Pi)
    participant Skill as Workflow skill<br/>(/consult, /debate, /collab)
    participant CLI as consult-llm CLI
    participant Config as Config resolver
    participant Backend as Backend adapter<br/>(API or local CLI)
    participant Model as External model
    participant Logs as Logs / monitor spool

    User->>Agent: Ask for a second opinion
    Agent->>Skill: Load matching workflow skill
    Skill->>Skill: Gather prompt and file context
    Skill->>CLI: Pipe prompt via stdin<br/>pass files with -f
    CLI->>Config: Resolve layered config and model selectors
    Config-->>CLI: Backend, model, prompt settings
    CLI->>Backend: Dispatch normalized request
    Backend->>Model: API request or local CLI invocation
    Model-->>Backend: Streaming response
    Backend-->>CLI: Normalized stream and metadata
    CLI-->>Logs: Write prompt, response, usage, run state
    CLI-->>Agent: Stream response on stdout
    Agent-->>User: Summarize and apply next steps
```

## Skills

### Architecture

The skill system has two layers:

**`consult-llm` (base CLI)** handles the mechanics: reading stdin, attaching file context, calling the right backend, streaming the response, and managing thread IDs for multi-turn conversations. A dedicated `consult-llm` reference skill documents this contract and is loaded by other skills before they invoke the CLI.

**Workflow skills** compose on top. They gather context from the codebase, decide which models to call and how, and synthesize the results for you. When you run `/consult` or `/debate`, the agent reads a skill file that tells it how to orchestrate one or more `consult-llm` calls and what to do with the responses.

### Invocation

When a workflow skill runs, the agent pipes the prompt via stdin and passes file context with `-f`:

```bash
cat <<'__CONSULT_LLM_END__' | consult-llm -m gemini -f src/main.rs -f src/config.rs
Your question here.
__CONSULT_LLM_END__
```

The response streams back to stdout and the agent sees it inline. If the response exceeds the shell tool's output limit (30k chars in Claude Code by default), the full output is saved to a file and the agent is notified where to find it; it can use `Read` to retrieve the rest. In practice this is rare; the large majority of responses are well under that limit.

### Install

```bash
consult-llm install-skills
```

Installs to all detected platforms. Target a specific one with `--platform`:

```bash
consult-llm install-skills --platform claude
consult-llm install-skills --platform opencode
consult-llm install-skills --platform codex
consult-llm install-skills --platform pi
```

Platforms supported:

- Claude Code: `~/.claude/skills/`
- OpenCode: `~/.config/opencode/skills/`
- Codex: `~/.codex/skills/`
- Pi: `~/.pi/agent/skills/`

### Workflow skills

All workflow skills accept `--<selector>` flags matching the selectors reported by `consult-llm models` (e.g. `--gemini`, `--openai`, `--deepseek`). With no selector flag, multi-model skills use the ordered `Default models` list printed by `consult-llm models`, which comes from `default_models`; duplicate entries are intentional and preserved.

- [`consult`](skills/consult/SKILL.md): ask one or more external LLMs; any number of `--<selector>` flags, plus `--browser` for clipboard/web mode
- [`collab`](skills/collab/SKILL.md): multiple LLMs brainstorm together, building on each other's ideas
- [`collab-vs`](skills/collab-vs/SKILL.md): the agent brainstorms with one partner LLM (`--<selector>` required) in alternating turns
- [`debate`](skills/debate/SKILL.md): multiple LLMs propose and critique competing approaches
- [`debate-vs`](skills/debate-vs/SKILL.md): the agent debates one opponent LLM (`--<selector>` required), then synthesizes the best answer
- [`panel`](skills/panel/SKILL.md): role-asymmetric advisory panel; each model speaks from one expert lens, agent synthesizes a trade-off resolution. The agent picks roles to fit the task (with a `--roles` override). Modes: `--mode design` (default) or `--mode review` for diff critique
- [`review-panel`](skills/review-panel/SKILL.md): standalone multi-model code review of a diff with identical prompts; agent dedupes findings by severity/confidence. Read-only by default; `--fix` opt-in for localized must-fix items
- [`implement`](skills/implement/SKILL.md): autonomous spec → plan → review → implement → red-team workflow. Evidence-gated reviewers, written feedback ledger, triggered debug loop, opt-in commits. Rigor knob: `--rigor lite|standard|deep`
- [`phased-implement`](skills/phased-implement/SKILL.md): coordinator that breaks a large task into a DAG of phases, each running `/implement` in its own [workmux](https://github.com/raine/workmux) worktree. Supports sequential, parallel, and mixed dependencies; per-phase merge with `/merge --keep` and ancestry verification; failure halts dependents. Requires `workmux`
- [`workshop`](skills/workshop/SKILL.md): interactive design session - agent clarifies the idea with the user, fans out to multiple LLMs in parallel for divergent approach generation, user picks one, then co-design with optional multi-LLM critique. Saves a design doc; hand it to `/implement` to build

See `skills/*/SKILL.md` for the exact prompts and invocation patterns.

## Updating

```bash
consult-llm update
```

This downloads the latest GitHub release, verifies its SHA-256 checksum, updates
`consult-llm`, and updates `consult-llm-monitor` if it lives alongside it.

## Migrating from MCP

If you previously used the MCP server version (`consult-llm-mcp` npm package):

1. **Install the CLI binary** (see [Quick Start](#quick-start)).

2. **Install skills** so your agent can call `consult-llm` for you:

   ```bash
   consult-llm install-skills
   ```

3. **Migrate your config.** Any env vars you set in the MCP `"env"` block can move to `~/.config/consult-llm/config.yaml`, including API keys.

   For example, this MCP config in `~/.claude.json`:

   ```json
   "mcpServers": {
     "consult-llm": {
       "command": "npx",
       "args": ["-y", "consult-llm-mcp"],
       "env": {
         "CONSULT_LLM_GEMINI_BACKEND": "api",
         "CONSULT_LLM_OPENAI_BACKEND": "codex-cli",
         "CONSULT_LLM_CODEX_REASONING_EFFORT": "xhigh",
         "CONSULT_LLM_ALLOWED_MODELS": "gpt-5.4,gemini-3.1-pro-preview,MiniMax-M2.7",
         "CONSULT_LLM_MINIMAX_BACKEND": "opencode",
         "CONSULT_LLM_OPENCODE_MINIMAX_PROVIDER": "minimax"
       }
     }
   }
   ```

   becomes:

   ```yaml
   allowed_models: [gpt-5.4, gemini-3.1-pro-preview, MiniMax-M2.7]

   gemini:
     backend: api

   openai:
     backend: codex-cli
     reasoning_effort: xhigh

   minimax:
     backend: opencode
     opencode_provider: minimax
   ```

   Put this in `~/.config/consult-llm/config.yaml` for user-wide settings, or in `.consult-llm.yaml` at the project root if the settings were specific to that project.

4. **Remove the MCP server registration** from your Claude Code config (`~/.claude.json`):

   ```json
   "mcpServers": {
     // remove this entry:
     "consult-llm": { ... }
   }
   ```

5. **Uninstall the npm package** if you installed it globally:

   ```bash
   npm uninstall -g consult-llm-mcp
   ```

## Development

```bash
git clone https://github.com/raine/consult-llm.git
cd consult-llm
just check
```

`just check` runs the standard local validation, including build and tests. Use `cargo build` or `cargo test` directly only when iterating on one step.

Try the local binary directly:

```bash
cat <<'EOF' | cargo run -- -m gemini
Sanity-check the local build and explain what this CLI does well.
EOF
```

## Releasing

See [RELEASE.md](RELEASE.md).

## Related Projects

- [workmux](https://github.com/raine/workmux)
- [claude-history](https://github.com/raine/claude-history)
- [tmux-file-picker](https://github.com/raine/tmux-file-picker)
- [tmux-agent-usage](https://github.com/raine/tmux-agent-usage)
