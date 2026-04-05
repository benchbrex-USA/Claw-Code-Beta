# Claw Code Beta

Claw Code Beta is the maintained Rust implementation of the `claw` CLI: an agentic coding harness with Anthropic streaming, permission-aware local execution, plugin tools, MCP integration, OAuth login, and persistent sessions.

The shipping product lives in [`rust/`](./rust). The root-level [`src/`](./src) and [`tests/`](./tests) trees remain in the repository as compatibility and reference surfaces, but the runtime, CLI, and tool system that ship today are in the Rust workspace.

## Architecture

- `rust/crates/rusty-claude-cli` builds the `claw` binary, REPL, one-shot prompt flow, auth, session management, status reports, and runtime assembly.
- `rust/crates/runtime` owns the conversation loop, config loading, OAuth storage, session persistence, permission policy, workspace-safe file ops, and MCP stdio/runtime plumbing.
- `rust/crates/tools` owns the built-in tool catalog, dispatch, notebook helpers, subagent helpers, and plugin/runtime tool integration.
- `rust/crates/api`, `commands`, `plugins`, `mock-anthropic-service`, `telemetry`, and `compat-harness` provide the Anthropic client, slash-command handling, plugin lifecycle, deterministic mock testing, telemetry types, and upstream manifest extraction.

## Quick Start

```bash
cd rust
cargo build --workspace

# interactive session
cargo run -p rusty-claude-cli -- --help

# run the built binary from the workspace you want to grant access to
cd ..
./rust/target/debug/claw
./rust/target/debug/claw prompt "summarize this repository"
```

Authentication:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_BASE_URL="https://your-proxy.example" # optional

cd rust
cargo run -p rusty-claude-cli -- login
```

## Current Runtime Guarantees

- The default permission mode is `workspace-write`. Escalation to `danger-full-access` must be explicit.
- Built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions resolve through one permission policy surface.
- Local file, notebook, and plugin path inputs are mediated against the active workspace boundary in `workspace-write` mode.
- Bash classification follows actual sandbox status: if filesystem isolation is inactive, the runtime upgrades bash to `danger-full-access`.
- The mock Anthropic parity harness is part of the active workspace test suite.

## Mock Parity Harness

The workspace includes a deterministic Anthropic-compatible mock service and a clean-environment CLI harness for end-to-end parity checks.

```bash
cd rust
./scripts/run_mock_parity_harness.sh
```

Primary artifacts:

- `rust/crates/mock-anthropic-service/`
- `rust/crates/rusty-claude-cli/tests/mock_parity_harness.rs`
- `rust/scripts/run_mock_parity_harness.sh`
- `rust/scripts/run_mock_parity_diff.py`
- `rust/mock_parity_scenarios.json`

## Verification

```bash
cd rust
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

For task-oriented examples, CLI workflows, permission prompts, sessions, and parity commands, use the root [`USAGE.md`](./USAGE.md).
