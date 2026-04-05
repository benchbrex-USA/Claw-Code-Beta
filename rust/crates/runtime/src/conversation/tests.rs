use super::{
    build_assistant_message, parse_auto_compaction_threshold, ApiClient, ApiRequest,
    AssistantEvent, AutoCompactionEvent, ConversationRuntime, PromptCacheEvent, RuntimeError,
    StaticToolExecutor, ToolExecutor, DEFAULT_AUTO_COMPACTION_INPUT_TOKENS_THRESHOLD,
};
use crate::compact::CompactionConfig;
use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
use crate::permissions::{
    PermissionMode, PermissionPolicy, PermissionPromptDecision, PermissionPrompter,
    PermissionRequest,
};
use crate::prompt::{ProjectContext, SystemPromptBuilder};
use crate::session::{ContentBlock, MessageRole, Session};
use crate::usage::TokenUsage;
use crate::ToolError;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use telemetry::{MemoryTelemetrySink, SessionTracer, TelemetryEvent};

struct ScriptedApiClient {
    call_count: usize,
}

struct PromptedWriteApiClient {
    call_count: usize,
}

impl ApiClient for PromptedWriteApiClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        self.call_count += 1;
        match self.call_count {
            1 => {
                assert!(request
                    .messages
                    .iter()
                    .any(|message| message.role == MessageRole::User));
                Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-write".to_string(),
                        name: "write_file".to_string(),
                        input: r#"{"path":"approved.txt","content":"approved"}"#.to_string(),
                    },
                    AssistantEvent::MessageStop,
                ])
            }
            2 => Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::MessageStop,
            ]),
            _ => unreachable!("extra API call"),
        }
    }
}

struct PromptModeApprover {
    approved: Arc<AtomicBool>,
}

impl PermissionPrompter for PromptModeApprover {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
        assert_eq!(request.tool_name, "write_file");
        assert_eq!(request.current_mode, PermissionMode::Prompt);
        assert_eq!(request.required_mode, PermissionMode::WorkspaceWrite);
        assert!(request
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("requires explicit confirmation")));
        self.approved.store(true, Ordering::SeqCst);
        PermissionPromptDecision::Allow
    }
}

fn build_prompt_mode_write_runtime() -> (
    ConversationRuntime<PromptedWriteApiClient, StaticToolExecutor>,
    PromptModeApprover,
    Arc<AtomicBool>,
    PathBuf,
) {
    let workspace = std::env::temp_dir().join(format!(
        "conversation-prompt-approval-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos()
    ));
    fs::create_dir_all(&workspace).expect("workspace should exist");
    let approved = Arc::new(AtomicBool::new(false));
    let approved_for_tool = approved.clone();
    let write_target = workspace.join("approved.txt");
    let tool_executor = StaticToolExecutor::new().register("write_file", move |input: &str| {
        assert!(
            approved_for_tool.load(Ordering::SeqCst),
            "tool should execute only after approval"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(input).expect("tool input should be valid JSON");
        let content = parsed["content"]
            .as_str()
            .expect("content should be present");
        fs::write(&write_target, content).expect("approved write should succeed");
        Ok("approved".to_string())
    });
    let permission_policy = PermissionPolicy::new(PermissionMode::Prompt)
        .with_tool_requirement("write_file", PermissionMode::WorkspaceWrite);
    let system_prompt = SystemPromptBuilder::new()
        .with_project_context(ProjectContext {
            cwd: workspace.clone(),
            current_date: "2026-03-31".to_string(),
            git_status: None,
            git_diff: None,
            instruction_files: Vec::new(),
        })
        .with_os("linux", "6.8")
        .build();
    let runtime = ConversationRuntime::new(
        Session::new(),
        PromptedWriteApiClient { call_count: 0 },
        tool_executor,
        permission_policy,
        system_prompt,
    );
    let prompter = PromptModeApprover {
        approved: approved.clone(),
    };

    (runtime, prompter, approved, workspace)
}

impl ApiClient for ScriptedApiClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        self.call_count += 1;
        match self.call_count {
            1 => {
                assert!(request
                    .messages
                    .iter()
                    .any(|message| message.role == MessageRole::User));
                Ok(vec![
                    AssistantEvent::TextDelta("Let me calculate that.".to_string()),
                    AssistantEvent::ToolUse {
                        id: "tool-1".to_string(),
                        name: "add".to_string(),
                        input: "2,2".to_string(),
                    },
                    AssistantEvent::Usage(TokenUsage {
                        input_tokens: 20,
                        output_tokens: 6,
                        cache_creation_input_tokens: 1,
                        cache_read_input_tokens: 2,
                    }),
                    AssistantEvent::MessageStop,
                ])
            }
            2 => {
                let last_message = request
                    .messages
                    .last()
                    .expect("tool result should be present");
                assert_eq!(last_message.role, MessageRole::Tool);
                Ok(vec![
                    AssistantEvent::TextDelta("The answer is 4.".to_string()),
                    AssistantEvent::Usage(TokenUsage {
                        input_tokens: 24,
                        output_tokens: 4,
                        cache_creation_input_tokens: 1,
                        cache_read_input_tokens: 3,
                    }),
                    AssistantEvent::PromptCache(PromptCacheEvent {
                        unexpected: true,
                        reason:
                            "cache read tokens dropped while prompt fingerprint remained stable"
                                .to_string(),
                        previous_cache_read_input_tokens: 6_000,
                        current_cache_read_input_tokens: 1_000,
                        token_drop: 5_000,
                    }),
                    AssistantEvent::MessageStop,
                ])
            }
            _ => unreachable!("extra API call"),
        }
    }
}

struct PromptAllowOnce;

impl PermissionPrompter for PromptAllowOnce {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
        assert_eq!(request.tool_name, "add");
        PermissionPromptDecision::Allow
    }
}

#[test]
fn runs_user_to_tool_to_result_loop_end_to_end_and_tracks_usage() {
    let api_client = ScriptedApiClient { call_count: 0 };
    let tool_executor = StaticToolExecutor::new().register("add", |input: &str| {
        let total = input
            .split(',')
            .map(|part| part.parse::<i32>().expect("input must be valid integer"))
            .sum::<i32>();
        Ok(total.to_string())
    });
    let permission_policy = PermissionPolicy::new(PermissionMode::WorkspaceWrite);
    let system_prompt = SystemPromptBuilder::new()
        .with_project_context(ProjectContext {
            cwd: PathBuf::from("/tmp/project"),
            current_date: "2026-03-31".to_string(),
            git_status: None,
            git_diff: None,
            instruction_files: Vec::new(),
        })
        .with_os("linux", "6.8")
        .build();
    let mut runtime = ConversationRuntime::new(
        Session::new(),
        api_client,
        tool_executor,
        permission_policy,
        system_prompt,
    );

    let summary = runtime
        .run_turn("what is 2 + 2?", Some(&mut PromptAllowOnce))
        .expect("conversation loop should succeed");

    assert_eq!(summary.iterations, 2);
    assert_eq!(summary.assistant_messages.len(), 2);
    assert_eq!(summary.tool_results.len(), 1);
    assert_eq!(summary.prompt_cache_events.len(), 1);
    assert_eq!(runtime.session().messages.len(), 4);
    assert_eq!(summary.usage.output_tokens, 10);
    assert_eq!(summary.auto_compaction, None);
    assert!(matches!(
        runtime.session().messages[1].blocks[1],
        ContentBlock::ToolUse { .. }
    ));
    assert!(matches!(
        runtime.session().messages[2].blocks[0],
        ContentBlock::ToolResult {
            is_error: false,
            ..
        }
    ));
}

#[test]
fn prompt_mode_confirmation_executes_write_tool_end_to_end() {
    let (mut runtime, mut prompter, approved, workspace) = build_prompt_mode_write_runtime();

    let summary = runtime
        .run_turn("write the file", Some(&mut prompter))
        .expect("prompt approval flow should succeed");

    assert!(approved.load(Ordering::SeqCst));
    assert_eq!(
        fs::read_to_string(workspace.join("approved.txt")).expect("approved file should exist"),
        "approved"
    );
    assert_eq!(summary.iterations, 2);
    assert_eq!(summary.tool_results.len(), 1);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn records_runtime_session_trace_events() {
    let sink = Arc::new(MemoryTelemetrySink::default());
    let tracer = SessionTracer::new("session-runtime", sink.clone());
    let mut runtime = ConversationRuntime::new(
        Session::new(),
        ScriptedApiClient { call_count: 0 },
        StaticToolExecutor::new().register("add", |_input| Ok("4".to_string())),
        PermissionPolicy::new(PermissionMode::WorkspaceWrite),
        vec!["system".to_string()],
    )
    .with_session_tracer(tracer);

    runtime
        .run_turn("what is 2 + 2?", Some(&mut PromptAllowOnce))
        .expect("conversation loop should succeed");

    let events = sink.events();
    let trace_names = events
        .iter()
        .filter_map(|event| match event {
            TelemetryEvent::SessionTrace(trace) => Some(trace.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(trace_names.contains(&"turn_started"));
    assert!(trace_names.contains(&"assistant_iteration_completed"));
    assert!(trace_names.contains(&"tool_execution_started"));
    assert!(trace_names.contains(&"tool_execution_finished"));
    assert!(trace_names.contains(&"turn_completed"));
}

#[test]
fn records_denied_tool_results_when_prompt_rejects() {
    struct RejectPrompter;
    impl PermissionPrompter for RejectPrompter {
        fn decide(&mut self, _request: &PermissionRequest) -> PermissionPromptDecision {
            PermissionPromptDecision::Deny {
                reason: "not now".to_string(),
            }
        }
    }

    struct SingleCallApiClient;
    impl ApiClient for SingleCallApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            if request
                .messages
                .iter()
                .any(|message| message.role == MessageRole::Tool)
            {
                return Ok(vec![
                    AssistantEvent::TextDelta("I could not use the tool.".to_string()),
                    AssistantEvent::MessageStop,
                ]);
            }
            Ok(vec![
                AssistantEvent::ToolUse {
                    id: "tool-1".to_string(),
                    name: "blocked".to_string(),
                    input: "secret".to_string(),
                },
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut runtime = ConversationRuntime::new(
        Session::new(),
        SingleCallApiClient,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::WorkspaceWrite),
        vec!["system".to_string()],
    );

    let summary = runtime
        .run_turn("use the tool", Some(&mut RejectPrompter))
        .expect("conversation should continue after denied tool");

    assert_eq!(summary.tool_results.len(), 1);
    assert!(matches!(
        &summary.tool_results[0].blocks[0],
        ContentBlock::ToolResult { is_error: true, output, .. } if output == "not now"
    ));
}

#[test]
fn denies_tool_use_when_pre_tool_hook_blocks() {
    struct SingleCallApiClient;
    impl ApiClient for SingleCallApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            if request
                .messages
                .iter()
                .any(|message| message.role == MessageRole::Tool)
            {
                return Ok(vec![
                    AssistantEvent::TextDelta("blocked".to_string()),
                    AssistantEvent::MessageStop,
                ]);
            }
            Ok(vec![
                AssistantEvent::ToolUse {
                    id: "tool-1".to_string(),
                    name: "blocked".to_string(),
                    input: r#"{"path":"secret.txt"}"#.to_string(),
                },
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        SingleCallApiClient,
        StaticToolExecutor::new().register("blocked", |_input| {
            panic!("tool should not execute when hook denies")
        }),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
            vec![shell_snippet("printf 'blocked by hook'; exit 2")],
            Vec::new(),
            Vec::new(),
        )),
    );

    let summary = runtime
        .run_turn("use the tool", None)
        .expect("conversation should continue after hook denial");

    assert_eq!(summary.tool_results.len(), 1);
    let ContentBlock::ToolResult {
        is_error, output, ..
    } = &summary.tool_results[0].blocks[0]
    else {
        panic!("expected tool result block");
    };
    assert!(
        is_error,
        "hook denial should produce an error result: {output}"
    );
    assert!(
        output.contains("denied tool") || output.contains("blocked by hook"),
        "unexpected hook denial output: {output:?}"
    );
}

#[test]
fn denies_tool_use_when_pre_tool_hook_fails() {
    struct SingleCallApiClient;
    impl ApiClient for SingleCallApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            if request
                .messages
                .iter()
                .any(|message| message.role == MessageRole::Tool)
            {
                return Ok(vec![
                    AssistantEvent::TextDelta("failed".to_string()),
                    AssistantEvent::MessageStop,
                ]);
            }
            Ok(vec![
                AssistantEvent::ToolUse {
                    id: "tool-1".to_string(),
                    name: "blocked".to_string(),
                    input: r#"{"path":"secret.txt"}"#.to_string(),
                },
                AssistantEvent::MessageStop,
            ])
        }
    }

    // given
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        SingleCallApiClient,
        StaticToolExecutor::new().register("blocked", |_input| {
            panic!("tool should not execute when hook fails")
        }),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
            vec![shell_snippet("printf 'broken hook'; exit 1")],
            Vec::new(),
            Vec::new(),
        )),
    );

    // when
    let summary = runtime
        .run_turn("use the tool", None)
        .expect("conversation should continue after hook failure");

    // then
    assert_eq!(summary.tool_results.len(), 1);
    let ContentBlock::ToolResult {
        is_error, output, ..
    } = &summary.tool_results[0].blocks[0]
    else {
        panic!("expected tool result block");
    };
    assert!(
        is_error,
        "hook failure should produce an error result: {output}"
    );
    assert!(
        output.contains("exited with status 1") || output.contains("broken hook"),
        "unexpected hook failure output: {output:?}"
    );
}

#[test]
fn appends_post_tool_hook_feedback_to_tool_result() {
    struct TwoCallApiClient {
        calls: usize,
    }

    impl ApiClient for TwoCallApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.calls += 1;
            match self.calls {
                1 => Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-1".to_string(),
                        name: "add".to_string(),
                        input: r#"{"lhs":2,"rhs":2}"#.to_string(),
                    },
                    AssistantEvent::MessageStop,
                ]),
                2 => {
                    assert!(request
                        .messages
                        .iter()
                        .any(|message| message.role == MessageRole::Tool));
                    Ok(vec![
                        AssistantEvent::TextDelta("done".to_string()),
                        AssistantEvent::MessageStop,
                    ])
                }
                _ => unreachable!("extra API call"),
            }
        }
    }

    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        TwoCallApiClient { calls: 0 },
        StaticToolExecutor::new().register("add", |_input| Ok("4".to_string())),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
            vec![shell_snippet("printf 'pre hook ran'")],
            vec![shell_snippet("printf 'post hook ran'")],
            Vec::new(),
        )),
    );

    let summary = runtime
        .run_turn("use add", None)
        .expect("tool loop succeeds");

    assert_eq!(summary.tool_results.len(), 1);
    let ContentBlock::ToolResult {
        is_error, output, ..
    } = &summary.tool_results[0].blocks[0]
    else {
        panic!("expected tool result block");
    };
    assert!(
        !is_error,
        "post hook should preserve non-error result: {output:?}"
    );
    assert!(
        output.contains('4'),
        "tool output missing value: {output:?}"
    );
    assert!(
        output.contains("pre hook ran"),
        "tool output missing pre hook feedback: {output:?}"
    );
    assert!(
        output.contains("post hook ran"),
        "tool output missing post hook feedback: {output:?}"
    );
}

#[test]
fn appends_post_tool_use_failure_hook_feedback_to_tool_result() {
    struct TwoCallApiClient {
        calls: usize,
    }

    impl ApiClient for TwoCallApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.calls += 1;
            match self.calls {
                1 => Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-1".to_string(),
                        name: "fail".to_string(),
                        input: r#"{"path":"README.md"}"#.to_string(),
                    },
                    AssistantEvent::MessageStop,
                ]),
                2 => {
                    assert!(request
                        .messages
                        .iter()
                        .any(|message| message.role == MessageRole::Tool));
                    Ok(vec![
                        AssistantEvent::TextDelta("done".to_string()),
                        AssistantEvent::MessageStop,
                    ])
                }
                _ => unreachable!("extra API call"),
            }
        }
    }

    // given
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        TwoCallApiClient { calls: 0 },
        StaticToolExecutor::new().register("fail", |_input| Err(ToolError::new("tool exploded"))),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
            Vec::new(),
            vec![shell_snippet("printf 'post hook should not run'")],
            vec![shell_snippet("printf 'failure hook ran'")],
        )),
    );

    // when
    let summary = runtime
        .run_turn("use fail", None)
        .expect("tool loop succeeds");

    // then
    assert_eq!(summary.tool_results.len(), 1);
    let ContentBlock::ToolResult {
        is_error, output, ..
    } = &summary.tool_results[0].blocks[0]
    else {
        panic!("expected tool result block");
    };
    assert!(
        is_error,
        "failure hook path should preserve error result: {output:?}"
    );
    assert!(
        output.contains("tool exploded"),
        "tool output missing failure reason: {output:?}"
    );
    assert!(
        output.contains("failure hook ran"),
        "tool output missing failure hook feedback: {output:?}"
    );
    assert!(
        !output.contains("post hook should not run"),
        "normal post hook should not run on tool failure: {output:?}"
    );
}

#[test]
fn reconstructs_usage_tracker_from_restored_session() {
    struct SimpleApi;
    impl ApiClient for SimpleApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut session = Session::new();
    session
        .messages
        .push(crate::session::ConversationMessage::assistant_with_usage(
            vec![ContentBlock::Text {
                text: "earlier".to_string(),
            }],
            Some(TokenUsage {
                input_tokens: 11,
                output_tokens: 7,
                cache_creation_input_tokens: 2,
                cache_read_input_tokens: 1,
            }),
        ));

    let runtime = ConversationRuntime::new(
        session,
        SimpleApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    );

    assert_eq!(runtime.usage().turns(), 1);
    assert_eq!(runtime.usage().cumulative_usage().total_tokens(), 21);
}

#[test]
fn compacts_session_after_turns() {
    struct SimpleApi;
    impl ApiClient for SimpleApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut runtime = ConversationRuntime::new(
        Session::new(),
        SimpleApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    );
    runtime.run_turn("a", None).expect("turn a");
    runtime.run_turn("b", None).expect("turn b");
    runtime.run_turn("c", None).expect("turn c");

    let result = runtime.compact(CompactionConfig {
        preserve_recent_messages: 2,
        max_estimated_tokens: 1,
    });
    assert!(result.summary.contains("Conversation summary"));
    assert_eq!(
        result.compacted_session.messages[0].role,
        MessageRole::System
    );
    assert_eq!(
        result.compacted_session.session_id,
        runtime.session().session_id
    );
    assert!(result.compacted_session.compaction.is_some());
}

#[test]
fn persists_conversation_turn_messages_to_jsonl_session() {
    struct SimpleApi;
    impl ApiClient for SimpleApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::MessageStop,
            ])
        }
    }

    let path = temp_session_path("persisted-turn");
    let session = Session::new().with_persistence_path(path.clone());
    let mut runtime = ConversationRuntime::new(
        session,
        SimpleApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    );

    runtime
        .run_turn("persist this turn", None)
        .expect("turn should succeed");

    let restored = Session::load_from_path(&path).expect("persisted session should reload");
    fs::remove_file(&path).expect("temp session file should be removable");

    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.messages[0].role, MessageRole::User);
    assert_eq!(restored.messages[1].role, MessageRole::Assistant);
    assert_eq!(restored.session_id, runtime.session().session_id);
}

#[test]
fn forks_runtime_session_without_mutating_original() {
    let mut session = Session::new();
    session
        .push_user_text("branch me")
        .expect("message should append");

    let runtime = ConversationRuntime::new(
        session.clone(),
        ScriptedApiClient { call_count: 0 },
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    );

    let forked = runtime.fork_session(Some("alt-path".to_string()));

    assert_eq!(forked.messages, session.messages);
    assert_ne!(forked.session_id, session.session_id);
    assert_eq!(
        forked
            .fork
            .as_ref()
            .map(|fork| (fork.parent_session_id.as_str(), fork.branch_name.as_deref())),
        Some((session.session_id.as_str(), Some("alt-path")))
    );
    assert!(runtime.session().fork.is_none());
}

fn temp_session_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("runtime-conversation-{label}-{nanos}.json"))
}

#[cfg(windows)]
fn shell_snippet(script: &str) -> String {
    script.replace('\'', "\"")
}

#[cfg(not(windows))]
fn shell_snippet(script: &str) -> String {
    script.to_string()
}

#[test]
fn auto_compacts_when_cumulative_input_threshold_is_crossed() {
    struct SimpleApi;
    impl ApiClient for SimpleApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::Usage(TokenUsage {
                    input_tokens: 120_000,
                    output_tokens: 4,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                }),
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut session = Session::new();
    session.messages = vec![
        crate::session::ConversationMessage::user_text("one"),
        crate::session::ConversationMessage::assistant(vec![ContentBlock::Text {
            text: "two".to_string(),
        }]),
        crate::session::ConversationMessage::user_text("three"),
        crate::session::ConversationMessage::assistant(vec![ContentBlock::Text {
            text: "four".to_string(),
        }]),
    ];

    let mut runtime = ConversationRuntime::new(
        session,
        SimpleApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    )
    .with_auto_compaction_input_tokens_threshold(100_000);

    let summary = runtime
        .run_turn("trigger", None)
        .expect("turn should succeed");

    assert_eq!(
        summary.auto_compaction,
        Some(AutoCompactionEvent {
            removed_message_count: 2,
        })
    );
    assert_eq!(runtime.session().messages[0].role, MessageRole::System);
}

#[test]
fn skips_auto_compaction_below_threshold() {
    struct SimpleApi;
    impl ApiClient for SimpleApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::TextDelta("done".to_string()),
                AssistantEvent::Usage(TokenUsage {
                    input_tokens: 99_999,
                    output_tokens: 4,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                }),
                AssistantEvent::MessageStop,
            ])
        }
    }

    let mut runtime = ConversationRuntime::new(
        Session::new(),
        SimpleApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    )
    .with_auto_compaction_input_tokens_threshold(100_000);

    let summary = runtime
        .run_turn("trigger", None)
        .expect("turn should succeed");
    assert_eq!(summary.auto_compaction, None);
    assert_eq!(runtime.session().messages.len(), 2);
}

#[test]
fn auto_compaction_threshold_defaults_and_parses_values() {
    assert_eq!(
        parse_auto_compaction_threshold(None),
        DEFAULT_AUTO_COMPACTION_INPUT_TOKENS_THRESHOLD
    );
    assert_eq!(parse_auto_compaction_threshold(Some("4321")), 4321);
    assert_eq!(
        parse_auto_compaction_threshold(Some("0")),
        DEFAULT_AUTO_COMPACTION_INPUT_TOKENS_THRESHOLD
    );
    assert_eq!(
        parse_auto_compaction_threshold(Some("not-a-number")),
        DEFAULT_AUTO_COMPACTION_INPUT_TOKENS_THRESHOLD
    );
}

#[test]
fn build_assistant_message_requires_message_stop_event() {
    // given
    let events = vec![AssistantEvent::TextDelta("hello".to_string())];

    // when
    let error = build_assistant_message(events)
        .expect_err("assistant messages should require a stop event");

    // then
    assert!(error
        .to_string()
        .contains("assistant stream ended without a message stop event"));
}

#[test]
fn build_assistant_message_requires_content() {
    // given
    let events = vec![AssistantEvent::MessageStop];

    // when
    let error =
        build_assistant_message(events).expect_err("assistant messages should require content");

    // then
    assert!(error
        .to_string()
        .contains("assistant stream produced no content"));
}

#[test]
fn static_tool_executor_rejects_unknown_tools() {
    // given
    let mut executor = StaticToolExecutor::new();

    // when
    let error = executor
        .execute("missing", "{}")
        .expect_err("unregistered tools should fail");

    // then
    assert_eq!(error.to_string(), "unknown tool: missing");
}

#[test]
fn run_turn_errors_when_max_iterations_is_exceeded() {
    struct LoopingApi;

    impl ApiClient for LoopingApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Ok(vec![
                AssistantEvent::ToolUse {
                    id: "tool-1".to_string(),
                    name: "echo".to_string(),
                    input: "payload".to_string(),
                },
                AssistantEvent::MessageStop,
            ])
        }
    }

    // given
    let mut runtime = ConversationRuntime::new(
        Session::new(),
        LoopingApi,
        StaticToolExecutor::new().register("echo", |input: &str| Ok(input.to_string())),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    )
    .with_max_iterations(1);

    // when
    let error = runtime
        .run_turn("loop", None)
        .expect_err("conversation loop should stop after the configured limit");

    // then
    assert!(error
        .to_string()
        .contains("conversation loop exceeded the maximum number of iterations"));
}

#[test]
fn run_turn_propagates_api_errors() {
    struct FailingApi;

    impl ApiClient for FailingApi {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            Err(RuntimeError::new("upstream failed"))
        }
    }

    // given
    let mut runtime = ConversationRuntime::new(
        Session::new(),
        FailingApi,
        StaticToolExecutor::new(),
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    );

    // when
    let error = runtime
        .run_turn("hello", None)
        .expect_err("API failures should propagate");

    // then
    assert_eq!(error.to_string(), "upstream failed");
}
