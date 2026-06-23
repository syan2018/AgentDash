//! Permission grant runtime surface update adapter.
//!
//! Permission accepts typed update requests through the AgentRun frame/surface
//! facade while keeping grant-state orchestration in the permission service.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::ApplicationError;
use crate::agent_run::AgentFrameRuntimeTarget;
use crate::agent_run::runtime_capability::{
    ToolCapabilityDimensionModule, project_capability_state_from_frame,
};
use crate::agent_run::{
    AgentFrameBuilder, AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
    AgentRunFrameSurfaceService, AgentRunGrantProjection, AgentRunRuntimeSurfaceUpdateAdapter,
    AgentRunRuntimeSurfaceUpdateService, RejectingFrameConstructionAdapter,
    RuntimeSurfaceUpdateRequest,
};
use agentdash_domain::permission::PermissionGrant;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository, ToolCapabilityPath};
use agentdash_spi::platform::tool_capability::capability_to_tool_clusters;
use agentdash_spi::{
    CapabilityState, RuntimeCapabilityTransition, SetToolAccessEffect, ToolCapability,
};
use tokio::sync::Mutex;

use super::compiler::PermissionGrantCompiler;

#[async_trait]
pub trait PermissionRuntimeSurfaceAdopter: Send + Sync {
    async fn adopt_permission_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<(), String>;
}

#[async_trait]
impl PermissionRuntimeSurfaceAdopter for AgentRunRuntimeSurfaceUpdateService {
    async fn adopt_permission_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<(), String> {
        self.adopt_persisted_frame_revision_into_active_runtime(target)
            .await
            .map(|_| ())
    }
}

#[derive(Debug)]
pub struct PermissionRuntimeSurfaceUpdateOutcome {
    pub transition: RuntimeCapabilityTransition,
    pub effect_frame: Option<AgentFrame>,
    adoption_target: Option<AgentFrameRuntimeTarget>,
}

impl PermissionRuntimeSurfaceUpdateOutcome {
    pub fn no_surface(transition: RuntimeCapabilityTransition) -> Self {
        Self {
            transition,
            effect_frame: None,
            adoption_target: None,
        }
    }
}

pub struct PermissionRuntimeSurfaceUpdateService {
    frame_repo: Arc<dyn AgentFrameRepository>,
    adopter: Option<Arc<dyn PermissionRuntimeSurfaceAdopter>>,
}

impl PermissionRuntimeSurfaceUpdateService {
    pub fn new(frame_repo: Arc<dyn AgentFrameRepository>) -> Self {
        Self {
            frame_repo,
            adopter: None,
        }
    }

    pub fn with_adopter(
        frame_repo: Arc<dyn AgentFrameRepository>,
        adopter: Arc<dyn PermissionRuntimeSurfaceAdopter>,
    ) -> Self {
        Self {
            frame_repo,
            adopter: Some(adopter),
        }
    }

    pub async fn apply_update_request(
        &self,
        grant: &PermissionGrant,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<PermissionRuntimeSurfaceUpdateOutcome, ApplicationError> {
        let outcome = Arc::new(Mutex::new(None));
        let adapter = PermissionGrantRuntimeSurfaceUpdateAdapter {
            projector: Self {
                frame_repo: self.frame_repo.clone(),
                adopter: self.adopter.clone(),
            },
            grant: grant.clone(),
            outcome: outcome.clone(),
        };
        let service = AgentRunFrameSurfaceService::new(
            Arc::new(RejectingFrameConstructionAdapter),
            Arc::new(adapter),
        );
        service
            .update_runtime_surface(request)
            .await
            .map_err(permission_frame_surface_error_to_application)?;
        outcome.lock().await.take().ok_or_else(|| {
            ApplicationError::Internal(
                "Permission runtime surface adapter did not return an update outcome".to_string(),
            )
        })
    }

    async fn project_update_request(
        &self,
        grant: &PermissionGrant,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<PermissionRuntimeSurfaceUpdateOutcome, ApplicationError> {
        let (request_grant_id, approve, created_by_kind) = permission_request_parts(&request)?;
        if request_grant_id != grant.id {
            return Err(ApplicationError::BadRequest(format!(
                "permission runtime surface request grant_id {} does not match grant {}",
                request_grant_id, grant.id
            )));
        }

        let mut transition = if approve {
            PermissionGrantCompiler::compile(grant)
        } else {
            PermissionGrantCompiler::compile_revoke(grant)
        };
        let (_, surface_paths) = AgentRunGrantProjection::partition_paths(&grant.requested_paths);
        if surface_paths.is_empty() {
            return Ok(PermissionRuntimeSurfaceUpdateOutcome::no_surface(
                transition,
            ));
        }

        let effect_frame_id = grant.effect_frame_id.ok_or_else(|| {
            ApplicationError::BadRequest(format!(
                "permission grant {} missing effect_frame_id; cannot apply capability effect",
                grant.id
            ))
        })?;
        let anchor_frame = self
            .frame_repo
            .get(effect_frame_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("effect frame not found: {effect_frame_id}"))
            })?;
        let current_frame = self
            .frame_repo
            .get_current(anchor_frame.agent_id)
            .await
            .map_err(ApplicationError::from)?
            .unwrap_or(anchor_frame);
        let mut next_state = project_capability_state_from_frame(&current_frame);
        apply_requested_paths(&mut next_state, &surface_paths, approve)?;

        push_toolset_effect(&mut transition, &next_state)?;
        let effect_frame = self
            .write_effect_frame(&current_frame, &next_state, created_by_kind, grant.id)
            .await?;
        let adoption_target = AgentFrameRuntimeTarget {
            frame_id: effect_frame.id,
            delivery_runtime_session_id: grant.source_runtime_session_id.clone(),
        };

        Ok(PermissionRuntimeSurfaceUpdateOutcome {
            transition,
            effect_frame: Some(effect_frame),
            adoption_target: Some(adoption_target),
        })
    }

    pub async fn adopt_update_outcome(
        &self,
        grant: &PermissionGrant,
        outcome: &PermissionRuntimeSurfaceUpdateOutcome,
    ) -> Result<(), ApplicationError> {
        let Some(target) = outcome.adoption_target.clone() else {
            return Ok(());
        };
        let Some(adopter) = self.adopter.as_ref() else {
            return Ok(());
        };
        adopter
            .adopt_permission_runtime_surface(target)
            .await
            .map_err(|error| {
                ApplicationError::Internal(format!(
                    "PermissionGrant active-runtime adoption failed for grant {}: {error}",
                    grant.id
                ))
            })
    }

    async fn write_effect_frame(
        &self,
        current_frame: &AgentFrame,
        next_state: &CapabilityState,
        created_by_kind: &str,
        grant_id: Uuid,
    ) -> Result<AgentFrame, ApplicationError> {
        let mut builder = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(next_state)
            .with_created_by(created_by_kind, Some(grant_id.to_string()));

        if let Some(context) = current_frame.context_slice_json.clone() {
            builder = builder.with_context(context);
        }
        if let Some(profile) = current_frame.execution_profile_json.clone() {
            builder = builder.with_execution_profile_raw(profile);
        }
        builder
            .build(self.frame_repo.as_ref())
            .await
            .map_err(ApplicationError::from)
    }
}

struct PermissionGrantRuntimeSurfaceUpdateAdapter {
    projector: PermissionRuntimeSurfaceUpdateService,
    grant: PermissionGrant,
    outcome: Arc<Mutex<Option<PermissionRuntimeSurfaceUpdateOutcome>>>,
}

#[async_trait]
impl AgentRunRuntimeSurfaceUpdateAdapter for PermissionGrantRuntimeSurfaceUpdateAdapter {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let outcome = self
            .projector
            .project_update_request(&self.grant, request)
            .await
            .map_err(|error| {
                AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected(error.to_string())
            })?;
        let mut command_outcome = AgentRunFrameSurfaceCommandOutcome::runtime_surface_update();
        command_outcome.runtime_session_id = Some(self.grant.source_runtime_session_id.clone());
        if let Some(frame) = outcome.effect_frame.as_ref() {
            command_outcome.frame_id = Some(frame.id);
            command_outcome.agent_id = Some(frame.agent_id);
            command_outcome.wrote_frame_revision = true;
        }
        *self.outcome.lock().await = Some(outcome);
        Ok(command_outcome)
    }
}

fn permission_frame_surface_error_to_application(
    error: AgentRunFrameSurfaceError,
) -> ApplicationError {
    match error {
        AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected(message) => {
            ApplicationError::BadRequest(message)
        }
        AgentRunFrameSurfaceError::ProjectionContextUnavailable(message) => {
            ApplicationError::Conflict(message)
        }
        AgentRunFrameSurfaceError::ConstructionRejected(message) => {
            ApplicationError::Internal(message)
        }
        AgentRunFrameSurfaceError::RoleMismatch { expected, actual } => ApplicationError::Internal(
            format!("permission runtime surface adapter returned {actual:?} for {expected:?}"),
        ),
    }
}

fn permission_request_parts(
    request: &RuntimeSurfaceUpdateRequest,
) -> Result<(Uuid, bool, &'static str), ApplicationError> {
    match request {
        RuntimeSurfaceUpdateRequest::PermissionGrantApplied { grant_id } => {
            Ok((*grant_id, true, "permission_grant_toolset_add"))
        }
        RuntimeSurfaceUpdateRequest::PermissionGrantRevoked { grant_id } => {
            Ok((*grant_id, false, "permission_grant_toolset_remove"))
        }
        _ => Err(ApplicationError::BadRequest(format!(
            "permission runtime surface adapter received non-permission request: {request:?}"
        ))),
    }
}

fn push_toolset_effect(
    transition: &mut RuntimeCapabilityTransition,
    state: &CapabilityState,
) -> Result<(), ApplicationError> {
    transition.effects.push(
        ToolCapabilityDimensionModule::set_tool_access_effect(SetToolAccessEffect {
            capabilities: state.tool.capabilities.clone(),
            enabled_clusters: state.tool.enabled_clusters.clone(),
            tool_policy: state.tool.tool_policy.clone(),
        })
        .map_err(ApplicationError::Internal)?,
    );
    Ok(())
}

fn apply_requested_paths(
    state: &mut CapabilityState,
    paths: &[ToolCapabilityPath],
    approve: bool,
) -> Result<(), ApplicationError> {
    for path in paths {
        if approve {
            apply_add_path(state, path);
        } else {
            apply_remove_path(state, path);
        }
    }
    recompute_tool_clusters(state);
    Ok(())
}

fn apply_add_path(state: &mut CapabilityState, path: &ToolCapabilityPath) {
    let capability = ToolCapability::new(path.capability.clone());
    let already_full = state.tool.capabilities.contains(&capability)
        && state
            .tool
            .tool_policy
            .get(&path.capability)
            .is_none_or(|filter| filter.include_only.is_empty());
    state.tool.capabilities.insert(capability);

    match &path.tool {
        Some(tool) if !already_full => {
            state
                .tool
                .tool_policy
                .entry(path.capability.clone())
                .or_default()
                .include_only
                .insert(tool.clone());
        }
        None => {
            if let Some(filter) = state.tool.tool_policy.get_mut(&path.capability) {
                filter.include_only.clear();
                filter.exclude.clear();
            }
            state
                .tool
                .tool_policy
                .retain(|_, filter| !filter.is_empty());
        }
        Some(_) => {}
    }
}

fn apply_remove_path(state: &mut CapabilityState, path: &ToolCapabilityPath) {
    match &path.tool {
        Some(tool) => {
            if let Some(filter) = state.tool.tool_policy.get_mut(&path.capability) {
                filter.include_only.remove(tool);
                if filter.include_only.is_empty() && filter.exclude.is_empty() {
                    state.tool.tool_policy.remove(&path.capability);
                    state
                        .tool
                        .capabilities
                        .remove(&ToolCapability::new(path.capability.clone()));
                }
            }
        }
        None => {
            state
                .tool
                .capabilities
                .remove(&ToolCapability::new(path.capability.clone()));
            state.tool.tool_policy.remove(&path.capability);
        }
    }
}

fn recompute_tool_clusters(state: &mut CapabilityState) {
    state.tool.enabled_clusters.clear();
    for capability in &state.tool.capabilities {
        state
            .tool
            .enabled_clusters
            .extend(capability_to_tool_clusters(capability));
    }
}
