use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
use agentdash_application_ports::workflow_graph_planning as workflow_graph_planning_port;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    WorkflowGraphRepository,
};
use agentdash_domain::workflow::{
    AgentLaunchDispatchResult, AgentLaunchIntent, ExecutionDispatchResult, ExecutionIntent,
    InteractionDispatchIntent, InteractionGateOpenedDispatchResult,
    LifecycleRunStartDispatchResult, LifecycleRunStartIntent, SubjectExecutionDispatchResult,
    SubjectExecutionIntent,
};

use super::WorkflowApplicationError;
use super::dispatch::{
    AgentRuntimeMaterializer, DispatchFacts, DispatchPlan, LifecycleRelationWriter,
    OrchestrationReducerBridge, RunOrchestrationStarter, SubjectAssociationWriter,
};
use agentdash_diagnostics::{Subsystem, diag};

/// 业务执行进入控制面的统一入口 service。
///
/// 接收 `ExecutionIntent`，根据 policy 决定：
/// - 复用 / 创建 LifecycleRun
/// - 创建 / 复用 OrchestrationInstance
/// - 创建 LifecycleSubjectAssociation（如果有 subject_ref）
/// - 创建 / 复用 LifecycleAgent
/// - 创建 AgentFrame initial revision
/// - 按需创建 LifecycleGate / AgentLineage
///
/// **不拥有** AgentFrame 内部构造细节（由 AgentRun frame materialization 边界处理）
/// 和 connector launch（T4 的工作）。
pub struct LifecycleDispatchService<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    _frame_repo: &'a dyn AgentFrameRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
    frame_construction:
        Option<&'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort>,
    workflow_agent_frame_materialization:
        Option<&'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort>,
    workflow_graph_planner: Option<&'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort>,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
}

impl<'a> LifecycleDispatchService<'a> {
    pub fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            agent_repo,
            _frame_repo: frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
            frame_construction: None,
            workflow_agent_frame_materialization: None,
            workflow_graph_planner: None,
            project_projection_notifications: None,
        }
    }

    pub fn with_frame_construction_port(
        mut self,
        port: &'a dyn agent_frame_materialization_port::AgentRunFrameConstructionPort,
    ) -> Self {
        self.frame_construction = Some(port);
        self
    }

    pub fn with_workflow_agent_frame_materialization_port(
        mut self,
        port: &'a dyn workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort,
    ) -> Self {
        self.workflow_agent_frame_materialization = Some(port);
        self
    }

    pub fn with_workflow_graph_planner(
        mut self,
        planner: &'a dyn workflow_graph_planning_port::WorkflowGraphPlanningPort,
    ) -> Self {
        self.workflow_graph_planner = Some(planner);
        self
    }

    pub fn with_project_projection_notifications(
        mut self,
        port: Option<Arc<dyn ProjectProjectionNotificationPort>>,
    ) -> Self {
        self.project_projection_notifications = port;
        self
    }

    /// 按 typed ExecutionIntent 编排对应目标锚点。
    pub async fn dispatch(
        &self,
        intent: &ExecutionIntent,
    ) -> Result<ExecutionDispatchResult, WorkflowApplicationError> {
        match intent {
            ExecutionIntent::AgentLaunch(intent) => self
                .launch_agent(intent)
                .await
                .map(ExecutionDispatchResult::AgentLaunch),
            ExecutionIntent::SubjectExecution(intent) => self
                .execute_subject(intent)
                .await
                .map(ExecutionDispatchResult::SubjectExecution),
            ExecutionIntent::LifecycleRunStart(intent) => self
                .start_lifecycle_run(intent)
                .await
                .map(ExecutionDispatchResult::LifecycleRunStart),
            ExecutionIntent::InteractionDispatch(intent) => self
                .open_interaction_gate(intent)
                .await
                .map(ExecutionDispatchResult::InteractionGateOpened),
        }
    }

    pub async fn launch_agent(
        &self,
        intent: &AgentLaunchIntent,
    ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            source = ?intent.source,
            has_graph = intent.workflow_graph_ref.is_some(),
            "dispatch: launch_agent 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        diag!(
            Info,
            Subsystem::Lifecycle,
            run_id = %facts.runtime_refs.run_ref,
            agent_id = %facts.runtime_refs.agent_ref,
            "dispatch: launch_agent 完成"
        );
        Ok(AgentLaunchDispatchResult {
            runtime_refs: facts.runtime_refs,
        })
    }

    pub async fn execute_subject(
        &self,
        intent: &SubjectExecutionIntent,
    ) -> Result<SubjectExecutionDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            subject_kind = %intent.subject_ref.kind,
            subject_id = %intent.subject_ref.id,
            "dispatch: execute_subject 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let runtime_refs = facts.runtime_refs;
        let subject_execution_ref = facts.subject_execution_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "SubjectExecutionIntent 未创建 subject_execution_ref".to_string(),
            )
        })?;
        Ok(SubjectExecutionDispatchResult {
            runtime_refs,
            subject_execution_ref,
        })
    }

    pub async fn open_interaction_gate(
        &self,
        intent: &InteractionDispatchIntent,
    ) -> Result<InteractionGateOpenedDispatchResult, WorkflowApplicationError> {
        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            parent_run_id = %intent.parent_run_id,
            parent_agent_id = %intent.parent_agent_id,
            gate_kind = %intent.gate_policy.gate_kind,
            "dispatch: open_interaction_gate 进入"
        );
        let facts = self.dispatch_common(DispatchPlan::from(intent)).await?;
        let gate_ref = facts.gate_ref.ok_or_else(|| {
            WorkflowApplicationError::Internal(
                "InteractionDispatchIntent 未创建 LifecycleGate".to_string(),
            )
        })?;
        Ok(InteractionGateOpenedDispatchResult {
            runtime_refs: facts.runtime_refs,
            gate_ref,
        })
    }

    pub async fn start_lifecycle_run(
        &self,
        intent: &LifecycleRunStartIntent,
    ) -> Result<LifecycleRunStartDispatchResult, WorkflowApplicationError> {
        let result = self
            .run_orchestration_starter()
            .start_lifecycle_run(intent)
            .await?;

        diag!(
            Info,
            Subsystem::Lifecycle,
            project_id = %intent.project_id,
            run_id = %result.run_ref,
            orchestration_id = %result.orchestration_ref,
            "dispatch: start_lifecycle_run 创建 root orchestration"
        );

        Ok(result)
    }

    pub async fn materialize_workflow_agent_node(
        &self,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, WorkflowApplicationError> {
        let context = self
            .run_orchestration_starter()
            .workflow_agent_node_context(request.run_id, &request.orchestration_binding)
            .await?;
        let result = self
            .agent_runtime_materializer()
            .materialize_workflow_agent_node(context, request)
            .await?;

        diag!(
            Info,
            Subsystem::Lifecycle,
            run_id = %result.runtime_refs.run_ref,
            agent_id = %result.runtime_refs.agent_ref,
            orchestration_id = ?result.runtime_refs.orchestration_ref(),
            node_path = ?result.runtime_refs.node_path(),
            "dispatch: workflow agent node materialized，已绑定 delivery anchor"
        );

        Ok(result)
    }

    fn run_orchestration_starter(&self) -> RunOrchestrationStarter<'_> {
        RunOrchestrationStarter::new(
            self.run_repo,
            self.workflow_graph_repo,
            self.workflow_graph_planner,
        )
    }

    fn agent_runtime_materializer(&self) -> AgentRuntimeMaterializer<'_> {
        AgentRuntimeMaterializer::new(
            self.agent_repo,
            self.frame_construction,
            self.workflow_agent_frame_materialization,
        )
    }

    fn subject_association_writer(&self) -> SubjectAssociationWriter<'_> {
        SubjectAssociationWriter::new(self.association_repo)
    }

    fn lifecycle_relation_writer(&self) -> LifecycleRelationWriter<'_> {
        LifecycleRelationWriter::new(
            self.gate_repo,
            self.lineage_repo,
            self.project_projection_notifications.clone(),
        )
    }

    fn orchestration_reducer_bridge(&self) -> OrchestrationReducerBridge<'_> {
        OrchestrationReducerBridge::new(self.run_repo)
    }

    async fn dispatch_common(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        if plan.workflow_graph_ref.is_none() {
            return self.dispatch_plain(plan).await;
        }

        let workflow_graph_ref = plan
            .workflow_graph_ref
            .as_ref()
            .expect("checked workflow graph ref");
        let prepared = self
            .run_orchestration_starter()
            .prepare_graph_dispatch(&plan, workflow_graph_ref)
            .await?;
        let materialized = self
            .agent_runtime_materializer()
            .materialize_dispatch_runtime(
                &prepared.run,
                &plan,
                Some(prepared.orchestration_binding.clone()),
            )
            .await?;
        let subject_result = self
            .subject_association_writer()
            .write_for_dispatch(prepared.run.id, materialized.agent.id, &plan)
            .await?;
        let relation_result = self
            .lifecycle_relation_writer()
            .write_for_dispatch(
                &prepared.run,
                &materialized.agent,
                materialized.frame_id,
                &plan,
            )
            .await?;
        self.orchestration_reducer_bridge()
            .mark_node_claimed(prepared.run, &prepared.orchestration_binding, &materialized)
            .await?;

        Ok(DispatchFacts {
            runtime_refs: materialized.runtime_refs,
            gate_ref: relation_result.gate_ref,
            subject_execution_ref: subject_result.subject_execution_ref,
        })
    }

    async fn dispatch_plain(
        &self,
        plan: DispatchPlan,
    ) -> Result<DispatchFacts, WorkflowApplicationError> {
        let run = self
            .run_orchestration_starter()
            .resolve_or_create_plain_run(&plan)
            .await?;
        let materialized = self
            .agent_runtime_materializer()
            .materialize_dispatch_runtime(&run, &plan, None)
            .await?;
        let subject_result = self
            .subject_association_writer()
            .write_for_dispatch(run.id, materialized.agent.id, &plan)
            .await?;
        let relation_result = self
            .lifecycle_relation_writer()
            .write_for_dispatch(&run, &materialized.agent, materialized.frame_id, &plan)
            .await?;

        Ok(DispatchFacts {
            runtime_refs: materialized.runtime_refs,
            gate_ref: relation_result.gate_ref,
            subject_execution_ref: subject_result.subject_execution_ref,
        })
    }
}
