use std::sync::Arc;

use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlanCompileQuery, AgentFrameHookPlanCompiler,
};
use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress;
use agentdash_application_ports::lifecycle_surface_projection as lifecycle_surface_port;
use agentdash_application_ports::workflow_agent_frame_materialization as workflow_node_frame_port;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
use agentdash_platform_spi::{CapabilityState, HookControlTarget, RuntimeAdapterProvenance};

use crate::agent_run::frame::builder::{
    AgentFrameActivationSurfaceInput, AgentFrameBuilder, build_lifecycle_activation_surface,
};

#[derive(Clone)]
pub struct AgentRunLaunchAnchorFrameConstructionAdapter {
    frame_repo: Arc<dyn AgentFrameRepository>,
    hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
}

impl AgentRunLaunchAnchorFrameConstructionAdapter {
    pub fn new(
        frame_repo: Arc<dyn AgentFrameRepository>,
        hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
    ) -> Self {
        Self {
            frame_repo,
            hook_plan_compiler,
        }
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
            run_id,
            agent_id,
            runtime_session_id,
            created_by_id,
            execution_profile,
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

        let mut builder = AgentFrameBuilder::new_launch_anchor(agent_id, created_by_id);
        if let Some(execution_profile) = execution_profile {
            builder = builder.with_execution_profile_raw(execution_profile);
        }
        let mut frame = builder
            .build_uncommitted(self.frame_repo.as_ref())
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
        attach_hook_plan(
            self.hook_plan_compiler.as_ref(),
            run_id,
            agent_id,
            runtime_session_id.as_deref(),
            &mut frame,
        )
        .await?;
        self.frame_repo.create(&frame).await.map_err(|error| {
            agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            }
        })?;
        let mut outcome = agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
            agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
        );
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_session_id = runtime_session_id;
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

#[derive(Clone)]
pub struct AgentRunWorkflowNodeFrameMaterializationAdapter {
    frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_surface_projection: Arc<dyn lifecycle_surface_port::LifecycleSurfaceProjectionPort>,
    hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
}

impl AgentRunWorkflowNodeFrameMaterializationAdapter {
    pub fn new(
        frame_repo: Arc<dyn AgentFrameRepository>,
        lifecycle_surface_projection: Arc<
            dyn lifecycle_surface_port::LifecycleSurfaceProjectionPort,
        >,
        hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
    ) -> Self {
        Self {
            frame_repo,
            lifecycle_surface_projection,
            hook_plan_compiler,
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
        let mut anchor_frame = AgentFrameBuilder::new_launch_anchor(
            input.agent_id,
            input.created_by_id.clone(),
        )
        .build_uncommitted(self.frame_repo.as_ref())
        .await
        .map_err(|error| {
            agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            }
        })?;
        attach_hook_plan(
            self.hook_plan_compiler.as_ref(),
            input.run_id,
            input.agent_id,
            input.runtime_session_id.as_deref(),
            &mut anchor_frame,
        )
        .await?;
        self.frame_repo
            .create(&anchor_frame)
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
        let mut surface_frame = AgentFrameBuilder::new(input.agent_id)
            .with_created_by("workflow_agent_node_materialization", input.created_by_id)
            .with_surface_draft(&draft)
            .build_uncommitted(self.frame_repo.as_ref())
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
        attach_hook_plan(
            self.hook_plan_compiler.as_ref(),
            input.run_id,
            input.agent_id,
            input.runtime_session_id.as_deref(),
            &mut surface_frame,
        )
        .await?;
        self.frame_repo
            .create(&surface_frame)
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
        outcome.runtime_session_id = input.runtime_session_id;
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

async fn attach_hook_plan(
    compiler: &dyn AgentFrameHookPlanCompiler,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    runtime_session_id: Option<&str>,
    frame: &mut AgentFrame,
) -> Result<(), agent_frame_materialization_port::AgentRunFrameSurfaceError> {
    let plan = compiler
        .compile_agent_frame_hook_plan(AgentFrameHookPlanCompileQuery {
            target: HookControlTarget {
                run_id,
                agent_id,
                frame_id: frame.id,
            },
            provenance: RuntimeAdapterProvenance::runtime_session(
                runtime_session_id
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("frame-construction-{run_id}-{agent_id}")),
                None,
                "agent_frame_hook_plan_construction",
            ),
        })
        .await
        .map_err(|error| {
            agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            }
        })?;
    let hook_plan = serde_json::to_value(plan).map_err(|error| {
        agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
            message: format!("AgentFrame HookPlan serialization failed: {error}"),
        }
    })?;
    frame.attach_immutable_hook_plan(hook_plan);
    Ok(())
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
mod tests {
    use super::*;
    use std::sync::Mutex;

    use agentdash_agent_runtime_contract::HookPlanRevision;
    use agentdash_application_ports::agent_frame_hook_plan::{
        AgentFrameHookPlan, AgentFrameHookPlanCompileError,
    };
    use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
    use agentdash_domain::DomainError;

    #[derive(Default)]
    struct FrameRepo(Mutex<Vec<AgentFrame>>);

    #[async_trait::async_trait]
    impl AgentFrameRepository for FrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.0.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: uuid::Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_latest(
            &self,
            agent_id: uuid::Uuid,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(
            &self,
            agent_id: uuid::Uuid,
        ) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    struct EmptyPlanCompiler;

    #[async_trait::async_trait]
    impl AgentFrameHookPlanCompiler for EmptyPlanCompiler {
        async fn compile_agent_frame_hook_plan(
            &self,
            _query: AgentFrameHookPlanCompileQuery,
        ) -> Result<AgentFrameHookPlan, AgentFrameHookPlanCompileError> {
            AgentFrameHookPlan::compile(HookPlanRevision(1), Vec::new())
        }
    }

    #[tokio::test]
    async fn generic_launch_anchor_persists_a_valid_hook_plan() {
        use crate::agent_run::frame::AgentFrameSurfaceExt;

        let repo = Arc::new(FrameRepo::default());
        let adapter = AgentRunLaunchAnchorFrameConstructionAdapter::new(
            repo.clone(),
            Arc::new(EmptyPlanCompiler),
        );
        let run_id = uuid::Uuid::new_v4();
        let agent_id = uuid::Uuid::new_v4();
        adapter
            .execute_frame_construction_command(
                agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
                    run_id,
                    agent_id,
                    subject_ref: None,
                    runtime_session_id: Some("generic-launch".to_string()),
                    created_by_id: None,
                    execution_profile: None,
                },
            )
            .await
            .expect("construct launch anchor");

        let frame = repo
            .get_latest(agent_id)
            .await
            .unwrap()
            .expect("persisted frame");
        assert!(frame.validated_hook_plan().unwrap().requirements.is_empty());
        assert_eq!(
            frame.surface_document().hook_plan,
            frame.hook_plan,
            "construction must persist HookPlan in the canonical surface document"
        );
    }
}
