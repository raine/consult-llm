# Rust project checks

set positional-arguments
set shell := ["bash", "-euo", "pipefail", "-c"]

# List available commands
default:
    @just --list

# Run project checks through checkle
check:
    checkle run all

# Run check and fail if there are uncommitted changes for CI
check-ci: check
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Error: check caused uncommitted changes"
        echo "Run 'just check' locally and commit the results"
        git diff --stat
        exit 1
    fi

# Install shims into the Git hooks directory
install-hooks:
    scripts/install-git-hook-shims

# Check Rust formatting through checkle
format:
    checkle run format-check

# Check clippy through checkle
clippy:
    checkle run clippy

# Check the build through checkle
build:
    checkle run build

# Run tests through checkle
test:
    checkle run test

# Install debug binaries globally via symlink
install-dev:
    cargo build && ln -sf $(pwd)/target/debug/consult-llm ~/.cargo/bin/consult-llm && ln -sf $(pwd)/target/debug/consult-llm-mcp ~/.cargo/bin/consult-llm-mcp && ln -sf $(pwd)/target/debug/consult-llm-monitor ~/.cargo/bin/consult-llm-monitor

# Install release binaries globally
install:
    cargo install --offline --path . --locked
    cargo install --offline --path crates/monitor --locked

# Install release binaries globally from GitHub Actions (Apple silicon macOS)
install-ci:
    scripts/install-ci

# Release a new version
release bump="patch":
    cargo-release --skip-publish {{bump}}

# Run the application
run *ARGS:
    cargo run -- "$@"

# Run the TUI monitor
monitor:
    cargo run -p consult-llm-monitor
