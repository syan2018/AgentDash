use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use agentdash_agent_runtime::{
    BusinessAgentSurfaceFacts, ContributionMeta, ContributionRequirement, HookDefinition,
    HookHandler, HookMatcher, SurfaceSourceRef, ToolContribution, WorkspaceRequirement,
};
use agentdash_agent_runtime_contract::{
    ConfigurationBoundary, ContextProvenance, ContextRecipe, ContextRecipeRevision,
    DeliveryMechanism, RuntimeThreadId, SurfaceRevision, ToolChannel, ToolPresentationEmitter,
    ToolProtocolProjection, ToolSetRevision, WorkspaceCapability,
};
use agentdash_application_ports::{
    agent_run_runtime::AgentRunRuntimeProvisionRequest, agent_run_surface::AgentRunRuntimeSurface,
};
use agentdash_domain::{
    common::AgentConfig,
    workflow::{AgentFrame, AgentFrameRepository},
};
use agentdash_spi::{
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, DynAgentTool, ExecutionContext,
    ExecutionHookProvider, ExecutionSessionFrame, ExecutionTurnFrame, HookControlTarget,
    RuntimeAdapterProvenance, connector::RuntimeToolProvider,
};

use super::{AgentFrameSurfaceExt, BusinessFrameSurfaceQuery, RuntimeSurfaceQueryPurpose};

/// Application-owned source facts passed to Business Agent Surface compilation.
///
/// The adapter preserves product coordinates and typed AgentFrame-derived surfaces. It does not
/// construct driver DTOs or presentation JSON.
#[derive(Debug, Clone)]
pub struct AgentContextSurfaceSourceFacts {
    pub runtime: AgentRunRuntimeSurface,
    pub projection_identity: AgentContextProjectionIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextProjectionIdentity {
    pub operation_id: String,
    pub source_frame_id: String,
    pub source_frame_revision: u64,
    pub recorded_at_ms: i64,
}

impl AgentContextSurfaceSourceFacts {
    pub fn from_runtime_surface(
        runtime: AgentRunRuntimeSurface,
        operation_id: impl Into<String>,
    ) -> Result<Self, AgentContextSurfaceSourceError> {
        let revision = u64::try_from(runtime.surface_revision)
            .map_err(|_| AgentContextSurfaceSourceError::InvalidRevision)?;
        if revision == 0 {
            return Err(AgentContextSurfaceSourceError::InvalidRevision);
        }
        let projection_identity = AgentContextProjectionIdentity {
            operation_id: operation_id.into(),
            source_frame_id: runtime.current_surface_frame_id.to_string(),
            source_frame_revision: revision,
            recorded_at_ms: runtime.provenance.anchor_updated_at.timestamp_millis(),
        };
        Ok(Self {
            runtime,
            projection_identity,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AgentContextSurfaceSourceError {
    #[error("AgentFrame surface revision must be greater than zero")]
    InvalidRevision,
}

pub struct AgentBusinessSurfaceSource {
    surface_query: Arc<BusinessFrameSurfaceQuery>,
    frame_repository: Arc<dyn AgentFrameRepository>,
    runtime_tools: Arc<dyn RuntimeToolProvider>,
    hooks: Arc<dyn ExecutionHookProvider>,
}

pub struct LoadedAgentBusinessSurfaceFacts {
    pub context_source: AgentContextSurfaceSourceFacts,
    pub frame: AgentFrame,
    pub executor: AgentConfig,
    pub tools: Vec<DynAgentTool>,
    pub hook_snapshot: AgentFrameHookSnapshot,
    pub hook_provider: Arc<dyn ExecutionHookProvider>,
    pub business_facts: BusinessAgentSurfaceFacts,
}

impl AgentBusinessSurfaceSource {
    pub fn new(
        surface_query: Arc<BusinessFrameSurfaceQuery>,
        frame_repository: Arc<dyn AgentFrameRepository>,
        runtime_tools: Arc<dyn RuntimeToolProvider>,
        hooks: Arc<dyn ExecutionHookProvider>,
    ) -> Self {
        Self {
            surface_query,
            frame_repository,
            runtime_tools,
            hooks,
        }
    }

    pub async fn load(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        operation_id: String,
    ) -> Result<LoadedAgentBusinessSurfaceFacts, String> {
        let surface = self
            .surface_query
            .surface_for_provision_target(
                &request.target,
                thread_id,
                RuntimeSurfaceQueryPurpose::new("canonical_agent_runtime_surface"),
            )
            .await
            .map_err(|error| error.to_string())?;
        let frame = self
            .frame_repository
            .get_current(request.target.agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or("AgentRun has no current AgentFrame")?;
        if frame.id != surface.current_surface_frame_id {
            return Err(
                "surface query and AgentFrame repository returned different revisions".to_string(),
            );
        }
        let profile = frame
            .surface
            .as_ref()
            .and_then(|document| document.execution_profile.clone())
            .or_else(|| frame.execution_profile_json.clone())
            .ok_or("AgentFrame has no execution profile")?;
        let executor: AgentConfig =
            serde_json::from_value(profile).map_err(|error| error.to_string())?;
        let working_directory = surface
            .vfs
            .default_mount()
            .map(|mount| PathBuf::from(mount.root_ref.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or("AgentRun VFS has no usable default mount")?;
        let execution_context = ExecutionContext {
            session: ExecutionSessionFrame {
                turn_id: surface.active_turn_id.clone().unwrap_or_else(|| {
                    format!("surface-bootstrap-{}", surface.current_surface_frame_id)
                }),
                working_directory,
                environment_variables: Default::default(),
                executor_config: executor.clone(),
                mcp_servers: surface.mcp_servers.clone(),
                vfs: Some(surface.vfs.clone()),
                vfs_access_policy: Some(surface.vfs_access_policy.clone()),
                backend_execution: None,
                runtime_backend_anchor: surface.runtime_backend_anchor.clone(),
                identity: request.identity.clone().or(surface.identity.clone()),
            },
            turn: ExecutionTurnFrame {
                capability_state: surface.capability_state.clone(),
                ..Default::default()
            },
        };
        let tools = self
            .runtime_tools
            .build_tools(&execution_context)
            .await
            .map_err(|error| error.to_string())?;
        let hook_snapshot = self
            .hooks
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: request.target.run_id,
                    agent_id: request.target.agent_id,
                    frame_id: frame.id,
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    surface.runtime_session_id.clone(),
                    surface.active_turn_id.clone(),
                    "canonical_agent_runtime_surface",
                ),
            })
            .await
            .map_err(|error| error.to_string())?;
        let context_source =
            AgentContextSurfaceSourceFacts::from_runtime_surface(surface, operation_id)
                .map_err(|error| error.to_string())?;
        let revision = context_source.projection_identity.source_frame_revision;
        let tool_set_revision = ToolSetRevision(revision);
        let source = SurfaceSourceRef {
            layer: "agent_frame".to_string(),
            key: context_source.runtime.current_surface_frame_id.to_string(),
        };
        let mut tool_contributions = Vec::with_capacity(tools.len());
        for tool in &tools {
            let name = tool.name().trim().to_string();
            if name.is_empty()
                || tool_contributions
                    .iter()
                    .any(|item: &ToolContribution| item.runtime_name == name)
            {
                return Err(format!(
                    "assembled runtime tool name is empty or duplicated: {name}"
                ));
            }
            let capability_key =
                resolve_tool_capability(&context_source.runtime.capability_state, &name)?;
            let (protocol_projection, parity_fixture_id) =
                project_tool_protocol(tool.as_ref(), &name)?;
            tool_contributions.push(ToolContribution {
                meta: ContributionMeta {
                    key: format!("tool:{capability_key}:{name}"),
                    source: source.clone(),
                    priority: 0,
                    requirement: ContributionRequirement::Required,
                },
                runtime_name: name.clone(),
                description: tool.description().to_string(),
                parameters_schema: tool.parameters_schema(),
                capability_key: capability_key.clone(),
                tool_path: format!("{capability_key}::{name}"),
                allowed_channels: [ToolChannel::DirectCallback].into(),
                configuration_boundary: ConfigurationBoundary::Binding,
                protocol_projection,
                presentation_emitter: ToolPresentationEmitter::ToolBroker,
                parity_fixture_id,
            });
        }
        tool_contributions.sort_by(|left, right| left.runtime_name.cmp(&right.runtime_name));
        let hook_plan = frame.validated_hook_plan()?;
        let hooks = hook_plan
            .requirements
            .iter()
            .map(|entry| HookDefinition {
                meta: ContributionMeta {
                    key: format!("hook:{}", entry.definition_id),
                    source: source.clone(),
                    priority: 0,
                    requirement: if entry.requirement.required {
                        ContributionRequirement::Required
                    } else {
                        ContributionRequirement::Optional
                    },
                },
                definition_id: entry.definition_id.clone(),
                point: entry.requirement.point,
                actions: entry.requirement.actions.iter().copied().collect(),
                minimum_strength: entry.requirement.minimum_strength,
                failure_policy: entry.requirement.failure_policy,
                matcher: HookMatcher::Any,
                handler: HookHandler::Builtin {
                    key: entry.definition_id.as_str().to_string(),
                },
            })
            .collect();
        let business_facts = BusinessAgentSurfaceFacts {
            revision: SurfaceRevision(revision),
            context_recipe: ContextRecipe {
                revision: ContextRecipeRevision(revision),
                provenance: ContextProvenance {
                    settings_revision: agentdash_agent_runtime_contract::ThreadSettingsRevision(0),
                    tool_set_revision,
                },
                source_item_ids: Vec::new(),
            },
            tool_set_revision,
            hook_plan_revision: hook_plan.revision,
            workspace: WorkspaceRequirement {
                capabilities: workspace_capabilities(&context_source.runtime.vfs),
                minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
                requirement: ContributionRequirement::Required,
            },
            source,
            transition_phase_node: context_source.runtime.provenance.node_path.clone(),
            instructions: hook_snapshot
                .injections
                .iter()
                .map(|injection| injection.content.clone())
                .collect(),
            tools: tool_contributions,
            hooks,
            projection_identity: agentdash_agent_runtime::ContextProjectionIdentity {
                operation_id: context_source.projection_identity.operation_id.clone(),
                source_frame_id: context_source.projection_identity.source_frame_id.clone(),
                source_frame_revision: revision,
                recorded_at_ms: context_source.projection_identity.recorded_at_ms,
            },
        };
        Ok(LoadedAgentBusinessSurfaceFacts {
            context_source,
            frame,
            executor,
            tools,
            hook_snapshot,
            hook_provider: self.hooks.clone(),
            business_facts,
        })
    }
}

pub fn resolve_tool_capability(
    state: &agentdash_spi::CapabilityState,
    tool_name: &str,
) -> Result<String, String> {
    use agentdash_spi::platform::tool_capability::{ToolSource, platform_tool_descriptors};
    let descriptors = platform_tool_descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.name == tool_name)
        .collect::<Vec<_>>();
    let mut candidates = descriptors
        .iter()
        .filter(|descriptor| {
            let cluster = match &descriptor.source {
                ToolSource::Platform { cluster } => Some(*cluster),
                ToolSource::PlatformMcp { .. } | ToolSource::Mcp { .. } => None,
            };
            state.is_capability_tool_enabled(&descriptor.capability_key, tool_name, cluster)
        })
        .map(|descriptor| descriptor.capability_key.clone())
        .collect::<BTreeSet<_>>();
    candidates.extend(
        state
            .tool
            .tool_policy
            .iter()
            .filter(|(key, filter)| {
                filter.allows(tool_name)
                    && state
                        .tool
                        .capabilities
                        .contains(&agentdash_spi::ToolCapability::new((*key).clone()))
            })
            .map(|(key, _)| key.clone()),
    );
    if candidates.len() == 1 {
        return Ok(candidates
            .into_iter()
            .next()
            .expect("one capability candidate"));
    }
    if !descriptors.is_empty() && candidates.is_empty() {
        return Err(format!(
            "assembled tool `{tool_name}` is not enabled by current AgentFrame capability"
        ));
    }
    Err(format!(
        "assembled tool `{tool_name}` has no unambiguous AgentFrame capability identity"
    ))
}

pub fn project_tool_protocol(
    tool: &dyn agentdash_agent::AgentTool,
    name: &str,
) -> Result<(ToolProtocolProjection, String), String> {
    let projection = tool.protocol_projector().ok_or_else(|| {
        format!("assembled runtime tool `{name}` has no owner-declared protocol projector")
    })?;
    let fixture_id = tool
        .protocol_fixture_id()
        .filter(|fixture| !fixture.trim().is_empty())
        .ok_or_else(|| {
            format!("assembled runtime tool `{name}` has no owner-declared main parity fixture")
        })?
        .to_string();
    let projection = match projection {
        agentdash_agent::ToolProtocolProjector::Command => ToolProtocolProjection::Command,
        agentdash_agent::ToolProtocolProjector::FileChange => ToolProtocolProjection::FileChange,
        agentdash_agent::ToolProtocolProjector::FsRead => ToolProtocolProjection::FsRead,
        agentdash_agent::ToolProtocolProjector::FsGrep => ToolProtocolProjection::FsGrep,
        agentdash_agent::ToolProtocolProjector::FsGlob => ToolProtocolProjection::FsGlob,
        agentdash_agent::ToolProtocolProjector::Mcp { server_key } => {
            ToolProtocolProjection::Mcp { server_key }
        }
        agentdash_agent::ToolProtocolProjector::Dynamic { namespace } => {
            ToolProtocolProjection::Dynamic { namespace }
        }
    };
    Ok((projection, fixture_id))
}

fn workspace_capabilities(vfs: &agentdash_spi::Vfs) -> BTreeSet<WorkspaceCapability> {
    let mut values = BTreeSet::new();
    for mount in &vfs.mounts {
        for capability in &mount.capabilities {
            match capability {
                agentdash_domain::common::MountCapability::Read
                | agentdash_domain::common::MountCapability::List => {
                    values.insert(WorkspaceCapability::Read);
                }
                agentdash_domain::common::MountCapability::Search => {
                    values.insert(WorkspaceCapability::Search);
                }
                agentdash_domain::common::MountCapability::Write => {
                    values.insert(WorkspaceCapability::Write);
                }
                agentdash_domain::common::MountCapability::Exec
                | agentdash_domain::common::MountCapability::Watch => {}
            }
        }
    }
    if vfs.mounts.len() > 1 {
        values.insert(WorkspaceCapability::MultipleRoots);
    }
    values.insert(WorkspaceCapability::VirtualFileSystem);
    values
}
