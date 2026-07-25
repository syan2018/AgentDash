use uuid::Uuid;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::workflow_graph_planning as workflow_graph_planning_port;
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, AgentRuntimeRefs, ExecutionSource, GatePolicy,
    InteractionDispatchIntent, LifecycleAgent, LifecycleRun, OrchestrationBindingRefs, RunPolicy,
    SubjectExecutionIntent, SubjectExecutionRef, SubjectRef, ValidationSeverity,
};

use crate::lifecycle::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub(crate) struct DispatchPlan {
    pub(crate) project_id: Uuid,
    pub(crate) project_agent_id: Option<Uuid>,
    pub(crate) execution_profile_override: Option<serde_json::Value>,
    pub(crate) source: ExecutionSource,
    pub(crate) created_by_user_id: Option<String>,
    pub(crate) subject_ref: Option<SubjectRef>,
    pub(crate) parent_run_id: Option<Uuid>,
    pub(crate) parent_agent_id: Option<Uuid>,
    pub(crate) workflow_graph_ref: Option<agentdash_domain::workflow::WorkflowGraphRef>,
    pub(crate) run_policy: RunPolicy,
    pub(crate) agent_policy: AgentPolicy,
    pub(crate) gate_policy: Option<GatePolicy>,
    pub(crate) stable_run_id: Option<Uuid>,
    pub(crate) stable_agent_id: Option<Uuid>,
    pub(crate) stable_frame_id: Option<Uuid>,
    pub(crate) stable_delivery_runtime_ref: Option<Uuid>,
}

pub(crate) struct DispatchFacts {
    pub(crate) runtime_refs: AgentRuntimeRefs,
    pub(crate) delivery_runtime_ref: Uuid,
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
    pub(crate) runtime_refs: AgentRuntimeRefs,
    pub(crate) delivery_runtime_ref: Uuid,
}

impl From<&AgentLaunchIntent> for DispatchPlan {
    fn from(intent: &AgentLaunchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            project_agent_id: intent.project_agent_id,
            execution_profile_override: intent.execution_profile_override.clone(),
            source: intent.source.clone(),
            created_by_user_id: intent.created_by_user_id.clone(),
            subject_ref: intent.subject_ref.clone(),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            gate_policy: None,
            stable_run_id: None,
            stable_agent_id: None,
            stable_frame_id: None,
            stable_delivery_runtime_ref: None,
        }
    }
}

impl From<&SubjectExecutionIntent> for DispatchPlan {
    fn from(intent: &SubjectExecutionIntent) -> Self {
        Self {
            project_id: intent.project_id,
            project_agent_id: intent.project_agent_id,
            execution_profile_override: None,
            source: intent.source.clone(),
            created_by_user_id: intent.created_by_user_id.clone(),
            subject_ref: Some(intent.subject_ref.clone()),
            parent_run_id: intent.parent_run_id,
            parent_agent_id: intent.parent_agent_id,
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: intent.run_policy.clone(),
            agent_policy: intent.agent_policy.clone(),
            gate_policy: None,
            stable_run_id: None,
            stable_agent_id: None,
            stable_frame_id: None,
            stable_delivery_runtime_ref: None,
        }
    }
}

impl From<&InteractionDispatchIntent> for DispatchPlan {
    fn from(intent: &InteractionDispatchIntent) -> Self {
        Self {
            project_id: intent.project_id,
            project_agent_id: None,
            execution_profile_override: None,
            source: intent.source.clone(),
            created_by_user_id: None,
            subject_ref: None,
            parent_run_id: Some(intent.parent_run_id),
            parent_agent_id: Some(intent.parent_agent_id),
            workflow_graph_ref: intent.workflow_graph_ref.clone(),
            run_policy: RunPolicy::AppendGraph,
            agent_policy: AgentPolicy::SpawnChild,
            gate_policy: Some(intent.gate_policy.clone()),
            stable_run_id: None,
            stable_agent_id: None,
            stable_frame_id: None,
            stable_delivery_runtime_ref: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{CapabilityPolicy, ContextPolicy, RuntimePolicy};

    #[test]
    fn project_agent_launch_preserves_run_execution_profile_override() {
        let profile = serde_json::json!({
            "executor": "PI_AGENT",
            "provider_id": "openai",
            "model_id": "gpt-5"
        });
        let intent = AgentLaunchIntent {
            project_id: Uuid::new_v4(),
            project_agent_id: Some(Uuid::new_v4()),
            execution_profile_override: Some(profile.clone()),
            source: ExecutionSource::ProjectAgent,
            created_by_user_id: Some("user-1".to_string()),
            subject_ref: None,
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
        };

        let plan = DispatchPlan::from(&intent);
        assert_eq!(plan.execution_profile_override, Some(profile));
    }
}
