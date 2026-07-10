use std::{path::PathBuf, sync::Arc};

use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameRuntimeTarget, RuntimeSurfaceAdoptionError, RuntimeSurfaceAdoptionPort,
};
use agentdash_application_vfs::VfsService;
use agentdash_domain::workflow::AgentFrameRepository;
use async_trait::async_trait;

use crate::agent_run::{
    AgentRunEffectiveCapabilityService, AgentRunEffectiveCapabilityView,
    AgentRunRuntimeSurfaceQueryPort, RuntimeSurfaceQueryPurpose,
};

/// 将已经持久化的 canonical AgentFrame surface 收敛到当前 active runtime。
///
/// Interaction 的 definition/instance/attachment 生命周期不再通过这里隐式改写；
/// attachment materialization 产生新 frame 后，只调用统一 adoption 入口。
#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceUpdateService {
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    active_adopter: Arc<dyn RuntimeSurfaceAdoptionPort>,
}

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceUpdateDeps {
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub vfs_service: Option<Arc<VfsService>>,
    pub active_adopter: Arc<dyn RuntimeSurfaceAdoptionPort>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
}

impl AgentRunRuntimeSurfaceUpdateService {
    pub fn new(deps: AgentRunRuntimeSurfaceUpdateDeps) -> Self {
        let _ = (
            deps.vfs_service,
            deps.extra_skill_dirs,
            deps.skill_discovery_providers,
        );
        Self {
            surface_query: deps.surface_query,
            frame_repo: deps.frame_repo,
            active_adopter: deps.active_adopter,
        }
    }

    pub async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, String> {
        self.active_adopter
            .adopt_runtime_surface(target)
            .await
            .map_err(|error| error.to_string())
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
            frame_id: surface.current_surface_frame_id,
            delivery_runtime_session_id: session_id.to_string(),
        };
        let frame = self
            .frame_repo
            .get(surface.current_surface_frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("AgentFrame `{}` 不存在", surface.current_surface_frame_id))?;
        Ok(AgentRunEffectiveCapabilityService::effective_view_from_frame(target, &frame))
    }
}

#[async_trait]
impl RuntimeSurfaceAdoptionPort for AgentRunRuntimeSurfaceUpdateService {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError> {
        self.active_adopter.adopt_runtime_surface(target).await
    }
}
