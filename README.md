# Claw Code Beta

Claw Code Beta is the maintained Rust implementation of the `claw` CLI: an agentic coding harness with Anthropic streaming, built-in and plugin tool execution, MCP integration, session persistence, OAuth login, and permission-aware local execution.

The active product lives in [`rust/`](./rust). The root-level [`src/`](./src) and [`tests/`](./tests) trees remain in the repository as compatibility and reference surfaces, but the runtime, CLI, and tool system that ship today are in the Rust workspace.

## Architecture

- `rust/crates/rusty-claude-cli` powers the `claw` binary, REPL, one-shot prompt flow, auth, session management, status reports, and runtime assembly. `main.rs` now delegates coordination to focused modules such as `cli_args.rs`, `auth.rs`, `workspace_status.rs`, `session_reports.rs`, `cli_support.rs`, and `runtime_build.rs`.
- `rust/crates/runtime` owns the conversation runtime, config loading, OAuth storage, session persistence, permission policy, permission enforcement, sandbox resolution, workspace-safe file ops, and MCP stdio/runtime plumbing.
- `rust/crates/tools` owns the built-in tool catalog, dispatch, registry, notebook helpers, and plugin/runtime tool integration. The former `lib.rs` monolith is split across `catalog.rs`, `dispatch.rs`, `registry.rs`, `subagent.rs`, and `utility_ops.rs`.
- `rust/crates/api`, `commands`, `plugins`, `mock-anthropic-service`, `telemetry`, and `compat-harness` provide the Anthropic client, slash-command handling, plugin lifecycle, deterministic mock testing, telemetry types, and upstream manifest extraction.

## Current Guarantees

- Built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions resolve through one permission policy surface.
- `read_file`, `write_file`, `edit_file`, `glob_search`, `grep_search`, and `NotebookEdit` use workspace-bounded helpers for local paths.
- Bash permission classification follows actual sandbox status. When filesystem isolation is active, bash can stay at `workspace-write`; when it is not, the runtime treats the command as `danger-full-access`.
- The MCP stdio timing regression was made deterministic and is back in the active test suite.

## Build And Run

```bash
cd rust
cargo build --workspace
cargo run -p rusty-claude-cli -- --help
```

Authentication:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# optional proxy or local service
export ANTHROPIC_BASE_URL="https://your-proxy.example"

# or use OAuth
cargo run -p rusty-claude-cli -- login
```

## Quick Start

Build the workspace, then run `claw` from the directory you want treated as the active workspace.

```bash
# build from the Rust workspace
cd rust
cargo build --workspace

# run from repo root if you want workspace-write to cover the whole repo
cd ..
./rust/target/debug/claw

# one-shot prompt
./rust/target/debug/claw prompt "summarize this repository"

# bounded write session
./rust/target/debug/claw --permission-mode workspace-write prompt "update README.md"
```

Verification baseline:

```bash
cd rust
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```
