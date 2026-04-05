use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use uuid::Uuid;

use super::config::{AgentSchedulingConfig, CronSessionMode};
use crate::repository_set::RepositorySet;

/// Cron 调度器在触发时刻回调的目标 —— 由 API/Host 层实现具体的 session 创建 / prompt 发送。
#[async_trait::async_trait]
pub trait CronTriggerTarget: Send + Sync + 'static {
    /// 唤醒指定 project 下的指定 agent。
    /// 实现方负责：查找/创建 session、构建 prompt、处理重入（agent 仍在运行时跳过）。
    async fn trigger_agent_session(
        &self,
        project_id: Uuid,
        agent_id: Uuid,
        session_mode: CronSessionMode,
    ) -> Result<(), String>;
}

struct CronEntry {
    project_id: Uuid,
    agent_id: Uuid,
    schedule: Schedule,
    session_mode: CronSessionMode,
    next_fire: chrono::DateTime<Utc>,
}

/// 启动 cron 调度器后台任务。
///
/// 启动时扫描所有 Agent 配置，找出带 cron_schedule 的条目，
/// 然后在后台 loop 中按 cron 表达式触发对应 Project Agent session。
pub async fn spawn_cron_scheduler(
    repos: RepositorySet,
    target: Arc<dyn CronTriggerTarget>,
) {
    let entries = match load_cron_entries(&repos).await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::error!("加载 cron 调度条目失败，调度器不启动: {err}");
            return;
        }
    };

    if entries.is_empty() {
        tracing::info!("未发现配置了 cron_schedule 的 Agent，调度器休眠");
        return;
    }

    tracing::info!(
        count = entries.len(),
        "Cron 调度器已加载 {} 条调度条目，启动后台循环",
        entries.len()
    );

    tokio::spawn(async move {
        run_cron_loop(entries, target).await;
    });
}

async fn load_cron_entries(repos: &RepositorySet) -> Result<Vec<CronEntry>, String> {
    let agents = repos
        .agent_repo
        .list_all()
        .await
        .map_err(|e| format!("查询 agents 失败: {e}"))?;

    let agent_map: HashMap<Uuid, _> = agents.into_iter().map(|a| (a.id, a)).collect();

    let projects = repos
        .project_repo
        .list_all()
        .await
        .map_err(|e| format!("查询 projects 失败: {e}"))?;

    let mut entries = Vec::new();

    for project in &projects {
        let links = repos
            .agent_link_repo
            .list_by_project(project.id)
            .await
            .map_err(|e| format!("查询 project {} 的 agent links 失败: {e}", project.id))?;

        for link in &links {
            let Some(agent) = agent_map.get(&link.agent_id) else {
                continue;
            };
            let merged_config = link.merged_config(&agent.base_config);
            let Some(scheduling) = AgentSchedulingConfig::from_merged_config(&merged_config)
            else {
                continue;
            };
            if !scheduling.has_cron() {
                continue;
            }

            let cron_expr = scheduling.cron_schedule.as_deref().unwrap();
            let schedule = match Schedule::from_str(cron_expr) {
                Ok(s) => s,
                Err(err) => {
                    tracing::warn!(
                        project_id = %project.id,
                        agent_id = %agent.id,
                        cron = cron_expr,
                        "无效的 cron 表达式，跳过: {err}"
                    );
                    continue;
                }
            };

            let next_fire = match schedule.upcoming(Utc).next() {
                Some(t) => t,
                None => continue,
            };

            tracing::info!(
                project_id = %project.id,
                agent_name = %agent.name,
                cron = cron_expr,
                next_fire = %next_fire,
                "注册 cron 调度条目"
            );

            entries.push(CronEntry {
                project_id: project.id,
                agent_id: agent.id,
                schedule,
                session_mode: scheduling.cron_session_mode,
                next_fire,
            });
        }
    }

    Ok(entries)
}

const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

async fn run_cron_loop(mut entries: Vec<CronEntry>, target: Arc<dyn CronTriggerTarget>) {
    loop {
        tokio::time::sleep(TICK_INTERVAL).await;
        let now = Utc::now();

        for entry in entries.iter_mut() {
            if now < entry.next_fire {
                continue;
            }

            tracing::info!(
                project_id = %entry.project_id,
                agent_id = %entry.agent_id,
                "Cron 触发: 唤醒 Agent session"
            );

            let target = target.clone();
            let project_id = entry.project_id;
            let agent_id = entry.agent_id;
            let mode = entry.session_mode;

            tokio::spawn(async move {
                if let Err(err) = target
                    .trigger_agent_session(project_id, agent_id, mode)
                    .await
                {
                    tracing::warn!(
                        project_id = %project_id,
                        agent_id = %agent_id,
                        "Cron 触发失败: {err}"
                    );
                }
            });

            entry.next_fire = entry
                .schedule
                .upcoming(Utc)
                .next()
                .unwrap_or_else(|| now + chrono::Duration::hours(24));
        }
    }
}
