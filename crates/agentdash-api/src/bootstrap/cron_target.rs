use agentdash_application::scheduling::config::CronSessionMode;
use agentdash_application::scheduling::cron_scheduler::CronTriggerTarget;
use agentdash_application::session::SessionHub;
use uuid::Uuid;

/// API 层 CronTriggerTarget 实现 — 将 cron 触发转化为 Project Agent session prompt。
///
/// 当前为最小实现（仅日志 + session 查询），后续需要对接完整的
/// companion session 创建/复用逻辑。
pub struct AppCronTriggerTarget {
    pub session_hub: SessionHub,
}

#[async_trait::async_trait]
impl CronTriggerTarget for AppCronTriggerTarget {
    async fn trigger_agent_session(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
        session_mode: CronSessionMode,
    ) -> Result<(), String> {
        // TODO: 对接 companion session 创建/复用，发送 cron-triggered prompt。
        // 当前仅记录触发事件，不实际创建 session。
        tracing::info!(
            project_id = %project_id,
            agent_id = %agent_id,
            session_mode = ?session_mode,
            "Cron trigger received — session dispatch not yet wired"
        );
        Ok(())
    }
}
