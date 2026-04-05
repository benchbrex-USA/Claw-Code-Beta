# Usage

This guide covers the current Rust workspace and the `claw` CLI binary.

## Build

```bash
cd rust
cargo build --workspace
```

The debug binary is `rust/target/debug/claw`.

## Authentication

Use either an API key or OAuth:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_BASE_URL="https://your-proxy.example" # optional

cd rust
cargo run -p rusty-claude-cli -- login
```

OAuth credentials are stored in `$CLAW_CONFIG_HOME/credentials.json` or `~/.claw/credentials.json`.

## Run The CLI

Build from `rust/`, then run `claw` from the directory you want treated as the active workspace.

```bash
# whole-repo workspace
cd "/Volumes/Project/Claw Code beta"
./rust/target/debug/claw

# Rust-only workspace
cd "/Volumes/Project/Claw Code beta/rust"
./target/debug/claw
```

Common commands:

```bash
./rust/target/debug/claw prompt "summarize this repository"
./rust/target/debug/claw --output-format json prompt "status"
./rust/target/debug/claw status
./rust/target/debug/claw sandbox
./rust/target/debug/claw mcp
./rust/target/debug/claw skills
./rust/target/debug/claw --resume latest
```

## Permission Modes

- `read-only`: inspection only. Mutating tool calls are denied.
- `workspace-write`: local reads and writes are allowed only inside the active workspace root.
- `danger-full-access`: unrestricted local mutation.

Examples:

```bash
./rust/target/debug/claw --permission-mode read-only prompt "find TODOs in rust/crates/runtime"
./rust/target/debug/claw --permission-mode workspace-write prompt "update README.md"
./rust/target/debug/claw --permission-mode danger-full-access prompt "run the full release script"
./rust/target/debug/claw --allowedTools read,glob,grep prompt "inspect the runtime crate"
```

## How Workspace-Write Works

- The active workspace is the directory where you launch `claw`.
- Local file and notebook tools normalize paths against that root.
- Absolute paths, `..` traversal, missing-target escapes, and symlink escapes are denied when they resolve outside the workspace.
- Bash can stay at `workspace-write` only when the active sandbox configuration is actually enforcing filesystem isolation. Otherwise the runtime treats the command as `danger-full-access`.

## Approval Prompts

The stock CLI exposes three modes, but the runtime also has an internal approval path. The same prompt is used when a `workspace-write` session needs to escalate a tool call to `danger-full-access`.

When approval is required, the CLI prints:

```text
Permission approval required
  Tool             ...
  Current mode     ...
  Required mode    ...
  Reason           ...
  Input            ...
Approve this tool call? [y/N]:
```

Type `y` or `yes` to approve. Any other response denies the tool call.

## Verification

```bash
cd rust
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```
