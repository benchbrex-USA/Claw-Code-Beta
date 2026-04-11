//! Permission enforcement layer that gates tool execution based on the
//! active `PermissionPolicy`.

use crate::file_ops::{
    canonical_workspace_root, normalize_path_allow_missing, validate_workspace_boundary,
};
use crate::permissions::{
    PermissionMode, PermissionOutcome, PermissionPolicy, PermissionPromptDecision,
    PermissionPrompter, PermissionRequest,
};
use crate::BashCommandInput;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome")]
pub enum EnforcementResult {
    /// Tool execution is allowed.
    Allowed,
    /// Tool execution requires explicit confirmation before it may proceed.
    ///
    /// This is returned when policy evaluation reaches an approval path but the
    /// current caller is only classifying the request, not running an
    /// interactive prompt. Callers should surface the payload to the approval
    /// UX and fail closed if no such UX is available.
    NeedsConfirmation {
        /// Tool name presented for approval.
        tool: String,
        /// Permission mode active when the check ran.
        active_mode: String,
        /// Minimum permission mode required to proceed without prompting.
        required_mode: String,
        /// Human-readable explanation for the approval requirement.
        reason: String,
    },
    /// Tool execution was denied due to insufficient permissions.
    Denied {
        tool: String,
        active_mode: String,
        required_mode: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionEnforcer {
    policy: PermissionPolicy,
}

#[derive(Debug, Default)]
struct ConfirmationProbe {
    request: Option<PermissionRequest>,
}

impl PermissionPrompter for ConfirmationProbe {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
        self.request = Some(request.clone());
        PermissionPromptDecision::Deny {
            reason: request.reason.clone().unwrap_or_else(|| {
                format!(
                    "tool '{}' requires explicit confirmation while mode is {}",
                    request.tool_name,
                    request.current_mode.as_str()
                )
            }),
        }
    }
}

impl PermissionEnforcer {
    #[must_use]
    pub fn new(policy: PermissionPolicy) -> Self {
        Self { policy }
    }

    /// Check whether a tool can be executed under the current permission policy.
    /// Returns [`EnforcementResult::NeedsConfirmation`] when interactive
    /// approval would be required but no prompt handler is wired.
    #[must_use]
    pub fn check(&self, tool_name: &str, input: &str) -> EnforcementResult {
        let active_mode = self.policy.active_mode();
        let required_mode = self.policy.required_mode_for_invocation(tool_name, input);
        let mut probe = ConfirmationProbe::default();
        let outcome = self.policy.authorize(tool_name, input, Some(&mut probe));

        if let Some(request) = probe.request {
            return EnforcementResult::NeedsConfirmation {
                tool: request.tool_name,
                active_mode: request.current_mode.as_str().to_owned(),
                required_mode: request.required_mode.as_str().to_owned(),
                reason: request.reason.unwrap_or_else(|| {
                    format!(
                        "tool '{tool_name}' requires explicit confirmation while mode is {}",
                        active_mode.as_str()
                    )
                }),
            };
        }

        match outcome {
            PermissionOutcome::Allow => EnforcementResult::Allowed,
            PermissionOutcome::Deny { reason } => EnforcementResult::Denied {
                tool: tool_name.to_owned(),
                active_mode: active_mode.as_str().to_owned(),
                required_mode: required_mode.as_str().to_owned(),
                reason,
            },
        }
    }

    #[must_use]
    pub fn is_allowed(&self, tool_name: &str, input: &str) -> bool {
        matches!(self.check(tool_name, input), EnforcementResult::Allowed)
    }

    #[must_use]
    pub fn active_mode(&self) -> PermissionMode {
        self.policy.active_mode()
    }

    /// Classify a file operation against workspace boundaries.
    #[must_use]
    pub fn check_file_write(&self, path: &Path, workspace_root: &Path) -> EnforcementResult {
        let mode = self.policy.active_mode();

        match mode {
            PermissionMode::ReadOnly => EnforcementResult::Denied {
                tool: "write_file".to_owned(),
                active_mode: mode.as_str().to_owned(),
                required_mode: PermissionMode::WorkspaceWrite.as_str().to_owned(),
                reason: format!("file writes are not allowed in '{}' mode", mode.as_str()),
            },
            PermissionMode::WorkspaceWrite => {
                match boundary_checked_write_path(path, workspace_root) {
                    Ok(()) => EnforcementResult::Allowed,
                    Err(error) => EnforcementResult::Denied {
                        tool: "write_file".to_owned(),
                        active_mode: mode.as_str().to_owned(),
                        required_mode: PermissionMode::DangerFullAccess.as_str().to_owned(),
                        reason: error.to_string(),
                    },
                }
            }
            // Allow and DangerFullAccess permit all writes
            PermissionMode::Allow | PermissionMode::DangerFullAccess => EnforcementResult::Allowed,
            PermissionMode::Prompt => match boundary_checked_write_path(path, workspace_root) {
                Ok(()) => EnforcementResult::NeedsConfirmation {
                    tool: "write_file".to_owned(),
                    active_mode: mode.as_str().to_owned(),
                    required_mode: PermissionMode::WorkspaceWrite.as_str().to_owned(),
                    reason: "file write requires explicit confirmation in prompt mode".to_owned(),
                },
                Err(error) => EnforcementResult::Denied {
                    tool: "write_file".to_owned(),
                    active_mode: mode.as_str().to_owned(),
                    required_mode: PermissionMode::DangerFullAccess.as_str().to_owned(),
                    reason: error.to_string(),
                },
            },
        }
    }

    /// Check if a bash command should be allowed based on current mode.
    #[must_use]
    pub fn check_bash(&self, command: &str) -> EnforcementResult {
        if serde_json::from_str::<BashCommandInput>(command).is_ok() {
            return self.check("bash", command);
        }

        let mode = self.policy.active_mode();

        match mode {
            PermissionMode::ReadOnly => {
                if is_read_only_command(command) {
                    EnforcementResult::Allowed
                } else {
                    EnforcementResult::Denied {
                        tool: "bash".to_owned(),
                        active_mode: mode.as_str().to_owned(),
                        required_mode: PermissionMode::WorkspaceWrite.as_str().to_owned(),
                        reason: format!(
                            "command may modify state; not allowed in '{}' mode",
                            mode.as_str()
                        ),
                    }
                }
            }
            PermissionMode::Prompt => EnforcementResult::NeedsConfirmation {
                tool: "bash".to_owned(),
                active_mode: mode.as_str().to_owned(),
                required_mode: PermissionMode::DangerFullAccess.as_str().to_owned(),
                reason: "bash requires explicit confirmation in prompt mode".to_owned(),
            },
            // WorkspaceWrite, Allow, DangerFullAccess: permit bash
            _ => EnforcementResult::Allowed,
        }
    }
}

fn boundary_checked_write_path(path: &Path, workspace_root: &Path) -> std::io::Result<()> {
    let resolved_path = normalize_path_allow_missing(path)?;
    let canonical_root = canonical_workspace_root(workspace_root);
    validate_workspace_boundary(&resolved_path, &canonical_root)
}

/// Conservative heuristic: is this bash command read-only?
fn is_read_only_command(command: &str) -> bool {
    let first_token = command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("");

    matches!(
        first_token,
        "cat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "wc"
            | "ls"
            | "find"
            | "grep"
            | "rg"
            | "awk"
            | "sed"
            | "echo"
            | "printf"
            | "which"
            | "where"
            | "whoami"
            | "pwd"
            | "env"
            | "printenv"
            | "date"
            | "cal"
            | "df"
            | "du"
            | "free"
            | "uptime"
            | "uname"
            | "file"
            | "stat"
            | "diff"
            | "sort"
            | "uniq"
            | "tr"
            | "cut"
            | "paste"
            | "xargs"
            | "test"
            | "true"
            | "false"
            | "type"
            | "readlink"
            | "realpath"
            | "basename"
            | "dirname"
            | "sha256sum"
            | "md5sum"
            | "b3sum"
            | "xxd"
            | "hexdump"
            | "od"
            | "strings"
            | "tree"
            | "jq"
            | "yq"
            | "python3"
            | "python"
            | "node"
            | "ruby"
            | "cargo"
            | "rustc"
            | "git"
            | "gh"
    ) && !command.contains("-i ")
        && !command.contains("--in-place")
        && !command.contains(" > ")
        && !command.contains(" >> ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn make_enforcer(mode: PermissionMode) -> PermissionEnforcer {
        let policy = PermissionPolicy::new(mode);
        PermissionEnforcer::new(policy)
    }

    fn temp_path(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("permission-enforcer-{nanos}-{counter}-{label}"))
    }

    fn create_workspace(label: &str) -> PathBuf {
        let workspace = temp_path(label);
        fs::create_dir_all(&workspace).expect("workspace should exist");
        workspace
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn allow_mode_permits_everything() {
        let enforcer = make_enforcer(PermissionMode::Allow);
        assert!(enforcer.is_allowed("bash", ""));
        assert!(enforcer.is_allowed("write_file", ""));
        assert!(enforcer.is_allowed("edit_file", ""));
        assert_eq!(
            enforcer.check_file_write(Path::new("/outside/path"), Path::new("/workspace")),
            EnforcementResult::Allowed
        );
        assert_eq!(enforcer.check_bash("rm -rf /"), EnforcementResult::Allowed);
    }

    #[test]
    fn read_only_denies_writes() {
        let policy = PermissionPolicy::new(PermissionMode::ReadOnly)
            .with_tool_requirement("read_file", PermissionMode::ReadOnly)
            .with_tool_requirement("grep_search", PermissionMode::ReadOnly)
            .with_tool_requirement("write_file", PermissionMode::WorkspaceWrite);

        let enforcer = PermissionEnforcer::new(policy);
        assert!(enforcer.is_allowed("read_file", ""));
        assert!(enforcer.is_allowed("grep_search", ""));

        // write_file requires WorkspaceWrite but we're in ReadOnly
        let result = enforcer.check("write_file", "");
        assert!(matches!(result, EnforcementResult::Denied { .. }));

        let result =
            enforcer.check_file_write(Path::new("/workspace/file.rs"), Path::new("/workspace"));
        assert!(matches!(result, EnforcementResult::Denied { .. }));
    }

    #[test]
    fn read_only_allows_read_commands() {
        let enforcer = make_enforcer(PermissionMode::ReadOnly);
        assert_eq!(
            enforcer.check_bash("cat src/main.rs"),
            EnforcementResult::Allowed
        );
        assert_eq!(
            enforcer.check_bash("grep -r 'pattern' ."),
            EnforcementResult::Allowed
        );
        assert_eq!(enforcer.check_bash("ls -la"), EnforcementResult::Allowed);
    }

    #[test]
    fn read_only_denies_write_commands() {
        let enforcer = make_enforcer(PermissionMode::ReadOnly);
        let result = enforcer.check_bash("rm file.txt");
        assert!(matches!(result, EnforcementResult::Denied { .. }));
    }

    #[test]
    fn workspace_write_allows_within_workspace() {
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("allows-within-workspace");
        let result = enforcer.check_file_write(&workspace.join("src/main.rs"), &workspace);
        assert_eq!(result, EnforcementResult::Allowed);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn workspace_write_denies_outside_workspace() {
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("denies-outside-workspace");
        let result = enforcer.check_file_write(&temp_path("outside.txt"), &workspace);
        assert!(matches!(result, EnforcementResult::Denied { .. }));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn prompt_mode_denies_without_prompter() {
        let enforcer = make_enforcer(PermissionMode::Prompt);
        let result = enforcer.check_bash("echo test");
        assert!(matches!(
            result,
            EnforcementResult::NeedsConfirmation { .. }
        ));

        let workspace = create_workspace("prompt-mode");
        let result = enforcer.check_file_write(&workspace.join("file.rs"), &workspace);
        assert!(matches!(
            result,
            EnforcementResult::NeedsConfirmation { .. }
        ));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn prompt_mode_denies_outside_workspace_without_confirmation() {
        let enforcer = make_enforcer(PermissionMode::Prompt);
        let workspace = create_workspace("prompt-outside-workspace");

        let result = enforcer.check_file_write(&temp_path("outside.txt"), &workspace);

        assert!(matches!(result, EnforcementResult::Denied { .. }));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn workspace_write_denies_parent_traversal_outside_workspace() {
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("parent-traversal");
        let result = enforcer.check_file_write(&workspace.join("../escape.txt"), &workspace);
        assert!(matches!(result, EnforcementResult::Denied { .. }));
        let _ = fs::remove_dir_all(workspace);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_write_denies_symlink_escape() {
        use std::os::unix::fs::symlink;

        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("symlink-escape-workspace");
        let outside = create_workspace("symlink-escape-outside");
        let link = workspace.join("escape-link");
        symlink(&outside, &link).expect("symlink should be created");

        let result = enforcer.check_file_write(&link.join("nested.txt"), &workspace);
        assert!(matches!(result, EnforcementResult::Denied { .. }));

        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn read_only_command_heuristic() {
        assert!(is_read_only_command("cat file.txt"));
        assert!(is_read_only_command("grep pattern file"));
        assert!(is_read_only_command("git log --oneline"));
        assert!(!is_read_only_command("rm file.txt"));
        assert!(!is_read_only_command("echo test > file.txt"));
        assert!(!is_read_only_command("sed -i 's/a/b/' file"));
    }

    #[test]
    fn active_mode_returns_policy_mode() {
        // given
        let modes = [
            PermissionMode::ReadOnly,
            PermissionMode::WorkspaceWrite,
            PermissionMode::DangerFullAccess,
            PermissionMode::Prompt,
            PermissionMode::Allow,
        ];

        // when
        let active_modes: Vec<_> = modes
            .into_iter()
            .map(|mode| make_enforcer(mode).active_mode())
            .collect();

        // then
        assert_eq!(active_modes, modes);
    }

    #[test]
    fn danger_full_access_permits_file_writes_and_bash() {
        // given
        let enforcer = make_enforcer(PermissionMode::DangerFullAccess);

        // when
        let workspace = create_workspace("danger-full-access");
        let file_result =
            enforcer.check_file_write(&temp_path("outside-workspace-file.txt"), &workspace);
        let bash_result = enforcer.check_bash("rm -rf /tmp/scratch");

        // then
        assert_eq!(file_result, EnforcementResult::Allowed);
        assert_eq!(bash_result, EnforcementResult::Allowed);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn check_denied_payload_contains_tool_and_modes() {
        // given
        let policy = PermissionPolicy::new(PermissionMode::ReadOnly)
            .with_tool_requirement("write_file", PermissionMode::WorkspaceWrite);
        let enforcer = PermissionEnforcer::new(policy);

        // when
        let result = enforcer.check("write_file", "{}");

        // then
        match result {
            EnforcementResult::Denied {
                tool,
                active_mode,
                required_mode,
                reason,
            } => {
                assert_eq!(tool, "write_file");
                assert_eq!(active_mode, "read-only");
                assert_eq!(required_mode, "workspace-write");
                assert!(reason.contains("requires workspace-write permission"));
            }
            other => {
                panic!("expected denied result, got {other:?}")
            }
        }
    }

    #[test]
    fn workspace_write_relative_path_resolved() {
        // given
        let _guard = env_lock();
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("relative-path");
        let original_dir = std::env::current_dir().expect("cwd should be readable");
        std::env::set_current_dir(&workspace).expect("workspace should be enterable");

        // when
        let result = enforcer.check_file_write(Path::new("src/main.rs"), &workspace);

        // then
        assert_eq!(result, EnforcementResult::Allowed);
        std::env::set_current_dir(&original_dir).expect("cwd should restore");
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn workspace_root_with_trailing_slash() {
        // given
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("workspace-root-trailing-slash");
        let workspace_root = PathBuf::from(format!("{}/", workspace.display()));

        // when
        let result = enforcer.check_file_write(&workspace.join("src/main.rs"), &workspace_root);

        // then
        assert_eq!(result, EnforcementResult::Allowed);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn workspace_write_allows_root_path() {
        let enforcer = make_enforcer(PermissionMode::WorkspaceWrite);
        let workspace = create_workspace("workspace-root");
        let result = enforcer.check_file_write(&workspace, &workspace);
        assert_eq!(result, EnforcementResult::Allowed);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn bash_heuristic_full_path_prefix() {
        // given
        let full_path_command = "/usr/bin/cat Cargo.toml";
        let git_path_command = "/usr/local/bin/git status";

        // when
        let cat_result = is_read_only_command(full_path_command);
        let git_result = is_read_only_command(git_path_command);

        // then
        assert!(cat_result);
        assert!(git_result);
    }

    #[test]
    fn bash_heuristic_redirects_block_read_only_commands() {
        // given
        let overwrite = "cat Cargo.toml > out.txt";
        let append = "echo test >> out.txt";

        // when
        let overwrite_result = is_read_only_command(overwrite);
        let append_result = is_read_only_command(append);

        // then
        assert!(!overwrite_result);
        assert!(!append_result);
    }

    #[test]
    fn bash_heuristic_in_place_flag_blocks() {
        // given
        let interactive_python = "python -i script.py";
        let in_place_sed = "sed --in-place 's/a/b/' file.txt";

        // when
        let interactive_result = is_read_only_command(interactive_python);
        let in_place_result = is_read_only_command(in_place_sed);

        // then
        assert!(!interactive_result);
        assert!(!in_place_result);
    }

    #[test]
    fn bash_heuristic_empty_command() {
        // given
        let empty = "";
        let whitespace = "   ";

        // when
        let empty_result = is_read_only_command(empty);
        let whitespace_result = is_read_only_command(whitespace);

        // then
        assert!(!empty_result);
        assert!(!whitespace_result);
    }

    #[test]
    fn prompt_mode_check_bash_denied_payload_fields() {
        // given
        let enforcer = make_enforcer(PermissionMode::Prompt);

        // when
        let result = enforcer.check_bash("git status");

        // then
        match result {
            EnforcementResult::NeedsConfirmation {
                tool,
                active_mode,
                required_mode,
                reason,
            } => {
                assert_eq!(tool, "bash");
                assert_eq!(active_mode, "prompt");
                assert_eq!(required_mode, "danger-full-access");
                assert_eq!(reason, "bash requires explicit confirmation in prompt mode");
            }
            other => {
                panic!("expected confirmation-needed result, got {other:?}")
            }
        }
    }

    #[test]
    fn prompt_mode_check_requires_confirmation_payload_fields() {
        let policy = PermissionPolicy::new(PermissionMode::Prompt)
            .with_tool_requirement("read_file", PermissionMode::ReadOnly);
        let enforcer = PermissionEnforcer::new(policy);

        let result = enforcer.check("read_file", "{}");

        match result {
            EnforcementResult::NeedsConfirmation {
                tool,
                active_mode,
                required_mode,
                reason,
            } => {
                assert_eq!(tool, "read_file");
                assert_eq!(active_mode, "prompt");
                assert_eq!(required_mode, "read-only");
                assert!(reason.contains("requires explicit confirmation while mode is prompt"));
            }
            other => {
                panic!("expected confirmation-needed result, got {other:?}")
            }
        }
    }

    #[test]
    fn read_only_check_file_write_denied_payload() {
        // given
        let enforcer = make_enforcer(PermissionMode::ReadOnly);

        // when
        let workspace = create_workspace("read-only-write-denied");
        let result = enforcer.check_file_write(&workspace.join("file.txt"), &workspace);

        // then
        match result {
            EnforcementResult::Denied {
                tool,
                active_mode,
                required_mode,
                reason,
            } => {
                assert_eq!(tool, "write_file");
                assert_eq!(active_mode, "read-only");
                assert_eq!(required_mode, "workspace-write");
                assert!(reason.contains("file writes are not allowed"));
            }
            other => {
                panic!("expected denied result, got {other:?}")
            }
        }
        let _ = fs::remove_dir_all(workspace);
    }
}
