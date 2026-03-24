use agentdash_domain::workflow::{
    WorkflowDefinition, WorkflowDefinitionRepository, WorkflowPhaseDefinition, WorkflowRun,
    WorkflowRunRepository, WorkflowRunStatus, WorkflowTargetKind,
};
use serde::Serialize;
use uuid::Uuid;

use super::binding::{BindingResolutionContext, ResolvedWorkflowBinding, resolve_binding};
use super::completion::completion_mode_tag;
use super::run::select_active_run;

/// 统一的 active workflow 运行时投影。
///
/// 所有需要读取当前 workflow 状态的消费者都从同一个 projection 取数据：
/// - hook snapshot 构建
/// - task / story / project bootstrap
/// - session context snapshot（前端查询）
/// - frontend workflow runtime 展示
#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: WorkflowRun,
    pub definition: WorkflowDefinition,
    pub phase: WorkflowPhaseDefinition,
    pub target: WorkflowTargetSummary,
    pub resolved_bindings: Vec<ResolvedWorkflowBinding>,
}

/// workflow 目标摘要（与 session owner 可能不同层级）。
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTargetSummary {
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
    pub target_label: Option<String>,
}

/// 前端可序列化的 workflow projection 快照。
/// 由 `ActiveWorkflowProjection` 派生，不携带完整的 domain 对象。
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowProjectionSnapshot {
    pub run_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_key: String,
    pub workflow_name: String,
    pub run_status: String,
    pub phase_key: String,
    pub phase_title: String,
    pub completion_mode: String,
    pub requires_session: bool,
    pub default_artifact_type: Option<String>,
    pub default_artifact_title: Option<String>,
    pub target: WorkflowTargetSummary,
    pub agent_instructions: Vec<String>,
    pub binding_count: usize,
    pub resolved_binding_count: usize,
}

impl ActiveWorkflowProjection {
    /// 生成可序列化的快照视图。
    pub fn to_snapshot(&self) -> WorkflowProjectionSnapshot {
        WorkflowProjectionSnapshot {
            run_id: self.run.id,
            workflow_id: self.definition.id,
            workflow_key: self.definition.key.clone(),
            workflow_name: self.definition.name.clone(),
            run_status: workflow_run_status_tag(self.run.status).to_string(),
            phase_key: self.phase.key.clone(),
            phase_title: self.phase.title.clone(),
            completion_mode: completion_mode_tag(self.phase.completion_mode).to_string(),
            requires_session: self.phase.requires_session,
            default_artifact_type: self.phase.default_artifact_type.map(|t| {
                serde_json::to_value(t)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
            }),
            default_artifact_title: self.phase.default_artifact_title.clone(),
            target: self.target.clone(),
            agent_instructions: self.phase.agent_instructions.clone(),
            binding_count: self.resolved_bindings.len(),
            resolved_binding_count: self
                .resolved_bindings
                .iter()
                .filter(|b| b.snapshot.resolved)
                .count(),
        }
    }
}

fn workflow_run_status_tag(status: WorkflowRunStatus) -> &'static str {
    match status {
        WorkflowRunStatus::Draft => "draft",
        WorkflowRunStatus::Ready => "ready",
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Blocked => "blocked",
        WorkflowRunStatus::Completed => "completed",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Cancelled => "cancelled",
    }
}

/// 从仓储加载 target 对应的 active workflow projection。
/// 如果 target 没有活跃的 workflow run，返回 None。
pub async fn resolve_active_workflow_projection(
    target_kind: WorkflowTargetKind,
    target_id: Uuid,
    target_label: Option<String>,
    definition_repo: &dyn WorkflowDefinitionRepository,
    run_repo: &dyn WorkflowRunRepository,
    binding_context: Option<&BindingResolutionContext<'_>>,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let runs = run_repo
        .list_by_target(target_kind, target_id)
        .await
        .map_err(|e| format!("加载 workflow runs 失败: {e}"))?;

    let Some(run) = select_active_run(runs) else {
        return Ok(None);
    };
    let Some(current_phase_key) = run.current_phase_key.as_deref() else {
        return Ok(None);
    };

    let definition = definition_repo
        .get_by_id(run.workflow_id)
        .await
        .map_err(|e| format!("加载 workflow definition 失败: {e}"))?
        .filter(|d| d.enabled);
    let Some(definition) = definition else {
        return Ok(None);
    };

    let Some(phase) = definition
        .phases
        .iter()
        .find(|p| p.key == current_phase_key)
        .cloned()
    else {
        return Ok(None);
    };

    let resolved_bindings = binding_context
        .map(|ctx| {
            phase
                .context_bindings
                .iter()
                .map(|b| resolve_binding(b, ctx))
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(ActiveWorkflowProjection {
        run,
        definition,
        phase,
        target: WorkflowTargetSummary {
            target_kind,
            target_id,
            target_label,
        },
        resolved_bindings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_definition};

    #[test]
    fn snapshot_captures_key_fields() {
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        let run = WorkflowRun::new(
            Uuid::new_v4(),
            definition.id,
            WorkflowTargetKind::Task,
            Uuid::new_v4(),
            &definition.phases,
        );
        let phase = definition.phases[0].clone();
        let projection = ActiveWorkflowProjection {
            target: WorkflowTargetSummary {
                target_kind: WorkflowTargetKind::Task,
                target_id: run.target_id,
                target_label: Some("测试任务".to_string()),
            },
            run,
            definition,
            phase,
            resolved_bindings: vec![],
        };

        let snapshot = projection.to_snapshot();
        assert_eq!(snapshot.workflow_key, TRELLIS_DEV_TASK_TEMPLATE_KEY);
        assert_eq!(snapshot.run_status, "ready");
        assert_eq!(snapshot.phase_key, "start");
        assert_eq!(snapshot.binding_count, 0);
    }
}
