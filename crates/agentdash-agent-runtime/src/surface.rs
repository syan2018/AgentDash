use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_runtime_contract::{
    ConfigurationBoundary, ContextBlock, ContextRecipe, DeliveryMechanism, HookAction,
    HookDefinitionId, HookFailurePolicy, HookPlanDigest, HookPlanRevision, HookPoint,
    InputModality, InstructionChannel, RuntimeProfile, RuntimeThreadId, SemanticStrength,
    SurfaceDigest, SurfaceRevision, ToolChannel, ToolPresentationEmitter, ToolProtocolProjection,
    ToolSetRevision, WorkspaceCapability,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    BoundRuntimeHookEntry, BoundRuntimeHookPlan, HookExecutionSite, RuntimeHookPlanBinding,
    context_projection::{
        ContextFrameFacts, ContextProjectionIdentity, ContextProjector,
        RuntimeSurfacePresentationPlan,
    },
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledBusinessAgentSurface {
    pub snapshot: AgentSurfaceSnapshot,
    pub presentation: RuntimeSurfacePresentationPlan,
}

#[derive(Debug, Clone)]
pub struct BusinessAgentSurfaceFacts {
    pub revision: SurfaceRevision,
    pub context_recipe: ContextRecipe,
    pub tool_set_revision: ToolSetRevision,
    pub hook_plan_revision: HookPlanRevision,
    pub workspace: WorkspaceRequirement,
    pub source: SurfaceSourceRef,
    pub transition_phase_node: Option<String>,
    pub instructions: Vec<String>,
    pub tools: Vec<ToolContribution>,
    pub hooks: Vec<HookDefinition>,
    pub projection_identity: ContextProjectionIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionRequirement {
    Required,
    Optional,
}

impl ContributionRequirement {
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceSourceRef {
    pub layer: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionMeta {
    pub key: String,
    pub source: SurfaceSourceRef,
    pub priority: i32,
    pub requirement: ContributionRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionContribution {
    pub meta: ContributionMeta,
    pub channel: InstructionChannel,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextContribution {
    pub meta: ContributionMeta,
    pub blocks: Vec<ContextBlock>,
    pub minimum_strength: SemanticStrength,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolContribution {
    pub meta: ContributionMeta,
    pub runtime_name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub capability_key: String,
    pub tool_path: String,
    pub allowed_channels: BTreeSet<ToolChannel>,
    pub configuration_boundary: ConfigurationBoundary,
    pub protocol_projection: ToolProtocolProjection,
    pub presentation_emitter: ToolPresentationEmitter,
    pub parity_fixture_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolProjectionError {
    #[error("tool protocol projection is invalid: {0}")]
    Invalid(String),
}

impl ToolContribution {
    pub fn project_update(
        &self,
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    ) -> agentdash_agent_runtime_contract::RuntimeConversationDelta {
        agentdash_agent_runtime_contract::RuntimeConversationDelta::ToolProgress { content_items }
    }
    pub fn project_started(
        &self,
        item_id: &str,
        arguments: serde_json::Value,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeItemContent, ToolProjectionError> {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
        use agentdash_agent_protocol::{AgentDashNativeThreadItem as Native, AgentDashThreadItem};

        let item = match &self.protocol_projection {
            ToolProtocolProjection::Command => shell_started_item(item_id, arguments),
            ToolProtocolProjection::FsRead => AgentDashThreadItem::AgentDash(Native::FsRead {
                id: item_id.to_string(),
                path: string_field(&arguments, "path"),
                offset: usize_field(&arguments, "offset"),
                limit: usize_field(&arguments, "limit"),
                arguments,
                status: codex::DynamicToolCallStatus::InProgress,
                content_items: None,
                success: None,
            }),
            ToolProtocolProjection::FsGrep => AgentDashThreadItem::AgentDash(Native::FsGrep {
                id: item_id.to_string(),
                pattern: string_field(&arguments, "pattern"),
                path: optional_string_field(&arguments, "path"),
                glob: optional_string_field(&arguments, "glob"),
                file_type: optional_string_field(&arguments, "file_type"),
                output_mode: optional_string_field(&arguments, "output_mode"),
                head_limit: usize_field(&arguments, "head_limit"),
                offset: usize_field(&arguments, "offset"),
                arguments,
                status: codex::DynamicToolCallStatus::InProgress,
                content_items: None,
                success: None,
            }),
            ToolProtocolProjection::FsGlob => AgentDashThreadItem::AgentDash(Native::FsGlob {
                id: item_id.to_string(),
                pattern: string_field(&arguments, "pattern"),
                path: optional_string_field(&arguments, "path"),
                max_results: usize_field(&arguments, "max_results"),
                arguments,
                status: codex::DynamicToolCallStatus::InProgress,
                content_items: None,
                success: None,
            }),
            ToolProtocolProjection::FileChange => codex_projected_item(
                serde_json::json!({"type":"fileChange","id":item_id,"changes":arguments.get("changes").cloned().unwrap_or_else(||serde_json::Value::Array(file_changes_from_patch(&arguments))),"status":"inProgress"}),
            )?,
            ToolProtocolProjection::Mcp { server_key } => codex_projected_item(
                serde_json::json!({"type":"mcpToolCall","id":item_id,"server":server_key,"tool":self.runtime_name,"arguments":arguments,"status":"inProgress"}),
            )?,
            ToolProtocolProjection::Dynamic { namespace } => dynamic_item(
                item_id,
                self.runtime_name.as_str(),
                namespace.clone(),
                arguments,
                false,
                false,
                None,
            )?,
        };
        Ok(agentdash_agent_runtime_contract::RuntimeItemContent::new(
            item,
        ))
    }

    pub fn project_updated(
        &self,
        item_id: &str,
        arguments: serde_json::Value,
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeItemContent, ToolProjectionError> {
        if matches!(self.protocol_projection, ToolProtocolProjection::Mcp { .. }) {
            return Err(ToolProjectionError::Invalid(
                "MCP progress is a mcp_tool_call_progress event, not an item_updated snapshot"
                    .to_string(),
            ));
        }
        let started = self.project_started(item_id, arguments)?;
        if !matches!(
            self.protocol_projection,
            ToolProtocolProjection::FsRead
                | ToolProtocolProjection::FsGrep
                | ToolProtocolProjection::FsGlob
                | ToolProtocolProjection::Dynamic { .. }
        ) {
            return Ok(started);
        }
        let mut item = serde_json::to_value(started.item())
            .map_err(|error| ToolProjectionError::Invalid(error.to_string()))?;
        let object = item.as_object_mut().ok_or_else(|| {
            ToolProjectionError::Invalid("projected tool item must be an object".to_string())
        })?;
        object.insert(
            "contentItems".to_string(),
            serde_json::to_value(content_items)
                .map_err(|error| ToolProjectionError::Invalid(error.to_string()))?,
        );
        let item = serde_json::from_value(item)
            .map_err(|error| ToolProjectionError::Invalid(error.to_string()))?;
        Ok(agentdash_agent_runtime_contract::RuntimeItemContent::new(
            item,
        ))
    }

    pub fn project_completed(
        &self,
        item_id: &str,
        arguments: serde_json::Value,
        output: &serde_json::Value,
        failed: bool,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeItemContent, ToolProjectionError> {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
        use agentdash_agent_protocol::{AgentDashNativeThreadItem as Native, AgentDashThreadItem};
        let status = if failed {
            codex::DynamicToolCallStatus::Failed
        } else {
            codex::DynamicToolCallStatus::Completed
        };
        let item = match &self.protocol_projection {
            ToolProtocolProjection::Command => {
                shell_terminal_item(item_id, arguments, output, status, failed)
            }
            ToolProtocolProjection::FsRead => AgentDashThreadItem::AgentDash(Native::FsRead {
                id: item_id.to_string(),
                path: string_field(&arguments, "path"),
                offset: usize_field(&arguments, "offset"),
                limit: usize_field(&arguments, "limit"),
                arguments,
                status,
                content_items: content_items(output)?,
                success: Some(!failed),
            }),
            ToolProtocolProjection::FsGrep => AgentDashThreadItem::AgentDash(Native::FsGrep {
                id: item_id.to_string(),
                pattern: string_field(&arguments, "pattern"),
                path: optional_string_field(&arguments, "path"),
                glob: optional_string_field(&arguments, "glob"),
                file_type: optional_string_field(&arguments, "file_type"),
                output_mode: optional_string_field(&arguments, "output_mode"),
                head_limit: usize_field(&arguments, "head_limit"),
                offset: usize_field(&arguments, "offset"),
                arguments,
                status,
                content_items: content_items(output)?,
                success: Some(!failed),
            }),
            ToolProtocolProjection::FsGlob => AgentDashThreadItem::AgentDash(Native::FsGlob {
                id: item_id.to_string(),
                pattern: string_field(&arguments, "pattern"),
                path: optional_string_field(&arguments, "path"),
                max_results: usize_field(&arguments, "max_results"),
                arguments,
                status,
                content_items: content_items(output)?,
                success: Some(!failed),
            }),
            ToolProtocolProjection::FileChange => codex_projected_item(
                serde_json::json!({"type":"fileChange","id":item_id,"changes":output.get("changes").cloned().or_else(||arguments.get("changes").cloned()).unwrap_or_else(||serde_json::Value::Array(file_changes_from_patch(&arguments))),"status":if failed{"failed"}else{"completed"}}),
            )?,
            ToolProtocolProjection::Mcp { server_key } => {
                let (result, error) = if failed {
                    let message = output
                        .get("message")
                        .or_else(|| output.get("error"))
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            ToolProjectionError::Invalid(
                                "MCP failure output requires message/error".to_string(),
                            )
                        })?;
                    (
                        serde_json::Value::Null,
                        serde_json::json!({"message":message}),
                    )
                } else {
                    (output.clone(), serde_json::Value::Null)
                };
                codex_projected_item(
                    serde_json::json!({"type":"mcpToolCall","id":item_id,"server":server_key,"tool":self.runtime_name,"arguments":arguments,"status":if failed{"failed"}else{"completed"},"result":result,"error":error}),
                )?
            }
            ToolProtocolProjection::Dynamic { namespace } => dynamic_item(
                item_id,
                self.runtime_name.as_str(),
                namespace.clone(),
                arguments,
                true,
                failed,
                content_items(output)?,
            )?,
        };
        Ok(agentdash_agent_runtime_contract::RuntimeItemContent::new(
            item,
        ))
    }
}

fn shell_operation(arguments: &serde_json::Value) -> String {
    optional_string_field(arguments, "operation").unwrap_or_else(|| "start".to_string())
}

fn shell_execution_mode(
    arguments: &serde_json::Value,
) -> agentdash_agent_protocol::ShellExecExecutionMode {
    match optional_string_field(arguments, "cwd")
        .as_deref()
        .map(str::trim)
    {
        None | Some("") | Some("platform://") => {
            agentdash_agent_protocol::ShellExecExecutionMode::Platform
        }
        Some(_) => agentdash_agent_protocol::ShellExecExecutionMode::MountExec,
    }
}

fn shell_started_item(
    item_id: &str,
    arguments: serde_json::Value,
) -> agentdash_agent_protocol::AgentDashThreadItem {
    use agentdash_agent_protocol::codex_app_server_protocol::DynamicToolCallStatus;
    use agentdash_agent_protocol::{AgentDashNativeThreadItem as Native, AgentDashThreadItem};
    let operation = shell_operation(&arguments);
    if operation == "start" {
        AgentDashThreadItem::AgentDash(Native::ShellExec {
            id: item_id.to_string(),
            command: string_field(&arguments, "command"),
            cwd: optional_string_field(&arguments, "cwd"),
            execution_mode: shell_execution_mode(&arguments),
            arguments,
            status: DynamicToolCallStatus::InProgress,
            aggregated_output: None,
            exit_code: None,
            success: Some(true),
        })
    } else {
        AgentDashThreadItem::AgentDash(Native::TerminalControl {
            id: item_id.to_string(),
            operation,
            terminal_id: string_field(&arguments, "terminal_id"),
            input: optional_string_field(&arguments, "data"),
            cols: usize_field(&arguments, "cols").and_then(|v| u16::try_from(v).ok()),
            rows: usize_field(&arguments, "rows").and_then(|v| u16::try_from(v).ok()),
            arguments,
            state: None,
            aggregated_output: None,
            exit_code: None,
            status: DynamicToolCallStatus::InProgress,
            success: None,
        })
    }
}

fn shell_terminal_item(
    item_id: &str,
    arguments: serde_json::Value,
    output: &serde_json::Value,
    status: agentdash_agent_protocol::codex_app_server_protocol::DynamicToolCallStatus,
    failed: bool,
) -> agentdash_agent_protocol::AgentDashThreadItem {
    use agentdash_agent_protocol::{AgentDashNativeThreadItem as Native, AgentDashThreadItem};
    let operation = shell_operation(&arguments);
    if operation == "start" {
        AgentDashThreadItem::AgentDash(Native::ShellExec {
            id: item_id.to_string(),
            command: optional_string_field(output, "original_command")
                .unwrap_or_else(|| string_field(&arguments, "command")),
            cwd: optional_string_field(output, "cwd")
                .or_else(|| optional_string_field(&arguments, "cwd")),
            execution_mode: shell_execution_mode(&arguments),
            arguments,
            status,
            aggregated_output: optional_string_field(output, "aggregated_output"),
            exit_code: output
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok()),
            success: Some(!failed),
        })
    } else {
        AgentDashThreadItem::AgentDash(Native::TerminalControl {
            id: item_id.to_string(),
            operation,
            terminal_id: optional_string_field(output, "terminal_id")
                .unwrap_or_else(|| string_field(&arguments, "terminal_id")),
            input: optional_string_field(&arguments, "data"),
            cols: usize_field(&arguments, "cols").and_then(|v| u16::try_from(v).ok()),
            rows: usize_field(&arguments, "rows").and_then(|v| u16::try_from(v).ok()),
            arguments,
            state: optional_string_field(output, "state")
                .or_else(|| optional_string_field(output, "status")),
            aggregated_output: optional_string_field(output, "aggregated_output"),
            exit_code: output
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .and_then(|v| i32::try_from(v).ok()),
            status,
            success: Some(!failed),
        })
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> String {
    optional_string_field(value, key).unwrap_or_default()
}
fn optional_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}
fn usize_field(value: &serde_json::Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}
fn content_items(
    value: &serde_json::Value,
) -> Result<
    Option<Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>>,
    ToolProjectionError,
> {
    let Some(items) = value.get("content_items") else {
        return Ok(None);
    };
    serde_json::from_value(items.clone())
        .map(Some)
        .map_err(|e| ToolProjectionError::Invalid(e.to_string()))
}

fn file_changes_from_patch(arguments: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(patch) = arguments.get("patch").and_then(serde_json::Value::as_str) else {
        return Vec::new();
    };
    patch_entry_diffs(patch)
        .into_iter()
        .filter_map(|diff| {
            let lines = diff.lines().collect::<Vec<_>>();
            let header = lines.first()?;
            let (path, kind) = if let Some(path) = header.strip_prefix("*** Add File: ") {
                (path, serde_json::json!({"type":"add"}))
            } else if let Some(path) = header.strip_prefix("*** Delete File: ") {
                (path, serde_json::json!({"type":"delete"}))
            } else if let Some(path) = header.strip_prefix("*** Update File: ") {
                let move_path = lines
                    .get(1)
                    .and_then(|next| next.strip_prefix("*** Move to: "));
                (
                    path,
                    serde_json::json!({"type":"update","move_path":move_path}),
                )
            } else {
                return None;
            };
            let diff = lines
                .iter()
                .skip(1)
                .filter(|line| **line != "*** End of File" && !line.starts_with("*** Move to: "))
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            Some(serde_json::json!({"path":path.trim(),"kind":kind,"diff":diff}))
        })
        .collect()
}

fn patch_entry_diffs(patch: &str) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut current = Vec::new();
    for line in patch.lines() {
        let starts_entry = line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Update File: ");
        if starts_entry && !current.is_empty() {
            diffs.push(current.join("\n"));
            current.clear();
        }
        if (starts_entry || !current.is_empty()) && line != "*** End Patch" {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        diffs.push(current.join("\n"));
    }
    diffs
}
fn codex_projected_item(
    value: serde_json::Value,
) -> Result<agentdash_agent_protocol::AgentDashThreadItem, ToolProjectionError> {
    serde_json::from_value(value)
        .map(agentdash_agent_protocol::AgentDashThreadItem::Codex)
        .map_err(|e| ToolProjectionError::Invalid(e.to_string()))
}
fn dynamic_item(
    item_id: &str,
    tool: &str,
    namespace: Option<String>,
    arguments: serde_json::Value,
    completed: bool,
    failed: bool,
    content_items: Option<Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>>,
) -> Result<agentdash_agent_protocol::AgentDashThreadItem, ToolProjectionError> {
    let mut value = serde_json::json!({
        "type": "dynamicToolCall",
        "id": item_id,
        "tool": tool,
        "arguments": arguments,
        "status": if failed { "failed" } else if completed { "completed" } else { "inProgress" },
    });
    let object = value
        .as_object_mut()
        .expect("dynamic tool projection object");
    if let Some(namespace) = namespace {
        object.insert(
            "namespace".to_string(),
            serde_json::Value::String(namespace),
        );
    }
    if completed {
        object.insert("success".to_string(), serde_json::Value::Bool(!failed));
    }
    if let Some(content_items) = content_items {
        object.insert(
            "contentItems".to_string(),
            serde_json::to_value(content_items)
                .map_err(|error| ToolProjectionError::Invalid(error.to_string()))?,
        );
    }
    serde_json::from_value(value)
        .map(agentdash_agent_protocol::AgentDashThreadItem::Codex)
        .map_err(|e| ToolProjectionError::Invalid(e.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpContribution {
    pub meta: ContributionMeta,
    pub server_key: String,
    pub credential_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillContribution {
    pub meta: ContributionMeta,
    pub resource_ref: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowContribution {
    pub meta: ContributionMeta,
    pub workflow_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionContribution {
    pub meta: ContributionMeta,
    pub capability_paths: BTreeSet<String>,
    pub policy_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookDefinition {
    pub meta: ContributionMeta,
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub actions: BTreeSet<HookAction>,
    pub minimum_strength: SemanticStrength,
    pub failure_policy: HookFailurePolicy,
    pub matcher: HookMatcher,
    pub handler: HookHandler,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookMatcher {
    Any,
    ToolNames { names: BTreeSet<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookHandler {
    Builtin {
        key: String,
    },
    Script {
        engine_key: String,
        script_ref: String,
        parameters: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CapabilityContribution {
    Instruction(InstructionContribution),
    Context(ContextContribution),
    Tool(ToolContribution),
    Mcp(McpContribution),
    Skill(SkillContribution),
    Workflow(WorkflowContribution),
    Permission(PermissionContribution),
    Hook(HookDefinition),
}

impl CapabilityContribution {
    pub fn meta(&self) -> &ContributionMeta {
        match self {
            Self::Instruction(value) => &value.meta,
            Self::Context(value) => &value.meta,
            Self::Tool(value) => &value.meta,
            Self::Mcp(value) => &value.meta,
            Self::Skill(value) => &value.meta,
            Self::Workflow(value) => &value.meta,
            Self::Permission(value) => &value.meta,
            Self::Hook(value) => &value.meta,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityPack {
    pub key: String,
    pub contributions: Vec<CapabilityContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionPlan {
    pub entries: Vec<InstructionContribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextEnvelope {
    pub recipe: ContextRecipe,
    pub instructions: InstructionPlan,
    pub contributions: Vec<ContextContribution>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCatalogRevision {
    pub revision: ToolSetRevision,
    pub digest: String,
    pub tools: Vec<ToolContribution>,
    pub mcp_servers: Vec<McpContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRequirement {
    pub capabilities: BTreeSet<WorkspaceCapability>,
    pub minimum_mechanism: DeliveryMechanism,
    pub requirement: ContributionRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookPlanSnapshot {
    pub revision: HookPlanRevision,
    pub digest: HookPlanDigest,
    pub definitions: Vec<HookDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSurfaceSnapshot {
    pub revision: SurfaceRevision,
    pub digest: SurfaceDigest,
    pub context: ContextEnvelope,
    pub tools: ToolCatalogRevision,
    pub workspace: WorkspaceRequirement,
    pub hook_plan: HookPlanSnapshot,
    pub skills: Vec<SkillContribution>,
    pub workflows: Vec<WorkflowContribution>,
    pub permissions: Vec<PermissionContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceDeliveryRoute {
    Instruction(InstructionChannel),
    Context,
    DirectToolCallback,
    McpToolFacade,
    DriverNativeTool,
    Workspace(DeliveryMechanism),
    NativeSkill,
    HostPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundSurfaceContribution {
    pub key: String,
    pub route: SurfaceDeliveryRoute,
    pub strength: SemanticStrength,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundAgentSurface {
    pub source_revision: SurfaceRevision,
    pub source_digest: SurfaceDigest,
    pub digest: SurfaceDigest,
    pub contributions: Vec<BoundSurfaceContribution>,
    pub hook_plan: RuntimeHookPlanBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSurfaceCompileInput {
    pub revision: SurfaceRevision,
    pub context_recipe: ContextRecipe,
    pub tool_set_revision: ToolSetRevision,
    pub hook_plan_revision: HookPlanRevision,
    pub workspace: WorkspaceRequirement,
    pub contributions: Vec<CapabilityContribution>,
    pub capability_packs: Vec<CapabilityPack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRouteSelection {
    pub definition_id: HookDefinitionId,
    pub site: HookExecutionSite,
    pub delivered_strength: SemanticStrength,
    pub actions: BTreeSet<HookAction>,
    pub failure_policies: BTreeSet<HookFailurePolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SurfaceCompileError {
    #[error("surface revisions must be greater than zero")]
    ZeroRevision,
    #[error("surface contribution field `{field}` must not be empty for `{key}`")]
    EmptyField { key: String, field: &'static str },
    #[error("surface contribution key `{key}` has conflicting definitions")]
    ConflictingContribution { key: String },
    #[error("tool `{key}` parameters schema must be a JSON object")]
    InvalidToolSchema { key: String },
    #[error("tool `{key}` protocol projector is invalid: {reason}")]
    InvalidToolProjector { key: String, reason: String },
    #[error("tool runtime identity `{runtime_name}` is contributed more than once")]
    ConflictingToolRuntimeName { runtime_name: String },
    #[error("tool path `{tool_path}` is contributed more than once")]
    ConflictingToolPath { tool_path: String },
    #[error("tool parity fixture `{fixture_id}` is contributed more than once")]
    ConflictingToolParityFixture { fixture_id: String },
    #[error("MCP server identity `{server_key}` is contributed more than once")]
    ConflictingMcpServerKey { server_key: String },
    #[error("hook definition `{definition_id}` has no actions")]
    EmptyHookActions { definition_id: HookDefinitionId },
    #[error("hook definition `{definition_id}` has conflicting definitions")]
    ConflictingHookDefinition { definition_id: HookDefinitionId },
    #[error("hook definition `{definition_id}` has no compatible bound route")]
    MissingHookRoute { definition_id: HookDefinitionId },
    #[error("hook definition `{definition_id}` route does not satisfy its requirement")]
    IncompatibleHookRoute { definition_id: HookDefinitionId },
    #[error("hook definition `{definition_id}` has more than one selected route")]
    ConflictingHookRoute { definition_id: HookDefinitionId },
    #[error("hook route targets unknown definition `{definition_id}`")]
    UnknownHookRoute { definition_id: HookDefinitionId },
    #[error("required surface contribution `{key}` is incompatible: {reason}")]
    IncompatibleContribution { key: String, reason: String },
    #[error("surface digest construction failed: {0}")]
    Digest(String),
}

#[derive(Debug, Default)]
pub struct AgentSurfaceCompiler;

impl AgentSurfaceCompiler {
    pub fn compile_business_facts(
        &self,
        facts: BusinessAgentSurfaceFacts,
    ) -> Result<CompiledBusinessAgentSurface, SurfaceCompileError> {
        let mut contributions = facts
            .tools
            .iter()
            .cloned()
            .map(CapabilityContribution::Tool)
            .collect::<Vec<_>>();
        contributions.extend(
            facts
                .instructions
                .iter()
                .enumerate()
                .map(|(index, content)| {
                    CapabilityContribution::Instruction(InstructionContribution {
                        meta: ContributionMeta {
                            key: format!("instruction:surface:{index}"),
                            source: facts.source.clone(),
                            priority: 0,
                            requirement: ContributionRequirement::Required,
                        },
                        channel: InstructionChannel::System,
                        content: content.clone(),
                    })
                }),
        );
        contributions.extend(
            facts
                .hooks
                .iter()
                .cloned()
                .map(CapabilityContribution::Hook),
        );
        let added_tools = facts
            .tools
            .iter()
            .map(|tool| agentdash_agent_protocol::RuntimeToolSchemaEntry {
                name: tool.runtime_name.clone(),
                description: tool.description.clone(),
                parameters_schema: tool.parameters_schema.clone(),
                capability_key: Some(tool.capability_key.clone()),
                source: Some(tool.meta.source.key.clone()),
                tool_path: Some(tool.tool_path.clone()),
                context_usage_kind: Some("system_tools".to_string()),
            })
            .collect();
        self.compile_with_presentation(
            AgentSurfaceCompileInput {
                revision: facts.revision,
                context_recipe: facts.context_recipe,
                tool_set_revision: facts.tool_set_revision,
                hook_plan_revision: facts.hook_plan_revision,
                workspace: facts.workspace,
                contributions,
                capability_packs: Vec::new(),
            },
            &facts.projection_identity,
            facts.transition_phase_node,
            [ContextFrameFacts {
                kind: agentdash_agent_protocol::ContextFrameKind::CapabilityStateDelta,
                source: agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
                phase_node: None,
                apply_mode: None,
                delivery_status: agentdash_agent_protocol::ContextDeliveryStatus::Accepted,
                delivery_channel:
                    agentdash_agent_protocol::ContextDeliveryChannel::ConnectorContext,
                message_role: agentdash_agent_protocol::ContextMessageRole::Context,
                rendered_text: "Runtime capability and tool surface".to_string(),
                sections: vec![
                    agentdash_agent_protocol::ContextFrameSection::ToolSchemaDelta { added_tools },
                ],
            }],
        )
    }

    pub fn compile_with_presentation(
        &self,
        input: AgentSurfaceCompileInput,
        projection_identity: &ContextProjectionIdentity,
        transition_phase_node: Option<String>,
        presentation_facts: impl IntoIterator<Item = ContextFrameFacts>,
    ) -> Result<CompiledBusinessAgentSurface, SurfaceCompileError> {
        let snapshot = self.compile(input)?;
        let mut presentation = ContextProjector::project(projection_identity, presentation_facts);
        presentation.transition_phase_node = transition_phase_node;
        if presentation.source_frame_revision != snapshot.revision.0 {
            return Err(SurfaceCompileError::Digest(
                "presentation source revision differs from compiled surface".to_string(),
            ));
        }
        Ok(CompiledBusinessAgentSurface {
            snapshot,
            presentation,
        })
    }

    pub fn compile(
        &self,
        input: AgentSurfaceCompileInput,
    ) -> Result<AgentSurfaceSnapshot, SurfaceCompileError> {
        if input.revision.0 == 0
            || input.context_recipe.revision.0 == 0
            || input.tool_set_revision.0 == 0
            || input.hook_plan_revision.0 == 0
        {
            return Err(SurfaceCompileError::ZeroRevision);
        }

        let mut pack_keys = BTreeSet::new();
        let mut expanded = input.contributions;
        for pack in input.capability_packs {
            if pack.key.trim().is_empty() {
                return Err(SurfaceCompileError::EmptyField {
                    key: "capability_pack".to_string(),
                    field: "key",
                });
            }
            if !pack_keys.insert(pack.key.clone()) {
                return Err(SurfaceCompileError::ConflictingContribution { key: pack.key });
            }
            expanded.extend(pack.contributions);
        }
        let mut by_key = BTreeMap::<String, CapabilityContribution>::new();
        for contribution in expanded {
            validate_contribution(&contribution)?;
            let key = contribution.meta().key.clone();
            match by_key.get(&key) {
                Some(existing) if existing != &contribution => {
                    return Err(SurfaceCompileError::ConflictingContribution { key });
                }
                Some(_) => {}
                None => {
                    by_key.insert(key, contribution);
                }
            }
        }

        let mut contributions = by_key.into_values().collect::<Vec<_>>();
        contributions.sort_by(|left, right| {
            right
                .meta()
                .priority
                .cmp(&left.meta().priority)
                .then_with(|| left.meta().key.cmp(&right.meta().key))
        });

        let mut instructions = Vec::new();
        let mut context = Vec::new();
        let mut tools = Vec::new();
        let mut mcp_servers = Vec::new();
        let mut skills = Vec::new();
        let mut workflows = Vec::new();
        let mut permissions = Vec::new();
        let mut hooks = Vec::new();
        let mut hook_by_id = BTreeMap::<HookDefinitionId, HookDefinition>::new();
        for contribution in contributions {
            match contribution {
                CapabilityContribution::Instruction(value) => instructions.push(value),
                CapabilityContribution::Context(value) => context.push(value),
                CapabilityContribution::Tool(value) => tools.push(value),
                CapabilityContribution::Mcp(value) => mcp_servers.push(value),
                CapabilityContribution::Skill(value) => skills.push(value),
                CapabilityContribution::Workflow(value) => workflows.push(value),
                CapabilityContribution::Permission(value) => permissions.push(value),
                CapabilityContribution::Hook(value) => match hook_by_id.get(&value.definition_id) {
                    Some(existing) if existing != &value => {
                        return Err(SurfaceCompileError::ConflictingHookDefinition {
                            definition_id: value.definition_id,
                        });
                    }
                    Some(_) => {}
                    None => {
                        hook_by_id.insert(value.definition_id.clone(), value);
                    }
                },
            }
        }
        hooks.extend(hook_by_id.into_values());

        let mut runtime_names = BTreeSet::new();
        let mut tool_paths = BTreeSet::new();
        let mut parity_fixtures = BTreeSet::new();
        for tool in &tools {
            if !runtime_names.insert(tool.runtime_name.clone()) {
                return Err(SurfaceCompileError::ConflictingToolRuntimeName {
                    runtime_name: tool.runtime_name.clone(),
                });
            }
            if !tool_paths.insert(tool.tool_path.clone()) {
                return Err(SurfaceCompileError::ConflictingToolPath {
                    tool_path: tool.tool_path.clone(),
                });
            }
            if !parity_fixtures.insert(tool.parity_fixture_id.clone()) {
                return Err(SurfaceCompileError::ConflictingToolParityFixture {
                    fixture_id: tool.parity_fixture_id.clone(),
                });
            }
        }
        let mut server_keys = BTreeSet::new();
        for server in &mcp_servers {
            if !server_keys.insert(server.server_key.clone()) {
                return Err(SurfaceCompileError::ConflictingMcpServerKey {
                    server_key: server.server_key.clone(),
                });
            }
        }

        let instruction_plan = InstructionPlan {
            entries: instructions,
        };
        let context_digest =
            digest_json(&(input.context_recipe.clone(), &instruction_plan, &context))?;
        let tool_digest = digest_json(&(input.tool_set_revision, &tools, &mcp_servers))?;
        let hook_digest_value = digest_json(&(input.hook_plan_revision, &hooks))?;
        let hook_digest = HookPlanDigest::new(hook_digest_value.clone())
            .map_err(|error| SurfaceCompileError::Digest(error.to_string()))?;
        let context_envelope = ContextEnvelope {
            recipe: input.context_recipe,
            instructions: instruction_plan,
            contributions: context,
            digest: context_digest,
        };
        let tool_catalog = ToolCatalogRevision {
            revision: input.tool_set_revision,
            digest: tool_digest,
            tools,
            mcp_servers,
        };
        let hook_plan = HookPlanSnapshot {
            revision: input.hook_plan_revision,
            digest: hook_digest,
            definitions: hooks,
        };
        let surface_digest_value = digest_json(&(
            input.revision,
            &context_envelope,
            &tool_catalog,
            &input.workspace,
            &hook_plan,
            &skills,
            &workflows,
            &permissions,
        ))?;
        let surface_digest = SurfaceDigest::new(surface_digest_value)
            .map_err(|error| SurfaceCompileError::Digest(error.to_string()))?;

        Ok(AgentSurfaceSnapshot {
            revision: input.revision,
            digest: surface_digest,
            context: context_envelope,
            tools: tool_catalog,
            workspace: input.workspace,
            hook_plan,
            skills,
            workflows,
            permissions,
        })
    }
}

impl HookPlanSnapshot {
    pub fn bind_runtime_plan(
        &self,
        thread_id: RuntimeThreadId,
        selections: impl IntoIterator<Item = HookRouteSelection>,
    ) -> Result<RuntimeHookPlanBinding, SurfaceCompileError> {
        let mut selections_by_id = BTreeMap::new();
        for selection in selections {
            let definition_id = selection.definition_id.clone();
            if selections_by_id
                .insert(definition_id.clone(), selection)
                .is_some()
            {
                return Err(SurfaceCompileError::ConflictingHookRoute { definition_id });
            }
        }
        let definition_ids = self
            .definitions
            .iter()
            .map(|definition| definition.definition_id.clone())
            .collect::<BTreeSet<_>>();
        if let Some(definition_id) = selections_by_id
            .keys()
            .find(|definition_id| !definition_ids.contains(*definition_id))
        {
            return Err(SurfaceCompileError::UnknownHookRoute {
                definition_id: definition_id.clone(),
            });
        }
        let mut entries = Vec::new();
        for definition in &self.definitions {
            let Some(selection) = selections_by_id.get(&definition.definition_id) else {
                if definition.meta.requirement.is_required() {
                    return Err(SurfaceCompileError::MissingHookRoute {
                        definition_id: definition.definition_id.clone(),
                    });
                }
                continue;
            };
            let compatible = selection
                .delivered_strength
                .satisfies(definition.minimum_strength)
                && definition.actions.is_subset(&selection.actions)
                && selection
                    .failure_policies
                    .contains(&definition.failure_policy);
            if !compatible {
                return Err(SurfaceCompileError::IncompatibleHookRoute {
                    definition_id: definition.definition_id.clone(),
                });
            }
            entries.push(BoundRuntimeHookEntry {
                definition_id: definition.definition_id.clone(),
                point: definition.point,
                actions: definition.actions.clone(),
                delivered_strength: selection.delivered_strength,
                failure_policy: definition.failure_policy,
                required: definition.meta.requirement.is_required(),
                site: selection.site,
            });
        }
        Ok(RuntimeHookPlanBinding {
            thread_id,
            plan: BoundRuntimeHookPlan {
                revision: self.revision,
                digest: self.digest.clone(),
                entries,
            },
        })
    }
}

impl AgentSurfaceSnapshot {
    pub fn bind_profile(
        &self,
        thread_id: RuntimeThreadId,
        profile: &RuntimeProfile,
        hook_routes: impl IntoIterator<Item = HookRouteSelection>,
    ) -> Result<BoundAgentSurface, SurfaceCompileError> {
        let mut bound = Vec::new();
        for instruction in &self.context.instructions.entries {
            if profile.instruction.channels.contains(&instruction.channel) {
                bound.push(BoundSurfaceContribution {
                    key: instruction.meta.key.clone(),
                    route: SurfaceDeliveryRoute::Instruction(instruction.channel),
                    strength: SemanticStrength::ExactDurableBoundary,
                });
            } else if instruction.meta.requirement.is_required() {
                return Err(incompatible(
                    &instruction.meta,
                    "instruction channel is unavailable",
                ));
            }
        }
        let context_strength = context_profile_strength(profile.context.fidelity);
        for contribution in &self.context.contributions {
            if context_strength.satisfies(contribution.minimum_strength) {
                bound.push(BoundSurfaceContribution {
                    key: contribution.meta.key.clone(),
                    route: SurfaceDeliveryRoute::Context,
                    strength: context_strength,
                });
            } else if contribution.meta.requirement.is_required() {
                return Err(incompatible(
                    &contribution.meta,
                    "context fidelity cannot satisfy the required semantic strength",
                ));
            }
        }
        for tool in &self.tools.tools {
            if profile.tools.configuration_boundary < tool.configuration_boundary {
                if tool.meta.requirement.is_required() {
                    return Err(incompatible(
                        &tool.meta,
                        "tool configuration boundary cannot apply the required revision",
                    ));
                }
                continue;
            }
            let channel = preferred_tool_channel(&tool.allowed_channels, &profile.tools.channels);
            match channel {
                Some(channel) => bound.push(BoundSurfaceContribution {
                    key: tool.meta.key.clone(),
                    route: match channel {
                        ToolChannel::DirectCallback => SurfaceDeliveryRoute::DirectToolCallback,
                        ToolChannel::McpFacade => SurfaceDeliveryRoute::McpToolFacade,
                        ToolChannel::DriverNative => SurfaceDeliveryRoute::DriverNativeTool,
                    },
                    strength: SemanticStrength::ExactSynchronous,
                }),
                None if tool.meta.requirement.is_required() => {
                    return Err(incompatible(
                        &tool.meta,
                        "no callable tool channel is available",
                    ));
                }
                None => {}
            }
        }
        for server in &self.tools.mcp_servers {
            if profile.tools.channels.contains(&ToolChannel::McpFacade)
                || profile.tools.channels.contains(&ToolChannel::DriverNative)
            {
                let route = if profile.tools.channels.contains(&ToolChannel::McpFacade) {
                    SurfaceDeliveryRoute::McpToolFacade
                } else {
                    SurfaceDeliveryRoute::DriverNativeTool
                };
                bound.push(BoundSurfaceContribution {
                    key: server.meta.key.clone(),
                    route,
                    strength: SemanticStrength::ExactSynchronous,
                });
            } else if server.meta.requirement.is_required() {
                return Err(incompatible(&server.meta, "MCP delivery is unavailable"));
            }
        }
        if self.workspace.requirement.is_required()
            && (!self
                .workspace
                .capabilities
                .is_subset(&profile.workspace.capabilities)
                || !delivery_satisfies(
                    profile.workspace.mechanism,
                    self.workspace.minimum_mechanism,
                ))
        {
            return Err(SurfaceCompileError::IncompatibleContribution {
                key: "workspace".to_string(),
                reason: "workspace capabilities or delivery mechanism are insufficient".to_string(),
            });
        }
        if self
            .workspace
            .capabilities
            .is_subset(&profile.workspace.capabilities)
            && delivery_satisfies(
                profile.workspace.mechanism,
                self.workspace.minimum_mechanism,
            )
        {
            bound.push(BoundSurfaceContribution {
                key: "workspace".to_string(),
                route: SurfaceDeliveryRoute::Workspace(profile.workspace.mechanism),
                strength: SemanticStrength::ExactDurableBoundary,
            });
        }
        for skill in &self.skills {
            if profile.input.modalities.contains(&InputModality::Skill) {
                bound.push(BoundSurfaceContribution {
                    key: skill.meta.key.clone(),
                    route: SurfaceDeliveryRoute::NativeSkill,
                    strength: SemanticStrength::ExactDurableBoundary,
                });
            } else if skill.meta.requirement.is_required() {
                return Err(incompatible(
                    &skill.meta,
                    "native Skill ingress is unavailable",
                ));
            }
        }
        bound.extend(self.workflows.iter().map(|value| BoundSurfaceContribution {
            key: value.meta.key.clone(),
            route: SurfaceDeliveryRoute::HostPolicy,
            strength: SemanticStrength::ExactDurableBoundary,
        }));
        bound.extend(
            self.permissions
                .iter()
                .map(|value| BoundSurfaceContribution {
                    key: value.meta.key.clone(),
                    route: SurfaceDeliveryRoute::HostPolicy,
                    strength: SemanticStrength::ExactSynchronous,
                }),
        );
        bound.sort_by(|left, right| left.key.cmp(&right.key));
        let hook_plan = self.hook_plan.bind_runtime_plan(thread_id, hook_routes)?;
        let digest = SurfaceDigest::new(digest_json(&(&self.digest, &bound, &hook_plan))?)
            .map_err(|error| SurfaceCompileError::Digest(error.to_string()))?;
        Ok(BoundAgentSurface {
            source_revision: self.revision,
            source_digest: self.digest.clone(),
            digest,
            contributions: bound,
            hook_plan,
        })
    }
}

fn incompatible(meta: &ContributionMeta, reason: &str) -> SurfaceCompileError {
    SurfaceCompileError::IncompatibleContribution {
        key: meta.key.clone(),
        reason: reason.to_string(),
    }
}

fn preferred_tool_channel(
    requested: &BTreeSet<ToolChannel>,
    offered: &BTreeSet<ToolChannel>,
) -> Option<ToolChannel> {
    [
        ToolChannel::DirectCallback,
        ToolChannel::McpFacade,
        ToolChannel::DriverNative,
    ]
    .into_iter()
    .find(|channel| requested.contains(channel) && offered.contains(channel))
}

fn delivery_satisfies(offered: DeliveryMechanism, required: DeliveryMechanism) -> bool {
    delivery_rank(offered) <= delivery_rank(required)
        && !matches!(offered, DeliveryMechanism::PromptOnly)
}

fn delivery_rank(mechanism: DeliveryMechanism) -> u8 {
    match mechanism {
        DeliveryMechanism::Native => 0,
        DeliveryMechanism::HostAdaptedExact => 1,
        DeliveryMechanism::HostAdaptedBoundary => 2,
        DeliveryMechanism::Observed => 3,
        DeliveryMechanism::PromptOnly => 4,
    }
}

fn context_profile_strength(
    fidelity: agentdash_agent_runtime_contract::ContextFidelity,
) -> SemanticStrength {
    use agentdash_agent_runtime_contract::ContextFidelity;
    match fidelity {
        ContextFidelity::PlatformExact | ContextFidelity::DriverExact => {
            SemanticStrength::ExactDurableBoundary
        }
        ContextFidelity::AgentReplay => SemanticStrength::BoundaryAdapted,
        ContextFidelity::EventProjected | ContextFidelity::Opaque => SemanticStrength::ObservedOnly,
    }
}

fn validate_contribution(contribution: &CapabilityContribution) -> Result<(), SurfaceCompileError> {
    let meta = contribution.meta();
    for (field, value) in [
        ("key", meta.key.as_str()),
        ("source.layer", meta.source.layer.as_str()),
        ("source.key", meta.source.key.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field,
            });
        }
    }
    match contribution {
        CapabilityContribution::Instruction(value) if value.content.trim().is_empty() => {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "content",
            })
        }
        CapabilityContribution::Tool(value) => {
            if !value.parameters_schema.is_object() {
                return Err(SurfaceCompileError::InvalidToolSchema {
                    key: meta.key.clone(),
                });
            }
            for (field, field_value) in [
                ("runtime_name", value.runtime_name.as_str()),
                ("description", value.description.as_str()),
                ("capability_key", value.capability_key.as_str()),
                ("tool_path", value.tool_path.as_str()),
            ] {
                if field_value.trim().is_empty() {
                    return Err(SurfaceCompileError::EmptyField {
                        key: meta.key.clone(),
                        field,
                    });
                }
            }
            if value.allowed_channels.is_empty() {
                return Err(SurfaceCompileError::EmptyField {
                    key: meta.key.clone(),
                    field: "allowed_channels",
                });
            }
            let projector_key = match &value.protocol_projection {
                ToolProtocolProjection::Mcp { server_key } => Some(("server_key", server_key)),
                _ => None,
            };
            if let Some((field, value)) = projector_key {
                if value.trim().is_empty() {
                    return Err(SurfaceCompileError::InvalidToolProjector {
                        key: meta.key.clone(),
                        reason: format!("{field} must not be empty"),
                    });
                }
            }
            if value.parity_fixture_id.trim().is_empty() {
                return Err(SurfaceCompileError::InvalidToolProjector {
                    key: meta.key.clone(),
                    reason: "parity_fixture_id must not be empty".to_string(),
                });
            }
            Ok(())
        }
        CapabilityContribution::Mcp(value) if value.server_key.trim().is_empty() => {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "server_key",
            })
        }
        CapabilityContribution::Mcp(value)
            if value
                .credential_refs
                .iter()
                .any(|reference| reference.trim().is_empty()) =>
        {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "credential_refs",
            })
        }
        CapabilityContribution::Skill(value)
            if value.resource_ref.trim().is_empty() || value.description.trim().is_empty() =>
        {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "resource_ref or description",
            })
        }
        CapabilityContribution::Workflow(value) if value.workflow_key.trim().is_empty() => {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "workflow_key",
            })
        }
        CapabilityContribution::Permission(value)
            if value.policy_key.trim().is_empty()
                || value.capability_paths.is_empty()
                || value
                    .capability_paths
                    .iter()
                    .any(|path| path.trim().is_empty()) =>
        {
            Err(SurfaceCompileError::EmptyField {
                key: meta.key.clone(),
                field: "policy_key or capability_paths",
            })
        }
        CapabilityContribution::Hook(value) if value.actions.is_empty() => {
            Err(SurfaceCompileError::EmptyHookActions {
                definition_id: value.definition_id.clone(),
            })
        }
        CapabilityContribution::Hook(value) => {
            let valid_matcher = match &value.matcher {
                HookMatcher::Any => true,
                HookMatcher::ToolNames { names } => {
                    !names.is_empty() && names.iter().all(|name| !name.trim().is_empty())
                }
            };
            let valid_handler = match &value.handler {
                HookHandler::Builtin { key } => !key.trim().is_empty(),
                HookHandler::Script {
                    engine_key,
                    script_ref,
                    ..
                } => !engine_key.trim().is_empty() && !script_ref.trim().is_empty(),
            };
            if valid_matcher && valid_handler {
                Ok(())
            } else {
                Err(SurfaceCompileError::EmptyField {
                    key: meta.key.clone(),
                    field: "hook matcher or handler",
                })
            }
        }
        _ => Ok(()),
    }
}

fn digest_json(value: &impl Serialize) -> Result<String, SurfaceCompileError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| SurfaceCompileError::Digest(error.to_string()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
