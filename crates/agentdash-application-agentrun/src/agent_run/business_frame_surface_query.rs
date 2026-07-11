use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
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
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
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
}

#[derive(Clone)]
pub struct BusinessFrameSurfaceQueryDeps {
    pub binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
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
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        self.surface_for_resolved_target(target, &thread_id.to_string(), purpose)
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
        self.surface_for_resolved_target(&binding.target, runtime_session_id, purpose)
            .await
    }

    async fn surface_for_resolved_target(
        &self,
        target: &AgentRunRuntimeTarget,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError> {
        let (run, frame) = self
            .frame_for_target(target, runtime_session_id, &purpose)
            .await?;
        let capability_state = project_capability_state_from_frame(&frame);
        let vfs = frame.typed_vfs().unwrap_or_default();
        let runtime_backend_anchor =
            runtime_backend_anchor_from_vfs(&vfs, Some("business_frame_surface_query".to_string()))
                .map_err(
                    |source| AgentRunRuntimeSurfaceQueryError::RuntimeBackendAnchor { source },
                )?;
        Ok(AgentRunRuntimeSurface {
            runtime_session_id: runtime_session_id.to_string(),
            run_id: run.id,
            project_id: run.project_id,
            agent_id: frame.agent_id,
            runtime_address: AgentRunRuntimeAddress {
                run_id: run.id,
                agent_id: frame.agent_id,
                frame_id: frame.id,
            },
            launch_evidence_frame_id: frame.id,
            current_surface_frame_id: frame.id,
            surface_revision: frame.revision,
            vfs_access_policy: agentdash_spi::RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs),
            mcp_servers: capability_state.tool.mcp_servers.clone(),
            runtime_backend_anchor,
            active_turn_id: None,
            identity: None,
            provenance: AgentRunRuntimeSurfaceProvenance {
                launch_evidence_frame_id: frame.id,
                launch_created_by_kind: frame.created_by_kind.clone(),
                current_surface_frame_id: frame.id,
                surface_revision: frame.revision,
                surface_created_by_kind: frame.created_by_kind.clone(),
                anchor_updated_at: frame.created_at,
                orchestration_id: None,
                node_path: None,
                node_attempt: None,
            },
            closure: AgentRunRuntimeSurfaceClosure {
                capability_field_present: frame.effective_capability_json.is_some(),
                vfs_field_present: frame.vfs_surface_json.is_some(),
                mcp_field_present: frame.mcp_surface_json.is_some(),
            },
            capability_state,
            vfs,
        })
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
