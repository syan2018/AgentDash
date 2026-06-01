use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutionClaimRepository, ActivityExecutorSpec, AgentProcedure,
    AgentProcedureRepository, AgentSessionPolicy, LifecycleNodeType, LifecycleRun,
    LifecycleRunRepository, WorkflowContract, WorkflowGraph, WorkflowGraphRepository,
};

/// 运行时聚合视图:单 activity 激活所需的全部定义域上下文。
///
/// 直接持有查到的 [`WorkflowGraph`] 与匹配的 [`ActivityDefinition`],
/// 不再合成 Step 壳。`active_node_type` / `active_procedure_key` 由 activity 的
/// executor 在构造时一次性推导出来,消费者无需重复解析 executor。
///
/// 不持有 `effective_contract` 字段——消费者需要 contract 4 字段时,
/// 直接通过 [`ActiveWorkflowProjection::active_contract`] 取到关联 workflow
/// 的 [`WorkflowContract`] 即可。SPI `ActiveWorkflowSnapshot.effective_contract`
/// 仍由 provider 在构造 snapshot 时按需用 `build_effective_contract` 派生,
/// 本结构不重复存一份。
#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub lifecycle: WorkflowGraph,
    pub active_activity: ActivityDefinition,
    /// 由 activity executor 推导的 node 语义:
    /// `ContinueRoot` → PhaseNode,`SpawnChild` / `AttachExisting` → AgentNode。
    pub active_node_type: LifecycleNodeType,
    /// agent executor 绑定的 procedure_key(若 activity 是 agent executor)。
    pub active_procedure_key: Option<String>,
    pub primary_workflow: Option<AgentProcedure>,
}

impl ActiveWorkflowProjection {
    /// 返回当前激活 activity 关联的 workflow contract。
    ///
    /// - `Some(&contract)`:activity 绑定了 workflow,返回其 contract
    /// - `None`:未绑定 workflow,消费者按"空 contract"语义处理
    pub fn active_contract(&self) -> Option<&WorkflowContract> {
        self.primary_workflow.as_ref().map(|w| &w.contract)
    }

    /// 当前激活 activity 的 advance 语义标签:绑定 workflow → `auto`,否则 `manual`。
    pub fn advance_label(&self) -> &'static str {
        match self.active_procedure_key.as_deref().map(str::trim) {
            Some(key) if !key.is_empty() => "auto",
            _ => "manual",
        }
    }
}

/// 由 activity executor 推导 (procedure_key, node_type)。
fn derive_node_facts(activity: &ActivityDefinition) -> (Option<String>, LifecycleNodeType) {
    match &activity.executor {
        ActivityExecutorSpec::Agent(spec) => {
            let node_type = match spec.session_policy {
                AgentSessionPolicy::ContinueRoot => LifecycleNodeType::PhaseNode,
                AgentSessionPolicy::SpawnChild | AgentSessionPolicy::AttachExisting => {
                    LifecycleNodeType::AgentNode
                }
            };
            (Some(spec.procedure_key.clone()), node_type)
        }
        _ => (None, LifecycleNodeType::AgentNode),
    }
}

/// 解析任意 session 的 Activity workflow projection。
///
/// 通过 claim executor_run_ref 反查 run / activity，不依赖 LifecycleRun.session_id。
pub async fn resolve_active_workflow_projection_for_session(
    session_id: &str,
    definition_repo: &dyn AgentProcedureRepository,
    activity_lifecycle_repo: &dyn WorkflowGraphRepository,
    claim_repo: &dyn ActivityExecutionClaimRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    if let Some(claim) = claim_repo
        .find_running_by_executor_session(session_id)
        .await
        .map_err(|error| format!("查询 activity execution claims 失败: {error}"))?
    {
        let run = run_repo
            .get_by_id(claim.run_id)
            .await
            .map_err(|error| format!("查询 lifecycle run 失败: {error}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {}", claim.run_id))?;
        if let Some(projection) = build_activity_projection_from_run(
            run,
            &claim.activity_key,
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
    definition_repo: &dyn AgentProcedureRepository,
    activity_lifecycle_repo: &dyn WorkflowGraphRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(activity_lifecycle) = activity_lifecycle_repo
        .get_by_id(run.lifecycle_id)
        .await
        .map_err(|e| format!("加载 activity lifecycle definition 失败: {e}"))?
    else {
        return Ok(None);
    };
    let Some(active_activity) = activity_lifecycle
        .activities
        .iter()
        .find(|activity| activity.key == activity_key)
        .cloned()
    else {
        return Ok(None);
    };
    let (active_procedure_key, active_node_type) = derive_node_facts(&active_activity);
    let primary_workflow = match active_procedure_key.as_deref() {
        Some(procedure_key) => definition_repo
            .get_by_project_and_key(activity_lifecycle.project_id, procedure_key)
            .await
            .map_err(|e| format!("加载 workflow 失败: {e}"))?,
        None => None,
    };

    Ok(Some(ActiveWorkflowProjection {
        run,
        lifecycle: activity_lifecycle,
        active_activity,
        active_node_type,
        active_procedure_key,
        primary_workflow,
    }))
}

/// 测试夹具:构造 Activity 形态的 [`ActiveWorkflowProjection`]，供 hooks / vfs
/// 等模块的单元测试复用，避免每处手搓 Activity lifecycle/run。
#[cfg(test)]
pub(crate) fn activity_projection(guidance: Option<String>) -> ActiveWorkflowProjection {
    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityDefinition, ActivityExecutorSpec,
        ActivityLifecycleRunState, ActivityRunStatus, AgentActivityExecutorSpec, AgentProcedure,
        OutputPortDefinition, WorkflowContract, WorkflowDefinitionSource, WorkflowGraph,
        WorkflowInjectionSpec,
    };
    use uuid::Uuid;

    let project_id = Uuid::new_v4();
    let contract = WorkflowContract {
        injection: WorkflowInjectionSpec {
            guidance,
            ..WorkflowInjectionSpec::default()
        },
        ..WorkflowContract::default()
    };
    let definition = AgentProcedure::new(
        Uuid::new_v4(),
        "trellis_dev_task_implement",
        "Trellis Dev Workflow / Implement",
        "workflow desc",
        WorkflowDefinitionSource::BuiltinSeed,
        contract,
    )
    .expect("workflow definition should build");
    let active_activity = ActivityDefinition {
        key: "implement".to_string(),
        description: "实现并记录结果".to_string(),
        executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            procedure_key: definition.key.clone(),
            session_policy: Default::default(),
        }),
        input_ports: vec![],
        output_ports: vec![OutputPortDefinition {
            key: "summary".to_string(),
            description: "实现摘要".to_string(),
            gate_strategy: Default::default(),
            gate_params: None,
        }],
        completion_policy: Default::default(),
        iteration_policy: Default::default(),
        join_policy: Default::default(),
    };
    let lifecycle = WorkflowGraph::new(
        project_id,
        "trellis_dev_task",
        "Trellis Dev Lifecycle",
        "lifecycle desc",
        WorkflowDefinitionSource::BuiltinSeed,
        "implement",
        vec![active_activity.clone()],
        vec![],
    )
    .expect("lifecycle definition should build");
    let activity_state = ActivityLifecycleRunState {
        graph_instance_id: uuid::Uuid::new_v4(),
        status: ActivityRunStatus::Running,
        attempts: vec![ActivityAttemptState {
            activity_key: "implement".to_string(),
            attempt: 1,
            status: ActivityAttemptStatus::Running,
            executor_run: None,
            started_at: None,
            completed_at: None,
            summary: None,
        }],
        outputs: Vec::new(),
        inputs: Vec::new(),
    };
    let run = LifecycleRun::new_activity(project_id, lifecycle.id, activity_state)
        .expect("activity run should build");
    let (active_procedure_key, active_node_type) = derive_node_facts(&active_activity);
    ActiveWorkflowProjection {
        run,
        lifecycle,
        active_activity,
        active_node_type,
        active_procedure_key,
        primary_workflow: Some(definition),
    }
}
