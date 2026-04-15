use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::routine::RoutineExecutor;
use agentdash_domain::routine::RoutineTriggerConfig;

/// Cron 调度器的外部控制句柄。
///
/// 当 Routine 的 cron 配置发生变更（新增 / 修改 / 删除 Routine），
/// 调用 `notify_config_changed()` 即可触发调度器重新加载条目并做 diff merge。
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

    /// 通知调度器 Routine cron 配置已变更，触发异步重新加载。
    pub fn notify_config_changed(&self) {
        self.notify.notify_one();
    }
}

struct CronEntry {
    routine_id: Uuid,
    /// 原始 cron 表达式，用于 reload 时比对是否变更
    cron_expr: String,
    schedule: Schedule,
    next_fire: chrono::DateTime<Utc>,
}

/// 用于 diff merge 的 entry 唯一键（以 routine_id 为唯一标识）
type EntryKey = Uuid;

fn entry_key(entry: &CronEntry) -> EntryKey {
    entry.routine_id
}

/// 启动 cron 调度器后台任务。
///
/// 从 Routine 表加载所有 `trigger_config.type = "scheduled"` 且 `enabled = true` 的条目，
/// 在后台 loop 中按 cron 表达式触发对应 Routine 执行。
///
/// 当 `handle` 收到配置变更通知时，调度器会重新加载条目并 diff merge。
pub async fn spawn_cron_scheduler(
    repos: RepositorySet,
    executor: Arc<RoutineExecutor>,
    handle: &CronSchedulerHandle,
) {
    let entries = match load_cron_entries(&repos).await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::error!("加载 Routine cron 调度条目失败，调度器不启动: {err}");
            return;
        }
    };

    if entries.is_empty() {
        tracing::info!(
            "未发现配置了 cron_schedule 的 Routine，调度器以空列表启动，等待 Routine 创建"
        );
        // 即使初始无条目也继续监听，这样后续新增 Routine 能动态生效
    }

    tracing::info!(
        count = entries.len(),
        "Cron 调度器已加载 {} 条 Routine 调度条目，启动后台循环",
        entries.len()
    );

    let notify = handle.notify.clone();
    tokio::spawn(async move {
        run_cron_loop(entries, repos, executor, notify).await;
    });
}

async fn load_cron_entries(repos: &RepositorySet) -> Result<Vec<CronEntry>, String> {
    let routines = repos
        .routine_repo
        .list_enabled_by_trigger_type("scheduled")
        .await
        .map_err(|e| format!("查询 scheduled routines 失败: {e}"))?;

    let mut entries = Vec::new();

    for routine in &routines {
        let RoutineTriggerConfig::Scheduled {
            ref cron_expression,
            ..
        } = routine.trigger_config
        else {
            continue;
        };

        let schedule = match Schedule::from_str(cron_expression) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(
                    routine_id = %routine.id,
                    routine_name = %routine.name,
                    cron = cron_expression,
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
            routine_id = %routine.id,
            routine_name = %routine.name,
            cron = cron_expression,
            next_fire = %next_fire,
            "注册 Routine cron 调度条目"
        );

        entries.push(CronEntry {
            routine_id: routine.id,
            cron_expr: cron_expression.clone(),
            schedule,
            next_fire,
        });
    }

    Ok(entries)
}

fn merge_entries(existing: Vec<CronEntry>, fresh: Vec<CronEntry>) -> Vec<CronEntry> {
    let mut existing_map: HashMap<EntryKey, CronEntry> =
        existing.into_iter().map(|e| (entry_key(&e), e)).collect();

    let mut result = Vec::with_capacity(fresh.len());
    let mut kept = 0usize;
    let mut updated = 0usize;
    let mut added = 0usize;

    for mut new_entry in fresh {
        let key = entry_key(&new_entry);
        if let Some(old) = existing_map.remove(&key) {
            if old.cron_expr == new_entry.cron_expr {
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
        kept,
        updated,
        added,
        removed,
        total = result.len(),
        "Routine cron 调度条目 diff merge 完成"
    );

    result
}

const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

async fn run_cron_loop(
    mut entries: Vec<CronEntry>,
    repos: RepositorySet,
    executor: Arc<RoutineExecutor>,
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
                        tracing::warn!("热更新 Routine cron 条目失败，保持现有调度: {err}");
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
                routine_id = %entry.routine_id,
                "Routine cron 触发"
            );

            let executor = executor.clone();
            let routine_id = entry.routine_id;

            tokio::spawn(async move {
                if let Err(err) = executor.fire_scheduled(routine_id).await {
                    tracing::warn!(
                        routine_id = %routine_id,
                        "Routine cron 触发失败: {err}"
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
