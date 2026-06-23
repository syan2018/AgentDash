use std::{path::PathBuf, sync::Arc};

use agentdash_agent_types::DynAgentTool;
use agentdash_domain::canvas::Canvas;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_spi::{CapabilityState, Vfs};
use async_trait::async_trait;

use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::{
    AgentFrameBuilder, AgentRunEffectiveCapabilityService, AgentRunEffectiveCapabilityView,
    AgentRunRuntimeSurfaceQueryPort, RuntimeSurfaceQueryPurpose,
};
use crate::canvas::resolve_canvas_binding_files;
use crate::session::capability_projection::{
    SessionCapabilityProjectionInput, derive_session_skill_baseline, merge_live_vfs_skill_entries,
};
use crate::session::capability_state::project_capability_state_from_frame;
use crate::session::types::AgentFrameRuntimeTarget;
use crate::vfs::{VfsService, append_canvas_mounts, refresh_canvas_mount_binding_files};

#[async_trait]
pub trait AgentRunActiveRuntimeSurfaceAdopter: Send + Sync {
    async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, String>;
}

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceUpdateService {
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    vfs_service: Option<Arc<VfsService>>,
    active_adopter: Arc<dyn AgentRunActiveRuntimeSurfaceAdopter>,
    extra_skill_dirs: Vec<PathBuf>,
    skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
}

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceUpdateDeps {
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub vfs_service: Option<Arc<VfsService>>,
    pub active_adopter: Arc<dyn AgentRunActiveRuntimeSurfaceAdopter>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
}

impl AgentRunRuntimeSurfaceUpdateService {
    pub fn new(deps: AgentRunRuntimeSurfaceUpdateDeps) -> Self {
        Self {
            surface_query: deps.surface_query,
            frame_repo: deps.frame_repo,
            vfs_service: deps.vfs_service,
            active_adopter: deps.active_adopter,
            extra_skill_dirs: deps.extra_skill_dirs,
            skill_discovery_providers: deps.skill_discovery_providers,
        }
    }

    pub async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, String> {
        self.active_adopter
            .adopt_persisted_frame_revision_into_active_runtime(target)
            .await
    }

    pub async fn expose_canvas_mount(
        &self,
        session_id: &str,
        canvas: &Canvas,
    ) -> Result<Vfs, String> {
        let surface = self
            .surface_query
            .current_runtime_surface(
                session_id,
                RuntimeSurfaceQueryPurpose::new("canvas_runtime_surface_update"),
            )
            .await
            .map_err(|error| error.to_string())?;
        let current_frame = self
            .frame_repo
            .get(surface.surface_frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("AgentFrame `{}` 不存在", surface.surface_frame_id))?;

        let before_state = project_capability_state_from_frame(&current_frame);
        let mut after_state = before_state.clone();
        let Some(mut active_vfs) = after_state.vfs.active.clone() else {
            return Err(format!(
                "AgentFrame `{}` 缺少 VFS surface，拒绝将 live VFS 作为 Canvas exposure 事实源",
                current_frame.id
            ));
        };
        append_canvas_mounts(&mut active_vfs, std::slice::from_ref(canvas));
        if let Some(vfs_service) = self.vfs_service.as_deref() {
            let binding_files =
                resolve_canvas_binding_files(canvas, &active_vfs, vfs_service).await;
            refresh_canvas_mount_binding_files(&mut active_vfs, canvas, &binding_files);
        }
        after_state.vfs.active = Some(active_vfs.clone());
        self.derive_skill_baseline_for_transition_state(Some(&before_state), &mut after_state)
            .await;

        let mut next_frame = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(&after_state)
            .with_created_by("canvas_expose", Some(current_frame.id.to_string()))
            .with_runtime_session(session_id.to_string())
            .build_uncommitted(self.frame_repo.as_ref())
            .await
            .map_err(|error| error.to_string())?;
        next_frame.append_visible_canvas_mount(&canvas.mount_id);
        next_frame.append_visible_workspace_module_ref(&format!("canvas:{}", canvas.mount_id));
        self.frame_repo
            .create(&next_frame)
            .await
            .map_err(|error| error.to_string())?;

        self.active_adopter
            .adopt_persisted_frame_revision_into_active_runtime(AgentFrameRuntimeTarget {
                frame_id: next_frame.id,
                delivery_runtime_session_id: session_id.to_string(),
            })
            .await?;

        next_frame
            .typed_vfs()
            .ok_or_else(|| format!("AgentFrame `{}` 写入后缺少 VFS surface", next_frame.id))
    }

    pub async fn effective_capability_view_for_delivery_runtime(
        &self,
        session_id: &str,
    ) -> Result<AgentRunEffectiveCapabilityView, String> {
        let surface = self
            .surface_query
            .current_runtime_surface(
                session_id,
                RuntimeSurfaceQueryPurpose::new("workspace_module_visibility"),
            )
            .await
            .map_err(|error| error.to_string())?;
        let target = AgentFrameRuntimeTarget {
            frame_id: surface.surface_frame_id,
            delivery_runtime_session_id: session_id.to_string(),
        };
        let frame = self
            .frame_repo
            .get(surface.surface_frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("AgentFrame `{}` 不存在", surface.surface_frame_id))?;
        Ok(AgentRunEffectiveCapabilityService::effective_view_from_frame(target, &frame))
    }

    async fn derive_skill_baseline_for_transition_state(
        &self,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self.derive_skill_entries_for_active_vfs(active_vfs).await else {
            return;
        };
        let existing = before_state
            .map(|state| state.skill.skills.as_slice())
            .unwrap_or_else(|| after_state.skill.skills.as_slice());
        after_state.skill.skills = merge_live_vfs_skill_entries(existing, skills);
    }

    async fn derive_skill_entries_for_active_vfs(
        &self,
        active_vfs: &Vfs,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        derive_session_skill_baseline(SessionCapabilityProjectionInput {
            vfs_service: self.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            identity: None,
            extra_skill_dirs: &self.extra_skill_dirs,
            skill_discovery_providers: &self.skill_discovery_providers,
            diagnostics_label: "agent_run_runtime_surface_update",
        })
        .await
        .map(|caps| caps.skills)
    }
}
