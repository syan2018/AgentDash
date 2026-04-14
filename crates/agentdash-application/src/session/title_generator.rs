//! 会话标题自动生成
//!
//! 定义 `SessionTitleGenerator` trait 供外部注入 LLM 实现。
//! SessionHub 在首轮 prompt 时异步 spawn 标题生成任务。

use async_trait::async_trait;

use super::hub::SessionHub;
use super::types::TitleSource;

/// 标题生成抽象 — 由服务启动时注入具体 LLM 实现。
#[async_trait]
pub trait SessionTitleGenerator: Send + Sync {
    /// 根据用户首轮 prompt 文本生成简短会话标题。
    /// 返回 Ok(title) 或 Err(原因) — 失败时调用方保留原标题。
    async fn generate_title(&self, user_prompt: &str) -> Result<String, String>;
}

impl SessionHub {
    /// 首轮 prompt 后异步触发标题生成。
    /// 不阻塞主 prompt 流程；失败仅打 warn 日志。
    pub(super) fn spawn_title_generation(&self, session_id: String, user_prompt: String) {
        let Some(generator) = self.title_generator.clone() else {
            return;
        };
        let hub = self.clone();

        tokio::spawn(async move {
            let result = generator.generate_title(&user_prompt).await;

            match result {
                Ok(title) if !title.trim().is_empty() => {
                    let title = title.trim().to_string();
                    if let Err(error) = hub.apply_auto_title(&session_id, &title).await {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %error,
                            "自动标题写入失败"
                        );
                    }
                }
                Ok(_) => {
                    tracing::warn!(session_id = %session_id, "LLM 返回了空标题，保留原标题");
                }
                Err(reason) => {
                    tracing::warn!(
                        session_id = %session_id,
                        reason = %reason,
                        "自动标题生成失败，保留原标题"
                    );
                }
            }
        });
    }

    /// 将自动生成的标题写入 meta 并通过 SSE 广播。
    /// 仅当 `title_source` 仍为 `Auto` 时写入（防止用户已手动修改）。
    async fn apply_auto_title(&self, session_id: &str, title: &str) -> std::io::Result<()> {
        let updated = self
            .update_session_meta(session_id, |meta| {
                if meta.title_source == TitleSource::User {
                    return;
                }
                meta.title = title.to_string();
                meta.title_source = TitleSource::Auto;
            })
            .await?;

        if let Some(meta) = updated {
            self.broadcast_session_meta_updated(session_id, &meta)
                .await;
        }
        Ok(())
    }

    /// 用户手动修改标题 — 设置 `title_source = User`，后续不再自动覆盖。
    pub async fn set_user_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> std::io::Result<Option<super::types::SessionMeta>> {
        let updated = self
            .update_session_meta(session_id, |meta| {
                meta.title = title.to_string();
                meta.title_source = TitleSource::User;
            })
            .await?;

        if let Some(ref meta) = updated {
            self.broadcast_session_meta_updated(session_id, meta).await;
        }
        Ok(updated)
    }

    /// 通过 SSE 通道广播 `session_meta_updated` 事件。
    async fn broadcast_session_meta_updated(
        &self,
        session_id: &str,
        meta: &super::types::SessionMeta,
    ) {
        use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
        use agentdash_acp_meta::{AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, merge_agentdash_meta};

        let source = AgentDashSourceV1::new("agentdash-server", "system");
        let mut event = AgentDashEventV1::new("session_meta_updated");
        event.severity = Some("info".to_string());
        event.data = Some(serde_json::json!({
            "title": meta.title,
            "title_source": meta.title_source,
        }));

        let agentdash = AgentDashMetaV1::new()
            .source(Some(source))
            .event(Some(event));
        let acp_meta = merge_agentdash_meta(None, &agentdash)
            .expect("构造 session_meta_updated ACP Meta 不应失败");

        let info = SessionInfoUpdate::new().meta(acp_meta);
        let notification = SessionNotification::new(
            SessionId::new(session_id),
            SessionUpdate::SessionInfoUpdate(info),
        );
        let _ = self.persist_notification(session_id, notification).await;
    }
}
