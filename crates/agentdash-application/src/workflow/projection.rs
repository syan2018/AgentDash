use agentdash_domain::workflow::{
    ActivityAttemptState, ActivityDefinition, AgentFrameRepository, AgentProcedure,
    AgentProcedureContract, AgentProcedureRepository, LifecycleAgentRepository, LifecycleNodeType,
    LifecycleRun, LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository, WorkflowGraph,
    WorkflowGraphRepository,
};
use agentdash_spi::hooks::HookControlTarget;

use super::session_association::ActivityRuntimeAssociationResolver;

/// 运行时聚合视图:单 activity 激活所需的全部定义域上下文。
///
/// 直接持有查到的 [`WorkflowGraph`] 与匹配的 [`ActivityDefinition`],
/// 不再合成 Step 壳。`active_node_type` / `active_procedure_key` 由 activity 的
/// executor 在构造时一次性推导出来,消费者无需重复解析 executor。
///
/// 不持有 `effective_contract` 字段——消费者需要 contract 4 字段时,
/// 直接通过 [`ActiveWorkflowProjection::active_contract`] 取到关联 workflow
/// 的 [`AgentProcedureContract`] 即可。SPI `ActiveWorkflowSnapshot.effective_contract`
/// 仍由 provider 在构造 snapshot 时按需用 `build_effective_contract` 派生,
/// 本结构不重复存一份。
#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub graph_instance_id: uuid::Uuid,
    pub lifecycle: WorkflowGraph,
    pub active_activity: ActivityDefinition,
    pub active_attempt: ActivityAttemptState,
    /// 由 activity executor policy 推导的 node 语义:
    /// `continue_current_agent` → PhaseNode,`create_activity_agent` → AgentNode。
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
    pub fn active_contract(&self) -> Option<&AgentProcedureContract> {
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
#[cfg(test)]
fn derive_node_facts(activity: &ActivityDefinition) -> (Option<String>, LifecycleNodeType) {
    use agentdash_domain::workflow::ActivityExecutorSpec;

    match &activity.executor {
        ActivityExecutorSpec::Agent(spec) => {
            let node_type = if spec.continues_current_agent() {
                LifecycleNodeType::PhaseNode
            } else {
                LifecycleNodeType::AgentNode
            };
            (Some(spec.procedure_key.clone()), node_type)
        }
        _ => (None, LifecycleNodeType::AgentNode),
    }
}

/// 解析任意 RuntimeSession 的 Activity workflow projection。
///
/// 生产链路只允许 RuntimeSession 作为 trace lookup 起点：
/// RuntimeSession -> RuntimeSessionExecutionAnchor -> LifecycleRun.orchestrations。
pub async fn resolve_active_workflow_projection_for_session(
    session_id: &str,
    _definition_repo: &dyn AgentProcedureRepository,
    _activity_lifecycle_repo: &dyn WorkflowGraphRepository,
    frame_repo: &dyn AgentFrameRepository,
    _agent_repo: &dyn LifecycleAgentRepository,
    run_repo: &dyn LifecycleRunRepository,
    anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let resolver =
        ActivityRuntimeAssociationResolver::new(frame_repo, run_repo).with_anchor_repo(anchor_repo);
    let Some(_association) = resolver
        .resolve_by_runtime_session(session_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    Ok(None)
}

pub async fn resolve_active_workflow_projection_for_target(
    target: &HookControlTarget,
    _definition_repo: &dyn AgentProcedureRepository,
    _activity_lifecycle_repo: &dyn WorkflowGraphRepository,
    _frame_repo: &dyn AgentFrameRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(_run) = run_repo
        .get_by_id(target.run_id)
        .await
        .map_err(|e| format!("查询 lifecycle run 失败: {e}"))?
    else {
        return Ok(None);
    };
    Ok(None)
}

/// 测试夹具:构造 Activity 形态的 [`ActiveWorkflowProjection`]，供 hooks / vfs
/// 等模块的单元测试复用，避免每处手搓 Activity lifecycle/run。
#[cfg(test)]
pub(crate) fn activity_projection(guidance: Option<String>) -> ActiveWorkflowProjection {
    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityDefinition, ActivityExecutorSpec,
        ActivityLifecycleRunState, ActivityRunStatus, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, DefinitionSource, OutputPortDefinition, WorkflowGraph,
        WorkflowInjectionSpec,
    };
    use uuid::Uuid;

    let project_id = Uuid::new_v4();
    let contract = AgentProcedureContract {
        injection: WorkflowInjectionSpec {
            guidance,
            ..WorkflowInjectionSpec::default()
        },
        ..AgentProcedureContract::default()
    };
    let definition = AgentProcedure::new(
        Uuid::new_v4(),
        "trellis_dev_task_implement",
        "Trellis Dev Workflow / Implement",
        "workflow desc",
        DefinitionSource::BuiltinSeed,
        contract,
    )
    .expect("workflow definition should build");
    let active_activity = ActivityDefinition {
        key: "implement".to_string(),
        description: "实现并记录结果".to_string(),
        executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec::create_activity_agent(
            definition.key.clone(),
        )),
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
        DefinitionSource::BuiltinSeed,
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
    let mut run = LifecycleRun::new_control(project_id, lifecycle.id);
    run.sync_graph_instance_activity_projections([(
        activity_state.graph_instance_id,
        &activity_state,
    )]);
    let active_attempt = activity_state.attempts[0].clone();
    let (active_procedure_key, active_node_type) = derive_node_facts(&active_activity);
    ActiveWorkflowProjection {
        run,
        graph_instance_id: activity_state.graph_instance_id,
        lifecycle,
        active_activity,
        active_attempt,
        active_node_type,
        active_procedure_key,
        primary_workflow: Some(definition),
    }
}

#[cfg(test)]
mod tests {
    use super::derive_node_facts;
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, LifecycleNodeType,
    };

    fn activity_with_agent_executor(executor: AgentActivityExecutorSpec) -> ActivityDefinition {
        ActivityDefinition {
            key: "implement".to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(executor),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        }
    }

    #[test]
    fn derives_node_type_from_agent_reuse_policy() {
        let (procedure_key, node_type) = derive_node_facts(&activity_with_agent_executor(
            AgentActivityExecutorSpec::create_activity_agent("wf_impl"),
        ));

        assert_eq!(procedure_key.as_deref(), Some("wf_impl"));
        assert_eq!(node_type, LifecycleNodeType::AgentNode);

        let (procedure_key, node_type) = derive_node_facts(&activity_with_agent_executor(
            AgentActivityExecutorSpec::continue_current_agent("wf_impl"),
        ));

        assert_eq!(procedure_key.as_deref(), Some("wf_impl"));
        assert_eq!(node_type, LifecycleNodeType::PhaseNode);
    }
}
