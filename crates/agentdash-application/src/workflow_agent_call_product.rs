use std::{collections::BTreeSet, sync::Arc};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeProvisioningRequest, ProductAgentFrameRef, ProductAgentSurfaceFacts,
    ProductExecutionProfileRef, WorkflowAgentCallProductGraphPort,
    WorkflowAgentCallProductGraphRepository, WorkflowAgentCallProductProvisioningPort,
    WorkflowAgentCallProductSaga, WorkflowAgentCallTargetMaterialization,
};
use agentdash_application_ports::lifecycle_materialization::{
    WorkflowAgentNodeMaterializationPort, WorkflowAgentNodeMaterializationRequest,
};
use agentdash_application_workflow::WorkflowAgentCallRequest;
use agentdash_domain::{
    agent::ProjectAgentRepository,
    workflow::{
        AgentFrameRepository, LifecycleAgentRepository, OrchestrationBindingRefs, RuntimePolicy,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

pub struct ApplicationWorkflowAgentCallProductAdapter {
    graph: Arc<dyn WorkflowAgentCallProductGraphRepository>,
    materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
    lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    project_agents: Arc<dyn ProjectAgentRepository>,
}

impl ApplicationWorkflowAgentCallProductAdapter {
    pub fn new(
        graph: Arc<dyn WorkflowAgentCallProductGraphRepository>,
        materialization: Arc<dyn WorkflowAgentNodeMaterializationPort>,
        lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        project_agents: Arc<dyn ProjectAgentRepository>,
    ) -> Self {
        Self {
            graph,
            materialization,
            lifecycle_agents,
            frames,
            project_agents,
        }
    }

    async fn resolve_project_agent_id(
        &self,
        request: &WorkflowAgentCallRequest,
    ) -> Result<Uuid, String> {
        let target = request.target_intent.target();
        if let Some(existing) = self
            .lifecycle_agents
            .get(target.agent_id)
            .await
            .map_err(|error| error.to_string())?
        {
            if existing.run_id != target.run_id || existing.project_id != request.project_id {
                return Err("Workflow AgentCall target agent authority drifted".to_string());
            }
            return existing.project_agent_id.ok_or_else(|| {
                "Workflow AgentCall target agent lacks Product ProjectAgent selection".to_string()
            });
        }

        let selections = self
            .lifecycle_agents
            .list_by_run(target.run_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|agent| agent.project_id == request.project_id && agent.status == "active")
            .filter_map(|agent| agent.project_agent_id)
            .collect::<BTreeSet<_>>();
        if selections.len() != 1 {
            return Err(format!(
                "Workflow AgentCall CreateNew requires one unambiguous run ProjectAgent selection, found {}",
                selections.len()
            ));
        }
        Ok(*selections.iter().next().expect("one selection"))
    }

    async fn execution_profile(
        &self,
        project_id: Uuid,
        project_agent_id: Uuid,
    ) -> Result<ProductExecutionProfileRef, String> {
        let project_agent = self
            .project_agents
            .get_by_project_and_id(project_id, project_agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!("Workflow AgentCall ProjectAgent {project_agent_id} does not exist")
            })?;
        let config = project_agent
            .preset_config()
            .map_err(|error| error.to_string())?
            .to_agent_config(&project_agent.agent_type);
        let mut profile = ProductExecutionProfileRef {
            profile_key: config.executor.clone(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::to_value(config).map_err(|error| error.to_string())?,
            credential_scope: None,
        };
        profile.refresh_digest();
        Ok(profile)
    }
}

#[async_trait]
impl WorkflowAgentCallProductGraphPort for ApplicationWorkflowAgentCallProductAdapter {
    async fn materialize_target(
        &self,
        request: &WorkflowAgentCallRequest,
        runtime_thread_id: &RuntimeThreadId,
        effect_id: &str,
    ) -> Result<(), String> {
        let target = request.target_intent.target();
        let project_agent_id = self.resolve_project_agent_id(request).await?;
        let execution_profile = self
            .execution_profile(request.project_id, project_agent_id)
            .await?;
        let delivery_runtime_ref = Uuid::parse_str(runtime_thread_id.as_str())
            .map_err(|error| format!("Workflow AgentCall RuntimeThreadId is not UUID: {error}"))?;
        let result = self
            .materialization
            .materialize_workflow_agent_node(WorkflowAgentNodeMaterializationRequest {
                run_id: target.run_id,
                target_agent_id: Some(target.agent_id),
                project_agent_id: Some(project_agent_id),
                delivery_runtime_ref: Some(delivery_runtime_ref),
                orchestration_binding: OrchestrationBindingRefs::new(
                    request.identity.orchestration_id,
                    request.identity.node_path.clone(),
                    request.identity.attempt,
                ),
                runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
                frame_created_by_id: Some(request.identity.request_id.clone()),
                workflow_contract: Some(request.procedure_contract.clone()),
                inherited_executor_config: Some(
                    serde_json::from_value(execution_profile.configuration.clone())
                        .map_err(|error| error.to_string())?,
                ),
            })
            .await
            .map_err(|error| error.to_string())?;
        if result.runtime_refs.run_ref != target.run_id
            || result.runtime_refs.agent_ref != target.agent_id
            || result.delivery_runtime_ref != delivery_runtime_ref
        {
            return Err(
                "Workflow AgentCall lifecycle materialization evidence drifted".to_string(),
            );
        }

        let expected = WorkflowAgentCallTargetMaterialization {
            request_id: request.identity.request_id.clone(),
            payload_digest: request.payload_digest.clone(),
            target: target.clone(),
            project_agent_id: Some(project_agent_id),
            effect_id: effect_id.to_string(),
        };
        let committed = self
            .graph
            .materialize_target_idempotent(expected.clone())
            .await?;
        if committed != expected {
            return Err("Workflow AgentCall Product graph evidence drifted".to_string());
        }
        Ok(())
    }

    async fn commit_runtime_binding(
        &self,
        request_id: &str,
        payload_digest: &str,
        target: &agentdash_domain::agent_run_target::AgentRunTarget,
        runtime_thread_id: &RuntimeThreadId,
        binding: &agentdash_agent_runtime_contract::ManagedRuntimeSourceBindingEvidence,
        effect_id: &str,
    ) -> Result<(), String> {
        let adapter = agentdash_application_agentrun::agent_run::
            DurableWorkflowAgentCallProductGraphAdapter::new(self.graph.clone());
        adapter
            .commit_runtime_binding(
                request_id,
                payload_digest,
                target,
                runtime_thread_id,
                binding,
                effect_id,
            )
            .await
    }
}

#[async_trait]
impl WorkflowAgentCallProductProvisioningPort for ApplicationWorkflowAgentCallProductAdapter {
    async fn resolve_provisioning(
        &self,
        saga: &WorkflowAgentCallProductSaga,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String> {
        let target = saga.target();
        let agent = self
            .lifecycle_agents
            .get(target.agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Workflow AgentCall LifecycleAgent is not materialized".to_string())?;
        if agent.run_id != target.run_id || agent.project_id != saga.request.project_id {
            return Err("Workflow AgentCall LifecycleAgent authority drifted".to_string());
        }
        let project_agent_id = agent.project_agent_id.ok_or_else(|| {
            "Workflow AgentCall LifecycleAgent lacks ProjectAgent selection".to_string()
        })?;
        let frame = self
            .frames
            .get_latest(agent.id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Workflow AgentCall AgentFrame is not materialized".to_string())?;
        if frame.created_by_id.as_deref() != Some(&saga.request.identity.request_id) {
            return Err("Workflow AgentCall AgentFrame provenance drifted".to_string());
        }
        let execution_profile = self
            .execution_profile(saga.request.project_id, project_agent_id)
            .await?;
        Ok(AgentRunProductRuntimeProvisioningRequest {
            target: target.clone(),
            runtime_thread_id: saga.runtime_thread_id.clone(),
            idempotency_key: format!("{}:runtime", saga.request.identity.request_id),
            frame: ProductAgentFrameRef {
                frame_id: frame.id,
                agent_id: frame.agent_id,
                revision: u64::try_from(frame.revision)
                    .map_err(|_| "Workflow AgentCall frame revision is invalid".to_string())?,
            },
            execution_profile,
            surface_facts: ProductAgentSurfaceFacts::from_frame(&frame),
        })
    }
}
