# Consult LLM MCP

An MCP server that lets Claude Code consult stronger AI models (GPT-5.2, Gemini
3.0 Pro, DeepSeek Reasoner) when Sonnet has you running in circles and you need
to bring in the heavy artillery.

```
> Still getting this error after your fix. Ask gemini
  E5108: Error executing lua: attempt to call method 'child' (a nil value)

⏺ Let me consult Gemini to understand the Neovim treesitter API changes:

⏺ consult-llm:consult_llm (MCP)(prompt: "Neovim plugin broken due to treesitter
                                API changes...", files: ["lua/testonly.lua"],
                                model: "gemini-2.5-pro")
  ⎿ The issue is that iter_matches() can return nil nodes in newer Neovim
    versions. Switch to iter_captures() which never returns nil nodes, and use
    named_child(0) instead of child(0) to get only meaningful syntax nodes.
    … +93 lines (ctrl+r to expand)

⏺ The LLM identified the issue - iter_matches() API changed. Switching to
  iter_captures() approach now.
```

## Features

- Query powerful AI models (GPT-5.2, Gemini 3.0 Pro, DeepSeek Reasoner) with
  relevant files as context
- Direct queries with optional file context
- Include git changes for code review and analysis
- Comprehensive logging with cost estimation
- [Gemini CLI mode](#gemini-cli): Use the `gemini` CLI to take advantage of
  [free quota](https://developers.google.com/gemini-code-assist/resources/quotas#quotas-for-agent-mode-gemini-cli)
- [Codex CLI mode](#codex-cli): Use the `codex` CLI for OpenAI models
- [Web mode](#web-mode): Copy formatted prompts to clipboard for browser-based
  LLM services
- Simple: provides just one MCP tool to not clutter the context

## Quick start

1. **Add to Claude Code**:

   ```bash
   claude mcp add consult-llm -e GEMINI_API_KEY=your_key -- npx -y consult-llm-mcp
   ```

   For global availability across projects, add `--scope user`.

   <details>
   <summary>Using multiple API keys or CLI mode</summary>

   ```bash
   claude mcp add consult-llm \
     -e OPENAI_API_KEY=your_openai_key \
     -e GEMINI_API_KEY=your_gemini_key \
     -e DEEPSEEK_API_KEY=your_deepseek_key \
     -e GEMINI_MODE=cli \
     -- npx -y consult-llm-mcp
   ```

   </details>

2. **Verify connection** with `/mcp`:

   ```
   ❯ 1. consult-llm            ✔ connected
   ```

3. **Ask a question**:
   > "Consult Gemini about how to fix the race condition in server.ts"

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
  ⎿  Wrote 53 lines to hint-segmentation-issue.md
     # Hint Segmentation Mismatch Issue
     ## Problem Description
     There's a mismatch between how the frontend segments text for clickable
     words and how the backend generates hints. This causes hint unlocking to
     fail when the segments don't match exactly.
     ## Example Case
     **Original Text**: "Hello! Nice weather, isn't it?"
     … +43 lines (ctrl+r to expand)

⏺ consult-llm:consult_llm (MCP)(model: "gemini-2.5-pro", files: ["/project/hint
                                -segmentation-issue.md","/project/web/src/compo
                                nents/HintsText.tsx","/project/web/src/utils/hi
                                nts.ts","/project/server/src/routes/public/hint
                                s.ts","/project/server/src/llm/hints.ts"])
  ⎿ An excellent analysis of the problem. The root cause has been correctly
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
  ⎿  ☐ Fix hint segmentation mismatch by implementing smarter hint key
       targeting in handleConfirmGenerateHints
```

</details>

<details>
<summary>Using web mode to copy prompts to clipboard for browser-based LLMs</summary>

```
> I need help implementing a -b/--background flag for my Rust CLI. Use consult LLM with web mode

⏺ I'll use web mode to copy the formatted prompt to your clipboard so you can
  paste it into any browser-based LLM service.

⏺ consult-llm:consult_llm (MCP)(prompt: "What is the best approach to implement a
                                 -b/--background flag for the \"workmux add\" command that
                                 creates a tmux window in the background without switching to
                                 it?\n\nThe current implementation automatically switches to
                                 the newly created window via `tmux::select_window()` in the
                                 `setup_environment()` function. I need to make this behavior
                                 conditional based on a flag passed from the
                                 CLI.\n\nConsider:\n1. Where should the background flag be
                                 threaded through the call chain?\n2. Should I modify the
                                 existing SetupOptions struct or add a parameter?\n3. What's
                                 the most maintainable approach given the codebase
                                 structure?\n4. Are there any edge cases to consider (e.g.,
                                 should select_pane also be conditional)?", files:
                                 ["src/cli.rs","src/workflow.rs","src/tmux.rs"], web_mode:
                                 true)
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

⏺ consult-llm:consult_llm (MCP)(prompt: "I'm debugging a Neovim plugin that's broken due to treesitter API changes. The error is \"attempt to call method 'child' (a nil value)\" on line 92. The
                               code is trying to access child nodes from treesitter matches but the API seems to have changed in newer Neovim versions.\n\nHere's the problematic
                               code:\n```lua\nfor _, match, _ in parsed_query:iter_matches(root, bufnr, start_row, end_row) do\n    for id, node in pairs(match) do\n        local name =
                               parsed_query.captures[id]\n        if name == \"call\" and node then\n            local child = node:child(0)  -- ERROR HERE\n            if child then\n
                                    local text = vim.treesitter.get_node_text(child, bufnr)\n                -- ... rest of logic\n            end\n        end\n    end\nend\n```\n\nWhat's the
                               correct way to iterate over treesitter query matches and access child nodes in current Neovim versions? What API changes should I be aware of?", files:
                               ["/Users/raine/code/testonly.nvim/lua/testonly.lua"])
  ⎿  Problem
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

⏺ Fixed! The key changes:

  1. Switched from iter_matches() to iter_captures() which never returns nil nodes
  2. Used named_child(0) instead of child(0) to get only meaningful syntax nodes (not punctuation)
  3. Simplified the iteration logic since iter_captures() directly gives us the node and capture ID

  Your <leader>tr mapping should now work properly with the current Neovim treesitter API.
````

</details>

<details>
<summary>Using slash command to consult multiple LLMs for comparison</summary>

```
> /consult The branch completions should dynamic. Git should be called only when
using tab to autocomplete. Ask gemini and codex

⏺ I'll consult both Gemini and Codex about making the branch completions dynamic (only
  calling git when tab-completing). Let me gather the relevant context first.

⏺ consult-llm - consult_llm (MCP)(prompt: "I have a Rust CLI tool using clap that needs dynamic shell completions. Currently, the branch completions are generated at completion script
                                 generation time via `possible_values()`. Instead, I need the git operations to only run when the user presses TAB to autocomplete.\n\nCurrent setup:\n1.
                                 `WorktreeBranchParser` implements `TypedValueParser` with `possible_values()` that calls git\n2. Shell completions are generated via clap_complete's
                                 `generate()` function\n3. The completions are sourced at shell startup: `source <(workmux completions zsh)`\n\nThe problem: Running `workmux completions
                                 zsh` takes ~210ms because it runs git operations during completion generation, not during actual tab completion.\n\nHow can I implement truly dynamic
                                 completions where git is only called when the user presses TAB? What's the best approach with clap and clap_complete for zsh?", model:
                                 "gemini-3-pro-preview", files: ["/Users/raine/code/workmux/src/cli.rs","/Users/raine/code/workmux/src/git.rs"], web_mode: false)
  ⎿ The root cause of the slow startup (~210ms) is that `clap_complete::generate()` executes your `possible_values()` logic (git operations) *while generating the script*. This bake
    s the *current* branches into the static script, which is both slow (happens every time you `source` it) and incorrect (branches become stale).
    … +116 lines (ctrl+o to expand)


⏺ consult-llm - consult_llm (MCP)(prompt: "I have a Rust CLI tool using clap that needs dynamic shell completions. Currently, the branch completions are generated at completion script
                                 generation time via `possible_values()`. Instead, I need the git operations to only run when the user presses TAB to autocomplete.\n\nCurrent setup:\n1.
                                 `WorktreeBranchParser` implements `TypedValueParser` with `possible_values()` that calls git\n2. Shell completions are generated via clap_complete's
                                 `generate()` function\n3. The completions are sourced at shell startup: `source <(workmux completions zsh)`\n\nThe problem: Running `workmux completions
                                 zsh` takes ~210ms because it runs git operations during completion generation, not during actual tab completion.\n\nHow can I implement truly dynamic
                                 completions where git is only called when the user presses TAB? What's the best approach with clap and clap_complete for zsh?", model:
                                 "gpt-5.1-codex-max", files: ["/Users/raine/code/workmux/src/cli.rs","/Users/raine/code/workmux/src/git.rs"], web_mode: false)
  ⎿ **Findings**
    - `WorktreeBranchParser::possible_values` (`cli.rs:41-72`) runs git during script generation, so `workmux completions zsh` always pays the git cost (~210ms) even though the resu
    lt is only needed at tab time.
    … +77 lines (ctrl+o to expand)


⏺ Summary from Gemini and Codex

  Both agree on the same solution:

  Root Cause

  clap_complete::generate() calls possible_values() during script generation, not at tab-completion time. This means git runs every shell startup.

...
```

This example shows using the `/consult` slash command to ask multiple LLMs
(Gemini and Codex) about the same problem **in parallel** and compare their
responses. Both LLMs independently arrived at the same solution, providing
confidence in the approach.

</details>

## Modes

consult-llm-mcp supports three modes of operation:

| Mode    | Description                   | When to use                                                      |
| ------- | ----------------------------- | ---------------------------------------------------------------- |
| **API** | Queries LLM APIs directly     | You have API keys and want the simplest setup                    |
| **CLI** | Shells out to local CLI tools | Free quota (Gemini), existing subscriptions, or prefer CLI tools |
| **Web** | Copies prompt to clipboard    | You prefer browser UIs or want to review prompts                 |

### API mode (default)

The default mode. Requires API keys configured via environment variables. See
[Configuration](#configuration) for details.

### CLI mode

Instead of making API calls, shell out to local CLI tools. The CLI agents can
explore the codebase themselves, so you don't need to pass all relevant files as
context, but it helps.

#### Gemini CLI

Use Gemini's local CLI to take advantage of Google's
[free quota](https://developers.google.com/gemini-code-assist/resources/quotas#quotas-for-agent-mode-gemini-cli).

**Requirements:**

1. Install the [Gemini CLI](https://github.com/google-gemini/gemini-cli)
2. Authenticate via `gemini login`

**Setup:**

```bash
claude mcp add consult-llm -e GEMINI_MODE=cli -- npx -y consult-llm-mcp
```

#### Codex CLI

Use OpenAI's Codex CLI for OpenAI models.

**Requirements:**

1. Install the Codex CLI
2. Authenticate via `codex login`

**Setup:**

```bash
claude mcp add consult-llm -e OPENAI_MODE=cli -- npx -y consult-llm-mcp
```

<!-- prettier-ignore -->
> [!TIP]
> Set reasoning effort with `-e CODEX_REASONING_EFFORT=high`. Options:
> `none`, `minimal`, `low`, `medium`, `high`, `xhigh` (gpt-5.1-codex-max only).

### Web mode

Copies the formatted prompt to clipboard instead of querying an LLM. Paste into
any browser-based LLM (ChatGPT, Claude.ai, Gemini, etc.).

**When to use:** Prefer a specific web UI, want to review the prompt first, or
don't have API keys.

**Workflow:**

1. Ask Claude to "use consult LLM with web mode"
2. Paste into your browser-based LLM
3. Paste the response back into Claude Code

See the "Using web mode..." example above for a concrete transcript.

## Configuration

### Environment variables

- `OPENAI_API_KEY` - Your OpenAI API key (required for OpenAI models in API
  mode)
- `GEMINI_API_KEY` - Your Google AI API key (required for Gemini models in API
  mode)
- `DEEPSEEK_API_KEY` - Your DeepSeek API key (required for DeepSeek models)
- `CONSULT_LLM_DEFAULT_MODEL` - Override the default model (optional)
  - Options: `gpt-5.2` (default), `gemini-2.5-pro`, `gemini-3-pro-preview`,
    `deepseek-v4-pro`, `deepseek-v4-flash`, `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.1-codex-max`,
    `gpt-5.1-codex`, `gpt-5.1-codex-mini`, `gpt-5.1`
- `GEMINI_MODE` - Choose between API or CLI mode for Gemini models (optional)
  - Options: `api` (default), `cli`
  - CLI mode uses the system-installed `gemini` CLI tool
- `OPENAI_MODE` - Choose between API or CLI mode for OpenAI models (optional)
  - Options: `api` (default), `cli`
  - CLI mode uses the system-installed `codex` CLI tool
- `CODEX_REASONING_EFFORT` - Configure reasoning effort for Codex CLI (optional)
  - See [Codex CLI](#codex-cli) for details and available options
- `CONSULT_LLM_ALLOWED_MODELS` - List of models to advertise (optional)
  - Comma-separated list, e.g., `gpt-5.2,gemini-3-pro-preview`
  - When set, only these models appear in the tool schema
  - If `CONSULT_LLM_DEFAULT_MODEL` is set, it must be in this list
  - See [Tips](#controlling-which-models-claude-uses) for usage examples
- `CONSULT_LLM_SYSTEM_PROMPT_PATH` - Custom path to system prompt file
  (optional)
  - Overrides the default `~/.consult-llm-mcp/SYSTEM_PROMPT.md` location
  - Useful for project-specific prompts

### Custom system prompt

You can customize the system prompt used when consulting LLMs by creating a
`SYSTEM_PROMPT.md` file in `~/.consult-llm-mcp/`:

```bash
npx consult-llm-mcp init-prompt
```

This creates a placeholder file with the default system prompt that you can edit
to customize how the consultant LLM behaves. The custom prompt is read on every
request, so changes take effect immediately without restarting the server.

To revert to the default prompt, simply delete the `SYSTEM_PROMPT.md` file.

#### Custom prompt path

Use `CONSULT_LLM_SYSTEM_PROMPT_PATH` to override the default prompt file
location. This is useful for project-specific prompts that you can commit to
your repository:

```bash
claude mcp add consult-llm \
  -e GEMINI_API_KEY=your_key \
  -e CONSULT_LLM_SYSTEM_PROMPT_PATH=/path/to/project/.consult-llm-mcp/SYSTEM_PROMPT.md \
  -- npx -y consult-llm-mcp
```

## Tips

### Controlling which models Claude uses

When you ask Claude to "consult an LLM" without specifying a model, it picks one
from the available options in the tool schema. The `CONSULT_LLM_DEFAULT_MODEL`
only affects the fallback when no model is specified in the tool call.

To control which models Claude can choose from, use
`CONSULT_LLM_ALLOWED_MODELS`:

```bash
claude mcp add consult-llm \
  -e GEMINI_API_KEY=your_key \
  -e CONSULT_LLM_ALLOWED_MODELS='gemini-3-pro-preview,gpt-5.2-codex' \
  -- npx -y consult-llm-mcp
```

This restricts the tool schema to only advertise these models. For example, to
ensure Claude always uses Gemini 3 Pro:

```bash
claude mcp add consult-llm \
  -e GEMINI_API_KEY=your_key \
  -e CONSULT_LLM_ALLOWED_MODELS='gemini-3-pro-preview' \
  -- npx -y consult-llm-mcp
```

Alternatively, use a [slash command](#example-slash-command) with hardcoded
model names for guaranteed model selection.

## MCP tool: consult_llm

The server provides a single tool called `consult_llm` for asking powerful AI
models complex questions.

### Parameters

- **prompt** (required): Your question or request for the consultant LLM

- **files** (optional): Array of file paths to include as context
  - All files are added as context with file paths and code blocks

- **model** (optional): LLM model to use
  - Options: `gpt-5.2` (default), `gemini-2.5-pro`, `gemini-3-pro-preview`,
    `deepseek-v4-pro`, `deepseek-v4-flash`, `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.1-codex-max`,
    `gpt-5.1-codex`, `gpt-5.1-codex-mini`, `gpt-5.1`

- **web_mode** (optional): Copy prompt to clipboard instead of querying LLM
  - Default: `false`
  - When `true`, the formatted prompt (including system prompt and file
    contents) is copied to clipboard for manual pasting into browser-based LLM
    services

- **git_diff** (optional): Include git diff output as context
  - **files** (required): Specific files to include in diff
  - **repo_path** (optional): Path to git repository (defaults to current
    directory)
  - **base_ref** (optional): Git reference to compare against (defaults to HEAD)

## Supported models

- **gemini-2.5-pro**: Google's Gemini 2.5 Pro ($1.25/$10 per million tokens)
- **gemini-3-pro-preview**: Google's Gemini 3 Pro Preview ($2/$12 per million
  tokens for prompts ≤200k tokens, $4/$18 for prompts >200k tokens)
- **deepseek-v4-pro**: DeepSeek's advanced/reasoning model ($0.435/$0.87 per
  million tokens). Replaces the retired `deepseek-reasoner` (deprecated
  2026-07-24).
- **deepseek-v4-flash**: DeepSeek's fast model ($0.14/$0.28 per million tokens).
  Replaces the retired `deepseek-chat` (deprecated 2026-07-24).
- **gpt-5.2**: OpenAI's latest GPT model
- **gpt-5.3-codex**: OpenAI's Codex model based on GPT-5.3
- **gpt-5.2-codex**: OpenAI's Codex model based on GPT-5.2
- **gpt-5.1-codex-max**: Strongest OpenAI Codex model
- **gpt-5.1-codex**: OpenAI's Codex model optimized for coding
- **gpt-5.1-codex-mini**: Lighter, faster version of gpt-5.1-codex
- **gpt-5.1**: Broad world knowledge with strong general reasoning

## Logging

All prompts and responses are logged to
`$XDG_STATE_HOME/consult-llm-mcp/mcp.log` (defaults to
`~/.local/state/consult-llm-mcp/mcp.log`) with:

- Tool call parameters
- Full prompts and responses
- Token usage and cost estimates

<details>
<summary>Example</summary>

```
[2025-06-22T20:16:04.673Z] TOOL CALL: consult_llm
Arguments: {
  "files": [
    "refactor-analysis.md",
    "src/main.ts",
    "src/schema.ts",
    "src/config.ts",
    "src/llm.ts",
    "src/llm-cost.ts"
  ],
  "model": "deepseek-reasoner"
}
================================================================================
[2025-06-22T20:16:04.675Z] PROMPT (model: deepseek-reasoner):
## Relevant Files

### File: src/main.ts

...

Please provide specific suggestions for refactoring with example code structure
where helpful.
================================================================================
[2025-06-22T20:19:20.632Z] RESPONSE (model: deepseek-reasoner):
Based on the analysis, here are the key refactoring suggestions to improve
separation of concerns and maintainability:

...

This refactoring maintains all existing functionality while significantly
improving maintainability and separation of concerns. The new structure makes
it easier to add features like new LLM providers, additional context sources,
or alternative prompt formats.

Tokens: 3440 input, 5880 output | Cost: $0.014769 (input: $0.001892, output: $0.012877)
```

</details>

## Activation methods

### 1. No custom activation (simplest)

When you add an MCP to Claude Code, the tool's schema is injected into the
agent's context. This allows Claude to infer when to call the MCP from natural
language (e.g., "ask gemini about..."). Works out of the box, but you have less
control over how the MCP is invoked.

### 2. Slash commands (most reliable)

Explicitly invoke with `/consult ask gemini about X`. Guaranteed activation with
full control over custom instructions, but requires the explicit syntax. For
example, you can instruct Claude to always find related files and pass them as
context via the `files` parameter. See the
[example slash command](#example-slash-command) below.

### 3. Skills

Automatically triggers when Claude detects matching intent. Like slash commands,
supports custom instructions (e.g., always gathering relevant files), but not
always reliably triggered. See the [example skill](#example-skill) below.

**Recommendation:** Start with no custom activation. Use slash commands if you
need reliability or custom instructions.

## Example skill

Here's an example [Claude Code skill](https://code.claude.com/docs/en/skills)
that uses the `consult_llm` MCP tool to create commands like "ask gemini" or
"ask codex". See [examples/SKILL.md](examples/SKILL.md) for the full content.

Save it as `~/.claude/skills/consult-llm/SKILL.md` and you can then use it by
typing "ask gemini about X" or "ask codex about X" in Claude Code.

This one is not strictly necessary either, Claude (or other agent) can infer
from the schema that "Ask gemini" should call this MCP, but it might be helpful
in case you want to have more precise control over how the agent calls this MCP.

## Example slash command

Here's an example
[Claude Code slash command](https://code.claude.com/docs/en/slash-commands) that
uses the `consult_llm` MCP tool. See [examples/consult.md](examples/consult.md)
for the full content.

Save it as `~/.claude/commands/consult.md` and you can then use it by typing
`/consult ask gemini about X` or `/consult ask codex about X` in Claude Code.

## Development

To work on the MCP server locally and use your development version:

1. Clone the repository and install dependencies:

   ```bash
   git clone https://github.com/yourusername/consult-llm-mcp.git
   cd consult-llm-mcp
   npm install
   ```

2. Build the project:

   ```bash
   npm run build
   ```

3. Install globally from the local directory:

   ```bash
   npm link
   ```

4. Add the MCP server to Claude Code using the global command:
   ```bash
   claude mcp add consult-llm -- consult-llm-mcp
   ```

Now when you make changes:

1. Rebuild: `npm run build`
2. Restart Claude Code to pick up the changes

Alternatively, you can use the dev script for development without building:

```bash
claude mcp add consult-llm -- npm run dev
```

This runs the TypeScript source directly with `tsx`, allowing faster iteration
without rebuilding.

To unlink the global version later:

```bash
npm unlink -g
```

## Related projects

- [workmux](https://github.com/raine/workmux) — Git worktrees + tmux windows for
  parallel AI agent workflows
- [claude-history](https://github.com/raine/claude-history) — Search and view
  Claude Code conversation history with fzf
- [tmux-file-picker](https://github.com/raine/tmux-file-picker) — Pop up fzf in
  tmux to quickly insert file paths, perfect for AI coding assistants
