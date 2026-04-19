use super::session_association::resolve_node_session_association;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleStepDefinition, WorkflowDefinition, WorkflowDefinitionRepository,
    build_effective_contract,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub active_step: LifecycleStepDefinition,
    pub primary_workflow: Option<WorkflowDefinition>,
    pub effective_contract: agentdash_domain::workflow::EffectiveSessionContract,
}

/// 通过 session_id 查找该 session 关联的活跃 lifecycle run 并构建 projection。
pub async fn resolve_active_workflow_projection(
    session_id: &str,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let runs = run_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| format!("加载 lifecycle runs 失败: {e}"))?;

    let Some(run) = super::run::select_active_run(runs) else {
        return Ok(None);
    };
    let Some(current_step_key) = run.current_step_key().map(str::to_string) else {
        return Ok(None);
    };

    build_projection_from_run(run, &current_step_key, definition_repo, lifecycle_repo).await
}

/// 解析任意 session 的 active workflow projection（兼容 lifecycle node 子 session）。
///
/// - 对普通 owner session，走 `lifecycle_runs.session_id` 直接解析；
/// - 对 lifecycle node 子 session，走 `session_binding(label=lifecycle_node:*)` 反查 run + node_key。
pub async fn resolve_active_workflow_projection_for_session(
    session_id: &str,
    session_binding_repo: &dyn SessionBindingRepository,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    if let Some(node_assoc) =
        resolve_node_session_association(session_id, session_binding_repo, run_repo).await?
    {
        if let Some(projection) = resolve_workflow_projection_by_run(
            node_assoc.run.id,
            &node_assoc.node_key,
            definition_repo,
            lifecycle_repo,
            run_repo,
        )
        .await?
        {
            return Ok(Some(projection));
        }
    }

    resolve_active_workflow_projection(session_id, definition_repo, lifecycle_repo, run_repo).await
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

    let node_keys: Vec<String> = run.active_node_keys.clone();

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

    build_projection_from_run(run, node_key, definition_repo, lifecycle_repo).await
}

/// 共享的 projection 构建核心：从已确定的 run + step_key 出发，
/// 加载 lifecycle → 查找 step → 加载 workflow → 构建 effective_contract。
async fn build_projection_from_run(
    run: LifecycleRun,
    step_key: &str,
    definition_repo: &dyn WorkflowDefinitionRepository,
    lifecycle_repo: &dyn LifecycleDefinitionRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(lifecycle) = lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))? else {
        return Ok(None);
    };

    let Some(active_step) = lifecycle
        .steps
        .iter()
        .find(|step| step.key == step_key)
        .cloned()
    else {
        return Ok(None);
    };

    let primary_workflow = match active_step.effective_workflow_key() {
        Some(wk) => {
            let Some(wf) = definition_repo
                .get_by_key(wk)
                .await
                .map_err(|e| format!("加载 workflow 失败: {e}"))? else {
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
    }))
}
