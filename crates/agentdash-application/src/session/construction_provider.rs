//! Session construction provider 契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! construction 逻辑才能拿到 owner context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"——auto-resume 拿到
//! 的是一个未补齐 owner 的 prompt，Agent 丢失工作流背景后容易复读。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionRuntimeInner::set_session_construction_provider`
//! 注入。SessionRuntimeInner 内部 follow-up 一律先经过 construction provider，与 HTTP 主通道对齐。

use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityDefinition, WorkflowGraph, LifecycleRun, AgentProcedure,
};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;
use uuid::Uuid;

use super::construction::SessionConstructionPlan;
use super::launch::LaunchCommand;
use super::runtime_commands::RuntimeCommandRecord;
use super::types::SessionMeta;
use crate::workflow::runtime_launch::RuntimeLaunchRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLaunchPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone, Default)]
pub struct TaskLaunchSource {
    pub phase: Option<TaskLaunchPhase>,
    pub override_prompt: Option<String>,
    pub additional_prompt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineLaunchSource {
    pub routine_id: Uuid,
    pub execution_id: Uuid,
    pub trigger_source: String,
    pub entity_key: Option<String>,
}

#[derive(Clone)]
pub struct CompanionLaunchWorkflowSource {
    pub run: LifecycleRun,
    pub lifecycle: WorkflowGraph,
    pub activity: ActivityDefinition,
    pub workflow: Option<AgentProcedure>,
}

#[derive(Clone)]
pub struct CompanionLaunchSource {
    pub parent_session_id: String,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub companion_executor_config: agentdash_spi::AgentConfig,
    pub dispatch_prompt: String,
    pub workflow: Option<CompanionLaunchWorkflowSource>,
}

#[derive(Clone)]
pub struct SessionConstructionProviderInput {
    pub session_id: String,
    pub command: LaunchCommand,
    pub session_meta: SessionMeta,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
}

/// Session launch 的 construction provider 契约。
///
/// 实现方负责从 session 元数据推断 owner / workspace / capability / context 等
/// 运行时字段，产出 `RuntimeLaunchRequest` 驱动 connector 启动。
#[async_trait]
pub trait SessionConstructionProvider: Send + Sync {
    /// 产出 RuntimeLaunchRequest —— session launch 的唯一输入。
    async fn build_frame_construction(
        &self,
        input: SessionConstructionProviderInput,
    ) -> Result<RuntimeLaunchRequest, ConnectorError>;
}

/// 从 `SessionConstructionPlan` 投影到 `RuntimeLaunchRequest`。
pub fn runtime_launch_request_from_construction_plan(
    plan: &SessionConstructionPlan,
) -> RuntimeLaunchRequest {
    use crate::workflow::runtime_launch::LaunchResolutionTrace;

    let typed_capability_state = plan.projections.capability_state.clone();
    let typed_vfs = plan.active_vfs().cloned();
    let typed_mcp_servers = plan.projections.mcp_servers.clone();
    let capability_surface = typed_capability_state
        .as_ref()
        .and_then(|s| serde_json::to_value(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let vfs_surface = typed_vfs
        .as_ref()
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(serde_json::Value::Null);
    let mcp_surface = if typed_mcp_servers.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::to_value(&typed_mcp_servers).unwrap_or(serde_json::Value::Null)
    };

    RuntimeLaunchRequest {
        agent_id: Uuid::nil(),
        frame_id: Uuid::nil(),
        frame_revision: 0,
        procedure_ref: None,
        capability_surface,
        context_slice: plan
            .context
            .bundle
            .as_ref()
            .and_then(|b| serde_json::to_value(b.bundle_id).ok())
            .unwrap_or(serde_json::Value::Null),
        vfs_surface,
        mcp_surface,
        runtime_session_id: None,
        graph_instance_id: None,
        activity_key: None,
        executor_config: plan.execution_profile.executor_config.clone(),
        working_directory: plan.workspace.working_directory.clone(),
        prompt_blocks: plan.prompt.prompt_blocks.clone(),
        environment_variables: plan.prompt.environment_variables.clone(),
        identity: plan.identity.identity.clone(),
        terminal_hook_effect_binding: plan.effects.terminal_hook_effect_binding.clone(),
        discovered_guidelines: plan.projections.discovered_guidelines.clone(),
        extension_runtime: plan.projections.extension_runtime.clone(),
        context_bundle: plan.context.bundle.clone(),
        typed_capability_state,
        typed_vfs,
        typed_mcp_servers,
        continuation_context_frame: plan.context.continuation_context_frame.clone(),
        base_capability_state: plan.resolution.runtime_base_capability_state.clone(),
        resolution_trace: LaunchResolutionTrace {
            vfs_source: plan.resolution.vfs_source.clone(),
            mcp_source: plan.resolution.mcp_source.clone(),
            capability_source: plan.resolution.capability_source.clone(),
            pending_overlay_applied: plan.resolution.pending_overlay_applied,
        },
    }
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedSessionConstructionProvider = Arc<dyn SessionConstructionProvider>;
