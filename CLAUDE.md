# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Detected stack
- Languages: Rust.
- Frameworks: none detected from the supported starter markers.

## Verification
- Run Rust verification from `rust/`: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`
- `src/` and `tests/` are both present; update both surfaces together when behavior changes.

## Repository shape
- `rust/` contains the Rust workspace and active CLI/runtime implementation.
- `src/` contains source files that should stay consistent with generated guidance and tests.
- `tests/` contains validation surfaces that should be reviewed alongside code changes.

## Working agreement
- Prefer small, reviewable changes and keep generated bootstrap files aligned with actual repo workflows.
- Keep shared defaults in `.claude.json`; reserve `.claude/settings.local.json` for machine-local overrides.
- When changing tool dispatch, permission behavior, or workspace file-mutation paths, update `README.md`, `USAGE.md`, `PARITY.md`, and their `rust/` counterparts in the same patch.
- Treat built-ins, plugin tools, `ToolSearch`, and runtime/MCP tools as one enforcement surface unless the code intentionally diverges.
- Do not overwrite existing `CLAUDE.md` content automatically; update it intentionally when repo workflows change.
