use std::{collections::BTreeSet, sync::Arc};

use agentdash_agent_runtime_contract::{PresentationThreadId, RuntimeThreadId};
use agentdash_application_ports::lifecycle_surface_projection as lifecycle_surface;
use agentdash_application_ports::runtime_gateway_mcp_surface::{
    RuntimeGatewayMcpSurface, RuntimeGatewayMcpSurfaceQueryError,
    RuntimeGatewayMcpSurfaceQueryPort, RuntimeGatewayMcpSurfaceQueryPurpose,
    RuntimeGatewayMcpSurfaceWithBackend,
};
use agentdash_application_ports::{
    agent_run_runtime::{AgentRunRuntimeBindingRepository, AgentRunRuntimeTarget},
    agent_run_surface::{
        AgentRunAdmissionDecision, AgentRunAdmissionRequest, AgentRunEffectiveCapabilityError,
        AgentRunEffectiveCapabilityPort, AgentRunEffectiveCapabilityRequest,
        AgentRunEffectiveCapabilityView, AgentRunResourceSurface,
        AgentRunResourceSurfaceQueryError, AgentRunResourceSurfaceQueryPort,
        AgentRunRuntimeAddress, AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure,
        AgentRunRuntimeSurfaceProvenance, AgentRunRuntimeSurfaceQueryError,
        AgentRunRuntimeSurfaceQueryPort, AgentRunRuntimeSurfaceWithBackend,
        RuntimeSurfaceQueryPurpose,
    },
};
use agentdash_domain::permission::{
    PermissionGrant, PermissionGrantRepository, PermissionGrantVfsOperation,
    PermissionGrantVfsPathScope,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
};
use agentdash_spi::{
    RuntimeVfsAccessPolicy, RuntimeVfsAccessRule, RuntimeVfsAccessSource, RuntimeVfsOperation,
    RuntimeVfsPathPattern, Vfs,
};
use async_trait::async_trait;

use super::AgentFrameSurfaceExt;
use crate::agent_run::frame::runtime_launch::runtime_backend_anchor_from_vfs;
use crate::agent_run::runtime_capability::project_capability_state_from_frame;

#[derive(Clone)]
pub struct BusinessFrameSurfaceQuery {
    binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

#[derive(Clone)]
pub struct BusinessFrameSurfaceQueryDeps {
    pub binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

#[derive(Clone)]
pub struct BusinessResourceSurfaceQuery {
    binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    lifecycle_surface_projection: Arc<dyn lifecycle_surface::LifecycleSurfaceProjectionPort>,
}

#[derive(Clone)]
pub struct BusinessResourceSurfaceQueryDeps {
    pub binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub lifecycle_surface_projection: Arc<dyn lifecycle_surface::LifecycleSurfaceProjectionPort>,
}

impl BusinessResourceSurfaceQuery {
    pub fn new(deps: BusinessResourceSurfaceQueryDeps) -> Self {
        Self {
            binding_repo: deps.binding_repo,
            surface_query: deps.surface_query,
            lifecycle_surface_projection: deps.lifecycle_surface_projection,
        }
    }

    async fn project(
        &self,
        runtime: AgentRunRuntimeSurface,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let lifecycle_surface = self
            .lifecycle_surface_projection
            .project_lifecycle_surface(lifecycle_surface::AgentRunLifecycleSurfaceInput {
                base_vfs: Some(runtime.vfs.clone()),
                address: runtime.runtime_address.clone(),
                message_stream: Some(lifecycle_surface::MessageStreamProjectionRef {
                    runtime_session_id: runtime.runtime_session_id.clone(),
                    trace_kind: lifecycle_surface::MessageStreamTraceKind::ConnectorRuntimeSession,
                }),
                project_id: runtime.project_id,
                mode: lifecycle_surface::AgentRunLifecycleSurfaceMode::WorkspaceReadSurface,
                explicit_skill_asset_keys: Vec::new(),
                builtin_skills: lifecycle_surface::BuiltinLifecycleSkillPolicy::PreserveProjected,
                node_evidence: None,
                node_projection: None,
            })
            .await
            .map_err(|error| AgentRunResourceSurfaceQueryError::Projection {
                message: error.to_string(),
            })?;
        Ok(AgentRunResourceSurface {
            runtime,
            lifecycle_surface,
        })
    }
}

#[async_trait]
impl AgentRunResourceSurfaceQueryPort for BusinessResourceSurfaceQuery {
    async fn resource_surface_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let runtime = self
            .surface_query
            .current_runtime_surface(
                runtime_session_id,
                RuntimeSurfaceQueryPurpose::resource_surface(),
            )
            .await?;
        self.project(runtime).await
    }

    async fn resource_surface_for_agent_run(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError> {
        let binding = self
            .binding_repo
            .load(&AgentRunRuntimeTarget { run_id, agent_id })
            .await
            .map_err(|error| AgentRunResourceSurfaceQueryError::Repository {
                operation: "agent run runtime binding",
                message: error.to_string(),
            })?
            .ok_or(AgentRunResourceSurfaceQueryError::MissingDeliveryAnchor { run_id, agent_id })?;
        self.resource_surface_for_runtime_session(&binding.thread_id.to_string())
            .await
    }
}

impl BusinessFrameSurfaceQuery {
    pub fn new(deps: BusinessFrameSurfaceQueryDeps) -> Self {
        Self {
            binding_repo: deps.binding_repo,
            run_repo: deps.run_repo,
            agent_repo: deps.agent_repo,
            frame_repo: deps.frame_repo,
            permission_grant_repo: deps.permission_grant_repo,
        }
    }

    pub async fn current_surface_for_target(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let binding = self
            .binding_repo
            .load(&AgentRunRuntimeTarget { run_id, agent_id })
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "agent run runtime binding",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingAnchor {
                purpose: purpose.clone(),
                runtime_session_id: format!("target:{run_id}:{agent_id}"),
            })?;
        self.surface(&binding.thread_id.to_string(), purpose).await
    }

    pub async fn surface_for_provision_target(
        &self,
        target: &AgentRunRuntimeTarget,
        thread_id: &RuntimeThreadId,
        presentation_thread_id: &PresentationThreadId,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        self.surface_for_resolved_target(
            target,
            &thread_id.to_string(),
            presentation_thread_id,
            purpose,
        )
        .await
    }

    async fn binding_for_thread(
        &self,
        runtime_session_id: &str,
        purpose: &RuntimeSurfaceQueryPurpose,
    ) -> Result<
        agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBinding,
        AgentRunRuntimeSurfaceQueryError,
    > {
        let thread_id = RuntimeThreadId::new(runtime_session_id).map_err(|error| {
            AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "runtime thread id",
                message: error.to_string(),
            }
        })?;
        self.binding_repo
            .load_by_thread_id(&thread_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "agent run runtime binding",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingAnchor {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
            })
    }

    async fn frame_for_target(
        &self,
        target: &AgentRunRuntimeTarget,
        runtime_session_id: &str,
        purpose: &RuntimeSurfaceQueryPurpose,
    ) -> Result<
        (agentdash_domain::workflow::LifecycleRun, AgentFrame),
        AgentRunRuntimeSurfaceQueryError,
    > {
        let run = self
            .run_repo
            .get_by_id(target.run_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "lifecycle run",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingLifecycleRun {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                run_id: target.run_id,
            })?;
        let agent = self
            .agent_repo
            .get(target.agent_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "lifecycle agent",
                message: error.to_string(),
            })?
            .filter(|agent| agent.run_id == target.run_id)
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingLifecycleAgent {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                agent_id: target.agent_id,
            })?;
        let frame = self
            .frame_repo
            .get_current(agent.id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "current agent frame",
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::MissingCurrentFrame {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                agent_id: target.agent_id,
            })?;
        Ok((run, frame))
    }

    async fn surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let binding = self
            .binding_for_thread(runtime_session_id, &purpose)
            .await?;
        self.surface_for_resolved_target(
            &binding.target,
            runtime_session_id,
            &binding.presentation_thread_id,
            purpose,
        )
        .await
    }

    async fn surface_for_resolved_target(
        &self,
        target: &AgentRunRuntimeTarget,
        runtime_session_id: &str,
        presentation_thread_id: &PresentationThreadId,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let (run, frame) = self
            .frame_for_target(target, runtime_session_id, &purpose)
            .await?;
        frame.typed_capability_state().ok_or_else(|| {
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
        let mcp_surface = frame.mcp_surface_json.as_ref().ok_or_else(|| {
            AgentRunRuntimeSurfaceQueryError::MissingSurfaceClosure {
                purpose: purpose.clone(),
                runtime_session_id: runtime_session_id.to_string(),
                frame_id: frame.id,
                field: "mcp",
            }
        })?;
        let mcp_servers = serde_json::from_value::<Vec<agentdash_spi::RuntimeMcpServer>>(
            mcp_surface.clone(),
        )
        .map_err(|error| AgentRunRuntimeSurfaceQueryError::Projection {
            message: format!(
                "runtime surface MCP closure is invalid: session_id={runtime_session_id}, frame_id={}, {error}",
                frame.id
            ),
        })?;
        frame.validated_hook_plan().map_err(|message| {
            AgentRunRuntimeSurfaceQueryError::Projection {
                message: format!(
                    "runtime surface HookPlan closure is invalid: session_id={runtime_session_id}, frame_id={}, {message}",
                    frame.id
                ),
            }
        })?;
        let capability_state = project_capability_state_from_frame(&frame);
        let active_grants = self
            .permission_grant_repo
            .list_active_by_frame(frame.id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "active permission grants",
                message: error.to_string(),
            })?;
        let vfs_access_policy =
            runtime_vfs_access_policy_for_grants(&vfs, runtime_session_id, &active_grants);
        let persisted_binding = self.binding_repo.load(target).await.map_err(|error| {
            AgentRunRuntimeSurfaceQueryError::Repository {
                operation: "agent run runtime binding provenance",
                message: error.to_string(),
            }
        })?;
        if persisted_binding
            .as_ref()
            .is_some_and(|binding| binding.thread_id.as_str() != runtime_session_id)
        {
            return Err(AgentRunRuntimeSurfaceQueryError::Projection {
                message: format!(
                    "runtime surface binding/thread mismatch: session_id={runtime_session_id}, target={}:{}",
                    target.run_id, target.agent_id
                ),
            });
        }
        if persisted_binding
            .as_ref()
            .is_some_and(|binding| &binding.presentation_thread_id != presentation_thread_id)
        {
            return Err(AgentRunRuntimeSurfaceQueryError::Projection {
                message: format!(
                    "runtime surface binding/presentation mismatch: presentation_thread_id={}, target={}:{}",
                    presentation_thread_id, target.run_id, target.agent_id
                ),
            });
        }
        let launch_frame = if let Some(binding) = persisted_binding.as_ref() {
            let launch_frame_id =
                uuid::Uuid::parse_str(&binding.surface.source_frame_id).map_err(|_| {
                    AgentRunRuntimeSurfaceQueryError::Projection {
                        message: format!(
                            "runtime binding surface source_frame_id is invalid: {}",
                            binding.surface.source_frame_id
                        ),
                    }
                })?;
            let launch_frame = self
                .frame_repo
                .get(launch_frame_id)
                .await
                .map_err(|error| AgentRunRuntimeSurfaceQueryError::Repository {
                    operation: "launch AgentFrame",
                    message: error.to_string(),
                })?
                .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::Projection {
                    message: format!(
                        "runtime launch AgentFrame `{launch_frame_id}` does not exist"
                    ),
                })?;
            if launch_frame.agent_id != target.agent_id {
                return Err(AgentRunRuntimeSurfaceQueryError::Projection {
                    message: format!(
                        "runtime launch AgentFrame `{launch_frame_id}` does not belong to agent `{}`",
                        target.agent_id
                    ),
                });
            }
            launch_frame
        } else {
            frame.clone()
        };
        let orchestration_coordinate = orchestration_coordinate_from_vfs(&vfs, run.id)?;
        if let Some((orchestration_id, node_path, attempt)) = orchestration_coordinate.as_ref() {
            let node = run
                .orchestrations
                .iter()
                .find(|value| value.orchestration_id == *orchestration_id)
                .and_then(|value| {
                    find_runtime_node_for_coordinate(&value.node_tree, node_path, *attempt)
                })
                .ok_or_else(|| AgentRunRuntimeSurfaceQueryError::Projection {
                    message: format!(
                        "AgentFrame lifecycle evidence points to a missing runtime node: orchestration_id={orchestration_id}, node_path={node_path}, attempt={attempt}"
                    ),
                })?;
            if let Some(executor_run_ref) = node.executor_run_ref.as_ref() {
                let matches_presentation = matches!(
                    executor_run_ref,
                    agentdash_domain::workflow::ExecutorRunRef::RuntimeSession { session_id }
                        if session_id == presentation_thread_id.as_str()
                );
                if !matches_presentation {
                    return Err(AgentRunRuntimeSurfaceQueryError::Projection {
                        message: format!(
                            "AgentFrame lifecycle evidence presentation mismatch: expected={}, node_path={node_path}",
                            presentation_thread_id
                        ),
                    });
                }
            }
        }
        let runtime_backend_anchor =
            runtime_backend_anchor_from_vfs(&vfs, Some("business_frame_surface_query".to_string()))
                .map_err(
                    |source| AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor { source },
                )?;
        Ok(AgentRunRuntimeSurface {
            runtime_session_id: runtime_session_id.to_string(),
            presentation_thread_id: presentation_thread_id.clone(),
            run_id: run.id,
            project_id: run.project_id,
            agent_id: frame.agent_id,
            runtime_address: AgentRunRuntimeAddress {
                run_id: run.id,
                agent_id: frame.agent_id,
                frame_id: frame.id,
            },
            launch_evidence_frame_id: launch_frame.id,
            current_surface_frame_id: frame.id,
            surface_revision: frame.revision,
            vfs_access_policy,
            mcp_servers,
            runtime_backend_anchor,
            active_turn_id: None,
            identity: None,
            provenance: AgentRunRuntimeSurfaceProvenance {
                launch_evidence_frame_id: launch_frame.id,
                launch_created_by_kind: launch_frame.created_by_kind.clone(),
                current_surface_frame_id: frame.id,
                surface_revision: frame.revision,
                surface_created_by_kind: frame.created_by_kind.clone(),
                anchor_updated_at: frame.created_at,
                orchestration_id: orchestration_coordinate.as_ref().map(|value| value.0),
                node_path: orchestration_coordinate
                    .as_ref()
                    .map(|value| value.1.clone()),
                node_attempt: orchestration_coordinate.map(|value| value.2),
            },
            closure: AgentRunRuntimeSurfaceClosure {
                capability_field_present: frame.effective_capability_json.is_some(),
                vfs_field_present: frame.vfs_surface_json.is_some(),
                mcp_field_present: frame.mcp_surface_json.is_some(),
            },
            capability_state,
            visible_workspace_module_refs: frame.visible_workspace_module_refs(),
            vfs,
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
                    .collect(),
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

fn find_runtime_node_for_coordinate<'a>(
    nodes: &'a [agentdash_domain::workflow::RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a agentdash_domain::workflow::RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_for_coordinate(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}

fn orchestration_coordinate_from_vfs(
    vfs: &Vfs,
    expected_run_id: uuid::Uuid,
) -> Result<Option<(uuid::Uuid, String, u32)>, AgentRunRuntimeSurfaceQueryError> {
    let Some(metadata) = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == "lifecycle")
        .map(|mount| &mount.metadata)
        .filter(|metadata| {
            metadata.get("scope").and_then(serde_json::Value::as_str) == Some("node_runtime")
        })
    else {
        return Ok(None);
    };
    let run_id = metadata
        .get("run_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok());
    let orchestration_id = metadata
        .get("orchestration_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok());
    let node_path = metadata
        .get("node_path")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty());
    let attempt = metadata
        .get("attempt")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let (Some(run_id), Some(orchestration_id), Some(node_path), Some(attempt)) =
        (run_id, orchestration_id, node_path, attempt)
    else {
        return Err(AgentRunRuntimeSurfaceQueryError::Projection {
            message: "AgentFrame lifecycle node evidence is incomplete".to_string(),
        });
    };
    if run_id != expected_run_id {
        return Err(AgentRunRuntimeSurfaceQueryError::Projection {
            message: format!(
                "AgentFrame lifecycle node evidence run mismatch: expected={expected_run_id}, actual={run_id}"
            ),
        });
    }
    Ok(Some((orchestration_id, node_path.to_string(), attempt)))
}

#[cfg(test)]
mod orchestration_evidence_tests {
    use agentdash_spi::{Mount, MountCapability};

    use super::*;

    #[test]
    fn workflow_coordinate_comes_from_agent_frame_lifecycle_evidence() {
        let run_id = uuid::Uuid::new_v4();
        let orchestration_id = uuid::Uuid::new_v4();
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "lifecycle".to_string(),
                provider: "lifecycle_vfs".to_string(),
                backend_id: String::new(),
                root_ref: "lifecycle://fixture".to_string(),
                capabilities: vec![MountCapability::Read],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::json!({
                    "scope": "node_runtime",
                    "run_id": run_id,
                    "orchestration_id": orchestration_id,
                    "node_path": "phase/execute",
                    "attempt": 3
                }),
            }],
            ..Default::default()
        };

        assert_eq!(
            orchestration_coordinate_from_vfs(&vfs, run_id).expect("typed lifecycle evidence"),
            Some((orchestration_id, "phase/execute".to_string(), 3))
        );
    }

    #[test]
    fn incomplete_workflow_coordinate_is_rejected() {
        let run_id = uuid::Uuid::new_v4();
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "lifecycle".to_string(),
                provider: "lifecycle_vfs".to_string(),
                backend_id: String::new(),
                root_ref: "lifecycle://fixture".to_string(),
                capabilities: vec![MountCapability::Read],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::json!({
                    "scope": "node_runtime",
                    "run_id": run_id,
                    "node_path": "phase/execute",
                    "attempt": 3
                }),
            }],
            ..Default::default()
        };

        assert!(matches!(
            orchestration_coordinate_from_vfs(&vfs, run_id),
            Err(AgentRunRuntimeSurfaceQueryError::Projection { .. })
        ));
    }
}

#[async_trait]
impl RuntimeGatewayMcpSurfaceQueryPort for BusinessFrameSurfaceQuery {
    async fn current_runtime_mcp_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeGatewayMcpSurfaceQueryPurpose,
    ) -> Result<RuntimeGatewayMcpSurfaceWithBackend, RuntimeGatewayMcpSurfaceQueryError> {
        let runtime = self
            .current_runtime_surface_with_backend(
                runtime_session_id,
                RuntimeSurfaceQueryPurpose::new(purpose.component),
            )
            .await
            .map_err(|error| RuntimeGatewayMcpSurfaceQueryError::new(error.to_string()))?;
        Ok(RuntimeGatewayMcpSurfaceWithBackend {
            runtime_backend_anchor: runtime.runtime_backend_anchor,
            surface: RuntimeGatewayMcpSurface {
                runtime_session_id: runtime.surface.runtime_session_id,
                capability_state: runtime.surface.capability_state,
                vfs: runtime.surface.vfs,
                vfs_access_policy: runtime.surface.vfs_access_policy,
                mcp_servers: runtime.surface.mcp_servers,
                active_turn_id: runtime.surface.active_turn_id,
                identity: runtime.surface.identity,
            },
        })
    }
}

#[async_trait]
impl AgentRunRuntimeSurfaceQueryPort for BusinessFrameSurfaceQuery {
    async fn current_runtime_surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        self.surface(runtime_session_id, purpose).await
    }

    async fn current_runtime_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError> {
        let surface = self.surface(runtime_session_id, purpose).await?;
        let runtime_backend_anchor = surface.runtime_backend_anchor.clone().ok_or_else(|| {
            AgentRunRuntimeSurfaceQueryError::Projection {
                message: "current AgentFrame VFS 缺少 runtime backend anchor".to_string(),
            }
        })?;
        Ok(AgentRunRuntimeSurfaceWithBackend {
            surface,
            runtime_backend_anchor,
        })
    }
}

#[async_trait]
impl AgentRunEffectiveCapabilityPort for BusinessFrameSurfaceQuery {
    async fn effective_capability(
        &self,
        request: AgentRunEffectiveCapabilityRequest,
    ) -> Result<AgentRunEffectiveCapabilityView, AgentRunEffectiveCapabilityError> {
        let purpose = RuntimeSurfaceQueryPurpose::new("effective_capability");
        let binding = self
            .binding_for_thread(&request.runtime_session_id, &purpose)
            .await
            .map_err(|error| AgentRunEffectiveCapabilityError::Repository {
                operation: "runtime binding",
                message: error.to_string(),
            })?;
        if binding.target.run_id != request.agent_run_id
            || binding.target.agent_id != request.agent_id
        {
            return Err(AgentRunEffectiveCapabilityError::MissingTarget {
                run_id: request.agent_run_id,
                agent_id: request.agent_id,
            });
        }
        let (_, frame) = self
            .frame_for_target(&binding.target, &request.runtime_session_id, &purpose)
            .await
            .map_err(|error| AgentRunEffectiveCapabilityError::Repository {
                operation: "current frame",
                message: error.to_string(),
            })?;
        Ok(effective_view(&binding.thread_id, &frame))
    }

    async fn admit_tool(
        &self,
        request: AgentRunAdmissionRequest,
    ) -> Result<AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError> {
        let purpose = RuntimeSurfaceQueryPurpose::new("tool_admission");
        let binding = self
            .binding_for_thread(&request.runtime_session_id, &purpose)
            .await
            .map_err(|error| AgentRunEffectiveCapabilityError::Repository {
                operation: "runtime binding",
                message: error.to_string(),
            })?;
        let (_, frame) = self
            .frame_for_target(&binding.target, &request.runtime_session_id, &purpose)
            .await
            .map_err(|error| AgentRunEffectiveCapabilityError::Repository {
                operation: "current frame",
                message: error.to_string(),
            })?;
        let capability = effective_view(&binding.thread_id, &frame);
        let allowed = capability.capability_state.is_capability_tool_enabled(
            &request.capability_key,
            &request.tool_name,
            request.cluster,
        );
        Ok(AgentRunAdmissionDecision {
            allowed,
            reason: (!allowed).then(|| {
                format!(
                    "tool {} is not enabled by current AgentFrame capability",
                    request.tool_name
                )
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::permission::{
        GrantScope, PermissionGrantVfsAccessRule, PolicyDecision, PolicyOutcome,
    };

    fn active_vfs_grant(frame_id: uuid::Uuid, surface_ref: Option<String>) -> PermissionGrant {
        let mut grant = PermissionGrant::new(
            uuid::Uuid::new_v4(),
            None,
            Vec::new(),
            "allow scoped reads",
            GrantScope::AgentFrame,
            None,
        )
        .with_effect_frame(frame_id)
        .with_requested_vfs_access(vec![PermissionGrantVfsAccessRule {
            surface_ref,
            mount_id: "workspace".to_string(),
            path_scope: PermissionGrantVfsPathScope::Prefix("src".to_string()),
            operations: vec![PermissionGrantVfsOperation::Read],
        }])
        .unwrap();
        grant.submit_for_policy().unwrap();
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: Vec::new(),
                reason: "manual".to_string(),
            })
            .unwrap();
        grant.user_approve("user-1").unwrap();
        grant.mark_applied().unwrap();
        grant
    }

    fn fixture_vfs() -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "fixture".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "fixture://workspace".to_string(),
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

    #[test]
    fn active_frame_grant_replaces_whole_mount_policy_before_execution() {
        let frame_id = uuid::Uuid::new_v4();
        let grant = active_vfs_grant(frame_id, Some("thread-1".to_string()));
        let policy = runtime_vfs_access_policy_for_grants(&fixture_vfs(), "thread-1", &[grant]);

        assert!(policy.admits("workspace", "src/lib.rs", RuntimeVfsOperation::Read));
        assert!(!policy.admits("workspace", "tests/lib.rs", RuntimeVfsOperation::Read));
        assert!(!policy.admits("workspace", "src/lib.rs", RuntimeVfsOperation::Write));
        assert!(policy.rules.iter().all(|rule| {
            rule.mount_id != "workspace"
                || rule.source != RuntimeVfsAccessSource::SystemRuntimeProjection
        }));
    }

    #[test]
    fn grant_for_other_surface_does_not_replace_current_surface_policy() {
        let grant = active_vfs_grant(uuid::Uuid::new_v4(), Some("thread-other".to_string()));
        let policy = runtime_vfs_access_policy_for_grants(&fixture_vfs(), "thread-1", &[grant]);

        assert!(policy.admits("workspace", "tests/lib.rs", RuntimeVfsOperation::Read));
        assert!(policy.admits("workspace", "src/lib.rs", RuntimeVfsOperation::Write));
    }
}

fn effective_view(
    thread_id: &RuntimeThreadId,
    frame: &AgentFrame,
) -> AgentRunEffectiveCapabilityView {
    let capability_state = project_capability_state_from_frame(frame);
    AgentRunEffectiveCapabilityView {
        target: agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget {
            frame_id: frame.id,
            runtime_thread_id: thread_id.clone(),
        },
        visible_capabilities: capability_state.tool.capabilities.clone(),
        vfs_surface: capability_state.vfs.active.clone().unwrap_or_default(),
        mcp_surface: capability_state.tool.mcp_servers.clone(),
        visible_workspace_module_refs: frame.visible_workspace_module_refs(),
        grant_projection: Default::default(),
        capability_state,
    }
}
