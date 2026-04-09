<p align="center">
  <h1 align="center">Claw Code</h1>
  <p align="center">
    The enterprise-grade agentic coding platform built in Rust.
    <br />
    Streaming AI assistance, permission-aware execution, plugin extensibility, and secure-by-default operation.
  </p>
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> &bull;
  <a href="#architecture">Architecture</a> &bull;
  <a href="#security-model">Security Model</a> &bull;
  <a href="#multi-provider-support">Providers</a> &bull;
  <a href="#plugin-system">Plugins</a> &bull;
  <a href="#observability">Observability</a> &bull;
  <a href="#contributing">Contributing</a>
</p>

---

## Overview

Claw Code is a production-grade CLI harness that brings agentic AI assistance directly into your development workflow. It connects to leading AI providers, executes tools on your behalf with fine-grained permission controls, and persists session state across interactions — all within a secure, auditable runtime.

**Key capabilities:**

- **Multi-provider AI integration** — Anthropic Claude, OpenAI GPT-4o/o1/o3, and xAI Grok, with automatic model routing and alias resolution
- **Permission-aware execution** — Three-tier permission model (`read-only`, `workspace-write`, `danger-full-access`) with unified enforcement across all tool types
- **Workspace boundary enforcement** — Strict path validation prevents file access outside the active workspace via traversal, symlinks, or absolute paths
- **Capability-based sandboxing** — Bash isolation follows actual host capabilities, not assumptions; Linux namespace restrictions activate only when the host supports them
- **Plugin and MCP ecosystem** — Extensible tool catalog with first-class Model Context Protocol support, plugin lifecycle management, and hook-driven automation
- **Session persistence and resumption** — Full conversation state is persisted to disk with rotation, compaction, and fork support
- **Prompt caching and cost tracking** — Intelligent cache fingerprinting with unexpected-break detection, per-session cost estimation, and usage analytics
- **Structured observability** — `tracing`-based instrumentation with environment-driven log levels and JSON output for production log aggregation

## Quick Start

### Build from source

```bash
cd rust
cargo build --workspace
```

### Run

```bash
# Interactive REPL
./rust/target/debug/claw

# One-shot prompt
./rust/target/debug/claw prompt "summarize this repository"

# With a specific model
./rust/target/debug/claw --model sonnet prompt "explain the permission model"
```

### Authentication

```bash
# API key (Anthropic)
export ANTHROPIC_API_KEY="sk-ant-..."

# OAuth login
./rust/target/debug/claw login

# OpenAI
export OPENAI_API_KEY="sk-..."

# xAI Grok
export XAI_API_KEY="xai-..."
```

### Configuration

Claw Code loads configuration from a three-tier precedence chain:

1. **User** — `~/.claw/settings.json`
2. **Project** — `.claw/settings.json` in the repository root
3. **Local** — `.claw/settings.local.json` (gitignored)

Set the permission mode, model, MCP servers, hooks, plugins, and sandbox behavior through configuration or CLI flags.

## Architecture

The system is organized as a Rust workspace with focused, single-responsibility crates:

| Crate | Responsibility |
|-------|---------------|
| `rusty-claude-cli` | Binary entry point, REPL, auth flows, session management, terminal rendering |
| `runtime` | Conversation loop, permission evaluation, workspace-safe file operations, MCP plumbing, session persistence, sandbox detection |
| `tools` | Built-in tool catalog, dispatch, subagent orchestration, plugin/runtime tool integration |
| `api` | Multi-provider API client (Anthropic, OpenAI, xAI), SSE streaming, retry with exponential backoff, prompt caching |
| `commands` | Slash-command registry and handlers |
| `plugins` | Plugin manifest validation, lifecycle management, hook execution |
| `telemetry` | Client identity, request profiling, session tracing, analytics event types |
| `mock-anthropic-service` | Deterministic Anthropic-compatible mock for end-to-end parity testing |
| `compat-harness` | Upstream manifest extraction for compatibility validation |

### Data Flow

```
User Input → REPL/CLI → ConversationRuntime → API Client (streaming)
                              ↓                        ↓
                        PermissionPolicy          SSE Parser
                              ↓                        ↓
                        ToolExecutor ← Assistant Message (tool_use)
                              ↓
                  GlobalToolRegistry → Built-in / Plugin / MCP / Runtime tools
                              ↓
                        Hook Runner (PreToolUse → execution → PostToolUse)
                              ↓
                        Session Persistence (JSONL with rotation)
```

## Security Model

Claw Code follows a **fail-closed** security posture. Unknown tools, unsupported permission labels, missing handlers, and unsafe path resolution default to denial.

### Permission Tiers

| Mode | Scope |
|------|-------|
| `read-only` | Inspection only; all mutations denied |
| `workspace-write` | Reads and writes confined to the active workspace root |
| `danger-full-access` | Unrestricted local mutation (explicit opt-in required) |

### Workspace Boundary Enforcement

All file operations pass through workspace-safe runtime APIs (`read_file_in_workspace`, `write_file_in_workspace`, `edit_file_in_workspace`). Path resolution normalizes against the workspace root and rejects:

- Absolute paths resolving outside the workspace
- `../` traversal escaping the boundary
- Symlink targets outside the workspace
- Missing intermediate directories used as escape vectors

### Sandbox Isolation

Bash command execution classifies permission requirements based on **actual sandbox status**, not binary presence. If filesystem isolation is inactive, the runtime automatically upgrades bash to `danger-full-access` to prevent silent privilege escalation.

## Multi-Provider Support

| Provider | Models | Auth |
|----------|--------|------|
| **Anthropic** | Claude Opus 4.6, Sonnet 4.6, Haiku 4.5 | `ANTHROPIC_API_KEY` or OAuth |
| **OpenAI** | GPT-4o, GPT-4o-mini, GPT-4-turbo, o1, o1-mini, o3, o3-mini | `OPENAI_API_KEY` |
| **xAI** | Grok-3, Grok-3-mini, Grok-2 | `XAI_API_KEY` |

Model aliases are supported for convenience: `opus`, `sonnet`, `haiku`, `gpt4`, `grok`, `grok-mini`.

All providers share production-grade HTTP configuration:

- Connection pooling with 90-second idle timeout
- 10-second connect timeout
- Automatic retry with exponential backoff (200ms base, 2s max, 2 retries)
- Retryable status detection (429, 500, 502, 503, 529)

## Plugin System

Plugins extend Claw Code with custom tools, hooks, and lifecycle behaviors. The plugin system supports three tiers:

- **Built-in** — Shipped with the binary
- **Bundled** — Distributed alongside the installation
- **External** — User-installed from the filesystem or registry

Plugins are validated at load time against a strict manifest schema. Tool names are checked for conflicts with built-ins and other plugins. Hook commands execute in the plugin's root directory with access to tool invocation context.

### MCP Integration

First-class support for the Model Context Protocol across multiple transports:

- **Stdio** — Local process communication
- **SSE** — Server-Sent Events over HTTP
- **HTTP** — Standard HTTP endpoints
- **WebSocket** — Persistent bidirectional connections
- **SDK** — Managed proxy connections with OAuth

MCP tools and resources are registered through the unified `GlobalToolRegistry` and share the central permission enforcement surface.

## Observability

Claw Code ships with structured `tracing` instrumentation across the runtime, tool dispatch, and file operation paths.

```bash
# Enable debug logging
CLAW_LOG=debug ./rust/target/debug/claw

# JSON output for log aggregation
CLAW_LOG=info CLAW_LOG_FORMAT=json ./rust/target/debug/claw
```

Key instrumented paths:

- Tool dispatch decisions (tool name, permission checks)
- Bash command execution (command length, background mode)
- Workspace file operations (path, operation type)
- Session persistence events

### Cost Tracking

Per-model token pricing with real-time cost estimation. The prompt cache subsystem tracks cache hits, misses, unexpected breaks, and creation/read token volumes across sessions.

## Verification

```bash
cd rust

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Test (704 tests across all crates)
cargo test --workspace

# Mock parity harness (deterministic end-to-end scenarios)
./scripts/run_mock_parity_harness.sh
```

## CI/CD

Automated pipelines run on every push and pull request:

- **Check** — Workspace compilation verification
- **Clippy** — Zero-warning lint enforcement
- **Test** — Full test suite on Ubuntu and macOS
- **Format** — `rustfmt` conformance check
- **Security Audit** — Dependency vulnerability scanning via `cargo-audit`
- **Release Build** — Cross-platform binary compilation (Linux x86_64, macOS ARM64)

Tagged releases automatically build and publish platform-specific binaries to GitHub Releases.

## Contributing

1. Fork the repository
2. Create a feature branch from `main`
3. Ensure `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` pass
4. Submit a pull request

For task-oriented examples, CLI workflows, permission prompts, sessions, and parity commands, see [`USAGE.md`](./USAGE.md).

## License

MIT
