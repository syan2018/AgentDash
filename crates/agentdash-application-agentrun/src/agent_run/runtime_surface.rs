use std::{collections::BTreeSet, sync::Arc};

use agentdash_application_ports::agent_run_surface as ports_agent_run_surface;
use agentdash_application_ports::lifecycle_surface_projection as ports_lifecycle_surface;
use agentdash_application_ports::runtime_gateway_mcp_surface::{
    RuntimeGatewayMcpSurface, RuntimeGatewayMcpSurfaceQueryError,
    RuntimeGatewayMcpSurfaceQueryPort, RuntimeGatewayMcpSurfaceQueryPurpose,
    RuntimeGatewayMcpSurfaceWithBackend,
};
use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorError};
use agentdash_domain::permission::{
    PermissionGrant, PermissionGrantRepository, PermissionGrantVfsOperation,
    PermissionGrantVfsPathScope,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunDeliveryBindingRepository, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::{
    AuthIdentity, CapabilityState, RuntimeMcpServer, RuntimeVfsAccessPolicy, RuntimeVfsAccessRule,
    RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern, Vfs,
};
use async_trait::async_trait;
use uuid::Uuid;

use super::delivery_runtime_selection::{
    DeliveryRuntimeSelectionRepositories, DeliveryRuntimeSelectionService,
};
use crate::agent_run::frame::runtime_launch::runtime_backend_anchor_from_vfs;
use crate::agent_run::frame::surface::AgentFrameSurfaceExt;
use crate::agent_run::runtime_capability::project_capability_state_from_frame;
use agentdash_application_vfs::PROVIDER_RELAY_FS;

pub use ports_agent_run_surface::AgentRunRuntimeAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSurfaceQueryPurpose {
    pub component: String,
}

impl RuntimeSurfaceQueryPurpose {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
        }
    }

    pub fn resource_surface() -> Self {
        Self::new("agent_run_resource_surface")
    }
}

impl From<&str> for RuntimeSurfaceQueryPurpose {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceQuery {
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

#[derive(Clone)]
pub struct AgentRunRuntimeSurfaceQueryDeps {
    pub anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

#[async_trait]
pub trait AgentRunRuntimeSurfaceQueryPort: Send + Sync {
    async fn current_runtime_surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError>;

    async fn current_runtime_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        Err(AgentRunRuntimeSurfaceQueryError::Repository {
            purpose,
            operation: "current delivery selection",
            message: format!(
                "AgentRun scoped runtime surface selection is not implemented for run_id={run_id}, agent_id={agent_id}"
            ),
        })
    }

    async fn current_runtime_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError>;
}

#[derive(Clone)]
pub struct AgentRunResourceSurfaceQuery {
    _anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    lifecycle_surface_projection: Arc<dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort>,
}

#[derive(Clone)]
pub struct AgentRunResourceSurfaceQueryDeps {
    pub anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub lifecycle_surface_projection:
        Arc<dyn ports_lifecycle_surface::LifecycleSurfaceProjectionPort>,
}

impl AgentRunResourceSurfaceQuery {
    pub fn new(deps: AgentRunResourceSurfaceQueryDeps) -> Self {
        Self {
            _anchor_repo: deps.anchor_repo,
            surface_query: deps.surface_query,
            lifecycle_surface_projection: deps.lifecycle_surface_projection,
        }
    }

    pub async fn resource_surface_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let runtime_surface = self
            .surface_query
            .current_runtime_surface(
                runtime_session_id,
                RuntimeSurfaceQueryPurpose::resource_surface(),
            )
            .await
            .map_err(AgentRunResourceSurfaceQueryError::RuntimeSurface)?;
        self.project_resource_surface(runtime_surface).await
    }

    pub async fn resource_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let surface = self
            .surface_query
            .current_runtime_surface_for_agent_run(
                run_id,
                agent_id,
                RuntimeSurfaceQueryPurpose::resource_surface(),
            )
            .await
            .map_err(AgentRunResourceSurfaceQueryError::RuntimeSurface)?;
        if surface.run_id != run_id || surface.agent_id != agent_id {
            return Err(AgentRunResourceSurfaceQueryError::ControlPlaneMismatch {
                field: "current_runtime_surface",
                expected: format!("run_id={run_id}, agent_id={agent_id}"),
                actual: format!("run_id={}, agent_id={}", surface.run_id, surface.agent_id),
            });
        }
        self.project_resource_surface(surface).await
    }

    async fn project_resource_surface(
        &self,
        runtime_surface: AgentRunRuntimeSurface,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let node_evidence = match (
            runtime_surface.provenance.orchestration_id,
            runtime_surface.provenance.node_path.as_ref(),
            runtime_surface.provenance.node_attempt,
        ) {
            (Some(orchestration_id), Some(node_path), Some(attempt)) => {
                Some(ports_lifecycle_surface::OrchestrationNodeEvidenceRef {
                    run_id: runtime_surface.run_id,
                    orchestration_id,
                    node_path: node_path.clone(),
                    attempt,
                })
            }
            _ => None,
        };
        let lifecycle_surface = self
            .lifecycle_surface_projection
            .project_lifecycle_surface(ports_lifecycle_surface::AgentRunLifecycleSurfaceInput {
                base_vfs: Some(runtime_surface.vfs.clone()),
                address: runtime_surface.runtime_address.clone(),
                message_stream: Some(ports_lifecycle_surface::MessageStreamProjectionRef {
                    runtime_session_id: runtime_surface.runtime_session_id.clone(),
                    trace_kind:
                        ports_lifecycle_surface::MessageStreamTraceKind::ConnectorRuntimeSession,
                }),
                project_id: runtime_surface.project_id,
                mode: ports_lifecycle_surface::AgentRunLifecycleSurfaceMode::WorkspaceReadSurface,
                explicit_skill_asset_keys: Vec::new(),
                builtin_skills:
                    ports_lifecycle_surface::BuiltinLifecycleSkillPolicy::PreserveProjected,
                node_evidence,
                node_projection: None,
            })
            .await
            .map_err(|error| AgentRunResourceSurfaceQueryError::Projection {
                message: error.to_string(),
            })?;

        Ok(AgentRunResourceSurface {
            runtime: runtime_surface,
            lifecycle_surface,
        })
    }
}

impl AgentRunRuntimeSurfaceQuery {
    pub fn new(deps: AgentRunRuntimeSurfaceQueryDeps) -> Self {
        Self {
            anchor_repo: deps.anchor_repo,
            run_repo: deps.run_repo,
            agent_repo: deps.agent_repo,
            frame_repo: deps.frame_repo,
            delivery_binding_repo: deps.delivery_binding_repo,
            permission_grant_repo: deps.permission_grant_repo,
        }
    }

    async fn resolve_surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let anchor = self
            .anchor_repo
            .find_by_session(runtime_session_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "runtime session execution anchor",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingAnchor {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
            })?;

        let run = self
            .run_repo
            .get_by_id(anchor.run_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "lifecycle run",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                run_id: anchor.run_id,
            })?;

        let agent = self
            .agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "lifecycle agent",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                agent_id: anchor.agent_id,
            })?;

        if agent.run_id != anchor.run_id {
            return Err(
                AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch {
                    purpose: purpose.clone(),
                    runtime_session_id: runtime_session_id.to_string(),
                    field: "agent.run_id",
                    expected: anchor.run_id.to_string(),
                    actual: agent.run_id.to_string(),
                },
            );
        }
        if run.id != anchor.run_id {
            return Err(
                AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch {
                    purpose: purpose.clone(),
                    runtime_session_id: runtime_session_id.to_string(),
                    field: "run.id",
                    expected: anchor.run_id.to_string(),
                    actual: run.id.to_string(),
                },
            );
        }
        if agent.project_id != run.project_id {
            return Err(
                AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch {
                    purpose: purpose.clone(),
                    runtime_session_id: runtime_session_id.to_string(),
                    field: "agent.project_id",
                    expected: run.project_id.to_string(),
                    actual: agent.project_id.to_string(),
                },
            );
        }

        let frame = self
            .frame_repo
            .get_current(agent.id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "current AgentFrame",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                agent_id: agent.id,
            })?;
        if frame.agent_id != agent.id {
            return Err(
                AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch {
                    purpose: purpose.clone(),
                    runtime_session_id: runtime_session_id.to_string(),
                    field: "frame.agent_id",
                    expected: agent.id.to_string(),
                    actual: frame.agent_id.to_string(),
                },
            );
        }

        let capability_state = frame.typed_capability_state().ok_or_else(|| {
            AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                frame_id: frame.id,
                field: "capability_state",
            }
        })?;
        let vfs = frame.typed_vfs().ok_or_else(|| {
            AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                frame_id: frame.id,
                field: "vfs",
            }
        })?;
        let active_grants = self
            .permission_grant_repo
            .list_active_by_frame(frame.id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "active permission grants",
                message: error.to_string(),
            })?;
        let vfs_access_policy =
            runtime_vfs_access_policy_for_grants(&vfs, runtime_session_id, &active_grants);
        let projected_capability_state = project_capability_state_from_frame(&frame);
        let mcp_servers = frame.typed_mcp_servers();
        let runtime_backend_anchor = runtime_backend_anchor_from_vfs(
            &vfs,
            Some(format!("agent_run.runtime_surface.{}", purpose.component)),
        )
        .map_err(
            |source| AgentRunRuntimeSurfaceQueryError::BackendAnchorDerivation {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                source,
            },
        )?;
        let _capability_state = capability_state;

        Ok(AgentRunRuntimeSurface {
            runtime_session_id: runtime_session_id.to_string(),
            run_id: run.id,
            project_id: run.project_id,
            agent_id: agent.id,
            runtime_address: AgentRunRuntimeAddress {
                run_id: run.id,
                agent_id: agent.id,
                frame_id: frame.id,
            },
            launch_evidence_frame_id: anchor.launch_frame_id,
            current_surface_frame_id: frame.id,
            surface_revision: frame.revision,
            capability_state: projected_capability_state,
            vfs,
            vfs_access_policy,
            mcp_servers,
            runtime_backend_anchor,
            active_turn_id: None,
            identity: None,
            provenance: AgentRunRuntimeSurfaceProvenance::from_anchor(
                &anchor,
                frame.id,
                frame.revision,
                frame.created_by_kind.clone(),
            ),
            closure: AgentRunRuntimeSurfaceClosure {
                capability_field_present: true,
                vfs_field_present: true,
                mcp_field_present: frame.mcp_surface_json.is_some(),
            },
        })
    }
}

fn runtime_vfs_access_policy_for_grants(
    vfs: &Vfs,
    runtime_session_id: &str,
    active_grants: &[PermissionGrant],
) -> RuntimeVfsAccessPolicy {
    let mut policy = RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs);
    let mut permission_grant_rules = Vec::new();
    let mut permission_grant_mounts = BTreeSet::new();
    for grant in active_grants
        .iter()
        .filter(|grant| grant.status.is_active())
    {
        for rule in &grant.requested_vfs_access {
            if rule
                .surface_ref
                .as_ref()
                .is_some_and(|surface_ref| surface_ref != runtime_session_id)
            {
                continue;
            }
            permission_grant_mounts.insert(rule.mount_id.clone());
            permission_grant_rules.push(RuntimeVfsAccessRule {
                mount_id: rule.mount_id.clone(),
                path_pattern: match &rule.path_scope {
                    PermissionGrantVfsPathScope::All => RuntimeVfsPathPattern::All,
                    PermissionGrantVfsPathScope::Prefix(prefix) => {
                        RuntimeVfsPathPattern::Prefix(prefix.clone())
                    }
                },
                operations: rule
                    .operations
                    .iter()
                    .copied()
                    .map(runtime_vfs_operation_from_grant)
                    .collect::<BTreeSet<_>>(),
                source: RuntimeVfsAccessSource::PermissionGrant,
            });
        }
    }
    if !permission_grant_rules.is_empty() {
        policy.rules.retain(|rule| {
            rule.source != RuntimeVfsAccessSource::SystemRuntimeProjection
                || !permission_grant_mounts.contains(&rule.mount_id)
        });
        policy.rules.extend(permission_grant_rules);
    }
    policy
}

fn runtime_vfs_operation_from_grant(operation: PermissionGrantVfsOperation) -> RuntimeVfsOperation {
    match operation {
        PermissionGrantVfsOperation::Read => RuntimeVfsOperation::Read,
        PermissionGrantVfsOperation::List => RuntimeVfsOperation::List,
        PermissionGrantVfsOperation::Search => RuntimeVfsOperation::Search,
        PermissionGrantVfsOperation::Write => RuntimeVfsOperation::Write,
        PermissionGrantVfsOperation::Exec => RuntimeVfsOperation::Exec,
        PermissionGrantVfsOperation::ApplyPatch => RuntimeVfsOperation::ApplyPatch,
    }
}

#[async_trait]
impl AgentRunRuntimeSurfaceQueryPort for AgentRunRuntimeSurfaceQuery {
    async fn current_runtime_surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        self.resolve_surface(runtime_session_id, purpose).await
    }

    async fn current_runtime_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let selection =
            DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: self.run_repo.as_ref(),
                lifecycle_agents: self.agent_repo.as_ref(),
                agent_frames: self.frame_repo.as_ref(),
                execution_anchors: self.anchor_repo.as_ref(),
                delivery_bindings: self.delivery_binding_repo.as_ref(),
            })
            .select_current_delivery(run_id, agent_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                purpose: purpose.clone(),
                operation: "current delivery selection",
                message: error.to_string(),
            })?;
        self.resolve_surface(&selection.runtime_session_id, purpose)
            .await
    }

    async fn current_runtime_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError> {
        let surface = self
            .current_runtime_surface(runtime_session_id, purpose.clone())
            .await?;
        let runtime_backend_anchor = surface.runtime_backend_anchor.clone().ok_or_else(|| {
            AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor {
                purpose,
                runtime_session_id: surface.runtime_session_id.clone(),
                turn_id: surface.active_turn_id.clone(),
            }
        })?;
        Ok(AgentRunRuntimeSurfaceWithBackend {
            surface,
            runtime_backend_anchor,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunRuntimeSurface {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_address: AgentRunRuntimeAddress,
    pub launch_evidence_frame_id: Uuid,
    pub current_surface_frame_id: Uuid,
    pub surface_revision: i32,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub vfs_access_policy: RuntimeVfsAccessPolicy,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub active_turn_id: Option<String>,
    pub identity: Option<AuthIdentity>,
    pub provenance: AgentRunRuntimeSurfaceProvenance,
    pub closure: AgentRunRuntimeSurfaceClosure,
}

#[derive(Debug, Clone)]
pub struct AgentRunResourceSurface {
    pub runtime: AgentRunRuntimeSurface,
    pub lifecycle_surface: ports_lifecycle_surface::AgentRunLifecycleSurface,
}

#[derive(Debug, Clone)]
pub struct AgentRunRuntimeSurfaceWithBackend {
    pub surface: AgentRunRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTerminalLaunchTarget {
    pub backend_id: String,
    pub mount_root_ref: String,
}

pub fn terminal_launch_target_from_current_surface(
    surface: &AgentRunRuntimeSurfaceWithBackend,
) -> Result<AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError> {
    terminal_launch_target_from_vfs(&surface.surface.vfs, &surface.runtime_backend_anchor)
}

pub fn terminal_launch_target_from_vfs(
    vfs: &Vfs,
    backend_anchor: &RuntimeBackendAnchor,
) -> Result<AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError> {
    let mount = if let Some(root_ref) = backend_anchor
        .root_ref
        .as_deref()
        .map(str::trim)
        .filter(|root_ref| !root_ref.is_empty())
    {
        vfs.mounts
            .iter()
            .find(|mount| mount.root_ref.trim() == root_ref)
            .ok_or_else(|| AgentRunTerminalLaunchTargetError::MissingAnchorMount {
                root_ref: root_ref.to_string(),
            })?
    } else {
        vfs.default_mount()
            .ok_or(AgentRunTerminalLaunchTargetError::MissingMount)?
    };
    if mount.provider != PROVIDER_RELAY_FS {
        return Err(
            AgentRunTerminalLaunchTargetError::UnsupportedMountProvider {
                mount_id: mount.id.clone(),
                provider: mount.provider.clone(),
            },
        );
    }
    let backend_id = backend_anchor.backend_id();
    if backend_id.is_empty() {
        return Err(AgentRunTerminalLaunchTargetError::MissingBackendId);
    }
    let mount_root_ref = mount.root_ref.trim();
    if mount_root_ref.is_empty() {
        return Err(AgentRunTerminalLaunchTargetError::MissingMountRootRef {
            mount_id: mount.id.clone(),
        });
    }
    Ok(AgentRunTerminalLaunchTarget {
        backend_id: backend_id.to_string(),
        mount_root_ref: mount_root_ref.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunTerminalLaunchTargetError {
    #[error(
        "AgentRun runtime surface 缺少 runtime backend anchor root_ref 对应的 relay mount: {root_ref}"
    )]
    MissingAnchorMount { root_ref: String },
    #[error("AgentRun runtime surface 缺少可用 mount，无法创建终端")]
    MissingMount,
    #[error(
        "AgentRun runtime surface mount `{mount_id}` 使用 provider `{provider}`，无法创建交互式终端"
    )]
    UnsupportedMountProvider { mount_id: String, provider: String },
    #[error("AgentRun runtime backend anchor 缺少 backend_id，无法创建终端")]
    MissingBackendId,
    #[error("AgentRun runtime surface mount `{mount_id}` 缺少 root_ref，无法创建终端")]
    MissingMountRootRef { mount_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeSurfaceProvenance {
    pub launch_evidence_frame_id: Uuid,
    pub launch_created_by_kind: String,
    pub current_surface_frame_id: Uuid,
    pub surface_revision: i32,
    pub surface_created_by_kind: String,
    pub anchor_updated_at: chrono::DateTime<chrono::Utc>,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
}

impl AgentRunRuntimeSurfaceProvenance {
    fn from_anchor(
        anchor: &RuntimeSessionExecutionAnchor,
        current_surface_frame_id: Uuid,
        surface_revision: i32,
        surface_created_by_kind: String,
    ) -> Self {
        Self {
            launch_evidence_frame_id: anchor.launch_frame_id,
            launch_created_by_kind: anchor.created_by_kind.clone(),
            current_surface_frame_id,
            surface_revision,
            surface_created_by_kind,
            anchor_updated_at: anchor.updated_at,
            orchestration_id: anchor.orchestration_id,
            node_path: anchor.node_path.clone(),
            node_attempt: anchor.node_attempt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeSurfaceClosure {
    pub capability_field_present: bool,
    pub vfs_field_present: bool,
    pub mcp_field_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunResourceSurfaceQueryError {
    #[error("{0}")]
    RuntimeSurface(#[from] AgentRunRuntimeSurfaceQueryError),
    #[error("AgentRun resource surface 缺少 delivery anchor: run_id={run_id}, agent_id={agent_id}")]
    MissingDeliveryAnchor { run_id: Uuid, agent_id: Uuid },
    #[error(
        "AgentRun resource surface 控制面不一致: field={field}, expected={expected}, actual={actual}"
    )]
    ControlPlaneMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("AgentRun resource surface projection 失败: {message}")]
    Projection { message: String },
    #[error("AgentRun resource surface repository 失败: operation={operation}, message={message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunRuntimeSurfaceQueryError {
    #[error(
        "runtime surface query 缺少 RuntimeSessionExecutionAnchor: component={component}, session_id={runtime_session_id}",
        component = purpose.component
    )]
    MissingAnchor {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
    },
    #[error(
        "runtime surface query 指向的 LifecycleRun 不存在: component={component}, session_id={runtime_session_id}, run_id={run_id}",
        component = purpose.component
    )]
    MissingLifecycleRun {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        run_id: Uuid,
    },
    #[error(
        "runtime surface query 指向的 LifecycleAgent 不存在: component={component}, session_id={runtime_session_id}, agent_id={agent_id}",
        component = purpose.component
    )]
    MissingLifecycleAgent {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        agent_id: Uuid,
    },
    #[error(
        "runtime surface query 缺少当前 AgentFrame: component={component}, session_id={runtime_session_id}, agent_id={agent_id}",
        component = purpose.component
    )]
    MissingCurrentFrame {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        agent_id: Uuid,
    },
    #[error(
        "runtime surface query anchor 控制面不一致: component={component}, session_id={runtime_session_id}, field={field}, expected={expected}, actual={actual}",
        component = purpose.component
    )]
    AnchorControlPlaneMismatch {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error(
        "runtime surface query 缺少 current surface closure: component={component}, session_id={runtime_session_id}, frame_id={frame_id}, field={field}",
        component = purpose.component
    )]
    MissingSurfaceClosure {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        frame_id: Uuid,
        field: &'static str,
    },
    #[error(
        "runtime surface query 缺少 backend anchor: component={component}, session_id={runtime_session_id}, turn_id={turn_id:?}",
        component = purpose.component
    )]
    MissingRuntimeBackendAnchor {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        turn_id: Option<String>,
    },
    #[error(
        "runtime surface query backend anchor 派生失败: component={component}, session_id={runtime_session_id}: {source}",
        component = purpose.component
    )]
    BackendAnchorDerivation {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        source: RuntimeBackendAnchorError,
    },
    #[error(
        "runtime surface query repository 失败: component={component}, operation={operation}, message={message}",
        component = purpose.component
    )]
    Repository {
        purpose: RuntimeSurfaceQueryPurpose,
        operation: &'static str,
        message: String,
    },
}

impl AgentRunRuntimeSurfaceQueryError {
    pub fn purpose(&self) -> &RuntimeSurfaceQueryPurpose {
        match self {
            Self::MissingAnchor { purpose, .. }
            | Self::MissingLifecycleRun { purpose, .. }
            | Self::MissingLifecycleAgent { purpose, .. }
            | Self::MissingCurrentFrame { purpose, .. }
            | Self::AnchorControlPlaneMismatch { purpose, .. }
            | Self::MissingSurfaceClosure { purpose, .. }
            | Self::MissingRuntimeBackendAnchor { purpose, .. }
            | Self::BackendAnchorDerivation { purpose, .. }
            | Self::Repository { purpose, .. } => purpose,
        }
    }

    pub fn as_runtime_backend_anchor_error(&self) -> Option<RuntimeBackendAnchorError> {
        match self {
            Self::MissingRuntimeBackendAnchor {
                purpose,
                runtime_session_id,
                turn_id,
            } => Some(RuntimeBackendAnchorError::Missing {
                component: purpose.component.clone(),
                session_id: Some(runtime_session_id.clone()),
                turn_id: turn_id.clone(),
            }),
            Self::BackendAnchorDerivation { source, .. } => Some(source.clone()),
            _ => None,
        }
    }
}

fn local_purpose_from_port(
    purpose: ports_agent_run_surface::RuntimeSurfaceQueryPurpose,
) -> RuntimeSurfaceQueryPurpose {
    RuntimeSurfaceQueryPurpose {
        component: purpose.component,
    }
}

fn port_purpose(
    purpose: RuntimeSurfaceQueryPurpose,
) -> ports_agent_run_surface::RuntimeSurfaceQueryPurpose {
    ports_agent_run_surface::RuntimeSurfaceQueryPurpose {
        component: purpose.component,
    }
}

fn port_runtime_address(
    address: AgentRunRuntimeAddress,
) -> ports_agent_run_surface::AgentRunRuntimeAddress {
    ports_agent_run_surface::AgentRunRuntimeAddress {
        run_id: address.run_id,
        agent_id: address.agent_id,
        frame_id: address.frame_id,
    }
}

fn port_runtime_surface(
    surface: AgentRunRuntimeSurface,
) -> ports_agent_run_surface::AgentRunRuntimeSurface {
    ports_agent_run_surface::AgentRunRuntimeSurface {
        runtime_session_id: surface.runtime_session_id,
        run_id: surface.run_id,
        project_id: surface.project_id,
        agent_id: surface.agent_id,
        runtime_address: port_runtime_address(surface.runtime_address),
        launch_evidence_frame_id: surface.launch_evidence_frame_id,
        current_surface_frame_id: surface.current_surface_frame_id,
        surface_revision: surface.surface_revision,
        capability_state: surface.capability_state,
        vfs: surface.vfs,
        mcp_servers: surface.mcp_servers,
        runtime_backend_anchor: surface.runtime_backend_anchor,
        active_turn_id: surface.active_turn_id,
        identity: surface.identity,
        provenance: ports_agent_run_surface::AgentRunRuntimeSurfaceProvenance {
            launch_evidence_frame_id: surface.provenance.launch_evidence_frame_id,
            launch_created_by_kind: surface.provenance.launch_created_by_kind,
            current_surface_frame_id: surface.provenance.current_surface_frame_id,
            surface_revision: surface.provenance.surface_revision,
            surface_created_by_kind: surface.provenance.surface_created_by_kind,
            anchor_updated_at: surface.provenance.anchor_updated_at,
            orchestration_id: surface.provenance.orchestration_id,
            node_path: surface.provenance.node_path,
            node_attempt: surface.provenance.node_attempt,
        },
        closure: ports_agent_run_surface::AgentRunRuntimeSurfaceClosure {
            capability_field_present: surface.closure.capability_field_present,
            vfs_field_present: surface.closure.vfs_field_present,
            mcp_field_present: surface.closure.mcp_field_present,
        },
    }
}

fn port_runtime_surface_with_backend(
    surface: AgentRunRuntimeSurfaceWithBackend,
) -> ports_agent_run_surface::AgentRunRuntimeSurfaceWithBackend {
    ports_agent_run_surface::AgentRunRuntimeSurfaceWithBackend {
        surface: port_runtime_surface(surface.surface),
        runtime_backend_anchor: surface.runtime_backend_anchor,
    }
}

fn port_resource_surface(
    surface: AgentRunResourceSurface,
) -> ports_agent_run_surface::AgentRunResourceSurface {
    ports_agent_run_surface::AgentRunResourceSurface {
        runtime: port_runtime_surface(surface.runtime),
        lifecycle_surface: surface.lifecycle_surface,
    }
}

fn port_runtime_surface_query_error(
    error: AgentRunRuntimeSurfaceQueryError,
) -> ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError {
    match error {
        AgentRunRuntimeSurfaceQueryError::MissingAnchor {
            purpose,
            runtime_session_id,
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::MissingAnchor {
            purpose: port_purpose(purpose),
            runtime_session_id,
        },
        AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun {
            purpose,
            runtime_session_id,
            run_id,
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun {
            purpose: port_purpose(purpose),
            runtime_session_id,
            run_id,
        },
        AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent {
            purpose,
            runtime_session_id,
            agent_id,
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent {
            purpose: port_purpose(purpose),
            runtime_session_id,
            agent_id,
        },
        AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame {
            purpose,
            runtime_session_id,
            agent_id,
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame {
            purpose: port_purpose(purpose),
            runtime_session_id,
            agent_id,
        },
        AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor {
            purpose,
            runtime_session_id,
            turn_id,
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor {
            source: RuntimeBackendAnchorError::Missing {
                component: purpose.component,
                session_id: Some(runtime_session_id),
                turn_id,
            },
        },
        AgentRunRuntimeSurfaceQueryError::BackendAnchorDerivation { source, .. } => {
            ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor {
                source,
            }
        }
        AgentRunRuntimeSurfaceQueryError::Repository {
            operation, message, ..
        } => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::Repository {
            operation,
            message,
        },
        other => ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError::Projection {
            message: other.to_string(),
        },
    }
}

fn port_resource_surface_query_error(
    error: AgentRunResourceSurfaceQueryError,
) -> ports_agent_run_surface::AgentRunResourceSurfaceQueryError {
    match error {
        AgentRunResourceSurfaceQueryError::RuntimeSurface(error) => {
            ports_agent_run_surface::AgentRunResourceSurfaceQueryError::RuntimeSurface(
                port_runtime_surface_query_error(error),
            )
        }
        AgentRunResourceSurfaceQueryError::MissingDeliveryAnchor { run_id, agent_id } => {
            ports_agent_run_surface::AgentRunResourceSurfaceQueryError::MissingDeliveryAnchor {
                run_id,
                agent_id,
            }
        }
        AgentRunResourceSurfaceQueryError::ControlPlaneMismatch {
            field,
            expected,
            actual,
        } => ports_agent_run_surface::AgentRunResourceSurfaceQueryError::ControlPlaneMismatch {
            field,
            expected,
            actual,
        },
        AgentRunResourceSurfaceQueryError::Projection { message } => {
            ports_agent_run_surface::AgentRunResourceSurfaceQueryError::Projection { message }
        }
        AgentRunResourceSurfaceQueryError::Repository { operation, message } => {
            ports_agent_run_surface::AgentRunResourceSurfaceQueryError::Repository {
                operation,
                message,
            }
        }
    }
}

#[async_trait]
impl ports_agent_run_surface::AgentRunRuntimeSurfaceQueryPort for AgentRunRuntimeSurfaceQuery {
    async fn current_runtime_surface(
        &self,
        runtime_session_id: &str,
        purpose: ports_agent_run_surface::RuntimeSurfaceQueryPurpose,
    ) -> Result<
        ports_agent_run_surface::AgentRunRuntimeSurface,
        ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError,
    > {
        AgentRunRuntimeSurfaceQueryPort::current_runtime_surface(
            self,
            runtime_session_id,
            local_purpose_from_port(purpose),
        )
        .await
        .map(port_runtime_surface)
        .map_err(port_runtime_surface_query_error)
    }

    async fn current_runtime_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: ports_agent_run_surface::RuntimeSurfaceQueryPurpose,
    ) -> Result<
        ports_agent_run_surface::AgentRunRuntimeSurfaceWithBackend,
        ports_agent_run_surface::AgentRunRuntimeSurfaceQueryError,
    > {
        AgentRunRuntimeSurfaceQueryPort::current_runtime_surface_with_backend(
            self,
            runtime_session_id,
            local_purpose_from_port(purpose),
        )
        .await
        .map(port_runtime_surface_with_backend)
        .map_err(port_runtime_surface_query_error)
    }
}

#[async_trait]
impl ports_agent_run_surface::AgentRunResourceSurfaceQueryPort for AgentRunResourceSurfaceQuery {
    async fn resource_surface_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<
        ports_agent_run_surface::AgentRunResourceSurface,
        ports_agent_run_surface::AgentRunResourceSurfaceQueryError,
    > {
        AgentRunResourceSurfaceQuery::resource_surface_for_runtime_session(self, runtime_session_id)
            .await
            .map(port_resource_surface)
            .map_err(port_resource_surface_query_error)
    }

    async fn resource_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<
        ports_agent_run_surface::AgentRunResourceSurface,
        ports_agent_run_surface::AgentRunResourceSurfaceQueryError,
    > {
        AgentRunResourceSurfaceQuery::resource_surface_for_agent_run(self, run_id, agent_id)
            .await
            .map(port_resource_surface)
            .map_err(port_resource_surface_query_error)
    }
}

impl From<AgentRunRuntimeSurfaceWithBackend> for RuntimeGatewayMcpSurfaceWithBackend {
    fn from(surface_with_backend: AgentRunRuntimeSurfaceWithBackend) -> Self {
        let AgentRunRuntimeSurfaceWithBackend {
            surface,
            runtime_backend_anchor,
        } = surface_with_backend;
        RuntimeGatewayMcpSurfaceWithBackend {
            surface: RuntimeGatewayMcpSurface {
                runtime_session_id: surface.runtime_session_id,
                capability_state: surface.capability_state,
                vfs: surface.vfs,
                vfs_access_policy: surface.vfs_access_policy,
                mcp_servers: surface.mcp_servers,
                active_turn_id: surface.active_turn_id,
                identity: surface.identity,
            },
            runtime_backend_anchor,
        }
    }
}

impl From<RuntimeGatewayMcpSurfaceQueryPurpose> for RuntimeSurfaceQueryPurpose {
    fn from(value: RuntimeGatewayMcpSurfaceQueryPurpose) -> Self {
        RuntimeSurfaceQueryPurpose::new(value.component)
    }
}

impl From<AgentRunRuntimeSurfaceQueryError> for RuntimeGatewayMcpSurfaceQueryError {
    fn from(error: AgentRunRuntimeSurfaceQueryError) -> Self {
        if let Some(anchor_error) = error.as_runtime_backend_anchor_error() {
            return RuntimeGatewayMcpSurfaceQueryError::with_runtime_backend_anchor_error(
                error.to_string(),
                anchor_error,
            );
        }
        RuntimeGatewayMcpSurfaceQueryError::new(error.to_string())
    }
}

#[async_trait]
impl RuntimeGatewayMcpSurfaceQueryPort for AgentRunRuntimeSurfaceQuery {
    async fn current_runtime_mcp_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeGatewayMcpSurfaceQueryPurpose,
    ) -> Result<RuntimeGatewayMcpSurfaceWithBackend, RuntimeGatewayMcpSurfaceQueryError> {
        self.current_runtime_surface_with_backend(runtime_session_id, purpose.into())
            .await
            .map(RuntimeGatewayMcpSurfaceWithBackend::from)
            .map_err(RuntimeGatewayMcpSurfaceQueryError::from)
    }

    async fn current_runtime_mcp_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        purpose: RuntimeGatewayMcpSurfaceQueryPurpose,
    ) -> Result<RuntimeGatewayMcpSurfaceWithBackend, RuntimeGatewayMcpSurfaceQueryError> {
        let surface = self
            .current_runtime_surface_for_agent_run(run_id, agent_id, purpose.clone().into())
            .await
            .map_err(RuntimeGatewayMcpSurfaceQueryError::from)?;
        let runtime_backend_anchor = surface.runtime_backend_anchor.clone().ok_or_else(|| {
            RuntimeGatewayMcpSurfaceQueryError::new(format!(
                "AgentRun MCP surface 缺少 backend anchor: run_id={run_id}, agent_id={agent_id}, component={}",
                purpose.component
            ))
        })?;
        Ok(RuntimeGatewayMcpSurfaceWithBackend::from(
            AgentRunRuntimeSurfaceWithBackend {
                surface,
                runtime_backend_anchor,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use agentdash_domain::DomainError;
    use agentdash_domain::backend::RuntimeBackendAnchorSource;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::permission::{
        GrantScope, GrantStatus, PermissionGrant, PermissionGrantRepository,
        PermissionGrantStatusFilter, PermissionGrantVfsAccessRule, PermissionGrantVfsOperation,
        PermissionGrantVfsPathScope, PolicyDecision, PolicyOutcome,
    };
    use agentdash_domain::workflow::{
        AgentFrame, AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository, AgentSource,
        DeliveryBindingStatus, LifecycleAgent, LifecycleRun, RuntimeSessionExecutionAnchor,
    };
    use agentdash_spi::{AgentConfig, McpTransportConfig, ToolCluster};
    use agentdash_test_support::workflow::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
        MemoryRuntimeSessionExecutionAnchorRepository,
    };
    use chrono::{DateTime, Utc};
    use tokio::sync::Mutex;

    use super::*;
    use crate::test_support::MemoryAgentRunDeliveryBindingRepository;

    #[derive(Default)]
    struct FixturePermissionGrantRepo {
        grants: Mutex<HashMap<Uuid, PermissionGrant>>,
    }

    #[async_trait::async_trait]
    impl PermissionGrantRepository for FixturePermissionGrantRepo {
        async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            self.grants.lock().await.insert(grant.id, grant.clone());
            Ok(())
        }

        async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            self.grants.lock().await.insert(grant.id, grant.clone());
            Ok(())
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(self.grants.lock().await.get(&id).cloned())
        }

        async fn list_by_frame(
            &self,
            effect_frame_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .values()
                .filter(|grant| grant.effect_frame_id == Some(effect_frame_id))
                .filter(|grant| matches_status_filter(grant.status, status_filter))
                .cloned()
                .collect())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .values()
                .filter(|grant| grant.run_id == run_id)
                .filter(|grant| matches_status_filter(grant.status, status_filter))
                .cloned()
                .collect())
        }

        async fn list_active_by_frame(
            &self,
            effect_frame_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.list_by_frame(effect_frame_id, Some(PermissionGrantStatusFilter::Active))
                .await
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.list_by_run(run_id, Some(PermissionGrantStatusFilter::Active))
                .await
        }

        async fn find_active_escalation_grant(
            &self,
            _effect_frame_id: Uuid,
            _target_subject_kind: &str,
        ) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(None)
        }

        async fn list_overdue_active(
            &self,
            now: DateTime<Utc>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .values()
                .filter(|grant| grant.status.is_active())
                .filter(|grant| grant.expires_at.is_some_and(|expires_at| expires_at < now))
                .cloned()
                .collect())
        }
    }

    fn matches_status_filter(
        status: GrantStatus,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> bool {
        match status_filter {
            Some(PermissionGrantStatusFilter::Exact(expected)) => status == expected,
            Some(PermissionGrantStatusFilter::Pending) => matches!(
                status,
                GrantStatus::Created
                    | GrantStatus::PendingPolicy
                    | GrantStatus::PendingUserApproval
                    | GrantStatus::Approved
            ),
            Some(PermissionGrantStatusFilter::Active) => status.is_active(),
            Some(PermissionGrantStatusFilter::Terminal) => status.is_terminal(),
            None => true,
        }
    }

    #[derive(Default)]
    struct TestLifecycleSurfaceProjection;

    #[async_trait::async_trait]
    impl ports_lifecycle_surface::LifecycleSurfaceProjectionPort for TestLifecycleSurfaceProjection {
        async fn project_lifecycle_surface(
            &self,
            input: ports_lifecycle_surface::AgentRunLifecycleSurfaceInput,
        ) -> Result<
            ports_lifecycle_surface::AgentRunLifecycleSurface,
            ports_lifecycle_surface::LifecycleSurfaceProjectionError,
        > {
            let lifecycle_mount = Mount {
                id: ports_lifecycle_surface::LIFECYCLE_MOUNT_ID.to_string(),
                provider: ports_lifecycle_surface::PROVIDER_LIFECYCLE_VFS.to_string(),
                backend_id: "lifecycle".to_string(),
                root_ref: format!("lifecycle://run/{}", input.address.run_id),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::json!({
                    "launch_frame_id": input.address.frame_id.to_string(),
                }),
            };
            let mut vfs = input.base_vfs.unwrap_or_default();
            vfs.mounts.push(lifecycle_mount.clone());
            Ok(ports_lifecycle_surface::AgentRunLifecycleSurface {
                vfs,
                lifecycle_mount,
                projections: ports_lifecycle_surface::AgentRunLifecycleProjectionSet {
                    agent_run_identity: true,
                    message_stream: input.message_stream.map(|message_stream| {
                        ports_lifecycle_surface::MessageStreamProjectionFacts {
                            runtime_session_id: message_stream.runtime_session_id,
                            trace_kind: message_stream.trace_kind,
                        }
                    }),
                    node_evidence: None,
                    orchestration_node: None,
                    skill_assets: Vec::new(),
                },
                skill_asset_keys: Vec::new(),
            })
        }
    }

    struct Fixture {
        query: AgentRunRuntimeSurfaceQuery,
        anchor_repo: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
        run_repo: Arc<MemoryLifecycleRunRepository>,
        agent_repo: Arc<MemoryLifecycleAgentRepository>,
        frame_repo: Arc<MemoryAgentFrameRepository>,
        delivery_binding_repo: Arc<MemoryAgentRunDeliveryBindingRepository>,
        permission_grant_repo: Arc<FixturePermissionGrantRepo>,
        run_id: Uuid,
        project_id: Uuid,
        agent_id: Uuid,
        launch_frame_id: Uuid,
    }

    async fn fixture() -> Fixture {
        let anchor_repo = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
        let run_repo = Arc::new(MemoryLifecycleRunRepository::default());
        let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        let delivery_binding_repo = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
        let permission_grant_repo = Arc::new(FixturePermissionGrantRepo::default());
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let run_id = run.id;
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let agent_id = agent.id;
        let launch_frame = frame(
            agent_id,
            1,
            Some(vfs_with_default_backend("backend-launch")),
        );
        let launch_frame_id = launch_frame.id;
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "session-1",
            run_id,
            launch_frame_id,
            agent_id,
        );
        let binding = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Running,
            anchor.updated_at,
        );
        run_repo.create(&run).await.expect("run");
        agent_repo.create(&agent).await.expect("agent");
        frame_repo
            .create(&launch_frame)
            .await
            .expect("launch frame");
        anchor_repo.create_once(&anchor).await.expect("anchor");
        delivery_binding_repo
            .upsert(&binding)
            .await
            .expect("binding");
        let query = AgentRunRuntimeSurfaceQuery::new(AgentRunRuntimeSurfaceQueryDeps {
            anchor_repo: anchor_repo.clone(),
            run_repo: run_repo.clone(),
            agent_repo: agent_repo.clone(),
            frame_repo: frame_repo.clone(),
            delivery_binding_repo: delivery_binding_repo.clone(),
            permission_grant_repo: permission_grant_repo.clone(),
        });
        Fixture {
            query,
            anchor_repo,
            run_repo,
            agent_repo,
            frame_repo,
            delivery_binding_repo,
            permission_grant_repo,
            run_id,
            project_id,
            agent_id,
            launch_frame_id,
        }
    }

    fn frame(agent_id: Uuid, revision: i32, vfs: Option<Vfs>) -> AgentFrame {
        let mut frame = AgentFrame::new_revision(agent_id, revision, "test_surface");
        let mcp_servers = vec![RuntimeMcpServer {
            name: "code-analyzer".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: true,
            readiness: Default::default(),
        }];
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.tool.mcp_servers = mcp_servers.clone();
        capability_state.vfs.active = vfs.clone();
        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = vfs.and_then(|value| serde_json::to_value(value).ok());
        frame.mcp_surface_json = Some(serde_json::to_value(&mcp_servers).unwrap());
        frame.execution_profile_json =
            Some(serde_json::to_value(AgentConfig::new("PI_AGENT")).unwrap());
        frame
    }

    fn frame_without_capability(agent_id: Uuid, revision: i32) -> AgentFrame {
        let mut frame = AgentFrame::new_revision(agent_id, revision, "broken_surface");
        frame.vfs_surface_json =
            Some(serde_json::to_value(vfs_with_default_backend("backend-1")).unwrap());
        frame.mcp_surface_json =
            Some(serde_json::to_value(Vec::<RuntimeMcpServer>::new()).unwrap());
        frame
    }

    fn vfs_with_default_backend(backend_id: &str) -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: backend_id.to_string(),
                root_ref: "F:/Projects/AgentDash".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::Write],
                default_write: true,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn vfs_with_workspace_metadata(backend_id: &str, workspace_id: Uuid, binding_id: Uuid) -> Vfs {
        let mut vfs = vfs_with_default_backend(backend_id);
        vfs.mounts[0].metadata = serde_json::json!({
            "workspace_id": workspace_id,
            "workspace_binding_id": binding_id,
        });
        vfs
    }

    fn vfs_without_backend() -> Vfs {
        let mut vfs = vfs_with_default_backend("");
        vfs.mounts[0].backend_id = " ".to_string();
        vfs
    }

    async fn insert_current_frame(fixture: &Fixture, frame: AgentFrame) {
        fixture
            .frame_repo
            .create(&frame)
            .await
            .expect("current frame");
    }

    #[tokio::test]
    async fn missing_anchor_returns_typed_error() {
        let fixture = fixture().await;

        let error = fixture
            .query
            .current_runtime_surface("missing-session", "resource_browser".into())
            .await
            .expect_err("missing anchor should fail");

        assert!(matches!(
            error,
            AgentRunRuntimeSurfaceQueryError::MissingAnchor { ref purpose, .. }
                if purpose.component == "resource_browser"
        ));
    }

    #[tokio::test]
    async fn anchor_run_agent_mismatch_returns_typed_error() {
        let fixture = fixture().await;
        let other_run = LifecycleRun::new_plain(Uuid::new_v4());
        let other_run_id = other_run.id;
        fixture
            .run_repo
            .create(&other_run)
            .await
            .expect("other run");
        let bad_agent = LifecycleAgent::new_root(
            other_run_id,
            other_run.project_id,
            AgentSource::ProjectAgent,
        );
        let bad_agent_id = bad_agent.id;
        fixture
            .agent_repo
            .create(&bad_agent)
            .await
            .expect("bad agent");
        fixture
            .anchor_repo
            .create_once(&RuntimeSessionExecutionAnchor::new_dispatch(
                "mismatch-session",
                fixture.run_id,
                fixture.launch_frame_id,
                bad_agent_id,
            ))
            .await
            .expect("bad anchor");

        let error = fixture
            .query
            .current_runtime_surface("mismatch-session", "mcp_access".into())
            .await
            .expect_err("mismatch should fail");

        assert!(matches!(
            error,
            AgentRunRuntimeSurfaceQueryError::AnchorControlPlaneMismatch {
                field: "agent.run_id",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn missing_required_surface_closure_returns_typed_error() {
        let fixture = fixture().await;
        insert_current_frame(&fixture, frame_without_capability(fixture.agent_id, 2)).await;

        let error = fixture
            .query
            .current_runtime_surface("session-1", "resource_browser".into())
            .await
            .expect_err("missing capability closure should fail");

        assert!(matches!(
            error,
            AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure {
                field: "capability_state",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn default_mount_backend_id_generates_anchor() {
        let fixture = fixture().await;
        insert_current_frame(
            &fixture,
            frame(
                fixture.agent_id,
                2,
                Some(vfs_with_default_backend("backend-current")),
            ),
        )
        .await;

        let surface = fixture
            .query
            .current_runtime_surface("session-1", "resource_browser".into())
            .await
            .expect("surface");

        let anchor = surface.runtime_backend_anchor.expect("anchor");
        assert_eq!(anchor.backend_id(), "backend-current");
        assert_eq!(anchor.source, RuntimeBackendAnchorSource::System);
        assert_eq!(anchor.root_ref.as_deref(), Some("F:/Projects/AgentDash"));
        assert_eq!(surface.run_id, fixture.run_id);
        assert_eq!(surface.project_id, fixture.project_id);
        assert_eq!(surface.agent_id, fixture.agent_id);
        assert_eq!(
            surface.runtime_address.frame_id,
            surface.current_surface_frame_id
        );
        assert_eq!(surface.launch_evidence_frame_id, fixture.launch_frame_id);
        assert_eq!(
            surface.provenance.launch_evidence_frame_id,
            fixture.launch_frame_id
        );
        assert_eq!(
            surface.provenance.current_surface_frame_id,
            surface.current_surface_frame_id
        );
    }

    #[tokio::test]
    async fn workspace_metadata_generates_workspace_binding_anchor() {
        let fixture = fixture().await;
        let workspace_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        insert_current_frame(
            &fixture,
            frame(
                fixture.agent_id,
                2,
                Some(vfs_with_workspace_metadata(
                    "backend-workspace",
                    workspace_id,
                    binding_id,
                )),
            ),
        )
        .await;

        let surface = fixture
            .query
            .current_runtime_surface("session-1", "resource_browser".into())
            .await
            .expect("surface");

        let anchor = surface.runtime_backend_anchor.expect("anchor");
        assert_eq!(anchor.backend_id(), "backend-workspace");
        assert_eq!(anchor.source, RuntimeBackendAnchorSource::WorkspaceBinding);
        assert_eq!(anchor.workspace_id, Some(workspace_id));
        assert_eq!(anchor.workspace_binding_id, Some(binding_id));
    }

    #[tokio::test]
    async fn resource_query_returns_surface_without_backend_anchor() {
        let fixture = fixture().await;
        insert_current_frame(
            &fixture,
            frame(fixture.agent_id, 2, Some(vfs_without_backend())),
        )
        .await;

        let surface = fixture
            .query
            .current_runtime_surface("session-1", RuntimeSurfaceQueryPurpose::resource_surface())
            .await
            .expect("resource surface should not require backend");

        assert!(surface.runtime_backend_anchor.is_none());
        assert_eq!(surface.vfs.default_mount_id.as_deref(), Some("workspace"));
    }

    #[tokio::test]
    async fn backend_required_query_missing_anchor_returns_typed_error() {
        let fixture = fixture().await;
        insert_current_frame(
            &fixture,
            frame(fixture.agent_id, 2, Some(vfs_without_backend())),
        )
        .await;

        let error = fixture
            .query
            .current_runtime_surface_with_backend(
                "session-1",
                RuntimeSurfaceQueryPurpose::new("runtime_mcp_tool_discovery"),
            )
            .await
            .expect_err("backend-required query should fail");

        assert!(matches!(
            &error,
            AgentRunRuntimeSurfaceQueryError::MissingRuntimeBackendAnchor { purpose, runtime_session_id, .. }
                if purpose.component == "runtime_mcp_tool_discovery"
                    && runtime_session_id == "session-1"
        ));
        assert!(matches!(
            error.as_runtime_backend_anchor_error(),
            Some(RuntimeBackendAnchorError::Missing { component, session_id, turn_id })
                if component == "runtime_mcp_tool_discovery"
                    && session_id.as_deref() == Some("session-1")
                    && turn_id.is_none()
        ));
    }

    #[tokio::test]
    async fn resource_surface_facade_projects_lifecycle_mount_from_current_surface() {
        let fixture = fixture().await;
        let current_frame = frame(
            fixture.agent_id,
            2,
            Some(vfs_with_default_backend("backend-current")),
        );
        let current_frame_id = current_frame.id;
        insert_current_frame(&fixture, current_frame).await;
        let surface_query = Arc::new(AgentRunRuntimeSurfaceQuery::new(
            AgentRunRuntimeSurfaceQueryDeps {
                anchor_repo: fixture.anchor_repo.clone(),
                run_repo: fixture.run_repo.clone(),
                agent_repo: fixture.agent_repo.clone(),
                frame_repo: fixture.frame_repo.clone(),
                delivery_binding_repo: fixture.delivery_binding_repo.clone(),
                permission_grant_repo: fixture.permission_grant_repo.clone(),
            },
        ));
        let resource_query: Arc<dyn ports_agent_run_surface::AgentRunResourceSurfaceQueryPort> =
            Arc::new(AgentRunResourceSurfaceQuery::new(
                AgentRunResourceSurfaceQueryDeps {
                    anchor_repo: fixture.anchor_repo.clone(),
                    surface_query,
                    lifecycle_surface_projection: Arc::new(TestLifecycleSurfaceProjection),
                },
            ));

        let resource_surface = resource_query
            .resource_surface_for_runtime_session("session-1")
            .await
            .expect("resource surface");

        assert_eq!(
            resource_surface.runtime.launch_evidence_frame_id,
            fixture.launch_frame_id
        );
        assert_eq!(
            resource_surface.runtime.current_surface_frame_id,
            current_frame_id
        );
        assert_eq!(
            resource_surface
                .lifecycle_surface
                .lifecycle_mount
                .metadata
                .get("launch_frame_id")
                .and_then(serde_json::Value::as_str),
            Some(current_frame_id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn resource_surface_for_agent_run_uses_current_delivery_binding() {
        let fixture = fixture().await;
        let current_frame = frame(
            fixture.agent_id,
            2,
            Some(vfs_with_default_backend("backend-current")),
        );
        let current_frame_id = current_frame.id;
        insert_current_frame(&fixture, current_frame).await;
        let mut unbound_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "session-unbound-later",
            fixture.run_id,
            fixture.launch_frame_id,
            fixture.agent_id,
        );
        unbound_anchor.updated_at = Utc::now() + chrono::Duration::seconds(60);
        fixture
            .anchor_repo
            .create_once(&unbound_anchor)
            .await
            .expect("unbound anchor");
        let surface_query = Arc::new(AgentRunRuntimeSurfaceQuery::new(
            AgentRunRuntimeSurfaceQueryDeps {
                anchor_repo: fixture.anchor_repo.clone(),
                run_repo: fixture.run_repo.clone(),
                agent_repo: fixture.agent_repo.clone(),
                frame_repo: fixture.frame_repo.clone(),
                delivery_binding_repo: fixture.delivery_binding_repo.clone(),
                permission_grant_repo: fixture.permission_grant_repo.clone(),
            },
        ));
        let resource_query = AgentRunResourceSurfaceQuery::new(AgentRunResourceSurfaceQueryDeps {
            anchor_repo: fixture.anchor_repo.clone(),
            surface_query,
            lifecycle_surface_projection: Arc::new(TestLifecycleSurfaceProjection),
        });

        let resource_surface = resource_query
            .resource_surface_for_agent_run(fixture.run_id, fixture.agent_id)
            .await
            .expect("resource surface");

        assert_eq!(resource_surface.runtime.runtime_session_id, "session-1");
        assert_eq!(
            resource_surface.runtime.current_surface_frame_id,
            current_frame_id
        );
        assert_eq!(
            resource_surface
                .lifecycle_surface
                .projections
                .message_stream
                .as_ref()
                .map(|stream| stream.runtime_session_id.as_str()),
            Some("session-1")
        );
    }

    #[tokio::test]
    async fn active_permission_grant_vfs_rule_is_projected_into_runtime_policy() {
        let fixture = fixture().await;
        let current_frame = frame(
            fixture.agent_id,
            2,
            Some(vfs_with_default_backend("backend-current")),
        );
        let current_frame_id = current_frame.id;
        insert_current_frame(&fixture, current_frame).await;
        let grant = active_vfs_grant(
            fixture.run_id,
            current_frame_id,
            None,
            PermissionGrantVfsPathScope::Prefix("src".to_string()),
            vec![PermissionGrantVfsOperation::Read],
        );
        fixture
            .permission_grant_repo
            .create(&grant)
            .await
            .expect("grant");

        let surface = fixture
            .query
            .current_runtime_surface("session-1", "resource_browser".into())
            .await
            .expect("surface");

        assert!(
            surface
                .vfs_access_policy
                .rules
                .iter()
                .any(
                    |rule| rule.source == RuntimeVfsAccessSource::PermissionGrant
                        && rule.mount_id == "workspace"
                        && rule.operations.contains(&RuntimeVfsOperation::Read)
                        && rule.path_pattern.matches_normalized_path("src/lib.rs")
                )
        );
        assert!(surface.vfs_access_policy.admits(
            "workspace",
            "src/lib.rs",
            RuntimeVfsOperation::Read
        ));
        assert!(
            !surface.vfs_access_policy.admits(
                "workspace",
                "tests/lib.rs",
                RuntimeVfsOperation::Read
            ),
            "PermissionGrant path rule must affect the final effective policy, not only add an audit rule"
        );
    }

    fn active_vfs_grant(
        run_id: Uuid,
        effect_frame_id: Uuid,
        surface_ref: Option<String>,
        path_scope: PermissionGrantVfsPathScope,
        operations: Vec<PermissionGrantVfsOperation>,
    ) -> PermissionGrant {
        let mut grant = PermissionGrant::new(
            run_id,
            "session-1",
            Vec::new(),
            "allow vfs access",
            GrantScope::AgentFrame,
            None,
        )
        .with_effect_frame(effect_frame_id)
        .with_requested_vfs_access(vec![PermissionGrantVfsAccessRule {
            surface_ref,
            mount_id: "workspace".to_string(),
            path_scope,
            operations,
        }])
        .expect("vfs access");
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: Vec::new(),
                reason: "manual".to_string(),
            })
            .expect("policy");
        grant.user_approve("user-1").expect("approve");
        grant.mark_applied().expect("applied");
        grant
    }

    #[test]
    fn terminal_target_uses_backend_anchor_relay_mount() {
        let vfs = Vfs {
            mounts: vec![
                relay_mount("other", "backend-other", "F:/Other"),
                relay_mount("main", "backend-main", "F:/Projects/AgentDash"),
            ],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let anchor = RuntimeBackendAnchor::new(
            "anchor-backend",
            RuntimeBackendAnchorSource::WorkspaceBinding,
        )
        .expect("anchor")
        .with_root_ref(Some("F:/Projects/AgentDash".to_string()));

        let target = terminal_launch_target_from_vfs(&vfs, &anchor).expect("target");

        assert_eq!(
            target,
            AgentRunTerminalLaunchTarget {
                backend_id: "anchor-backend".to_string(),
                mount_root_ref: "F:/Projects/AgentDash".to_string(),
            }
        );
    }

    #[test]
    fn terminal_target_rejects_non_relay_mount() {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "lifecycle".to_string(),
                provider: "lifecycle_vfs".to_string(),
                backend_id: String::new(),
                root_ref: "lifecycle://run/example".to_string(),
                capabilities: vec![MountCapability::Read],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("lifecycle".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let anchor = RuntimeBackendAnchor::new("backend-main", RuntimeBackendAnchorSource::System)
            .expect("anchor")
            .with_root_ref(Some("lifecycle://run/example".to_string()));

        let error = terminal_launch_target_from_vfs(&vfs, &anchor).expect_err("provider");

        assert!(matches!(
            error,
            AgentRunTerminalLaunchTargetError::UnsupportedMountProvider { .. }
        ));
    }

    fn relay_mount(id: &str, backend_id: &str, root_ref: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: PROVIDER_RELAY_FS.to_string(),
            backend_id: backend_id.to_string(),
            root_ref: root_ref.to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: true,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }
}
