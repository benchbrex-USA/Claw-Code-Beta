# Rust usage guide

The canonical task-oriented usage guide lives at [`../USAGE.md`](../USAGE.md).

Use that guide for:

- workspace build and test commands
- authentication setup
- interactive and one-shot `claw` examples
- session resume workflows
- mock parity harness commands

Important current-runtime guarantees:

- built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions all run under the same permission policy surface
- `write_file`, `edit_file`, and notebook mutations are bounded to the active workspace in `workspace-write` mode
- the verification baseline is `cargo clippy --all-targets --all-features -- -D warnings` plus `cargo test --workspace`
- temp-dir helpers used by config and tool tests are hardened for clean full-workspace runs
