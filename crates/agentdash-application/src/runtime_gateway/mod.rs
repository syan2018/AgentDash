mod error;
mod extension_actions;
mod gateway;
mod mcp_access;
mod provider;
mod session_actions;
mod setup_actions;
mod tool_adapter;
mod types;

pub use agentdash_application_ports::extension_runtime::{
    ExtensionRuntimeActionTransport, ExtensionRuntimeActionTransportError,
    ExtensionRuntimeChannelTransport,
};
pub use error::{RuntimeInvocationError, RuntimeInvocationErrorKind};
pub use extension_actions::{
    ExtensionInvocationWorkspaceContext, ExtensionRuntimeActionProvider,
    ExtensionRuntimeChannelConsumer, ExtensionRuntimeChannelInvokeRequest,
    ExtensionRuntimeChannelInvokeResult, ExtensionRuntimeChannelInvoker,
    attach_extension_invocation_workspace,
};
pub use gateway::RuntimeGateway;
pub use mcp_access::CurrentSurfaceRuntimeMcpAccess;
pub use provider::RuntimeProvider;
pub(crate) use session_actions::execute_runtime_mcp_tool;
pub use session_actions::{
    MCP_CALL_TOOL_ACTION, MCP_LIST_TOOLS_ACTION, McpCallToolInput, McpCallToolProvider,
    McpListToolsInput, McpListToolsOutput, McpListToolsProvider, RuntimeMcpToolDescriptor,
    RuntimeSessionMcpAccess, RuntimeSessionMcpError,
};
pub use setup_actions::{
    MCP_PROBE_TRANSPORT_ACTION, McpProbeTransportInput, McpProbeTransportProvider,
    WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION, WORKSPACE_DETECT_GIT_ACTION,
    WorkspaceBrowseDirectoryEntry, WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectoryOutput,
    WorkspaceBrowseDirectoryProvider, WorkspaceDetectGitInput, WorkspaceDetectGitOutput,
    WorkspaceDetectGitProvider, WorkspaceDetectInput, WorkspaceDetectProvider,
};
pub use tool_adapter::{RuntimeActionToolAdapter, RuntimeActionToolSpec};
pub use types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKeyError, RuntimeActionKind,
    RuntimeActor, RuntimeContext, RuntimeInvocationOutput, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimePolicy, RuntimeSurface, RuntimeTarget, RuntimeTrace,
};
