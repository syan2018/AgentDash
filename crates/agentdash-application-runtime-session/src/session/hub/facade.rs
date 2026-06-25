//! `SessionRuntimeInner` 装配对象的内部 helper 与测试入口。
//!
//! 新的外部调用点必须依赖具体 service；Commit 8 会继续把内部业务实现
//! 下沉到明确的能力服务或依赖包。

use std::io;

#[cfg(test)]
use super::super::types::SessionMeta;
use super::super::{AgentFrameTransitionRecord, RuntimeDeliveryCommand};
use super::SessionRuntimeInner;
use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_spi::hooks::ContextFrame;
#[cfg(test)]
use agentdash_spi::session_persistence::SessionStoreResult;

impl SessionRuntimeInner {
    #[cfg(test)]
    pub async fn create_session(&self, title: &str) -> SessionStoreResult<SessionMeta> {
        self.core_service().create_session(title).await
    }

    #[cfg(test)]
    pub async fn ensure_session(&self, session_id: &str) {
        let _ = self.eventing_service().ensure_session(session_id).await;
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
