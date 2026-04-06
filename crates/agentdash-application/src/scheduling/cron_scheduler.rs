use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tokio::sync::Notify;
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

/// Cron 调度器的外部控制句柄。
///
/// 当 Agent 的 cron 配置发生变更（新增 / 修改 / 删除 ProjectAgentLink 或 Agent base_config），
/// 调用 `notify_config_changed()` 即可触发调度器重新加载条目并做 diff merge。
///
/// 句柄始终可用——即使调度器因初始无条目而未启动，通知也只是被丢弃（无 listener）。
#[derive(Clone)]
pub struct CronSchedulerHandle {
    notify: Arc<Notify>,
}

impl CronSchedulerHandle {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }

    /// 通知调度器 Agent cron 配置已变更，触发异步重新加载。
    pub fn notify_config_changed(&self) {
        self.notify.notify_one();
    }
}

struct CronEntry {
    project_id: Uuid,
    agent_id: Uuid,
    /// 原始 cron 表达式，用于 reload 时比对是否变更
    cron_expr: String,
    schedule: Schedule,
    session_mode: CronSessionMode,
    next_fire: chrono::DateTime<Utc>,
}

/// 用于 diff merge 的 entry 唯一键
type EntryKey = (Uuid, Uuid);

fn entry_key(entry: &CronEntry) -> EntryKey {
    (entry.project_id, entry.agent_id)
}

/// 启动 cron 调度器后台任务。
///
/// 启动时扫描所有 Agent 配置，找出带 cron_schedule 的条目，
/// 然后在后台 loop 中按 cron 表达式触发对应 Project Agent session。
///
/// 当 `handle` 收到配置变更通知时，调度器会重新加载条目并 diff merge，
/// 已有且表达式未变的条目保留 `next_fire` 时间，避免重复触发。
///
/// 如果初始无条目，调度器不启动（后续新增的条目需重启服务才能生效）。
pub async fn spawn_cron_scheduler(
    repos: RepositorySet,
    target: Arc<dyn CronTriggerTarget>,
    handle: &CronSchedulerHandle,
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

    let notify = handle.notify.clone();
    tokio::spawn(async move {
        run_cron_loop(entries, repos, target, notify).await;
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
                cron_expr: cron_expr.to_string(),
                schedule,
                session_mode: scheduling.cron_session_mode,
                next_fire,
            });
        }
    }

    Ok(entries)
}

/// 将新加载的条目与现有条目做 diff merge：
/// - cron 表达式未变的条目保留 `next_fire`（避免重复触发）
/// - cron 表达式变更的条目使用新的 `next_fire`
/// - 新增的条目直接加入
/// - 不再存在的条目自动移除
fn merge_entries(existing: Vec<CronEntry>, fresh: Vec<CronEntry>) -> Vec<CronEntry> {
    let mut existing_map: HashMap<EntryKey, CronEntry> = existing
        .into_iter()
        .map(|e| (entry_key(&e), e))
        .collect();

    let mut result = Vec::with_capacity(fresh.len());
    let mut kept = 0usize;
    let mut updated = 0usize;
    let mut added = 0usize;

    for mut new_entry in fresh {
        let key = entry_key(&new_entry);
        if let Some(old) = existing_map.remove(&key) {
            if old.cron_expr == new_entry.cron_expr && old.session_mode == new_entry.session_mode {
                new_entry.next_fire = old.next_fire;
                kept += 1;
            } else {
                updated += 1;
            }
        } else {
            added += 1;
        }
        result.push(new_entry);
    }

    let removed = existing_map.len();
    tracing::info!(
        kept, updated, added, removed,
        total = result.len(),
        "Cron 调度条目 diff merge 完成"
    );

    result
}

const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

async fn run_cron_loop(
    mut entries: Vec<CronEntry>,
    repos: RepositorySet,
    target: Arc<dyn CronTriggerTarget>,
    notify: Arc<Notify>,
) {
    let mut tick = tokio::time::interval(TICK_INTERVAL);
    tick.tick().await; // 消费立即触发的第一个 tick

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = notify.notified() => {
                match load_cron_entries(&repos).await {
                    Ok(fresh) => {
                        entries = merge_entries(entries, fresh);
                    }
                    Err(err) => {
                        tracing::warn!("热更新 cron 条目失败，保持现有调度: {err}");
                    }
                }
            }
        }

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
