# Releasing

Requires [`cargo-release`](https://github.com/raine/rust-release-tools) from rust-release-tools.

```bash
cargo-release --skip-publish patch   # or minor / major
```

This bumps the version in `Cargo.toml`, generates a changelog entry, commits,
tags, and pushes. GitHub Actions then builds binaries, creates the GitHub
release, publishes all three crates to crates.io, and updates the Homebrew tap.

Use `--skip-publish` because crates.io publishing is handled by CI
(`publish-crates` job) so the workspace dependency order is resolved correctly.

## Bundled skill release checks

`consult-llm` owns the canonical `consult-llm` reference skill and every bundled
`llm-*` workflow skill under `skills/`. The release binary embeds these files;
it does not download them at install time.

Before releasing, run `just check` and verify:

```bash
cargo run -- skill list --json
cargo run -- version --json
```

Every catalog entry must report the release version as `cli_version` and skill
schema `1`. Update the corresponding SKILL.md frontmatter in the same commit as
any workflow or CLI-contract change. Do not release if `skill print <name>` and
a dry-run install disagree with the source bytes.
