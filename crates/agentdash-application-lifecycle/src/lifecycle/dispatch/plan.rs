use uuid::Uuid;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::runtime_session_delivery as runtime_session_delivery_port;
use agentdash_application_ports::workflow_graph_planning as workflow_graph_planning_port;
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, AgentRuntimeRefs, ExecutionSource, GatePolicy,
    InteractionDispatchIntent, LifecycleAgent, LifecycleRun, OrchestrationBindingRefs, RunPolicy,
    RuntimePolicy, SubjectExecutionIntent, SubjectExecutionRef, SubjectRef, ValidationSeverity,
};

use crate::lifecycle::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub(crate) struct DispatchPlan {
    pub(crate) project_id: Uuid,
    pub(crate) source: ExecutionSource,
    pub(crate) created_by_user_id: Option<String>,
    pub(crate) subject_ref: Option<SubjectRef>,
    pub(crate) parent_run_id: Option<Uuid>,
    pub(crate) parent_agent_id: Option<Uuid>,
    pub(crate) workflow_graph_ref: Option<agentdash_domain::workflow::WorkflowGraphRef>,
    pub(crate) run_policy: RunPolicy,
    pub(crate) agent_policy: AgentPolicy,
    pub(crate) runtime_policy: RuntimePolicy,
    pub(crate) gate_policy: Option<GatePolicy>,
}

pub(crate) struct DispatchFacts {
    pub(crate) runtime_refs: AgentRuntimeRefs,
    pub(crate) runtime_session_ref: Option<Uuid>,
    pub(crate) gate_ref: Option<Uuid>,
    pub(crate) subject_execution_ref: Option<SubjectExecutionRef>,
}

pub(crate) struct PreparedGraphDispatch {
    pub(crate) run: LifecycleRun,
    pub(crate) orchestration_binding: OrchestrationBindingRefs,
}

pub(crate) struct WorkflowAgentNodeRuntimeContext {
    pub(crate) run: LifecycleRun,
    pub(crate) lifecycle_key: String,
    pub(crate) activity: agentdash_domain::workflow::ActivityDefinition,
}

pub(crate) struct MaterializedAgentRuntime {
    pub(crate) agent: LifecycleAgent,
    pub(crate) frame_id: Uuid,
    pub(crate) runtime_session_ref: Option<Uuid>,
    pub(crate) runtime_refs: AgentRuntimeRefs,
}

impl From<&AgentLaunchIntent> for DispatchPlan {
    fn from(intent: &AgentLaunchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            created_by_user_id: intent.created_by_user_id.clone(),
            subject_ref: intent.subject_ref.clone(),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: None,
        }
    }
}

impl From<&SubjectExecutionIntent> for DispatchPlan {
    fn from(intent: &SubjectExecutionIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            created_by_user_id: intent.created_by_user_id.clone(),
            subject_ref: Some(intent.subject_ref.clone()),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: None,
        }
    }
}

impl From<&InteractionDispatchIntent> for DispatchPlan {
    fn from(intent: &InteractionDispatchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            source: intent.source.clone(),
            created_by_user_id: None,
            subject_ref: None,
            parent_run_id: Some(intent.parent_run_id),
            parent_agent_id: Some(intent.parent_agent_id),
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: RunPolicy::AppendGraph,
            agent_policy: AgentPolicy::SpawnChild,
            runtime_policy: intent.runtime_policy.clone(),
            gate_policy: Some(intent.gate_policy.clone()),
        }
    }
}

pub(crate) fn workflow_error_from_runtime_session_delivery_error(
    error: runtime_session_delivery_port::RuntimeSessionDeliveryError,
) -> WorkflowApplicationError {
    match error {
        runtime_session_delivery_port::RuntimeSessionDeliveryError::NotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        runtime_session_delivery_port::RuntimeSessionDeliveryError::Rejected { .. } => {
            WorkflowApplicationError::Conflict(error.to_string())
        }
        runtime_session_delivery_port::RuntimeSessionDeliveryError::Unavailable { .. } => {
            WorkflowApplicationError::Internal(error.to_string())
        }
        runtime_session_delivery_port::RuntimeSessionDeliveryError::Internal { message } => {
            WorkflowApplicationError::Internal(message)
        }
    }
}

pub(crate) fn workflow_error_from_agent_frame_materialization_error(
    error: agent_frame_materialization_port::AgentRunFrameSurfaceError,
) -> WorkflowApplicationError {
    match error {
        agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
            message,
        }
        | agent_frame_materialization_port::AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected {
            message,
        }
        | agent_frame_materialization_port::AgentRunFrameSurfaceError::ProjectionContextUnavailable {
            message,
        } => WorkflowApplicationError::Internal(message),
        agent_frame_materialization_port::AgentRunFrameSurfaceError::RoleMismatch { .. } => {
            WorkflowApplicationError::Internal(error.to_string())
        }
    }
}

pub(crate) fn workflow_error_from_workflow_graph_planning_error(
    error: workflow_graph_planning_port::WorkflowGraphPlanningError,
) -> WorkflowApplicationError {
    match error {
        workflow_graph_planning_port::WorkflowGraphPlanningError::BadRequest { message } => {
            WorkflowApplicationError::BadRequest(message)
        }
        workflow_graph_planning_port::WorkflowGraphPlanningError::NotFound { message } => {
            WorkflowApplicationError::NotFound(message)
        }
        workflow_graph_planning_port::WorkflowGraphPlanningError::BlockingDiagnostics {
            workflow_graph_id,
            diagnostics,
        } => WorkflowApplicationError::BadRequest(blocking_planning_diagnostics_message(
            workflow_graph_id,
            &diagnostics,
        )),
        workflow_graph_planning_port::WorkflowGraphPlanningError::Internal { message } => {
            WorkflowApplicationError::Internal(message)
        }
    }
}

fn blocking_planning_diagnostics_message(
    workflow_graph_id: Uuid,
    diagnostics: &[workflow_graph_planning_port::WorkflowGraphPlanningDiagnostic],
) -> String {
    let details = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == ValidationSeverity::Error)
        .map(|diagnostic| {
            format!(
                "{} at {}: {}",
                diagnostic.code, diagnostic.source_path, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "WorkflowGraph {} 无法编译为 OrchestrationPlanSnapshot: {}",
        workflow_graph_id, details
    )
}
