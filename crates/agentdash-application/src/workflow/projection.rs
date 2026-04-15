use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleStepDefinition, WorkflowBindingKind, WorkflowDefinition,
    WorkflowDefinitionRepository, build_effective_contract,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub active_step: LifecycleStepDefinition,
    pub primary_workflow: Option<WorkflowDefinition>,
    pub effective_contract: agentdash_domain::workflow::EffectiveSessionContract,
    /// 当前 projection 绑定到的 owner 摘要。
    pub binding: WorkflowBindingSummary,
}

#[derive(Debug, Clone, Serialize)]
/// Workflow 当前绑定对象摘要。
pub struct WorkflowBindingSummary {
    pub binding_kind: WorkflowBindingKind,
    pub binding_id: Uuid,
    pub binding_label: Option<String>,
}

pub async fn resolve_active_workflow_projection(
    binding_kind: WorkflowBindingKind,
    binding_id: Uuid,
    binding_label: Option<String>,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let runs = run_repo
        .list_by_binding(binding_kind, binding_id)
        .await
        .map_err(|e| format!("加载 lifecycle runs 失败: {e}"))?;

    let Some(run) = super::run::select_active_run(runs) else {
        return Ok(None);
    };
    let Some(current_step_key) = run.current_step_key.as_deref() else {
        return Ok(None);
    };

    let lifecycle = lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))?
        .filter(|definition| definition.is_active());
    let Some(lifecycle) = lifecycle else {
        return Ok(None);
    };

    let Some(active_step) = lifecycle
        .steps
        .iter()
        .find(|step| step.key == current_step_key)
        .cloned()
    else {
        return Ok(None);
    };

    let primary_workflow = match active_step
        .workflow_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(wk) => {
            let Some(wf) = definition_repo
                .get_by_key(wk)
                .await
                .map_err(|e| format!("加载 workflow 失败: {e}"))?
                .filter(|definition| definition.is_active())
            else {
                return Ok(None);
            };
            Some(wf)
        }
        None => None,
    };

    let effective_contract =
        build_effective_contract(&lifecycle.key, &active_step.key, primary_workflow.as_ref());

    Ok(Some(ActiveWorkflowProjection {
        run,
        lifecycle,
        active_step,
        primary_workflow,
        effective_contract,
        binding: WorkflowBindingSummary {
            binding_kind,
            binding_id,
            binding_label,
        },
    }))
}

/// 解析 lifecycle run 中所有当前活跃 node 的 workflow projection 列表。
///
/// 用于 Lifecycle Orchestrator 在 DAG 中获取全部可执行 node 的 contract。
/// 返回的列表按 `active_node_keys` 顺序排列。
pub async fn resolve_active_workflow_projections_for_run(
    run_id: Uuid,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Vec<ActiveWorkflowProjection>, String> {
    let run = run_repo
        .get_by_id(run_id)
        .await
        .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?;
    let Some(run) = run else {
        return Ok(Vec::new());
    };

    let node_keys: Vec<String> = if !run.active_node_keys.is_empty() {
        run.active_node_keys.clone()
    } else {
        // 线性兼容：使用 current_step_key
        run.current_step_key
            .as_deref()
            .map(|k| vec![k.to_string()])
            .unwrap_or_default()
    };

    if node_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut projections = Vec::new();
    for node_key in &node_keys {
        if let Some(proj) = resolve_workflow_projection_by_run(
            run_id,
            node_key,
            definition_repo,
            lifecycle_repo,
            run_repo,
        )
        .await?
        {
            projections.push(proj);
        }
    }

    Ok(projections)
}

/// 通过 run_id 和 node_key 直接解析 workflow projection。
///
/// 与 `resolve_active_workflow_projection` 不同，此函数不依赖 binding 查询，
/// 也不假设 `current_step_key` 是目标 node —— 而是直接指定要查看的 node_key。
/// 用于 Lifecycle Orchestrator 在 DAG 中针对特定 node 构建 contract。
pub async fn resolve_workflow_projection_by_run(
    run_id: Uuid,
    node_key: &str,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let run = run_repo
        .get_by_id(run_id)
        .await
        .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?;
    let Some(run) = run else {
        return Ok(None);
    };

    let lifecycle = lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))?
        .filter(|definition| definition.is_active());
    let Some(lifecycle) = lifecycle else {
        return Ok(None);
    };

    let Some(active_step) = lifecycle
        .steps
        .iter()
        .find(|step| step.key == node_key)
        .cloned()
    else {
        return Ok(None);
    };

    let primary_workflow = match active_step
        .workflow_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(wk) => {
            let Some(wf) = definition_repo
                .get_by_key(wk)
                .await
                .map_err(|e| format!("加载 workflow 失败: {e}"))?
                .filter(|definition| definition.is_active())
            else {
                return Ok(None);
            };
            Some(wf)
        }
        None => None,
    };

    let effective_contract =
        build_effective_contract(&lifecycle.key, &active_step.key, primary_workflow.as_ref());

    Ok(Some(ActiveWorkflowProjection {
        run,
        lifecycle,
        active_step,
        primary_workflow,
        effective_contract,
        binding: WorkflowBindingSummary {
            binding_kind: WorkflowBindingKind::Task,
            binding_id: Uuid::nil(),
            binding_label: Some(format!("lifecycle_node:{node_key}")),
        },
    }))
}
