# CLAUDE.md

Guidance for AI agents working in this repository.

## Scope

- The maintained implementation is the Rust workspace in `rust/`.
- Treat the directory where `claw` is launched as the active workspace root. In `workspace-write`, local path mutations must stay inside that root.

## Dispatch And Enforcement

- Build tool policy from the registry, not by hand. The canonical path is `cli_support::permission_policy(...)`.
- Register built-ins, plugin tools, and runtime tools through `GlobalToolRegistry`. Use `with_plugin_tools(...)`, `with_runtime_tools(...)`, and `with_enforcer(...)`.
- `GlobalToolRegistry::execute(...)` is for built-ins, plugin tools, and `ToolSearch`.
- `GlobalToolRegistry::execute_with_handlers(...)` is required for runtime/MCP tool definitions. Do not add ad hoc runtime dispatch branches that bypass the registry.
- Keep `PermissionPolicy` and `PermissionEnforcer` as the single permission surface for built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions.

## Workspace Boundaries

- For local file access, use the workspace-safe runtime APIs: `read_file_in_workspace`, `write_file_in_workspace`, `edit_file_in_workspace`, `glob_search_in_workspace`, and `grep_search_in_workspace`.
- `NotebookEdit` is expected to stay on the same boundary-safe path through `tools::utility_ops`.
- `workspace-write` is strict. Absolute paths, `..` traversal, missing-target escapes, and symlink escapes must be denied when they resolve outside the active workspace.

## Sandbox And Permissions

- The user-facing CLI exposes `read-only`, `workspace-write`, and `danger-full-access`.
- Internal permission states `Prompt` and `Allow` exist in the runtime. Approval UX is implemented by `CliPermissionPrompter`.
- Bash permission requirements are derived from actual sandbox status, not from the presence of a sandbox binary alone.
- Plugin tools share the central permission gate, but they do not get automatic workspace path mediation. If a plugin writes local files, its implementation must enforce boundaries explicitly or require `danger-full-access`.

## APIs To Prefer

- Runtime assembly: `runtime_build::build_runtime_plugin_state(...)` and `build_runtime_with_plugin_state(...)`
- Permission classification: `runtime::PermissionPolicy`, `runtime::PermissionEnforcer`
- Runtime MCP definitions: `runtime_build::mcp_runtime_tool_definition(...)` and `mcp_wrapper_tool_definitions()`
- Local file mutation: the `*_in_workspace` runtime helpers, not raw `std::fs` calls in tool paths

## Working Rule

- If you change tool dispatch, permission semantics, workspace boundaries, or CLI behavior, update the root docs and the `rust/` docs in the same patch.
