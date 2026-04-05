#![allow(clippy::wildcard_imports)]

mod client;
mod completion;
mod mcp_state;
mod progress;
mod summary;

use super::*;
pub(crate) use client::AnthropicRuntimeClient;
use client::CliHookProgressReporter;
pub(crate) use completion::slash_command_completion_candidates_with_sessions;
pub(crate) use mcp_state::{
    build_plugin_manager, build_runtime_plugin_state, build_runtime_plugin_state_with_loader,
    BuiltRuntime, RuntimeMcpState, RuntimePluginState,
};
pub(crate) use progress::InternalPromptProgressReporter;
#[cfg(test)]
pub(crate) use progress::{
    describe_tool_progress, format_internal_prompt_progress_line, InternalPromptProgressEvent,
    InternalPromptProgressState,
};
pub(super) use summary::{
    collect_prompt_cache_events, collect_tool_results, collect_tool_uses, final_assistant_text,
};

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
pub(super) fn build_runtime(
    session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    let runtime_plugin_state = build_runtime_plugin_state()?;
    build_runtime_with_plugin_state(
        session,
        session_id,
        model,
        system_prompt,
        enable_tools,
        emit_output,
        allowed_tools,
        permission_mode,
        progress_reporter,
        runtime_plugin_state,
    )
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
pub(super) fn build_runtime_with_plugin_state(
    session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
    runtime_plugin_state: RuntimePluginState,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    let RuntimePluginState {
        feature_config,
        tool_registry,
        plugin_registry,
        mcp_state,
    } = runtime_plugin_state;
    plugin_registry.initialize()?;
    let policy = permission_policy(permission_mode, &feature_config, &tool_registry)
        .map_err(std::io::Error::other)?;
    let mut runtime = ConversationRuntime::new_with_features(
        session,
        AnthropicRuntimeClient::new(
            session_id,
            model,
            enable_tools,
            emit_output,
            allowed_tools.clone(),
            tool_registry.clone(),
            progress_reporter,
        )?,
        CliToolExecutor::new(
            allowed_tools.clone(),
            emit_output,
            tool_registry.clone(),
            mcp_state.clone(),
        ),
        policy,
        system_prompt,
        &feature_config,
    );
    if emit_output {
        runtime = runtime.with_hook_progress_reporter(Box::new(CliHookProgressReporter));
    }
    Ok(BuiltRuntime::new(runtime, plugin_registry, mcp_state))
}
