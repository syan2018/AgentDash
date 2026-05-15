//! `SessionHub` 装配对象的内部 helper 与测试入口。
//!
//! 新的外部调用点必须依赖具体 service；Commit 8 会继续把内部业务实现
//! 下沉到明确的能力服务或依赖包。

use std::io;

use agentdash_agent_protocol::BackboneEnvelope;
use tokio::sync::broadcast;

#[cfg(test)]
use super::super::construction::SessionConstructionPlan;
use super::super::hub_support::*;
#[cfg(test)]
use super::super::prompt_pipeline::{SessionLaunchDeps, SessionLaunchExecutor};
use super::super::types::*;
use super::SessionHub;
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{ContextFrame, SharedHookSessionRuntime};

impl SessionHub {
    /// 启动时调用：将上次进程异常退出时残留的 `running` 状态修正为 `interrupted`。
    ///
    /// 统一通过事件投影驱动状态变更，不直接修改 SessionMeta。
    pub async fn recover_interrupted_sessions(&self) -> std::io::Result<()> {
        self.runtime_service().recover_interrupted_sessions().await
    }

    pub async fn create_session(&self, title: &str) -> std::io::Result<SessionMeta> {
        self.core_service().create_session(title).await
    }

    /// 创建会话并显式指定标题来源。
    /// Task 绑定的会话应使用 `TitleSource::User` 以阻止自动覆盖。
    pub async fn create_session_with_title_source(
        &self,
        title: &str,
        title_source: super::super::types::TitleSource,
    ) -> std::io::Result<SessionMeta> {
        self.core_service()
            .create_session_with_title_source(title, title_source)
            .await
    }

    pub async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        self.core_service().list_sessions().await
    }

    pub async fn get_session_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        self.core_service().get_session_meta(session_id).await
    }

    /// 批量获取多个 session 的 meta，并发读取。
    pub async fn get_session_metas_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionMeta>> {
        self.core_service()
            .get_session_metas_bulk(session_ids)
            .await
    }

    /// 批量查询 session 执行状态。
    ///
    /// 优先从内存 map 判断是否正在运行（无延迟），
    /// 否则读 meta 的 last_execution_status（持久化的终态）。
    pub async fn inspect_execution_states_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionExecutionState>> {
        self.core_service()
            .inspect_execution_states_bulk(session_ids)
            .await
    }

    pub async fn update_session_meta<F>(
        &self,
        session_id: &str,
        updater: F,
    ) -> std::io::Result<Option<SessionMeta>>
    where
        F: FnOnce(&mut SessionMeta),
    {
        self.core_service()
            .update_session_meta(session_id, updater)
            .await
    }

    /// 查询单个 session 的执行状态。
    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> std::io::Result<SessionExecutionState> {
        self.core_service()
            .inspect_session_execution_state(session_id)
            .await
    }

    pub async fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        self.core_service().delete_session(session_id).await
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<super::super::persistence::PersistedSessionEvent> {
        self.eventing_service().ensure_session(session_id).await
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        self.runtime_registry.hook_session_runtime(session_id).await
    }

    /// 向运行中 session 的 agent 注入一条 out-of-band user message。
    ///
    /// 走 connector 的 steering 队列（in-process connector 实现）。
    /// 消息会在下一次 LLM 调用前被合并到对话末尾，对 KV cache 前缀友好。
    pub async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        self.control_service()
            .push_session_notification(session_id, message)
            .await
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
            .upsert_pending_runtime_command(session_id, transition)
            .await?;
        Ok(())
    }

    pub async fn has_runtime_entry(&self, session_id: &str) -> bool {
        self.core_service().has_runtime_entry(session_id).await
    }

    pub async fn has_active_turn(&self, session_id: &str) -> bool {
        self.core_service().has_active_turn(session_id).await
    }

    pub async fn has_live_executor_session(&self, session_id: &str) -> bool {
        self.core_service()
            .has_live_executor_session(session_id)
            .await
    }

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
    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> std::io::Result<agentdash_agent_types::ProjectedTranscript> {
        self.eventing_service()
            .build_projected_transcript(session_id)
            .await
    }

    /// 测试专用入口：跳过 source provider，直接进入 prompt pipeline。
    ///
    /// 生产入口必须走 [`LaunchCommand`]，不能重新引入已组装 prompt 的旁路。
    #[cfg(test)]
    pub(crate) async fn start_prompt(
        &self,
        session_id: &str,
        construction: SessionConstructionPlan,
    ) -> Result<String, ConnectorError> {
        SessionLaunchExecutor::new(SessionLaunchDeps::from_hub(self))
            .execute_constructed_launch_for_test(session_id, construction)
            .await
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> io::Result<SessionEventSubscription> {
        self.eventing_service()
            .subscribe_with_history(session_id)
            .await
    }

    pub async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventSubscription> {
        self.eventing_service()
            .subscribe_after(session_id, after_seq)
            .await
    }

    pub async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<super::super::persistence::SessionEventPage> {
        self.eventing_service()
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    /// 向指定 session 主动注入补充通知（bridge 事件 / companion / canvas 等）。
    /// 直接 persist + broadcast，不经过 turn processor。
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
