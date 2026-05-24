//! `SessionRuntimeInner` 装配对象的内部 helper 与测试入口。
//!
//! 新的外部调用点必须依赖具体 service；Commit 8 会继续把内部业务实现
//! 下沉到明确的能力服务或依赖包。

use std::io;

#[cfg(test)]
#[cfg(test)]
use super::super::construction::SessionConstructionPlan;
#[cfg(test)]
use super::super::launch::{SessionLaunchDeps, SessionLaunchOrchestrator};
use super::super::types::*;
use super::SessionRuntimeInner;
use agentdash_agent_protocol::BackboneEnvelope;
#[cfg(test)]
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{ContextFrame, SharedHookSessionRuntime};

impl SessionRuntimeInner {
    #[cfg(test)]
    pub async fn create_session(&self, title: &str) -> std::io::Result<SessionMeta> {
        self.core_service().create_session(title).await
    }

    #[cfg(test)]
    pub async fn get_session_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        self.core_service().get_session_meta(session_id).await
    }

    /// 查询单个 session 的执行状态。
    #[cfg(test)]
    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> std::io::Result<SessionExecutionState> {
        self.core_service()
            .inspect_session_execution_state(session_id)
            .await
    }

    #[cfg(test)]
    pub async fn ensure_session(&self, session_id: &str) {
        let _ = self.eventing_service().ensure_session(session_id).await;
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        self.runtime_registry.hook_session_runtime(session_id).await
    }

    /// 持久化一条结构化能力状态变更事件。
    ///
    /// 这是 UI / 审计 / 回放的事实源；connector steering 消息只是 live agent 的
    /// 尽力投递通道。
    pub(crate) async fn emit_capability_state_changed(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        value: serde_json::Value,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        self.eventing_service()
            .emit_capability_state_changed(session_id, turn_id, value)
            .await
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

    pub(crate) async fn enqueue_pending_capability_state_transition(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<()> {
        self.stores
            .runtime_commands
            .upsert_runtime_command_request(session_id, transition)
            .await?;
        Ok(())
    }

    pub async fn has_live_executor_session(&self, session_id: &str) -> bool {
        self.core_service()
            .has_live_executor_session(session_id)
            .await
    }

    #[cfg(test)]
    pub async fn mark_owner_bootstrap_pending(&self, session_id: &str) -> std::io::Result<()> {
        self.core_service()
            .mark_owner_bootstrap_pending(session_id)
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
    pub(crate) async fn start_prompt(
        &self,
        session_id: &str,
        construction: SessionConstructionPlan,
    ) -> Result<String, ConnectorError> {
        SessionLaunchOrchestrator::new(SessionLaunchDeps::from_inner(self))
            .launch_with_construction_for_test(session_id, construction)
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
