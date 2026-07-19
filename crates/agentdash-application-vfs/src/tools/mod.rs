pub mod common;
pub mod factory;
pub mod fs;
pub mod mounts;

pub use common::{RuntimeVfsState, SharedRuntimeVfs};
pub use factory::{VfsToolFactory, VfsToolFactoryInput};
pub use fs::{
    FsApplyPatchExecutor, FsApplyPatchTool, FsGlobExecutor, FsGlobTool, FsGrepExecutor, FsGrepTool,
    FsReadExecutor, FsReadTool, ShellExecExecutor, ShellExecTool, ShellTerminalOutputSnapshot,
    ShellTerminalOwner, ShellTerminalRegistration, ShellTerminalRegistry,
};
pub use mounts::{MountsListExecutor, MountsListTool};

use crate::runtime_tool_execution::{
    VfsToolContent, VfsToolExecutionError, VfsToolExecutionResult, VfsToolUpdateSink,
};

pub(crate) fn legacy_result(
    result: VfsToolExecutionResult,
) -> agentdash_agent_types::AgentToolResult {
    agentdash_agent_types::AgentToolResult {
        content: result
            .content
            .into_iter()
            .map(|part| match part {
                VfsToolContent::Text { text } => agentdash_agent_types::ContentPart::Text { text },
                VfsToolContent::Image { mime_type, data } => {
                    agentdash_agent_types::ContentPart::Image { mime_type, data }
                }
            })
            .collect(),
        is_error: result.is_error,
        details: result.details,
    }
}

pub(crate) fn legacy_error(error: VfsToolExecutionError) -> agentdash_agent_types::AgentToolError {
    match error {
        VfsToolExecutionError::InvalidArguments(message) => {
            agentdash_agent_types::AgentToolError::InvalidArguments(message)
        }
        VfsToolExecutionError::ExecutionFailed(message) => {
            agentdash_agent_types::AgentToolError::ExecutionFailed(message)
        }
        VfsToolExecutionError::Cancelled => agentdash_agent_types::AgentToolError::ExecutionFailed(
            "tool execution cancelled".into(),
        ),
    }
}

pub(crate) fn legacy_update_sink(
    callback: Option<agentdash_agent_types::ToolUpdateCallback>,
) -> Option<VfsToolUpdateSink> {
    callback.map(|callback| {
        std::sync::Arc::new(move |update| callback(legacy_result(update))) as VfsToolUpdateSink
    })
}
