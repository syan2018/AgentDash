use std::sync::Arc;

use agentdash_application_ports::agent_frame_hook_plan::{
    AgentFrameHookPlanCompileQuery, AgentFrameHookPlanCompiler,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentFrameWriteRole, AgentRunFrameConstructionPort, AgentRunFrameSurfaceCommandOutcome,
    AgentRunFrameSurfaceError, FrameConstructionCommand,
};
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_application_vfs::{VfsService, validate_vfs};
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::{AgentConfig, HookControlTarget, RuntimeAdapterProvenance};

use crate::agent_run::frame::{
    AgentFrameBuilder, AgentFrameSurfaceExt, runtime_backend_anchor_from_vfs,
};
use crate::context::SharedContextAuditBus;
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;
use crate::workspace::BackendAvailability;

use super::OwnerPromptLaunchPath;
use super::composer_project_agent::{
    ProjectAgentOwnerCompositionContext, ProjectAgentOwnerCompositionInput,
    compose_project_agent_owner_frame,
};

#[derive(Clone)]
pub struct AgentRunProjectOwnerFrameConstructionAdapter {
    repos: RepositorySet,
    vfs_service: Arc<VfsService>,
    availability: Arc<dyn BackendAvailability>,
    platform_config: Arc<PlatformConfig>,
    lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    audit_bus: SharedContextAuditBus,
    hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
}

pub struct AgentRunProjectOwnerFrameConstructionDeps {
    pub repos: RepositorySet,
    pub vfs_service: Arc<VfsService>,
    pub availability: Arc<dyn BackendAvailability>,
    pub platform_config: Arc<PlatformConfig>,
    pub lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub audit_bus: SharedContextAuditBus,
    pub hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
}

impl AgentRunProjectOwnerFrameConstructionAdapter {
    pub fn new(deps: AgentRunProjectOwnerFrameConstructionDeps) -> Self {
        Self {
            repos: deps.repos,
            vfs_service: deps.vfs_service,
            availability: deps.availability,
            platform_config: deps.platform_config,
            lifecycle_surface_projection: deps.lifecycle_surface_projection,
            audit_bus: deps.audit_bus,
            hook_plan_compiler: deps.hook_plan_compiler,
        }
    }

    fn composition_context(&self) -> ProjectAgentOwnerCompositionContext<'_> {
        ProjectAgentOwnerCompositionContext {
            repos: &self.repos,
            vfs_service: self.vfs_service.as_ref(),
            availability: self.availability.as_ref(),
            platform_config: self.platform_config.as_ref(),
            lifecycle_surface_projection: self.lifecycle_surface_projection.as_ref(),
            audit_bus: Some(self.audit_bus.clone()),
        }
    }
}

#[async_trait::async_trait]
impl AgentRunFrameConstructionPort for AgentRunProjectOwnerFrameConstructionAdapter {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let FrameConstructionCommand::DispatchLaunchAnchor {
            run_id,
            agent_id,
            subject_ref,
            runtime_session_id,
            created_by_id,
            execution_profile,
        } = command
        else {
            return Err(construction_rejected(
                "ProjectAgent owner frame materializer only supports DispatchLaunchAnchor",
            ));
        };

        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(construction_rejected)?
            .ok_or_else(|| construction_rejected(format!("LifecycleRun {run_id} 不存在")))?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(agent_id)
            .await
            .map_err(construction_rejected)?
            .ok_or_else(|| construction_rejected(format!("LifecycleAgent {agent_id} 不存在")))?;
        if agent.run_id != run.id || agent.project_id != run.project_id {
            return Err(construction_rejected(format!(
                "LifecycleAgent {agent_id} 与 LifecycleRun {run_id} 的 owner 坐标不一致"
            )));
        }
        let runtime_session_id = runtime_session_id
            .ok_or_else(|| construction_rejected("DispatchLaunchAnchor 缺少 runtime_session_id"))?;

        let executor_config_override = execution_profile
            .map(serde_json::from_value::<AgentConfig>)
            .transpose()
            .map_err(|error| {
                construction_rejected(format!(
                    "AgentRun effective execution profile 无法解析: {error}"
                ))
            })?;
        let builder = AgentFrameBuilder::new_launch_anchor(agent.id, created_by_id);
        let (builder, _extras) = compose_project_agent_owner_frame(
            &self.composition_context(),
            ProjectAgentOwnerCompositionInput {
                builder,
                agent: &agent,
                run: &run,
                subject_ref,
                executor_config_override,
                user_input: Vec::new(),
                existing_vfs: None,
                active_workflow: None,
                launch_path: OwnerPromptLaunchPath::OwnerBootstrap,
                runtime_session_id: runtime_session_id.clone(),
            },
        )
        .await
        .map_err(construction_rejected)?;
        let mut frame = builder
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(construction_rejected)?;
        let hook_plan = self
            .hook_plan_compiler
            .compile_agent_frame_hook_plan(AgentFrameHookPlanCompileQuery {
                target: HookControlTarget {
                    run_id: run.id,
                    agent_id: agent.id,
                    frame_id: frame.id,
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    runtime_session_id.clone(),
                    None,
                    "agent_frame_hook_plan_construction",
                ),
            })
            .await
            .map_err(construction_rejected)?;
        let hook_plan = serde_json::to_value(hook_plan).map_err(|error| {
            construction_rejected(format!("AgentFrame HookPlan 无法序列化: {error}"))
        })?;
        frame.attach_immutable_hook_plan(hook_plan);
        validate_project_agent_launch_surface(&frame)?;
        self.repos
            .agent_frame_repo
            .create(&frame)
            .await
            .map_err(construction_rejected)?;

        let mut outcome =
            AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::FrameConstruction);
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_session_id = Some(runtime_session_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

fn validate_project_agent_launch_surface(
    frame: &AgentFrame,
) -> Result<(), AgentRunFrameSurfaceError> {
    let vfs = frame.typed_vfs().ok_or_else(|| {
        construction_rejected("ProjectAgent owner Business Surface 未生成 canonical VFS")
    })?;
    validate_vfs(&vfs)
        .map_err(|error| construction_rejected(format!("ProjectAgent owner VFS 无效: {error}")))?;
    if vfs.default_mount().is_none() {
        return Err(construction_rejected(
            "ProjectAgent owner Business Surface 缺少可用的 workspace default mount",
        ));
    }
    let backend_anchor = runtime_backend_anchor_from_vfs(
        &vfs,
        Some("project_agent_owner_frame_construction".to_string()),
    )
    .map_err(|error| {
        construction_rejected(format!(
            "ProjectAgent owner default mount 的 runtime backend anchor 无效: {error}"
        ))
    })?;
    if backend_anchor.is_none() {
        return Err(construction_rejected(
            "ProjectAgent owner workspace default mount 缺少 canonical runtime backend anchor",
        ));
    }
    frame.validated_hook_plan().map_err(construction_rejected)?;
    Ok(())
}

fn construction_rejected(error: impl std::fmt::Display) -> AgentRunFrameSurfaceError {
    AgentRunFrameSurfaceError::ConstructionRejected {
        message: error.to_string(),
    }
}
