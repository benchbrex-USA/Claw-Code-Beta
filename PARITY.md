# Parity

Last updated: 2026-04-05

Permission enforcement parity is complete across built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions. Workspace-boundary parity is narrower: it is fully enforced for local built-in file and notebook paths, but external plugin and MCP implementations still own their own filesystem behavior.

| Surface | Permission enforcement | Workspace boundary behavior | Current status |
| --- | --- | --- | --- |
| Built-in local file and notebook tools | Enforced through `PermissionPolicy` and `PermissionEnforcer` | `read_file`, `write_file`, `edit_file`, `glob_search`, `grep_search`, and `NotebookEdit` all route through workspace-safe helpers | Complete |
| Built-in bash | Enforced through the same policy surface | `workspace-write` depends on actual sandbox capability and command classification; shell path semantics remain heuristic, not path-by-path mediation | Strong, but not identical to file ops |
| `ToolSearch` and other non-mutating built-ins | Enforced through the same policy surface | No local mutation path | Complete |
| Plugin tools | Enforced through the same policy surface using plugin-declared required permissions | No automatic workspace path mediation inside plugin implementations | Permission parity complete, boundary parity partial |
| Runtime/MCP tool definitions | Enforced before runtime dispatch through `GlobalToolRegistry::execute_with_handlers(...)` | Local workspace boundaries are not implied; behavior depends on the wrapper or remote tool implementation | Permission parity complete, boundary parity partial |
| MCP resource wrappers | Enforced as read-only runtime tools | No local write path | Complete for read-only access |

Verification for the current state is green:

- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
