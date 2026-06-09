//! `SessionRuntimeInner` 装配对象的内部 helper 与测试入口。
//!
//! 新的外部调用点必须依赖具体 service；Commit 8 会继续把内部业务实现
//! 下沉到明确的能力服务或依赖包。

use std::io;

#[cfg(test)]
#[allow(deprecated)]
use super::super::construction::RuntimeContextInspectionPlan;
#[cfg(test)]
use super::super::launch::{SessionLaunchDeps, SessionLaunchOrchestrator};
#[cfg(test)]
use super::super::types::{SessionExecutionState, SessionMeta};
use super::super::{AgentFrameTransitionRecord, RuntimeDeliveryCommand};
use super::SessionRuntimeInner;
#[cfg(test)]
use crate::workflow::runtime_launch::{
    FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface, LaunchResolutionTrace,
};
use agentdash_agent_protocol::BackboneEnvelope;
#[cfg(test)]
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{ContextFrame, SharedHookRuntime};
#[cfg(test)]
use agentdash_spi::session_persistence::SessionStoreResult;

impl SessionRuntimeInner {
    #[cfg(test)]
    pub async fn create_session(&self, title: &str) -> SessionStoreResult<SessionMeta> {
        self.core_service().create_session(title).await
    }

    /// 查询单个 session 的执行状态。
    #[cfg(test)]
    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<SessionExecutionState> {
        self.core_service()
            .inspect_session_execution_state(session_id)
            .await
    }

    #[cfg(test)]
    pub async fn ensure_session(&self, session_id: &str) {
        let _ = self.eventing_service().ensure_session(session_id).await;
    }

    /// Delivery adapter: 根据 RuntimeSession id 查找已缓存的 hook runtime。
    ///
    /// 业务控制路径应使用 `SessionHookService::get_hook_runtime_for_target`，
    /// 此方法仅供 hub 内部 adapter 场景使用。
    pub(crate) async fn get_hook_runtime_by_delivery_session(
        &self,
        session_id: &str,
    ) -> Option<SharedHookRuntime> {
        self.runtime_registry.hook_runtime(session_id).await
    }

    pub(crate) async fn emit_context_frame(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        notice: &ContextFrame,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        self.eventing_service()
            .emit_context_frame(session_id, turn_id, notice)
            .await
    }

    pub(crate) async fn enqueue_runtime_delivery_command(
        &self,
        delivery_runtime_session_id: &str,
        delivery: RuntimeDeliveryCommand,
        frame_transition: AgentFrameTransitionRecord,
    ) -> io::Result<()> {
        self.stores
            .runtime_commands
            .upsert_runtime_delivery_command(
                delivery_runtime_session_id,
                delivery,
                frame_transition,
            )
            .await?;
        Ok(())
    }

    pub async fn has_live_executor_session(&self, session_id: &str) -> bool {
        self.core_service()
            .has_live_executor_session(session_id)
            .await
    }

    /// 从持久化事件重建投影 transcript。
    ///
    /// 消费者自选渲染方式：
    /// - `.into_messages()` → 执行器原生 session restore
    /// - `build_continuation_context_frame(&transcript, owner)` → continuation frame 注入
    #[cfg(test)]
    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> std::io::Result<agentdash_agent_types::ProjectedTranscript> {
        self.eventing_service()
            .build_projected_transcript(session_id)
            .await
    }

    /// 测试专用入口：跳过 source provider，直接进入 launch stage runner。
    ///
    /// 生产入口必须走 [`LaunchCommand`]，不能重新引入已组装 prompt 的旁路。
    #[cfg(test)]
    #[allow(deprecated)]
    pub(crate) async fn start_prompt(
        &self,
        session_id: &str,
        construction: RuntimeContextInspectionPlan,
    ) -> Result<String, ConnectorError> {
        let envelope = envelope_from_construction(construction);
        SessionLaunchOrchestrator::new(SessionLaunchDeps::from_inner(self))
            .launch_with_envelope_for_test(session_id, envelope)
            .await
    }

    /// 向指定 session 主动注入补充通知（bridge 事件 / companion / canvas 等）。
    /// 直接 persist + broadcast，不经过 turn processor。
    #[cfg(test)]
    pub async fn inject_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> std::io::Result<()> {
        self.eventing_service()
            .inject_notification(session_id, envelope)
            .await
    }

    pub(crate) async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        self.eventing_service()
            .persist_notification(session_id, envelope)
            .await
    }
}

#[cfg(test)]
#[allow(deprecated)]
fn envelope_from_construction(construction: RuntimeContextInspectionPlan) -> FrameLaunchEnvelope {
    let executor_config = construction
        .execution_profile
        .executor_config
        .unwrap_or_else(|| agentdash_spi::AgentConfig::new("test"));
    let capability_state = construction
        .projections
        .capability_state
        .unwrap_or_default();
    let vfs = construction.surface.vfs.unwrap_or_default();
    let working_directory = construction
        .workspace
        .working_directory
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));

    FrameLaunchEnvelope {
        surface: FrameRuntimeSurface {
            agent_id: uuid::Uuid::new_v4(),
            frame_id: uuid::Uuid::new_v4(),
            frame_revision: 1,
            capability_surface: serde_json::Value::Null,
            context_slice: serde_json::Value::Null,
            vfs_surface: serde_json::Value::Null,
            mcp_surface: serde_json::Value::Null,
            runtime_session_id: Some(construction.session_id.clone()),
        },
        intent: FrameLaunchIntent {
            input: construction.prompt.input,
            environment_variables: construction.prompt.environment_variables,
            identity: None,
            terminal_hook_effect_binding: None,
            discovered_guidelines: construction.projections.discovered_guidelines,
        },
        working_directory,
        executor_config,
        capability_state,
        vfs,
        mcp_servers: construction.projections.mcp_servers,
        context_bundle: construction.context.bundle,
        continuation_context_frame: None,
        base_capability_state: construction.resolution.runtime_base_capability_state,
        resolution_trace: LaunchResolutionTrace {
            vfs_source: construction.resolution.vfs_source,
            mcp_source: construction.resolution.mcp_source,
            capability_source: construction.resolution.capability_source,
            pending_overlay_applied: construction.resolution.pending_overlay_applied,
        },
    }
}
