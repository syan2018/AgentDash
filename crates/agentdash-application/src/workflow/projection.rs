use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentFrameRepository,
    AgentProcedure, AgentProcedureContract, AgentProcedureRepository, AgentReusePolicy,
    BashExecExecutorSpec, ExecutorSpec, FunctionActivityExecutorSpec, LifecycleAgentRepository,
    LifecycleNodeType, LifecycleRun, LifecycleRunRepository, OrchestrationInstance,
    OrchestrationSourceRef, PlanNode, RuntimeNodeState, RuntimeSessionExecutionAnchorRepository,
    RuntimeSessionPolicy,
};
use agentdash_spi::hooks::HookControlTarget;

use super::session_association::ActivityRuntimeAssociationResolver;

/// 运行时聚合视图:单 activity 激活所需的全部定义域上下文。
///
/// 从 `LifecycleRun.orchestrations[].plan_snapshot` 与 runtime node 派生 activation
/// 所需上下文。`active_node_type` / `active_procedure_key` 由 plan executor 在构造时
/// 一次性推导出来,消费者无需重复解析 executor。
///
/// 不持有 `effective_contract` 字段——消费者需要 contract 4 字段时,
/// 直接通过 [`ActiveWorkflowProjection::active_contract`] 取到关联 workflow
/// 的 [`AgentProcedureContract`] 即可。SPI `ActiveWorkflowSnapshot.effective_contract`
/// 仍由 provider 在构造 snapshot 时按需用 `build_effective_contract` 派生,
/// 本结构不重复存一份。
#[derive(Debug, Clone)]
pub struct ActiveWorkflowProjection {
    pub run: LifecycleRun,
    pub orchestration_id: uuid::Uuid,
    pub node_path: String,
    pub lifecycle_graph_id: Option<uuid::Uuid>,
    pub lifecycle_key: String,
    pub lifecycle_name: String,
    pub active_activity: ActivityDefinition,
    pub active_attempt: RuntimeNodeState,
    /// 由 activity executor policy 推导的 node 语义:
    /// `continue_current_agent` → PhaseNode,`create_activity_agent` → AgentNode。
    pub active_node_type: LifecycleNodeType,
    /// agent executor 绑定的 procedure_key(若 activity 是 agent executor)。
    pub active_procedure_key: Option<String>,
    pub snapshot_contract: Option<AgentProcedureContract>,
    pub primary_workflow: Option<AgentProcedure>,
}

impl ActiveWorkflowProjection {
    /// 返回当前激活 activity 关联的 workflow contract。
    ///
    /// - `Some(&contract)`:activity 绑定了 workflow,返回其 contract
    /// - `None`:未绑定 workflow,消费者按"空 contract"语义处理
    pub fn active_contract(&self) -> Option<&AgentProcedureContract> {
        self.snapshot_contract.as_ref().or_else(|| {
            self.primary_workflow
                .as_ref()
                .map(|workflow| &workflow.contract)
        })
    }

    /// 当前激活 activity 的 advance 语义标签:绑定 workflow → `auto`,否则 `manual`。
    pub fn advance_label(&self) -> &'static str {
        if self.active_contract().is_some() {
            "auto"
        } else {
            "manual"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleProjectionIdentity {
    pub graph_id: Option<uuid::Uuid>,
    pub key: String,
    pub name: String,
}

/// 将 runtime plan node 投影成 activation 需要的 activity 形状。
///
/// `ActivityDefinition` 在这里是 activation 的窄 DTO；来源是已冻结的
/// `OrchestrationPlanSnapshot`，不是运行时再次查询到的 WorkflowGraph。
pub fn activity_definition_from_plan_node(plan_node: &PlanNode) -> ActivityDefinition {
    let executor = match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) => ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            procedure_key: procedure
                .procedure_key()
                .unwrap_or("__inline_agent_procedure")
                .to_string(),
            agent_reuse_policy: *agent_reuse_policy,
            runtime_session_policy: *runtime_session_policy,
        }),
        Some(ExecutorSpec::Function { spec }) => ActivityExecutorSpec::Function(spec.clone()),
        Some(ExecutorSpec::Human { spec }) => ActivityExecutorSpec::Human(spec.clone()),
        Some(ExecutorSpec::LocalEffect { .. })
        | Some(ExecutorSpec::ExtensionAction { .. })
        | None => ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "true".to_string(),
                args: Vec::new(),
                working_directory: None,
            },
        )),
    };

    ActivityDefinition {
        key: plan_node.node_path.clone(),
        description: plan_node
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("description"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| plan_node.label.clone())
            .unwrap_or_default(),
        executor,
        input_ports: plan_node.input_ports.clone(),
        output_ports: plan_node.output_ports.clone(),
        completion_policy: plan_node.completion_policy.clone().unwrap_or_default(),
        iteration_policy: plan_node.iteration_policy.clone().unwrap_or_default(),
        join_policy: plan_node.join_policy.unwrap_or_default(),
    }
}

pub fn lifecycle_identity_from_orchestration(
    orchestration: &OrchestrationInstance,
) -> LifecycleProjectionIdentity {
    let metadata_source = orchestration
        .plan_snapshot
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("source"));
    let key = metadata_source
        .and_then(|source| source.get("key"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| lifecycle_key_from_source_ref(&orchestration.source_ref));
    let name = metadata_source
        .and_then(|source| source.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| key.clone());
    let graph_id = match &orchestration.source_ref {
        OrchestrationSourceRef::WorkflowGraph { graph_id, .. } => Some(*graph_id),
        _ => None,
    };

    LifecycleProjectionIdentity {
        graph_id,
        key,
        name,
    }
}

fn lifecycle_key_from_source_ref(source_ref: &OrchestrationSourceRef) -> String {
    match source_ref {
        OrchestrationSourceRef::WorkflowGraph { graph_id, .. } => {
            format!("workflow_graph:{graph_id}")
        }
        OrchestrationSourceRef::RunScriptArtifact { artifact_id, .. } => {
            format!("run_script:{artifact_id}")
        }
        OrchestrationSourceRef::WorkflowScript { script_id, .. } => {
            format!("workflow_script:{script_id}")
        }
        OrchestrationSourceRef::Inline { source_digest } => {
            format!("inline:{}", digest_suffix(source_digest))
        }
    }
}

fn digest_suffix(digest: &str) -> &str {
    digest
        .strip_prefix("sha256:")
        .unwrap_or(digest)
        .get(..12)
        .unwrap_or(digest)
}

/// 由 plan executor 推导 (procedure_key, node_type)。
fn derive_node_facts(plan_node: &PlanNode) -> (Option<String>, LifecycleNodeType) {
    match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure {
            procedure,
            agent_reuse_policy,
            runtime_session_policy,
        }) => {
            let node_type = if *agent_reuse_policy == AgentReusePolicy::ContinueCurrentAgent
                && *runtime_session_policy == RuntimeSessionPolicy::DeliverToCurrentTrace
            {
                LifecycleNodeType::PhaseNode
            } else {
                LifecycleNodeType::AgentNode
            };
            (procedure.procedure_key().map(str::to_string), node_type)
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
    definition_repo: &dyn AgentProcedureRepository,
    frame_repo: &dyn AgentFrameRepository,
    _agent_repo: &dyn LifecycleAgentRepository,
    run_repo: &dyn LifecycleRunRepository,
    anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let resolver =
        ActivityRuntimeAssociationResolver::new(frame_repo, run_repo).with_anchor_repo(anchor_repo);
    let Some(association) = resolver
        .resolve_by_runtime_session(session_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    active_workflow_projection_from_runtime_node(
        association.run,
        association.orchestration_id,
        &association.node_path,
        association.attempt,
        definition_repo,
    )
    .await
}

pub async fn resolve_active_workflow_projection_for_target(
    target: &HookControlTarget,
    definition_repo: &dyn AgentProcedureRepository,
    _frame_repo: &dyn AgentFrameRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(run) = run_repo
        .get_by_id(target.run_id)
        .await
        .map_err(|e| format!("查询 lifecycle run 失败: {e}"))?
    else {
        return Ok(None);
    };
    let Some((orchestration_id, node_path, attempt)) = run
        .orchestrations
        .iter()
        .find_map(active_node_ref_for_orchestration)
    else {
        return Ok(None);
    };
    active_workflow_projection_from_runtime_node(
        run,
        orchestration_id,
        &node_path,
        attempt,
        definition_repo,
    )
    .await
}

async fn active_workflow_projection_from_runtime_node(
    run: LifecycleRun,
    orchestration_id: uuid::Uuid,
    node_path: &str,
    attempt: u32,
    definition_repo: &dyn AgentProcedureRepository,
) -> Result<Option<ActiveWorkflowProjection>, String> {
    let Some(orchestration) = run.orchestration_by_id(orchestration_id) else {
        return Ok(None);
    };
    let Some(active_attempt) =
        find_runtime_node(&orchestration.node_tree, node_path, attempt).cloned()
    else {
        return Ok(None);
    };
    let Some(plan_node) = orchestration.plan_snapshot.nodes.iter().find(|node| {
        node.node_path == active_attempt.node_path || node.node_id == active_attempt.node_id
    }) else {
        return Ok(None);
    };
    let lifecycle_identity = lifecycle_identity_from_orchestration(orchestration);
    let active_activity = activity_definition_from_plan_node(plan_node);
    let (active_procedure_key, active_node_type) = derive_node_facts(plan_node);
    let snapshot_contract = match &plan_node.executor {
        Some(ExecutorSpec::AgentProcedure { procedure, .. }) => {
            procedure.snapshot_contract().cloned()
        }
        _ => None,
    };
    let primary_workflow = match active_procedure_key.as_deref() {
        Some(key) if !key.trim().is_empty() && snapshot_contract.is_none() => definition_repo
            .get_by_project_and_key(run.project_id, key)
            .await
            .map_err(|error| format!("查询 AgentProcedure 失败: {error}"))?,
        _ => None,
    };
    Ok(Some(ActiveWorkflowProjection {
        run,
        orchestration_id,
        node_path: node_path.to_string(),
        lifecycle_graph_id: lifecycle_identity.graph_id,
        lifecycle_key: lifecycle_identity.key,
        lifecycle_name: lifecycle_identity.name,
        active_activity,
        active_attempt,
        active_node_type,
        active_procedure_key,
        snapshot_contract,
        primary_workflow,
    }))
}

fn active_node_ref_for_orchestration(
    orchestration: &OrchestrationInstance,
) -> Option<(uuid::Uuid, String, u32)> {
    find_first_active_node(&orchestration.node_tree).map(|node| {
        (
            orchestration.orchestration_id,
            node.node_path.clone(),
            node.attempt,
        )
    })
}

fn find_first_active_node(nodes: &[RuntimeNodeState]) -> Option<&RuntimeNodeState> {
    for node in nodes {
        if matches!(
            node.status,
            agentdash_domain::workflow::RuntimeNodeStatus::Ready
                | agentdash_domain::workflow::RuntimeNodeStatus::Claiming
                | agentdash_domain::workflow::RuntimeNodeStatus::Running
                | agentdash_domain::workflow::RuntimeNodeStatus::Blocked
        ) {
            return Some(node);
        }
        if let Some(child) = find_first_active_node(&node.children) {
            return Some(child);
        }
    }
    None
}

fn find_runtime_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(child) = find_runtime_node(&node.children, node_path, attempt) {
            return Some(child);
        }
    }
    None
}

/// 测试夹具:构造 Activity 形态的 [`ActiveWorkflowProjection`]，供 hooks / vfs
/// 等模块的单元测试复用，避免每处手搓 Activity lifecycle/run。
#[cfg(test)]
pub(crate) fn activity_projection(guidance: Option<String>) -> ActiveWorkflowProjection {
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, DefinitionSource, OutputPortDefinition, PlanNodeKind,
        RuntimeNodeState, RuntimeNodeStatus, WorkflowGraph, WorkflowGraphDraft,
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
    let lifecycle = WorkflowGraph::new(WorkflowGraphDraft {
        project_id,
        key: "trellis_dev_task".to_string(),
        name: "Trellis Dev Lifecycle".to_string(),
        description: "lifecycle desc".to_string(),
        source: DefinitionSource::BuiltinSeed,
        entry_activity_key: "implement".to_string(),
        activities: vec![active_activity.clone()],
        transitions: vec![],
    })
    .expect("lifecycle definition should build");
    let active_attempt = RuntimeNodeState {
        node_id: "implement".to_string(),
        node_path: "implement".to_string(),
        kind: PlanNodeKind::AgentCall,
        status: RuntimeNodeStatus::Running,
        attempt: 1,
        inputs: Vec::new(),
        outputs: Vec::new(),
        executor_run_ref: None,
        children: Vec::new(),
        phase_path: Vec::new(),
        started_at: None,
        completed_at: None,
        error: None,
        trace_refs: Vec::new(),
        cache: None,
    };
    let run = LifecycleRun::new_control(project_id);
    ActiveWorkflowProjection {
        run,
        orchestration_id: uuid::Uuid::new_v4(),
        node_path: "implement".to_string(),
        lifecycle_graph_id: Some(lifecycle.id),
        lifecycle_key: lifecycle.key.clone(),
        lifecycle_name: lifecycle.name.clone(),
        active_activity,
        active_attempt,
        active_node_type: LifecycleNodeType::AgentNode,
        active_procedure_key: Some(definition.key.clone()),
        snapshot_contract: None,
        primary_workflow: Some(definition),
    }
}

#[cfg(test)]
mod tests {
    use super::{activity_definition_from_plan_node, derive_node_facts};
    use agentdash_domain::workflow::{
        AgentProcedureExecutionSpec, AgentReusePolicy, ExecutorSpec, LifecycleNodeType, PlanNode,
        PlanNodeKind, RuntimeSessionPolicy,
    };

    fn plan_node_with_agent_executor(
        agent_reuse_policy: AgentReusePolicy,
        runtime_session_policy: RuntimeSessionPolicy,
    ) -> PlanNode {
        PlanNode {
            node_id: "implement".to_string(),
            node_path: "implement".to_string(),
            parent_node_id: None,
            kind: PlanNodeKind::AgentCall,
            label: None,
            executor: Some(ExecutorSpec::AgentProcedure {
                procedure: AgentProcedureExecutionSpec::by_key("wf_impl"),
                agent_reuse_policy,
                runtime_session_policy,
            }),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: None,
        }
    }

    #[test]
    fn derives_node_type_from_agent_reuse_policy() {
        let (procedure_key, node_type) = derive_node_facts(&plan_node_with_agent_executor(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeSessionPolicy::CreateNew,
        ));

        assert_eq!(procedure_key.as_deref(), Some("wf_impl"));
        assert_eq!(node_type, LifecycleNodeType::AgentNode);

        let (procedure_key, node_type) = derive_node_facts(&plan_node_with_agent_executor(
            AgentReusePolicy::ContinueCurrentAgent,
            RuntimeSessionPolicy::DeliverToCurrentTrace,
        ));

        assert_eq!(procedure_key.as_deref(), Some("wf_impl"));
        assert_eq!(node_type, LifecycleNodeType::PhaseNode);
    }

    #[test]
    fn projects_activity_from_plan_node_without_graph_lookup() {
        let plan_node = plan_node_with_agent_executor(
            AgentReusePolicy::CreateActivityAgent,
            RuntimeSessionPolicy::CreateNew,
        );

        let activity = activity_definition_from_plan_node(&plan_node);

        assert_eq!(activity.key, "implement");
        assert_eq!(activity.executor.kind(), "agent");
    }
}
