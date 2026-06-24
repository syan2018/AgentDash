use std::{path::PathBuf, sync::Arc};

use agentdash_agent_types::DynAgentTool;
use agentdash_domain::canvas::Canvas;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_spi::{AuthIdentity, CapabilityState, Vfs};
use async_trait::async_trait;

use crate::agent_run::AgentFrameRuntimeTarget;
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::runtime_capability::project_capability_state_from_frame;
use crate::agent_run::runtime_capability_projection::{
    RuntimeCapabilityProjectionInput, derive_runtime_skill_baseline, merge_live_vfs_skill_entries,
};
use crate::agent_run::{
    AgentFrameBuilder, AgentRunEffectiveCapabilityService, AgentRunEffectiveCapabilityView,
    AgentRunRuntimeSurfaceQueryPort, RuntimeSurfaceQueryPurpose,
};
use crate::canvas::resolve_canvas_binding_files;
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
            .get(surface.current_surface_frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("AgentFrame `{}` 不存在", surface.current_surface_frame_id))?;

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
        self.derive_skill_baseline_for_transition_state(
            Some(&before_state),
            &mut after_state,
            surface.identity.as_ref(),
        )
        .await;

        let workspace_module_ref = format!("canvas:{}", canvas.mount_id);
        if canvas_surface_projection_is_noop(
            &current_frame,
            &before_state,
            &after_state,
            &canvas.mount_id,
            &workspace_module_ref,
        ) {
            return Ok(active_vfs);
        }

        let mut next_frame = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(&after_state)
            .with_created_by("canvas_expose", Some(current_frame.id.to_string()))
            .with_runtime_session(session_id.to_string())
            .build_uncommitted(self.frame_repo.as_ref())
            .await
            .map_err(|error| error.to_string())?;
        next_frame.append_visible_canvas_mount(&canvas.mount_id);
        next_frame.append_visible_workspace_module_ref(&workspace_module_ref);
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

    async fn derive_skill_baseline_for_transition_state(
        &self,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
        identity: Option<&AuthIdentity>,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self
            .derive_skill_entries_for_active_vfs(active_vfs, identity)
            .await
        else {
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
        identity: Option<&AuthIdentity>,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        derive_runtime_skill_baseline(RuntimeCapabilityProjectionInput {
            vfs_service: self.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            identity,
            extra_skill_dirs: &self.extra_skill_dirs,
            skill_discovery_providers: &self.skill_discovery_providers,
            diagnostics_label: "agent_run_runtime_surface_update",
        })
        .await
        .map(|caps| caps.skills)
    }
}

fn canvas_surface_projection_is_noop(
    current_frame: &agentdash_domain::workflow::AgentFrame,
    before_state: &CapabilityState,
    after_state: &CapabilityState,
    canvas_mount_id: &str,
    workspace_module_ref: &str,
) -> bool {
    before_state == after_state
        && current_frame
            .visible_canvas_mount_ids()
            .iter()
            .any(|mount_id| mount_id == canvas_mount_id)
        && current_frame
            .visible_workspace_module_refs()
            .iter()
            .any(|module_ref| module_ref == workspace_module_ref)
}

#[async_trait]
impl AgentRunActiveRuntimeSurfaceAdopter for AgentRunRuntimeSurfaceUpdateService {
    async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, String> {
        self.active_adopter
            .adopt_persisted_frame_revision_into_active_runtime(target)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentdash_domain::canvas::Canvas;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::AgentFrame;
    use agentdash_spi::{AgentConfig, RuntimeMcpServer, ToolCluster};
    use chrono::Utc;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::agent_run::{
        AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure, AgentRunRuntimeSurfaceProvenance,
        AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceWithBackend,
    };
    use crate::lifecycle::AgentRunRuntimeAddress;
    use crate::test_support::workflow_repositories::MemoryAgentFrameRepository;

    struct FixedSurfaceQuery {
        surface: AgentRunRuntimeSurface,
    }

    #[async_trait::async_trait]
    impl AgentRunRuntimeSurfaceQueryPort for FixedSurfaceQuery {
        async fn current_runtime_surface(
            &self,
            _runtime_session_id: &str,
            _purpose: RuntimeSurfaceQueryPurpose,
        ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
            Ok(self.surface.clone())
        }

        async fn current_runtime_surface_with_backend(
            &self,
            _runtime_session_id: &str,
            _purpose: RuntimeSurfaceQueryPurpose,
        ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError> {
            unreachable!("canvas expose no-op tests do not require backend surface")
        }
    }

    #[derive(Default)]
    struct RecordingAdopter {
        targets: Mutex<Vec<AgentFrameRuntimeTarget>>,
    }

    #[async_trait::async_trait]
    impl AgentRunActiveRuntimeSurfaceAdopter for RecordingAdopter {
        async fn adopt_persisted_frame_revision_into_active_runtime(
            &self,
            target: AgentFrameRuntimeTarget,
        ) -> Result<Vec<DynAgentTool>, String> {
            self.targets.lock().await.push(target);
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn canvas_expose_noops_when_surface_and_visibility_are_unchanged() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas = Canvas::new(
            project_id,
            "cvs-dashboard-a".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let active_vfs = vfs_with_canvas(&canvas);
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(active_vfs.clone());

        let mut frame = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&active_vfs).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());
        frame.append_visible_canvas_mount(&canvas.mount_id);
        frame.append_visible_workspace_module_ref(&format!("canvas:{}", canvas.mount_id));

        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        frame_repo.create(&frame).await.expect("frame should save");
        let adopter = Arc::new(RecordingAdopter::default());
        let service = AgentRunRuntimeSurfaceUpdateService::new(AgentRunRuntimeSurfaceUpdateDeps {
            surface_query: Arc::new(FixedSurfaceQuery {
                surface: runtime_surface_for_frame(
                    "runtime-1",
                    run_id,
                    project_id,
                    &frame,
                    capability_state,
                    active_vfs.clone(),
                ),
            }),
            frame_repo: frame_repo.clone(),
            vfs_service: None,
            active_adopter: adopter.clone(),
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });

        let returned_vfs = service
            .expose_canvas_mount("runtime-1", &canvas)
            .await
            .expect("repeated expose should succeed");

        assert_eq!(returned_vfs, active_vfs);
        assert_eq!(
            frame_repo
                .list_by_agent(agent_id)
                .await
                .expect("frames should list")
                .len(),
            1,
            "unchanged Canvas exposure must not create an AgentFrame revision"
        );
        assert!(
            adopter.targets.lock().await.is_empty(),
            "unchanged Canvas exposure must not adopt active runtime"
        );
    }

    fn runtime_surface_for_frame(
        runtime_session_id: &str,
        run_id: Uuid,
        project_id: Uuid,
        frame: &AgentFrame,
        capability_state: CapabilityState,
        vfs: Vfs,
    ) -> AgentRunRuntimeSurface {
        AgentRunRuntimeSurface {
            runtime_session_id: runtime_session_id.to_string(),
            run_id,
            project_id,
            agent_id: frame.agent_id,
            runtime_address: AgentRunRuntimeAddress {
                run_id,
                agent_id: frame.agent_id,
                frame_id: frame.id,
            },
            launch_evidence_frame_id: frame.id,
            current_surface_frame_id: frame.id,
            surface_revision: frame.revision,
            capability_state,
            vfs,
            mcp_servers: Vec::new(),
            runtime_backend_anchor: None,
            active_turn_id: None,
            identity: None,
            provenance: AgentRunRuntimeSurfaceProvenance {
                launch_evidence_frame_id: frame.id,
                launch_created_by_kind: frame.created_by_kind.clone(),
                current_surface_frame_id: frame.id,
                surface_revision: frame.revision,
                surface_created_by_kind: frame.created_by_kind.clone(),
                anchor_updated_at: Utc::now(),
                orchestration_id: None,
                node_path: None,
                node_attempt: None,
            },
            closure: AgentRunRuntimeSurfaceClosure {
                capability_field_present: true,
                vfs_field_present: true,
                mcp_field_present: true,
            },
        }
    }

    fn vfs_with_canvas(canvas: &Canvas) -> Vfs {
        let mut vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-a".to_string(),
                root_ref: "D:/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: true,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: Some(canvas.project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        };
        append_canvas_mounts(&mut vfs, std::slice::from_ref(canvas));
        vfs
    }
}
