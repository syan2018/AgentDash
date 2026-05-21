use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    ActivityExecutorSpec, ActivityLifecycleDefinitionRepository, LifecycleDefinition,
    LifecycleNodeType, LifecycleRun, LifecycleRunRepository, LifecycleStepDefinition,
    WorkflowContract, WorkflowDefinition, WorkflowDefinitionRepository,
};

/// 运行时聚合视图:单 step 激活所需的全部定义域上下文。
///
/// 不再持有 `effective_contract` 字段——消费者需要 contract 4 字段时,
/// 直接通过 [`ActiveWorkflowProjection::active_contract`] 取到关联 workflow
/// 的 [`WorkflowContract`] 即可。SPI `ActiveWorkflowSnapshot.effective_contract`
/// 仍由 provider 在构造 snapshot 时按需用 `build_effective_contract` 派生,
/// 本结构不重复存一份。
#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub active_step: LifecycleStepDefinition,
    pub primary_workflow: Option<WorkflowDefinition>,
}

impl ActiveWorkflowProjection {
    /// 返回当前激活 step 关联的 workflow contract。
    ///
    /// - `Some(&contract)`:step 绑定了 workflow,返回其 contract
    /// - `None`:未绑定 workflow(manual step),消费者按"空 contract"语义处理
    pub fn active_contract(&self) -> Option<&WorkflowContract> {
        self.primary_workflow.as_ref().map(|w| &w.contract)
    }
}

/// 解析任意 session 的 Activity workflow projection。
///
/// Activity executor session 通过 `lifecycle_activity:*` binding 反查 run / activity。
/// 普通 owner session 若没有活跃 Activity attempt，则不再回落到旧 step run。
pub async fn resolve_active_workflow_projection_for_session(
    session_id: &str,
    session_binding_repo: &dyn SessionBindingRepository,
    definition_repo: &dyn WorkflowDefinitionRepository,
    activity_lifecycle_repo: &dyn ActivityLifecycleDefinitionRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    if let Some(activity_assoc) = super::session_association::resolve_activity_session_association(
        session_id,
        session_binding_repo,
        run_repo,
    )
    .await?
    {
        if let Some(projection) = build_activity_projection_from_run(
            activity_assoc.run,
            &activity_assoc.activity_key,
            definition_repo,
            activity_lifecycle_repo,
        )
        .await?
        {
            return Ok(Some(projection));
        }
    }

    Ok(None)
}

async fn build_activity_projection_from_run(
    run: LifecycleRun,
    activity_key: &str,
    definition_repo: &dyn WorkflowDefinitionRepository,
    activity_lifecycle_repo: &dyn ActivityLifecycleDefinitionRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(activity_lifecycle) = activity_lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 activity lifecycle definition 失败: {e}"))?
    else {
        return Ok(None);
    };
    let Some(activity) = activity_lifecycle
        .activities
        .iter()
        .find(|activity| activity.key == activity_key)
    else {
        return Ok(None);
    };
    let (workflow_key, node_type) = match &activity.executor {
        ActivityExecutorSpec::Agent(spec) => {
            let node_type = match spec.session_policy {
                agentdash_domain::workflow::AgentSessionPolicy::ContinueRoot => {
                    LifecycleNodeType::PhaseNode
                }
                agentdash_domain::workflow::AgentSessionPolicy::SpawnChild
                | agentdash_domain::workflow::AgentSessionPolicy::AttachExisting => {
                    LifecycleNodeType::AgentNode
                }
            };
            (Some(spec.workflow_key.clone()), node_type)
        }
        _ => (None, LifecycleNodeType::AgentNode),
    };
    let active_step = LifecycleStepDefinition {
        key: activity.key.clone(),
        description: activity.description.clone(),
        workflow_key: workflow_key.clone(),
        node_type,
        output_ports: activity.output_ports.clone(),
        input_ports: activity.input_ports.clone(),
        capability_config: Default::default(),
    };
    let lifecycle = LifecycleDefinition {
        id: activity_lifecycle.id,
        project_id: activity_lifecycle.project_id,
        key: activity_lifecycle.key.clone(),
        name: activity_lifecycle.name.clone(),
        description: activity_lifecycle.description.clone(),
        binding_kinds: activity_lifecycle.binding_kinds.clone(),
        source: activity_lifecycle.source,
        installed_source: activity_lifecycle.installed_source.clone(),
        version: activity_lifecycle.version,
        entry_step_key: activity_lifecycle.entry_activity_key.clone(),
        steps: vec![active_step.clone()],
        edges: Vec::new(),
        created_at: activity_lifecycle.created_at,
        updated_at: activity_lifecycle.updated_at,
    };
    let primary_workflow = match workflow_key {
        Some(workflow_key) => definition_repo
            .get_by_project_and_key(activity_lifecycle.project_id, &workflow_key)
            .await
            .map_err(|e| format!("加载 workflow 失败: {e}"))?,
        None => None,
    };

    Ok(Some(ActiveWorkflowProjection {
        run,
        lifecycle,
        active_step,
        primary_workflow,
    }))
}
