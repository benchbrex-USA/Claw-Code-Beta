# Roadmap

Last updated: 2026-04-05

Priority order for the current hardening track:

1. Deterministic MCP timing coverage. Completed in this session. The flaky MCP stdio timing test is back in the active suite in `rust/crates/runtime/src/mcp_stdio.rs`.
2. Full monolith split of `rusty-claude-cli/src/main.rs` and `tools/src/lib.rs`. Completed in this session. Coordination logic now lives in focused modules such as `cli_args.rs`, `auth.rs`, `workspace_status.rs`, `session_reports.rs`, `cli_support.rs`, `catalog.rs`, and `dispatch.rs`.
3. Capability-based isolation. Completed in this session. Bash permission classification now follows real sandbox status, and Linux namespace isolation only activates when the host actually supports it.
4. Proptest expansion. Next. Extend property-based coverage around workspace-boundary normalization, permission classification, and tool input edge cases.
5. Live API test coverage. Next. Add opt-in integration coverage for real Anthropic auth, streaming, and live MCP paths behind explicit environment gates so deterministic CI stays intact.
