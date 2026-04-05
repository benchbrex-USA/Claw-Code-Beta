# Philosophy

Claw Code Beta is built around a small set of constraints.

## One Enforcement Surface

Tool permissions are declared once and applied everywhere. Built-ins, plugin tools, `ToolSearch`, and runtime/MCP tool definitions should all flow through the same `PermissionPolicy` and `PermissionEnforcer` path instead of accumulating bespoke checks.

## Workspace-Write Means Inside The Workspace

`workspace-write` is not "mostly safe write access". Local path operations are normalized against the active workspace root and denied when they escape through absolute paths, missing targets, `..` traversal, or symlinks. Local mutation helpers must preserve that contract.

## Fail Closed

Unknown tools, unsupported permission labels, missing handlers, unavailable approvals, and unsafe path resolution should deny or degrade safely. The default answer to ambiguous authority is no.

## Capability Over Assumption

Isolation decisions should follow the machine's real capabilities. Bash permission classification uses actual sandbox status. Linux namespace isolation is only activated when `unshare` works on the current host; binary presence alone is not treated as proof.

## Smaller Coordination Surfaces

Large coordination files hide policy and dispatch mistakes. Splitting `rusty-claude-cli/src/main.rs` and `tools/src/lib.rs` into focused modules is part of the design, not cleanup for its own sake.
