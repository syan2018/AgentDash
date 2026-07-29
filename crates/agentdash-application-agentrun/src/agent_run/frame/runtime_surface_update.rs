use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization::{
    AgentFrameWriteRole, AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
    AgentRunRuntimeSurfaceUpdatePort, RuntimeSurfaceChange, RuntimeSurfaceUpdateRequest,
};
use agentdash_domain::common::Vfs;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_platform_spi::{
    CapabilityState, RuntimeMcpServer, WorkspaceModuleVisibilityMode, compute_mcp_surface_delta,
    compute_vfs_surface_delta,
};
use async_trait::async_trait;

use crate::agent_run::frame::{AgentFrameBuilder, FrameSurfaceDraft};
use crate::agent_run::runtime_capability::compose_vfs_with_overlay_and_directives;
use crate::agent_run::{
    AgentRunProductRuntimeBindingStore, AgentRunProductRuntimeSurfaceRebindPort,
    AgentRunProductRuntimeSurfaceRebindRequest, ProductAgentFrameRef, ProductAgentSurfaceFacts,
};

#[derive(Clone)]
pub struct ProductAgentRunRuntimeSurfaceUpdater {
    frames: Arc<dyn AgentFrameRepository>,
    bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
    rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
}

impl ProductAgentRunRuntimeSurfaceUpdater {
    pub fn new(
        frames: Arc<dyn AgentFrameRepository>,
        bindings: Arc<dyn AgentRunProductRuntimeBindingStore>,
        rebind: Arc<dyn AgentRunProductRuntimeSurfaceRebindPort>,
    ) -> Self {
        Self {
            frames,
            bindings,
            rebind,
        }
    }
}

#[async_trait]
impl AgentRunRuntimeSurfaceUpdatePort for ProductAgentRunRuntimeSurfaceUpdater {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        validate_request(&request)?;
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(rejected)?
            .ok_or_else(|| rejected("AgentRun Product runtime binding 不存在"))?;
        let current = self
            .frames
            .get(binding.launch_frame.frame_id)
            .await
            .map_err(|error| rejected(error.to_string()))?
            .ok_or_else(|| rejected("Product binding 指向的 AgentFrame 不存在"))?;
        if current.agent_id != request.target.agent_id
            || u64::try_from(current.revision).ok() != Some(binding.launch_frame.revision)
        {
            return Err(rejected(
                "Product binding 指向的 AgentFrame identity/revision 不一致",
            ));
        }

        let mut draft = FrameSurfaceDraft::from_frame(&current);
        let mut capability = draft
            .capability_state
            .take()
            .ok_or_else(|| rejected("AgentFrame 缺少 canonical CapabilityState"))?;
        let mut vfs = draft
            .vfs
            .take()
            .ok_or_else(|| rejected("AgentFrame 缺少 canonical VFS"))?;
        let mut mcp_servers = std::mem::take(&mut draft.mcp_servers);
        let mut execution_profile = draft.execution_profile.take();
        let previous_capability = capability.clone();
        let previous_vfs = vfs.clone();
        let previous_mcp_servers = mcp_servers.clone();
        let previous_execution_profile = execution_profile.clone();

        (capability, vfs, mcp_servers, execution_profile) = apply_runtime_surface_changes(
            capability,
            vfs,
            mcp_servers,
            execution_profile,
            request.changes,
        )?;

        let mut outcome =
            AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::RuntimeSurfaceUpdate);
        outcome.agent_id = Some(request.target.agent_id);
        outcome.runtime_thread_id = Some(binding.runtime_thread_id.to_string());
        let vfs_delta = compute_vfs_surface_delta(Some(&previous_vfs), Some(&vfs));
        let mcp_delta = compute_mcp_surface_delta(&previous_mcp_servers, &mcp_servers);
        if capability == previous_capability
            && vfs_delta.is_empty()
            && mcp_delta.is_empty()
            && execution_profile == previous_execution_profile
        {
            outcome.frame_id = Some(current.id);
            outcome.frame_revision = u64::try_from(current.revision).ok();
            return Ok(outcome);
        }

        draft.capability_state = Some(capability);
        draft.vfs = Some(vfs);
        draft.mcp_servers = mcp_servers;
        draft.execution_profile = execution_profile;
        let mut builder = AgentFrameBuilder::new(request.target.agent_id)
            .with_surface_draft(&draft)
            .with_created_by(request.created_by_kind, request.created_by_id);
        if let Some(hook_plan) = current.surface.hook_plan.clone() {
            builder = builder.with_hook_plan_raw(hook_plan);
        }
        let next = builder
            .build_uncommitted(self.frames.as_ref())
            .await
            .map_err(|error| rejected(error.to_string()))?;
        self.frames
            .create(&next)
            .await
            .map_err(|error| rejected(error.to_string()))?;
        let next_ref = ProductAgentFrameRef {
            frame_id: next.id,
            agent_id: next.agent_id,
            revision: u64::try_from(next.revision)
                .map_err(|_| rejected("AgentFrame revision 无法投影为 Product revision"))?,
        };
        let evidence = self
            .rebind
            .prepare_runtime_surface_rebind(AgentRunProductRuntimeSurfaceRebindRequest {
                target: request.target,
                runtime_thread_id: binding.runtime_thread_id.clone(),
                idempotency_key: request.idempotency_key,
                frame: next_ref.clone(),
                execution_profile: binding.execution_profile.clone(),
                surface_facts: ProductAgentSurfaceFacts::from_frame(&next),
            })
            .await
            .map_err(|error| rejected(error.to_string()))?;
        let previous_digest = binding.calculated_digest().map_err(rejected)?;
        let mut next_binding = binding;
        next_binding.launch_frame = next_ref;
        self.bindings
            .replace_product_binding(&previous_digest, &next_binding)
            .await
            .map_err(rejected)?;

        outcome.frame_id = Some(next.id);
        outcome.frame_revision = u64::try_from(next.revision).ok();
        outcome.applied_generation = Some(evidence.prepared_generation);
        outcome.wrote_frame_revision = true;
        outcome.adopted_active_runtime = true;
        outcome.diagnostics.push(format!(
            "vfs mounts +{} -{} ~{}; links +{} -{} ~{}; default_changed={}; mcp +{} -{} ~{}; execution_profile_changed={}",
            vfs_delta.mounts.added.len(),
            vfs_delta.mounts.removed.len(),
            vfs_delta.mounts.changed.len(),
            vfs_delta.links.added.len(),
            vfs_delta.links.removed.len(),
            vfs_delta.links.changed.len(),
            vfs_delta.default_mount.changed,
            mcp_delta.servers.added.len(),
            mcp_delta.servers.removed.len(),
            mcp_delta.servers.changed.len(),
            draft.execution_profile != previous_execution_profile,
        ));
        Ok(outcome)
    }
}

fn apply_runtime_surface_changes(
    mut capability: CapabilityState,
    mut vfs: Vfs,
    mut mcp_servers: Vec<RuntimeMcpServer>,
    mut execution_profile: Option<agentdash_domain::common::AgentConfig>,
    changes: Vec<RuntimeSurfaceChange>,
) -> Result<
    (
        CapabilityState,
        Vfs,
        Vec<RuntimeMcpServer>,
        Option<agentdash_domain::common::AgentConfig>,
    ),
    AgentRunFrameSurfaceError,
> {
    for change in changes {
        match change {
            RuntimeSurfaceChange::ReplaceCapabilityState { state } => {
                capability = state;
            }
            RuntimeSurfaceChange::ReplaceVfsSurface { vfs: next } => {
                vfs = next;
            }
            RuntimeSurfaceChange::ReplaceMcpSurface { servers } => {
                mcp_servers = servers;
            }
            RuntimeSurfaceChange::ReplaceExecutionProfile { profile } => {
                execution_profile = Some(profile);
            }
            RuntimeSurfaceChange::ApplyVfsDirectives { directives } => {
                if directives.is_empty() {
                    return Err(rejected("VFS directives 不能为空"));
                }
                vfs = compose_vfs_with_overlay_and_directives(
                    Some(&vfs),
                    &Default::default(),
                    &directives,
                );
            }
            RuntimeSurfaceChange::AllowWorkspaceModule { module_ref } => {
                let module_ref = module_ref.trim();
                if module_ref.is_empty() {
                    return Err(rejected("Workspace Module ref 不能为空"));
                }
                if capability.workspace_module.mode == WorkspaceModuleVisibilityMode::Allowlist {
                    capability
                        .workspace_module
                        .allowed_module_ids
                        .push(module_ref.to_owned());
                    capability.workspace_module.allowed_module_ids.sort();
                    capability.workspace_module.allowed_module_ids.dedup();
                }
            }
        }
    }
    Ok((capability, vfs, mcp_servers, execution_profile))
}

fn validate_request(
    request: &RuntimeSurfaceUpdateRequest,
) -> Result<(), AgentRunFrameSurfaceError> {
    if request.target.run_id.is_nil()
        || request.target.agent_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.created_by_kind.trim().is_empty()
        || request.changes.is_empty()
    {
        return Err(rejected(
            "runtime surface update 的 target、idempotency、provenance 与 changes 必须有效",
        ));
    }
    Ok(())
}

fn rejected(message: impl Into<String>) -> AgentRunFrameSurfaceError {
    AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::mcp_preset::McpTransportConfig;
    use agentdash_domain::workflow::MountDirective;

    use super::*;

    fn canvas_mount() -> Mount {
        Mount {
            id: "cvs-canvas".to_owned(),
            provider: "canvas_fs".to_owned(),
            backend_id: "definition".to_owned(),
            root_ref: "canvas-root://definition".to_owned(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: "Canvas".to_owned(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn reducer_applies_workspace_visibility_and_vfs_directive_idempotently() {
        let changes = vec![
            RuntimeSurfaceChange::AllowWorkspaceModule {
                module_ref: "canvas:definition".to_owned(),
            },
            RuntimeSurfaceChange::ApplyVfsDirectives {
                directives: vec![MountDirective::AddMount {
                    mount: canvas_mount(),
                }],
            },
        ];

        let (capability, vfs) = apply_runtime_surface_changes(
            CapabilityState::default(),
            Vfs::default(),
            Vec::new(),
            None,
            changes.clone(),
        )
        .map(|(capability, vfs, _, _)| (capability, vfs))
        .expect("first");
        let (capability, vfs) =
            apply_runtime_surface_changes(capability, vfs, Vec::new(), None, changes)
                .map(|(capability, vfs, _, _)| (capability, vfs))
                .expect("second");

        assert!(capability.workspace_module.allows("canvas:definition"));
        assert_eq!(
            capability
                .workspace_module
                .allowed_module_ids
                .iter()
                .filter(|module_ref| *module_ref == "canvas:definition")
                .count(),
            1
        );
        assert_eq!(vfs.mounts, vec![canvas_mount()]);
    }

    #[test]
    fn reducer_replaces_independent_mcp_and_execution_surfaces() {
        let server = RuntimeMcpServer::new(
            "docs".to_owned(),
            McpTransportConfig::Http {
                url: "https://example.test/mcp".to_owned(),
                headers: Vec::new(),
            },
            false,
        );
        let profile = agentdash_domain::common::AgentConfig::new("PI_AGENT");

        let (_, _, servers, execution_profile) = apply_runtime_surface_changes(
            CapabilityState::default(),
            Vfs::default(),
            Vec::new(),
            None,
            vec![
                RuntimeSurfaceChange::ReplaceMcpSurface {
                    servers: vec![server.clone()],
                },
                RuntimeSurfaceChange::ReplaceExecutionProfile {
                    profile: profile.clone(),
                },
            ],
        )
        .expect("replace runtime surfaces");

        assert_eq!(servers, vec![server]);
        assert_eq!(execution_profile, Some(profile));
    }
}
