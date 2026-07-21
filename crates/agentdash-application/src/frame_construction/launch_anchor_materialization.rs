use std::{path::PathBuf, sync::Arc};

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
use agentdash_platform_spi::{
    AgentConfig, HookControlTarget, MemoryDiscoveryProvider, RuntimeAdapterProvenance,
    SkillDiscoveryProvider,
};

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
    pub(super) repos: RepositorySet,
    pub(super) vfs_service: Arc<VfsService>,
    availability: Arc<dyn BackendAvailability>,
    pub(super) platform_config: Arc<PlatformConfig>,
    pub(super) lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub(super) audit_bus: SharedContextAuditBus,
    hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
    pub(super) extra_skill_dirs: Vec<PathBuf>,
    pub(super) skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub(super) memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub(super) product_runtime_bindings:
        Arc<dyn agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository>,
}

pub struct AgentRunProjectOwnerFrameConstructionDeps {
    pub repos: RepositorySet,
    pub vfs_service: Arc<VfsService>,
    pub availability: Arc<dyn BackendAvailability>,
    pub platform_config: Arc<PlatformConfig>,
    pub lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
    pub audit_bus: SharedContextAuditBus,
    pub hook_plan_compiler: Arc<dyn AgentFrameHookPlanCompiler>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub product_runtime_bindings:
        Arc<dyn agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository>,
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
            extra_skill_dirs: deps.extra_skill_dirs,
            skill_discovery_providers: deps.skill_discovery_providers,
            memory_discovery_providers: deps.memory_discovery_providers,
            product_runtime_bindings: deps.product_runtime_bindings,
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
            target_frame_id,
            subject_ref,
            runtime_thread_id,
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
        let runtime_thread_id = runtime_thread_id
            .ok_or_else(|| construction_rejected("DispatchLaunchAnchor 缺少 runtime_thread_id"))?;

        if let Some(frame_id) = target_frame_id
            && let Some(frame) = self
                .repos
                .agent_frame_repo
                .get(frame_id)
                .await
                .map_err(construction_rejected)?
        {
            if frame.agent_id != agent.id
                || frame.created_by_kind != "dispatch_launch_anchor"
                || frame.created_by_id != created_by_id
                || frame.execution_profile_json.as_ref() != execution_profile.as_ref()
            {
                return Err(construction_rejected(
                    "target_frame_id 已存在但不属于同一 launch anchor intent",
                ));
            }
            validate_project_agent_launch_surface(&frame)?;
            let mut outcome =
                AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::FrameConstruction);
            outcome.frame_id = Some(frame.id);
            outcome.agent_id = Some(frame.agent_id);
            outcome.runtime_thread_id = Some(runtime_thread_id);
            outcome.wrote_frame_revision = false;
            return Ok(outcome);
        }

        let executor_config_override = execution_profile
            .map(serde_json::from_value::<AgentConfig>)
            .transpose()
            .map_err(|error| {
                construction_rejected(format!(
                    "AgentRun effective execution profile 无法解析: {error}"
                ))
            })?;
        let mut builder = AgentFrameBuilder::new_launch_anchor(agent.id, created_by_id);
        if let Some(frame_id) = target_frame_id {
            builder = builder.with_frame_id(frame_id);
        }
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
                runtime_thread_id: runtime_thread_id.clone(),
            },
        )
        .await
        .map_err(construction_rejected)?;
        let mut frame = builder
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(construction_rejected)?;
        materialize_frame_context_discovery(
            &mut frame,
            self.vfs_service.as_ref(),
            &self.extra_skill_dirs,
            &self.skill_discovery_providers,
            &self.memory_discovery_providers,
        )
        .await?;
        let hook_plan = self
            .hook_plan_compiler
            .compile_agent_frame_hook_plan(AgentFrameHookPlanCompileQuery {
                target: HookControlTarget {
                    run_id: run.id,
                    agent_id: agent.id,
                    frame_id: frame.id,
                },
                provenance: RuntimeAdapterProvenance::runtime_thread(
                    runtime_thread_id.clone(),
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
        outcome.runtime_thread_id = Some(runtime_thread_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

pub(super) async fn materialize_frame_context_discovery(
    frame: &mut AgentFrame,
    vfs_service: &VfsService,
    extra_skill_dirs: &[PathBuf],
    skill_discovery_providers: &[Arc<dyn SkillDiscoveryProvider>],
    memory_discovery_providers: &[Arc<dyn MemoryDiscoveryProvider>],
) -> Result<(), AgentRunFrameSurfaceError> {
    use crate::agent_run::frame::AgentContextSourceFragment;
    use agentdash_application_agentrun::agent_run::runtime_capability_projection::{
        LaunchContextDiscoveryInput, derive_launch_context_discovery,
        normalize_capability_state_dimensions,
    };

    let vfs = frame
        .typed_vfs()
        .ok_or_else(|| construction_rejected("AgentFrame 缺少 discovery 所需 canonical VFS"))?;
    let mut capability_state = frame.typed_capability_state().ok_or_else(|| {
        construction_rejected("AgentFrame 缺少 discovery 所需 canonical CapabilityState")
    })?;
    let mcp_servers = frame.typed_mcp_servers();
    let discovery = derive_launch_context_discovery(LaunchContextDiscoveryInput {
        vfs_service,
        launch_vfs: &vfs,
        identity: None,
        extra_skill_dirs,
        skill_discovery_providers,
        memory_discovery_providers,
        diagnostics_label: "product_frame_context_discovery",
    })
    .await;

    normalize_capability_state_dimensions(
        &mut capability_state,
        Some(vfs.clone()),
        mcp_servers,
        &discovery.session_capabilities,
    );
    capability_state.memory.inventory = discovery.discovered_memory;
    let discovered_skill_names = capability_state
        .skill
        .skills
        .iter()
        .flat_map(|skill| [skill.name.as_str(), skill.local_name.as_str()])
        .collect::<std::collections::BTreeSet<_>>();
    let mut missing_skill_assets = Vec::new();
    for mount in &vfs.mounts {
        if mount
            .metadata
            .get(agentdash_application_vfs::SKILL_ASSET_KEYS_METADATA_KEY)
            .is_none()
        {
            continue;
        }
        for key in agentdash_application_vfs::projected_skill_asset_keys(mount)
            .map_err(construction_rejected)?
        {
            if !discovered_skill_names.contains(key.as_str()) {
                missing_skill_assets.push(key);
            }
        }
    }
    missing_skill_assets.sort();
    missing_skill_assets.dedup();
    if !missing_skill_assets.is_empty() {
        return Err(construction_rejected(format!(
            "AgentFrame final VFS 声明的 SkillAsset 未进入 discovery: {}",
            missing_skill_assets.join(", ")
        )));
    }

    let mut surface = frame.surface_document();
    surface.capability_state = Some(
        serde_json::to_value(capability_state)
            .map_err(|error| construction_rejected(error.to_string()))?,
    );
    if let Some(snapshot) = surface.context_source_snapshot.as_mut() {
        let mut snapshot = serde_json::from_value::<
            crate::agent_run::frame::AgentContextSourceSnapshot,
        >(snapshot.clone())
        .map_err(|error| {
            construction_rejected(format!("AgentFrame context source 无法解析: {error}"))
        })?;
        snapshot
            .fragments
            .retain(|fragment| !fragment.source.starts_with("vfs_guideline:"));
        let next_order = snapshot
            .fragments
            .iter()
            .map(|fragment| fragment.order)
            .max()
            .unwrap_or_default()
            .saturating_add(10);
        snapshot
            .fragments
            .extend(discovery.discovered_guidelines.into_iter().enumerate().map(
                |(index, guideline)| {
                    let uri = format!("{}://{}", guideline.mount_id, guideline.path);
                    AgentContextSourceFragment {
                        slot: "constraint".to_owned(),
                        label: format!("project_guideline:{}", guideline.file_name),
                        order: next_order.saturating_add(i32::try_from(index).unwrap_or(i32::MAX)),
                        runtime_agent_scope: true,
                        source: format!("vfs_guideline:{uri}"),
                        content: format!("## Project Guidelines — `{uri}`\n{}", guideline.content),
                        context_usage_kind: Some(
                            agentdash_platform_spi::context_usage_kind::SYSTEM_DEVELOPER.to_owned(),
                        ),
                    }
                },
            ));
        surface.context_source_snapshot = Some(
            serde_json::to_value(snapshot)
                .map_err(|error| construction_rejected(error.to_string()))?,
        );
    } else if !discovery.discovered_guidelines.is_empty() {
        return Err(construction_rejected(
            "AgentFrame 缺少承载已发现 project guidelines 的 canonical context source",
        ));
    }
    frame.surface = Some(surface);
    frame.apply_surface_projection();
    Ok(())
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
