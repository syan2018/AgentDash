use std::{path::PathBuf, sync::Arc};

use agentdash_agent_types::DynAgentTool;
use agentdash_application_ports::agent_frame_materialization::{
    CanvasVisibilityReason, RuntimeSurfaceUpdateRequest,
};
use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameRuntimeTarget, RuntimeSurfaceAdoptionError, RuntimeSurfaceAdoptionPort,
};
use agentdash_application_vfs::tools::RuntimeVfsState;
use agentdash_domain::canvas::{
    Canvas, CanvasAccessProjection, CanvasScope, canvas_access_projection,
};
use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::project::{ProjectAuthorization, ProjectAuthorizationContext};
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_spi::{
    AuthIdentity, CapabilityState, RuntimeVfsAccessPolicy, RuntimeVfsAccessSource, Vfs,
};
use agentdash_workspace_module::canvas::{
    CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY, canvas_module_id, canvas_provider_root_ref,
    upsert_canvas_runtime_data_binding,
};
use async_trait::async_trait;

use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::runtime_capability::project_capability_state_from_frame;
use crate::agent_run::runtime_capability_projection::{
    RuntimeCapabilityProjectionInput, derive_runtime_skill_baseline, merge_live_vfs_skill_entries,
};
use crate::agent_run::{
    AgentFrameBuilder, AgentRunEffectiveCapabilityService, AgentRunEffectiveCapabilityView,
    AgentRunRuntimeSurfaceQueryPort, RuntimeSurfaceQueryPurpose,
};
use agentdash_application_vfs::VfsService;

const PROVIDER_CANVAS_FS: &str = "canvas_fs";

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceUpdateService {
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    vfs_service: Option<Arc<VfsService>>,
    active_adopter: Arc<dyn RuntimeSurfaceAdoptionPort>,
    extra_skill_dirs: Vec<PathBuf>,
    skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
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
            .adopt_runtime_surface(target)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn expose_canvas_mount(
        &self,
        session_id: &str,
        canvas: &Canvas,
        current_user: Option<&ProjectAuthorizationContext>,
    ) -> Result<RuntimeVfsState, String> {
        self.apply_canvas_runtime_surface_update(
            session_id,
            canvas,
            current_user,
            RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
                canvas_mount_id: canvas.mount_id.clone(),
                reason: CanvasVisibilityReason::Presented,
            },
        )
        .await
    }

    pub async fn apply_canvas_runtime_surface_update(
        &self,
        session_id: &str,
        canvas: &Canvas,
        current_user: Option<&ProjectAuthorizationContext>,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<RuntimeVfsState, String> {
        ensure_canvas_runtime_surface_request_targets_canvas(&request, canvas)?;
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
        let Some(mount_access) =
            canvas_runtime_mount_access(canvas, current_user, surface.identity.as_ref())
        else {
            return Err(format!(
                "当前身份无权将 Canvas `{}` 暴露到运行时",
                canvas.id
            ));
        };
        append_canvas_mount(&mut active_vfs, canvas, mount_access);
        if let RuntimeSurfaceUpdateRequest::CanvasBindingChanged { binding, .. } = &request {
            upsert_canvas_runtime_data_binding(&mut active_vfs, canvas, binding.clone())
                .map_err(|error| error.to_string())?;
        }
        let _ = self.vfs_service.as_deref();
        after_state.vfs.active = Some(active_vfs.clone());
        self.derive_skill_baseline_for_transition_state(
            Some(&before_state),
            &mut after_state,
            surface.identity.as_ref(),
        )
        .await;
        let active_policy = runtime_vfs_access_policy_after_canvas_mount_update(
            &surface.vfs_access_policy,
            &active_vfs,
            &canvas.mount_id,
        )?;

        let workspace_module_ref = canvas_module_id(&canvas.mount_id);
        let created_by_kind = match request {
            RuntimeSurfaceUpdateRequest::CanvasBindingChanged { .. } => "canvas_bind_data",
            RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested { .. } => "canvas_expose",
            _ => "canvas_surface_update",
        };
        let mut next_frame = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(&after_state)
            .with_created_by(created_by_kind, Some(current_frame.id.to_string()))
            .with_runtime_session(session_id.to_string())
            .build_uncommitted(self.frame_repo.as_ref())
            .await
            .map_err(|error| error.to_string())?;
        materialize_visible_canvas_mount_projection(&mut next_frame, &canvas.mount_id);
        materialize_visible_workspace_module_ref_projection(&mut next_frame, &workspace_module_ref);

        if agent_frame_runtime_surface_unchanged(&current_frame, &next_frame) {
            return Ok(RuntimeVfsState::new(active_vfs, active_policy));
        }

        self.frame_repo
            .create(&next_frame)
            .await
            .map_err(|error| error.to_string())?;

        self.active_adopter
            .adopt_runtime_surface(AgentFrameRuntimeTarget {
                frame_id: next_frame.id,
                delivery_runtime_session_id: session_id.to_string(),
            })
            .await
            .map_err(|error| error.to_string())?;

        let vfs = next_frame
            .typed_vfs()
            .ok_or_else(|| format!("AgentFrame `{}` 写入后缺少 VFS surface", next_frame.id))?;
        Ok(RuntimeVfsState::new(vfs, active_policy))
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

fn ensure_canvas_runtime_surface_request_targets_canvas(
    request: &RuntimeSurfaceUpdateRequest,
    canvas: &Canvas,
) -> Result<(), String> {
    let canvas_mount_id = match request {
        RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
            canvas_mount_id, ..
        }
        | RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id, ..
        } => canvas_mount_id,
        other => {
            return Err(format!(
                "Canvas runtime surface update received non-Canvas request: {other:?}"
            ));
        }
    };
    if canvas_mount_id == &canvas.mount_id {
        Ok(())
    } else {
        Err(format!(
            "Canvas runtime surface request target `{canvas_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasMountAccess {
    pub runtime_write_allowed: bool,
}

impl CanvasMountAccess {
    pub const fn read_only() -> Self {
        Self {
            runtime_write_allowed: false,
        }
    }
}

fn canvas_runtime_mount_access(
    canvas: &Canvas,
    current_user: Option<&ProjectAuthorizationContext>,
    surface_identity: Option<&AuthIdentity>,
) -> Option<CanvasMountAccess> {
    let current_user = current_user
        .cloned()
        .or_else(|| surface_identity.map(project_authorization_context_from_identity));
    if current_user.is_none() && canvas.scope == CanvasScope::Project {
        return Some(CanvasMountAccess::read_only());
    }
    let current_user = current_user?;
    let access = canvas_access_projection(
        canvas,
        &current_user,
        &runtime_canvas_project_access(canvas, &current_user),
    );
    if access.can_view {
        Some(CanvasMountAccess::from(access))
    } else {
        None
    }
}

impl From<CanvasAccessProjection> for CanvasMountAccess {
    fn from(access: CanvasAccessProjection) -> Self {
        Self {
            runtime_write_allowed: access.runtime_write_allowed,
        }
    }
}

fn runtime_canvas_project_access(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
) -> ProjectAuthorization {
    ProjectAuthorization {
        role: None,
        via_admin_bypass: current_user.is_admin,
        via_template_visibility: canvas.scope == CanvasScope::Project,
    }
}

fn project_authorization_context_from_identity(
    identity: &AuthIdentity,
) -> ProjectAuthorizationContext {
    ProjectAuthorizationContext::new_with_subjects(
        identity.user_id.clone(),
        vec![identity.subject.clone()],
        identity
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
        identity.is_admin,
    )
}

fn append_canvas_mount(vfs: &mut Vfs, canvas: &Canvas, access: CanvasMountAccess) {
    let mut mount = build_canvas_mount(canvas, access);
    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|existing| existing.id == mount.id)
    {
        if let Some(runtime_bindings) = existing
            .metadata
            .get(CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY)
            .cloned()
        {
            if !mount.metadata.is_object() {
                mount.metadata = serde_json::json!({});
            }
            if let Some(metadata) = mount.metadata.as_object_mut() {
                metadata.insert(
                    CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY.to_string(),
                    runtime_bindings,
                );
            }
        }
        *existing = mount;
    } else {
        vfs.mounts.push(mount);
    }
}

fn materialize_visible_canvas_mount_projection(
    frame: &mut agentdash_domain::workflow::AgentFrame,
    mount_id: &str,
) {
    frame.visible_canvas_mount_ids_json =
        merge_string_projection(frame.visible_canvas_mount_ids_json.as_ref(), mount_id);
}

fn materialize_visible_workspace_module_ref_projection(
    frame: &mut agentdash_domain::workflow::AgentFrame,
    module_ref: &str,
) {
    frame.visible_workspace_module_refs_json = merge_string_projection(
        frame.visible_workspace_module_refs_json.as_ref(),
        module_ref,
    );
}

fn merge_string_projection(
    current: Option<&serde_json::Value>,
    next_value: &str,
) -> Option<serde_json::Value> {
    let mut values = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(serde_json::Value::Array(current)) = current {
        for value in current.iter().filter_map(serde_json::Value::as_str) {
            let value = value.trim();
            if !value.is_empty() && seen.insert(value.to_string()) {
                values.push(value.to_string());
            }
        }
    }
    let next_value = next_value.trim();
    if !next_value.is_empty() && seen.insert(next_value.to_string()) {
        values.push(next_value.to_string());
    }
    if values.is_empty() {
        None
    } else {
        Some(serde_json::Value::Array(
            values.into_iter().map(serde_json::Value::String).collect(),
        ))
    }
}

fn runtime_vfs_access_policy_after_canvas_mount_update(
    current_policy: &RuntimeVfsAccessPolicy,
    active_vfs: &Vfs,
    canvas_mount_id: &str,
) -> Result<RuntimeVfsAccessPolicy, String> {
    let mut policy = current_policy.clone();
    policy.rules.retain(|rule| {
        rule.mount_id != canvas_mount_id
            || rule.source != RuntimeVfsAccessSource::SystemRuntimeProjection
    });
    let Some(canvas_mount) = active_vfs
        .mounts
        .iter()
        .find(|mount| mount.id == canvas_mount_id)
        .cloned()
    else {
        return Err(format!(
            "更新后的运行期 VFS 缺少 Canvas mount `{canvas_mount_id}`，无法同步访问策略"
        ));
    };
    let canvas_vfs = Vfs {
        mounts: vec![canvas_mount],
        default_mount_id: None,
        source_project_id: active_vfs.source_project_id.clone(),
        source_story_id: active_vfs.source_story_id.clone(),
        links: Vec::new(),
    };
    policy.rules.extend(
        RuntimeVfsAccessPolicy::whole_mounts_from_vfs_with_source(
            &canvas_vfs,
            RuntimeVfsAccessSource::SystemRuntimeProjection,
        )
        .rules,
    );
    Ok(policy)
}

fn build_canvas_mount(canvas: &Canvas, access: CanvasMountAccess) -> Mount {
    let mut capabilities = vec![
        MountCapability::Read,
        MountCapability::List,
        MountCapability::Search,
    ];
    if access.runtime_write_allowed {
        capabilities.insert(1, MountCapability::Write);
    }

    Mount {
        id: canvas.mount_id.clone(),
        provider: PROVIDER_CANVAS_FS.to_string(),
        backend_id: String::new(),
        root_ref: canvas_provider_root_ref(canvas.id),
        capabilities,
        default_write: false,
        display_name: if canvas.title.trim().is_empty() {
            format!("Canvas {}", canvas.id)
        } else {
            canvas.title.clone()
        },
        metadata: serde_json::json!({
            "canvas_id": canvas.id.to_string(),
            "canvas_mount_id": canvas.mount_id,
            "vfs_mount_id": canvas.mount_id,
            "project_id": canvas.project_id.to_string(),
            "entry_file": canvas.entry_file,
        }),
    }
}

fn agent_frame_runtime_surface_unchanged(
    current_frame: &agentdash_domain::workflow::AgentFrame,
    next_frame: &agentdash_domain::workflow::AgentFrame,
) -> bool {
    current_frame.effective_capability_json == next_frame.effective_capability_json
        && current_frame.context_slice_json == next_frame.context_slice_json
        && current_frame.vfs_surface_json == next_frame.vfs_surface_json
        && current_frame.mcp_surface_json == next_frame.mcp_surface_json
        && current_frame.execution_profile_json == next_frame.execution_profile_json
        && current_frame.visible_canvas_mount_ids_json == next_frame.visible_canvas_mount_ids_json
        && current_frame.visible_workspace_module_refs_json
            == next_frame.visible_workspace_module_refs_json
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;

    use agentdash_domain::canvas::{Canvas, CanvasDataBinding};
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::AgentFrame;
    use agentdash_spi::{
        AgentConfig, AuthIdentity, AuthMode, RuntimeMcpServer, RuntimeVfsAccessPolicy,
        RuntimeVfsAccessRule, RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern,
        ToolCluster,
    };
    use chrono::Utc;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::agent_run::{
        AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure, AgentRunRuntimeSurfaceProvenance,
        AgentRunRuntimeSurfaceQueryError, AgentRunRuntimeSurfaceWithBackend,
    };
    use crate::test_support::workflow_repositories::MemoryAgentFrameRepository;
    use agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress;
    use agentdash_application_vfs::MountProviderRegistry;

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
    impl RuntimeSurfaceAdoptionPort for RecordingAdopter {
        async fn adopt_runtime_surface(
            &self,
            target: AgentFrameRuntimeTarget,
        ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError> {
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
        materialize_visible_canvas_mount_projection(&mut frame, &canvas.mount_id);
        materialize_visible_workspace_module_ref_projection(
            &mut frame,
            &canvas_module_id(&canvas.mount_id),
        );

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
                    None,
                ),
            }),
            frame_repo: frame_repo.clone(),
            vfs_service: Some(Arc::new(VfsService::new(Arc::new(
                MountProviderRegistry::default(),
            )))),
            active_adopter: adopter.clone(),
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });

        let returned_state = service
            .expose_canvas_mount("runtime-1", &canvas, None)
            .await
            .expect("repeated expose should succeed");

        assert_eq!(returned_state.vfs, active_vfs);
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

    #[tokio::test]
    async fn project_shared_canvas_expose_appends_read_only_runtime_mount() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas = Canvas::new(
            project_id,
            "cvs-dashboard-a".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let active_vfs = base_vfs(project_id);
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(active_vfs.clone());

        let mut frame = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&active_vfs).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());

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
                    active_vfs,
                    Some(identity("alice")),
                ),
            }),
            frame_repo,
            vfs_service: Some(Arc::new(VfsService::new(Arc::new(
                MountProviderRegistry::default(),
            )))),
            active_adopter: adopter,
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });

        let returned_state = service
            .expose_canvas_mount("runtime-1", &canvas, None)
            .await
            .expect("project shared canvas expose should succeed");

        let mount = returned_state
            .vfs
            .mounts
            .iter()
            .find(|mount| mount.id == canvas.mount_id)
            .expect("Canvas mount should be appended");
        assert!(mount.supports(MountCapability::Read));
        assert!(!mount.supports(MountCapability::Write));
        assert!(mount.supports(MountCapability::List));
        assert!(mount.supports(MountCapability::Search));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::Read
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src",
            RuntimeVfsOperation::List
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::Search
        ));
    }

    #[tokio::test]
    async fn canvas_expose_preserves_existing_runtime_vfs_policy() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas = Canvas::new(
            project_id,
            "cvs-dashboard-a".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let active_vfs = base_vfs(project_id);
        let restricted_policy = RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "workspace".to_string(),
                path_pattern: RuntimeVfsPathPattern::Prefix("docs".to_string()),
                operations: BTreeSet::from([RuntimeVfsOperation::Read]),
                source: RuntimeVfsAccessSource::PermissionGrant,
            }],
        };
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(active_vfs.clone());

        let mut frame = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&active_vfs).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());

        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        frame_repo.create(&frame).await.expect("frame should save");
        let adopter = Arc::new(RecordingAdopter::default());
        let mut surface = runtime_surface_for_frame(
            "runtime-1",
            run_id,
            project_id,
            &frame,
            capability_state,
            active_vfs,
            Some(identity("alice")),
        );
        surface.vfs_access_policy = restricted_policy;
        let service = AgentRunRuntimeSurfaceUpdateService::new(AgentRunRuntimeSurfaceUpdateDeps {
            surface_query: Arc::new(FixedSurfaceQuery { surface }),
            frame_repo,
            vfs_service: Some(Arc::new(VfsService::new(Arc::new(
                MountProviderRegistry::default(),
            )))),
            active_adopter: adopter,
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });

        let returned_state = service
            .expose_canvas_mount("runtime-1", &canvas, None)
            .await
            .expect("project shared canvas expose should succeed");

        assert!(returned_state.access_policy.admits(
            "workspace",
            "docs/readme.md",
            RuntimeVfsOperation::Read
        ));
        assert!(
            !returned_state.access_policy.admits(
                "workspace",
                "src/lib.rs",
                RuntimeVfsOperation::Read
            ),
            "Canvas expose must not rebuild existing mounts to whole-mount access"
        );
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "index.html",
            RuntimeVfsOperation::Read
        ));
    }

    #[tokio::test]
    async fn canvas_binding_update_writes_agent_run_vfs_metadata_without_source_write() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas = Canvas::new(
            project_id,
            "cvs-dashboard-a".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let active_vfs = base_vfs(project_id);
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(active_vfs.clone());

        let mut frame = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&active_vfs).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());

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
                    active_vfs,
                    Some(identity("alice")),
                ),
            }),
            frame_repo: frame_repo.clone(),
            vfs_service: Some(Arc::new(VfsService::new(Arc::new(
                MountProviderRegistry::default(),
            )))),
            active_adopter: adopter.clone(),
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });

        let returned_state = service
            .apply_canvas_runtime_surface_update(
                "runtime-1",
                &canvas,
                None,
                RuntimeSurfaceUpdateRequest::CanvasBindingChanged {
                    canvas_mount_id: canvas.mount_id.clone(),
                    binding: CanvasDataBinding::new(
                        "stats".to_string(),
                        "workspace://reports/stats.csv".to_string(),
                    ),
                },
            )
            .await
            .expect("runtime binding update should succeed");

        let mount = returned_state
            .vfs
            .mounts
            .iter()
            .find(|mount| mount.id == canvas.mount_id)
            .expect("Canvas mount should be appended");
        assert!(mount.supports(MountCapability::Read));
        assert!(!mount.supports(MountCapability::Write));
        let runtime_bindings = mount
            .metadata
            .get(CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY)
            .and_then(|value| serde_json::from_value::<Vec<CanvasDataBinding>>(value.clone()).ok())
            .expect("runtime data bindings metadata");
        assert_eq!(runtime_bindings.len(), 1);
        assert_eq!(runtime_bindings[0].alias, "stats");
        assert_eq!(
            runtime_bindings[0].source_uri,
            "workspace://reports/stats.csv"
        );

        let frames = frame_repo
            .list_by_agent(agent_id)
            .await
            .expect("frames should list");
        assert_eq!(frames.len(), 2);
        assert_eq!(
            adopter.targets.lock().await.len(),
            1,
            "runtime binding surface change should be adopted"
        );
    }

    #[tokio::test]
    async fn personal_canvas_expose_uses_requesting_user_when_surface_identity_is_empty() {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas = Canvas::new_personal(
            project_id,
            "alice".to_string(),
            "cvs-dashboard-a".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        let active_vfs = base_vfs(project_id);
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(active_vfs.clone());

        let mut frame = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&active_vfs).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());

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
                    active_vfs,
                    None,
                ),
            }),
            frame_repo,
            vfs_service: Some(Arc::new(VfsService::new(Arc::new(
                MountProviderRegistry::default(),
            )))),
            active_adopter: adopter,
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
        });
        let current_user = ProjectAuthorizationContext::new("alice".to_string(), Vec::new(), false);

        let returned_state = service
            .expose_canvas_mount("runtime-1", &canvas, Some(&current_user))
            .await
            .expect("requesting owner should expose personal canvas");

        let mount = returned_state
            .vfs
            .mounts
            .iter()
            .find(|mount| mount.id == canvas.mount_id)
            .expect("Canvas mount should be appended");
        assert!(mount.supports(MountCapability::Read));
        assert!(mount.supports(MountCapability::Write));
        assert!(mount.supports(MountCapability::List));
        assert!(mount.supports(MountCapability::Search));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::Read
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src",
            RuntimeVfsOperation::List
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::Search
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::Write
        ));
        assert!(returned_state.access_policy.admits(
            &canvas.mount_id,
            "src/main.tsx",
            RuntimeVfsOperation::ApplyPatch
        ));
    }

    #[test]
    fn runtime_surface_noop_compare_uses_frame_surface_not_revision_identity() {
        let agent_id = Uuid::new_v4();
        let mut current = AgentFrame::new_revision(agent_id, 1, "owner_bootstrap");
        current.effective_capability_json = Some(serde_json::json!({"tools": []}));
        current.vfs_surface_json = Some(serde_json::json!({"mounts": []}));
        current.visible_canvas_mount_ids_json = Some(serde_json::json!(["cvs-dashboard-a"]));
        current.visible_workspace_module_refs_json =
            Some(serde_json::json!(["canvas:cvs-dashboard-a"]));

        let mut candidate = AgentFrame::new_revision(agent_id, 2, "canvas_expose");
        candidate.effective_capability_json = current.effective_capability_json.clone();
        candidate.vfs_surface_json = current.vfs_surface_json.clone();
        candidate.visible_canvas_mount_ids_json = current.visible_canvas_mount_ids_json.clone();
        candidate.visible_workspace_module_refs_json =
            current.visible_workspace_module_refs_json.clone();

        assert!(
            agent_frame_runtime_surface_unchanged(&current, &candidate),
            "revision id, revision number and created_by are not model-visible surface changes"
        );

        materialize_visible_workspace_module_ref_projection(&mut candidate, "canvas:cvs-other");
        assert!(!agent_frame_runtime_surface_unchanged(&current, &candidate));
    }

    fn runtime_surface_for_frame(
        runtime_session_id: &str,
        run_id: Uuid,
        project_id: Uuid,
        frame: &AgentFrame,
        capability_state: CapabilityState,
        vfs: Vfs,
        identity: Option<AuthIdentity>,
    ) -> AgentRunRuntimeSurface {
        let vfs_access_policy = agentdash_spi::RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs);
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
            vfs_access_policy,
            mcp_servers: Vec::new(),
            runtime_backend_anchor: None,
            active_turn_id: None,
            identity,
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

    fn identity(user_id: &str) -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            display_name: Some(user_id.to_string()),
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: Some("test".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    fn base_vfs(project_id: Uuid) -> Vfs {
        Vfs {
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
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn vfs_with_canvas(canvas: &Canvas) -> Vfs {
        let mut vfs = base_vfs(canvas.project_id);
        append_canvas_mount(&mut vfs, canvas, CanvasMountAccess::read_only());
        vfs
    }
}
