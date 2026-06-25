use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress;
use agentdash_application_ports::lifecycle_surface_projection as lifecycle_surface_port;
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
#[cfg(test)]
use agentdash_domain::workflow::AgentFrame;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_spi::CapabilityState;
#[cfg(test)]
use agentdash_spi::Vfs;
#[cfg(test)]
use uuid::Uuid;

use crate::agent_run::frame::builder::{
    AgentFrameActivationSurfaceInput, AgentFrameBuilder, build_lifecycle_activation_surface,
};
#[cfg(test)]
use crate::error::WorkflowApplicationError;

#[derive(Clone)]
pub struct AgentRunLaunchAnchorFrameConstructionAdapter {
    frame_repo: Arc<dyn AgentFrameRepository>,
}

impl AgentRunLaunchAnchorFrameConstructionAdapter {
    pub fn new(frame_repo: Arc<dyn AgentFrameRepository>) -> Self {
        Self { frame_repo }
    }
}

#[async_trait::async_trait]
impl agent_frame_materialization_port::AgentRunFrameConstructionPort
    for AgentRunLaunchAnchorFrameConstructionAdapter
{
    async fn execute_frame_construction_command(
        &self,
        command: agent_frame_materialization_port::FrameConstructionCommand,
    ) -> Result<
        agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome,
        agent_frame_materialization_port::AgentRunFrameSurfaceError,
    > {
        let agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
            agent_id,
            runtime_session_id,
            created_by_id,
            ..
        } = command
        else {
            return Err(
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: "launch anchor adapter only supports DispatchLaunchAnchor commands"
                        .to_string(),
                },
            );
        };

        let frame = AgentFrameBuilder::new_launch_anchor(agent_id, created_by_id)
            .with_runtime_session(runtime_session_id.clone())
            .build(self.frame_repo.as_ref())
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
        let mut outcome = agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
            agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
        );
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_session_id = Some(runtime_session_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

#[derive(Clone)]
pub struct AgentRunWorkflowNodeFrameMaterializationAdapter {
    frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_surface_projection: Arc<dyn lifecycle_surface_port::LifecycleSurfaceProjectionPort>,
}

impl AgentRunWorkflowNodeFrameMaterializationAdapter {
    pub fn new(
        frame_repo: Arc<dyn AgentFrameRepository>,
        lifecycle_surface_projection: Arc<
            dyn lifecycle_surface_port::LifecycleSurfaceProjectionPort,
        >,
    ) -> Self {
        Self {
            frame_repo,
            lifecycle_surface_projection,
        }
    }
}

#[async_trait::async_trait]
impl workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationPort
    for AgentRunWorkflowNodeFrameMaterializationAdapter
{
    async fn materialize_workflow_agent_node_frame(
        &self,
        input: workflow_node_frame_port::WorkflowAgentNodeFrameMaterializationInput,
    ) -> Result<
        agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome,
        agent_frame_materialization_port::AgentRunFrameSurfaceError,
    > {
        let anchor_frame = AgentFrameBuilder::new_launch_anchor(
            input.agent_id,
            input.created_by_id.clone(),
        )
        .with_runtime_session(input.runtime_session_id.clone())
        .build(self.frame_repo.as_ref())
        .await
        .map_err(|error| {
            agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            }
        })?;

        let node_projection = lifecycle_surface_port::OrchestrationNodeProjectionInput {
            run_id: input.run_id,
            orchestration_id: input.orchestration_id,
            node_path: input.node_path.clone(),
            lifecycle_key: input.lifecycle_key.clone(),
            attempt: input.attempt,
            writable_port_keys: lifecycle_surface_port::writable_port_keys_for_activity(
                &input.activity,
            ),
        };
        let projected_surface = self
            .lifecycle_surface_projection
            .project_lifecycle_surface(lifecycle_surface_port::AgentRunLifecycleSurfaceInput {
                base_vfs: input.base_vfs,
                address: AgentRunRuntimeAddress {
                    run_id: input.run_id,
                    agent_id: input.agent_id,
                    frame_id: anchor_frame.id,
                },
                message_stream: None,
                project_id: input.project_id,
                mode: lifecycle_surface_port::AgentRunLifecycleSurfaceMode::WorkflowNodeExecutionSurface,
                explicit_skill_asset_keys: Vec::new(),
                builtin_skills: lifecycle_surface_port::BuiltinLifecycleSkillPolicy::PreserveProjected,
                node_evidence: Some(node_projection.evidence_ref()),
                node_projection: Some(node_projection),
            })
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;

        let activation = lifecycle_surface_port::ActivityActivation {
            capability_state: CapabilityState::default(),
            mcp_servers: Vec::new(),
            capability_keys: Default::default(),
            kickoff_prompt: kickoff_prompt_for_activity(
                &input.lifecycle_key,
                &input.activity,
                &input.ready_port_keys,
            ),
            lifecycle_mount: projected_surface.lifecycle_mount,
            lifecycle_vfs: projected_surface.vfs,
            mount_directives: input
                .workflow_contract
                .as_ref()
                .map(|contract| contract.capability_config.mount_directives.clone())
                .unwrap_or_default(),
        };
        let surface = build_lifecycle_activation_surface(AgentFrameActivationSurfaceInput {
            activation: &activation,
            base_vfs: None,
            inherit_skills_from: None,
        });
        let mut draft = surface.to_surface_draft();
        draft.execution_profile = input.inherited_executor_config;
        let _surface_frame = AgentFrameBuilder::new(input.agent_id)
            .with_created_by("workflow_agent_node_materialization", input.created_by_id)
            .with_runtime_session(input.runtime_session_id.clone())
            .with_surface_draft(&draft)
            .build(self.frame_repo.as_ref())
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;

        let mut outcome = agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
            agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
        );
        outcome.frame_id = Some(anchor_frame.id);
        outcome.agent_id = Some(input.agent_id);
        outcome.runtime_session_id = Some(input.runtime_session_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

fn kickoff_prompt_for_activity(
    lifecycle_key: &str,
    activity: &agentdash_domain::workflow::ActivityDefinition,
    ready_port_keys: &std::collections::BTreeSet<String>,
) -> lifecycle_surface_port::KickoffPromptFragment {
    let node_key = &activity.key;
    let desc = activity.description.trim();
    let node_title = if desc.is_empty() {
        format!("`{node_key}`")
    } else {
        format!("`{node_key}`({desc})")
    };
    lifecycle_surface_port::KickoffPromptFragment {
        title_line: format!("你正在执行 lifecycle `{lifecycle_key}` 的 node {node_title}。"),
        output_section: render_output_section(&activity.output_ports),
        input_section: render_input_section(&activity.input_ports, ready_port_keys),
    }
}

fn render_output_section(ports: &[agentdash_domain::workflow::OutputPortDefinition]) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items = ports
        .iter()
        .map(|port| format!("- `{}`: {}", port.key, port.description))
        .collect::<Vec<_>>()
        .join("\n");
    format!("请写入以下输出端口：\n{items}")
}

fn render_input_section(
    ports: &[agentdash_domain::workflow::InputPortDefinition],
    ready_port_keys: &std::collections::BTreeSet<String>,
) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items = ports
        .iter()
        .map(|port| {
            let status = if ready_port_keys.contains(&port.key) {
                "ready"
            } else {
                "pending"
            };
            format!("- `{}` ({status}): {}", port.key, port.description)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("输入端口状态：\n{items}")
}

#[cfg(test)]
pub(crate) async fn construct_launch_anchor_frame_with_vfs(
    frame_repo: &dyn AgentFrameRepository,
    agent_id: Uuid,
    runtime_session_ref: Option<Uuid>,
    frame_created_by_id: Option<String>,
    vfs: &Vfs,
) -> Result<AgentFrame, WorkflowApplicationError> {
    let mut builder = AgentFrameBuilder::new_launch_anchor(agent_id, frame_created_by_id);
    if let Some(session_id) = runtime_session_ref {
        builder = builder.with_runtime_session(session_id.to_string());
    }
    Ok(builder.with_vfs_typed(vfs).build(frame_repo).await?)
}
