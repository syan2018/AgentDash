//! 通用启动对账管线
//!
//! 服务重启后按固定顺序执行：Session 恢复 → Task view 投影 → Infrastructure。
//! Phase 之间存在依赖：Task view 投影依赖 Session 先完成（否则会误判 session 仍在运行）。
//!
//! **定位说明**：本管线只覆盖 projection 方向（session/lifecycle 真相源 → Task view）。
//! 运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。

use std::sync::Arc;

use crate::session::SessionHub;
use crate::task::view_projector::project_task_views_on_boot;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{LifecycleDefinitionRepository, LifecycleRunRepository};

/// 启动对账管线的依赖集合
///
/// M2-c：Task view 改为"从 LifecycleRun/step state 反投影"（Scheme A），
/// 不再依赖 `TaskSessionStateReader` / `SessionBindingRepository`。
pub struct BootReconcileDeps {
    pub session_hub: SessionHub,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub lifecycle_def_repo: Arc<dyn LifecycleDefinitionRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

/// 单阶段对账结果
#[derive(Debug)]
pub struct PhaseReport {
    pub phase: &'static str,
    pub reconciled: usize,
    pub errors: Vec<String>,
}

/// 完整管线执行结果
#[derive(Debug)]
pub struct BootReconcileReport {
    pub phases: Vec<PhaseReport>,
}

impl BootReconcileReport {
    pub fn total_reconciled(&self) -> usize {
        self.phases.iter().map(|p| p.reconciled).sum()
    }

    pub fn has_errors(&self) -> bool {
        self.phases.iter().any(|p| !p.errors.is_empty())
    }
}

/// 执行完整的启动对账管线。
///
/// 阶段执行顺序固定且不可跳过：
/// 1. **Session 恢复** — 将残留 running 状态的 session 标记为 interrupted
/// 2. **Task view 投影** — 根据 LifecycleRun/step state 反投影 Task view
/// 3. **Infrastructure 恢复** — 预留（定时触发器重建等）
pub async fn run_boot_reconcile(deps: &BootReconcileDeps) -> BootReconcileReport {
    let mut phases = Vec::with_capacity(3);

    // ── Phase 1: Session Reconcile ──────────────────────────
    let session_report = run_session_reconcile(&deps.session_hub).await;
    phases.push(session_report);

    // ── Phase 2: Task View Projection ───────────────────────
    let task_report = run_task_view_projection(deps).await;
    phases.push(task_report);

    // ── Phase 3: Infrastructure Restore ─────────────────────
    // 目前仅占位，后续 tick-loop 触发器重建等逻辑在此扩展
    phases.push(PhaseReport {
        phase: "infrastructure_restore",
        reconciled: 0,
        errors: Vec::new(),
    });

    let report = BootReconcileReport { phases };

    tracing::info!(
        total_reconciled = report.total_reconciled(),
        has_errors = report.has_errors(),
        "启动对账管线执行完成"
    );

    report
}

async fn run_session_reconcile(session_hub: &SessionHub) -> PhaseReport {
    match session_hub.recover_interrupted_sessions().await {
        Ok(()) => {
            tracing::info!("Phase 1 (Session Recovery) 完成");
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0, // recover_interrupted_sessions 暂未返回计数
                errors: Vec::new(),
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "Phase 1 (Session Recovery) 出错（非致命）");
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}

async fn run_task_view_projection(deps: &BootReconcileDeps) -> PhaseReport {
    match project_task_views_on_boot(
        &deps.project_repo,
        &deps.state_change_repo,
        &deps.story_repo,
        &deps.lifecycle_def_repo,
        &deps.lifecycle_run_repo,
    )
    .await
    {
        Ok(()) => {
            tracing::info!("Phase 2 (Task View Projection) 完成");
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                errors: Vec::new(),
            }
        }
        Err(err) => {
            tracing::error!(error = %err, "Phase 2 (Task View Projection) 失败");
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}
