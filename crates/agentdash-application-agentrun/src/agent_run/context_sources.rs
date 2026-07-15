use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use agentdash_agent_protocol::{
    RuntimeCompanionAgentEntry, RuntimeContextFragmentEntry, RuntimeMemoryDiagnosticEntry,
    RuntimeMemorySourceEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry,
    SkillContextExposure as ProtocolSkillContextExposure,
};
use agentdash_agent_runtime::{
    AssignmentContextFacts, AssignmentFragmentFacts, BootstrapContextFacts,
    BusinessAgentSurfaceFacts, ContributionMeta, ContributionRequirement, DiscoveredGuidelineFacts,
    EnvironmentContextFacts, GuidelinesContextFacts, HookDefinition, HookHandler, HookMatcher,
    IdentityContextFacts, MemoryContextFacts, MemoryDiagnosticFacts, MemorySourceFacts,
    NormalizedAssignmentContext, NormalizedContextSurfaceState, NormalizedMcpServerReadiness,
    NormalizedSkillCluster, NormalizedSurfaceEntity, SurfaceSourceRef, ToolContribution,
    UserContextFacts, WorkspaceRequirement,
};
use agentdash_agent_runtime_contract::{
    ConfigurationBoundary, ContextProvenance, ContextRecipe, ContextRecipeRevision,
    DeliveryMechanism, RuntimeThreadId, SurfaceRevision, ToolChannel, ToolPresentationEmitter,
    ToolProtocolProjection, ToolSetRevision, WorkspaceCapability,
};
use agentdash_application_ports::{
    agent_run_runtime::AgentRunRuntimeProvisionRequest, agent_run_surface::AgentRunRuntimeSurface,
};
use agentdash_application_vfs::VfsService;
use agentdash_domain::{
    common::AgentConfig,
    settings::{SettingScope, SettingsRepository},
    workflow::{AgentFrame, AgentFrameRepository},
};
use agentdash_spi::{
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, DynAgentTool, ExecutionContext,
    ExecutionHookProvider, ExecutionSessionFrame, ExecutionTurnFrame, HookControlTarget,
    MemoryDiscoveryProvider, RuntimeAdapterProvenance, RuntimeMcpSourceReadiness,
    SkillContextExposure, SkillDiscoveryProvider, connector::RuntimeToolProvider,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    AgentContextSourceSnapshot, AgentFrameSurfaceExt, BusinessFrameSurfaceQuery,
    LaunchContextDiscoveryInput, RuntimeSurfaceQueryPurpose, derive_launch_context_discovery,
};

const BASE_SYSTEM_PROMPT_SETTING_KEY: &str = "agent.pi.base_system_prompt";
const USER_PREFERENCES_SETTING_KEY: &str = "agent.pi.user_preferences";
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("default_system_prompt.md");

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
    context: AgentBusinessSurfaceContextDeps,
}

#[derive(Clone)]
pub struct AgentBusinessSurfaceContextDeps {
    pub vfs_service: Arc<VfsService>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub settings_repository: Arc<dyn SettingsRepository>,
    pub base_identity: BaseIdentitySource,
}

/// Application-owned startup identity resolved once from settings/environment/default assets.
#[derive(Debug, Clone)]
pub struct BaseIdentitySource {
    prompt: Arc<str>,
}

impl BaseIdentitySource {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: Arc::from(prompt.into()),
        }
    }

    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub async fn resolve(settings: &dyn SettingsRepository) -> Self {
        let setting = settings
            .get(&SettingScope::system(), BASE_SYSTEM_PROMPT_SETTING_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|setting| setting.value.as_str().map(ToString::to_string))
            .filter(|value| !value.is_empty());
        Self::new(
            setting
                .or_else(|| std::env::var("PI_AGENT_SYSTEM_PROMPT").ok())
                .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
        )
    }
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
        context: AgentBusinessSurfaceContextDeps,
    ) -> Self {
        Self {
            surface_query,
            frame_repository,
            runtime_tools,
            hooks,
            context,
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
        let effective_identity = request
            .identity
            .clone()
            .or_else(|| surface.identity.clone());
        let discovery = derive_launch_context_discovery(LaunchContextDiscoveryInput {
            vfs_service: self.context.vfs_service.as_ref(),
            launch_vfs: &surface.vfs,
            identity: effective_identity.as_ref(),
            extra_skill_dirs: &self.context.extra_skill_dirs,
            skill_discovery_providers: &self.context.skill_discovery_providers,
            memory_discovery_providers: &self.context.memory_discovery_providers,
            diagnostics_label: "business_agent_surface",
        })
        .await;
        let source_snapshot = frame.context_source_snapshot();
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
        let bootstrap_context = build_bootstrap_context_facts(
            &context_source,
            &executor,
            effective_identity.as_ref(),
            source_snapshot.as_ref(),
            &hook_snapshot,
            &discovery,
            self.context.settings_repository.as_ref(),
            &self.context.base_identity,
        )
        .await?;
        let normalized_context_surface = build_normalized_context_surface(
            &context_source,
            revision,
            &tool_contributions,
            &bootstrap_context,
            &discovery,
        )?;
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
            bootstrap_context,
            normalized_context_surface,
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

async fn build_bootstrap_context_facts(
    context_source: &AgentContextSurfaceSourceFacts,
    executor: &AgentConfig,
    identity: Option<&agentdash_spi::AuthIdentity>,
    source_snapshot: Option<&AgentContextSourceSnapshot>,
    hook_snapshot: &AgentFrameHookSnapshot,
    discovery: &super::LaunchContextDiscoveryOutput,
    settings: &dyn SettingsRepository,
    base_identity: &BaseIdentitySource,
) -> Result<BootstrapContextFacts, String> {
    let recorded_at =
        chrono::DateTime::from_timestamp_millis(context_source.projection_identity.recorded_at_ms)
            .ok_or("AgentFrame projection timestamp is outside chrono range")?;
    let assignment = assignment_context_facts(source_snapshot, hook_snapshot);
    let agent_identity_markdown = assignment
        .fragments
        .iter()
        .find(|fragment| fragment.slot == "agent_identity" && !fragment.content.trim().is_empty())
        .map(|fragment| fragment.content.clone())
        .or_else(|| {
            source_snapshot.and_then(|snapshot| {
                snapshot
                    .fragments
                    .iter()
                    .find(|fragment| {
                        fragment.slot == "agent_identity" && !fragment.content.trim().is_empty()
                    })
                    .map(|fragment| fragment.content.clone())
            })
        });
    let user_preferences = load_user_preferences(settings, identity).await;
    let memory = memory_context_facts(&discovery.discovered_memory);

    Ok(BootstrapContextFacts {
        // A compiled bootstrap plan is consumed only by the first canonical ThreadStart. Live
        // adoption uses the normalized previous/target state and never replays this plan.
        include_startup_context: true,
        identity: IdentityContextFacts {
            base_system_prompt: base_identity.prompt().to_string(),
            agent_identity_markdown,
            agent_system_prompt: executor.system_prompt.clone(),
        },
        user: identity.map(|identity| UserContextFacts {
            user_id: identity.user_id.clone(),
            display_name: identity.display_name.clone(),
            email: identity.email.clone(),
            groups: identity
                .groups
                .iter()
                .map(|group| {
                    group
                        .display_name
                        .as_deref()
                        .unwrap_or(&group.group_id)
                        .to_string()
                })
                .collect(),
            provider: identity.provider.clone(),
            extra: identity.extra.clone(),
        }),
        environment: EnvironmentContextFacts {
            date_utc: recorded_at.format("%Y-%m-%d").to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            model_id: executor.model_id.clone(),
            executor: executor.executor.clone(),
            working_directory: context_source
                .runtime
                .vfs
                .default_mount()
                .map(|mount| mount.root_ref.clone())
                .filter(|value| !value.is_empty()),
        },
        guidelines: GuidelinesContextFacts {
            user_preferences,
            discovered_guidelines: discovery
                .discovered_guidelines
                .iter()
                .map(|guideline| DiscoveredGuidelineFacts {
                    path: guideline.path.clone(),
                    content: guideline.content.clone(),
                })
                .collect(),
        },
        memory,
        assignment,
    })
}

async fn load_user_preferences(
    settings: &dyn SettingsRepository,
    identity: Option<&agentdash_spi::AuthIdentity>,
) -> Vec<String> {
    let Some(identity) = identity else {
        return Vec::new();
    };
    let Ok(Some(setting)) = settings
        .get(
            &SettingScope::user(identity.user_id.clone()),
            USER_PREFERENCES_SETTING_KEY,
        )
        .await
    else {
        return Vec::new();
    };
    setting
        .value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn assignment_context_facts(
    source_snapshot: Option<&AgentContextSourceSnapshot>,
    hook_snapshot: &AgentFrameHookSnapshot,
) -> AssignmentContextFacts {
    if let Some(snapshot) = source_snapshot {
        return AssignmentContextFacts {
            phase_tag: Some(snapshot.phase_tag.clone()),
            apply_mode: None,
            fragments: snapshot
                .fragments
                .iter()
                .map(|fragment| AssignmentFragmentFacts {
                    slot: fragment.slot.clone(),
                    label: fragment.label.clone(),
                    order: fragment.order,
                    runtime_agent_scope: fragment.runtime_agent_scope,
                    source: fragment.source.clone(),
                    content: fragment.content.clone(),
                    context_usage_kind: fragment.context_usage_kind.clone(),
                })
                .collect(),
        };
    }

    AssignmentContextFacts {
        phase_tag: Some("bootstrap".to_string()),
        apply_mode: None,
        fragments: hook_snapshot
            .injections
            .iter()
            .map(|injection| AssignmentFragmentFacts {
                slot: injection.slot.clone(),
                label: injection.source.clone(),
                order: match injection.slot.as_str() {
                    "workflow" => 83,
                    "constraint" => 84,
                    _ => 200,
                },
                runtime_agent_scope: true,
                source: injection.source.clone(),
                content: injection.content.clone(),
                context_usage_kind: agentdash_spi::ASSIGNMENT_CONTEXT_SLOTS
                    .contains(&injection.slot.as_str())
                    .then(|| agentdash_spi::context_usage_kind::SYSTEM_DEVELOPER.to_string()),
            })
            .collect(),
    }
}

fn memory_context_facts(inventory: &agentdash_spi::MemoryDiscoveryOutput) -> MemoryContextFacts {
    MemoryContextFacts {
        sources: inventory
            .clusters
            .iter()
            .flat_map(|cluster| cluster.sources.iter())
            .map(|source| MemorySourceFacts {
                provider_key: source.provider_key.clone(),
                source_key: source.source_key.clone(),
                display_name: source.display_name.clone(),
                source_uri: source.source_uri.clone(),
                index_uri: source.index_uri.clone(),
                mount_id: source.mount_id.clone(),
                scope: enum_name(source.scope),
                capabilities: source
                    .capabilities
                    .iter()
                    .map(|capability| format!("{capability:?}").to_ascii_lowercase())
                    .collect(),
                index_status: enum_name(source.index_status),
                trust_level: enum_name(source.trust_level),
                revision: memory_source_revision(source),
                summary: source.summary.clone(),
                bounded_index_content: source.bounded_index_content.clone(),
                context_usage_kind: Some(agentdash_spi::context_usage_kind::MEMORY.to_string()),
            })
            .collect(),
        diagnostics: inventory
            .diagnostics
            .iter()
            .map(|diagnostic| MemoryDiagnosticFacts {
                provider_key: diagnostic.provider_key.clone(),
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                source_key: diagnostic.source_key.clone(),
                uri: diagnostic.uri.clone(),
                context_usage_kind: Some(agentdash_spi::context_usage_kind::MEMORY.to_string()),
            })
            .collect(),
    }
}

fn build_normalized_context_surface(
    context_source: &AgentContextSurfaceSourceFacts,
    revision: u64,
    tools: &[ToolContribution],
    bootstrap: &BootstrapContextFacts,
    discovery: &super::LaunchContextDiscoveryOutput,
) -> Result<NormalizedContextSurfaceState, String> {
    let runtime = &context_source.runtime;
    let capability_state = &runtime.capability_state;
    let mcp_servers = runtime
        .mcp_servers
        .iter()
        .map(|server| {
            Ok((
                server.name.clone(),
                NormalizedSurfaceEntity {
                    fingerprint: fingerprint(server)?,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let unavailable_mcp_servers = runtime
        .mcp_servers
        .iter()
        .filter_map(|server| match &server.readiness {
            RuntimeMcpSourceReadiness::Unavailable {
                reason_code,
                message,
            } => Some(NormalizedMcpServerReadiness {
                name: server.name.clone(),
                reason_code: reason_code.clone(),
                message: message.clone(),
            }),
            RuntimeMcpSourceReadiness::Pending | RuntimeMcpSourceReadiness::Ready { .. } => None,
        })
        .collect();
    let companion_agents = capability_state
        .companion
        .agents
        .iter()
        .map(|agent| {
            (
                agent.name.clone(),
                RuntimeCompanionAgentEntry {
                    agent_key: agent.name.clone(),
                    executor: agent.executor.clone(),
                    display_name: agent.display_name.clone(),
                    context_usage_kind: Some("agents".to_string()),
                },
            )
        })
        .collect();
    let companion_agent_order = capability_state
        .companion
        .agents
        .iter()
        .map(|agent| agent.name.clone())
        .collect();
    let vfs_mounts = runtime
        .vfs
        .mounts
        .iter()
        .map(|mount| {
            Ok((
                mount.id.clone(),
                NormalizedSurfaceEntity {
                    fingerprint: fingerprint(mount)?,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let vfs_links = runtime
        .vfs
        .links
        .iter()
        .map(|link| {
            Ok((
                format!(
                    "{}:{}->{}:{}",
                    link.from_mount_id, link.from_path, link.to_mount_id, link.to_path
                ),
                NormalizedSurfaceEntity {
                    fingerprint: fingerprint(link)?,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let memory_sources = bootstrap
        .memory
        .sources
        .iter()
        .map(|source| {
            let key = format!("{}:{}", source.provider_key, source.source_key);
            (key, runtime_memory_source_entry(source))
        })
        .collect::<BTreeMap<_, _>>();
    let memory_source_order = bootstrap
        .memory
        .sources
        .iter()
        .map(|source| format!("{}:{}", source.provider_key, source.source_key))
        .collect();
    let memory_diagnostics = bootstrap
        .memory
        .diagnostics
        .iter()
        .map(runtime_memory_diagnostic_entry)
        .collect();
    let skills = discovery
        .session_capabilities
        .skills
        .iter()
        .map(|skill| {
            let key = skill.capability_key_or_name().to_string();
            (
                key,
                RuntimeSkillEntry {
                    name: skill.name.clone(),
                    capability_key: skill.capability_key.clone(),
                    provider_key: skill.provider_key.clone(),
                    local_name: skill.local_name.clone(),
                    display_name: skill.display_name.clone(),
                    description: skill.description.clone(),
                    file_path: skill.file_path.clone(),
                    base_dir: skill.base_dir.clone(),
                    exposure: match skill.exposure {
                        SkillContextExposure::DefaultExposed => {
                            ProtocolSkillContextExposure::DefaultExposed
                        }
                        SkillContextExposure::ExplicitOnly => {
                            ProtocolSkillContextExposure::ExplicitOnly
                        }
                    },
                    disable_model_invocation: skill.disable_model_invocation,
                    context_usage_kind: Some("skills".to_string()),
                },
            )
        })
        .collect();
    let skill_clusters = discovery
        .session_capabilities
        .skill_clusters
        .iter()
        .map(|cluster| NormalizedSkillCluster {
            provider_key: cluster.provider_key.clone(),
            display_name: cluster.display_name.clone(),
            model_summary: cluster.model_summary.clone(),
        })
        .collect();
    let tool_schemas = tools
        .iter()
        .map(|tool| {
            (
                tool.tool_path.clone(),
                RuntimeToolSchemaEntry {
                    name: tool.runtime_name.clone(),
                    description: tool.description.clone(),
                    parameters_schema: tool.parameters_schema.clone(),
                    capability_key: Some(tool.capability_key.clone()),
                    source: Some(tool.meta.source.key.clone()),
                    tool_path: Some(tool.tool_path.clone()),
                    context_usage_kind: Some("system_tools".to_string()),
                },
            )
        })
        .collect();
    let assignment_fragments = normalized_assignment_fragments(&bootstrap.assignment);
    let assignment = (!assignment_fragments.is_empty()).then_some(NormalizedAssignmentContext {
        revision,
        fragments: assignment_fragments,
    });

    Ok(NormalizedContextSurfaceState {
        capability_keys: capability_state.capability_keys(),
        excluded_tool_paths: capability_state.excluded_tool_paths(),
        included_tool_paths: capability_state.included_tool_paths(),
        mcp_servers,
        unavailable_mcp_servers,
        companion_agents,
        companion_agent_order,
        vfs_mounts,
        vfs_links,
        default_vfs_mount: runtime.vfs.default_mount_id.clone(),
        memory_sources,
        memory_source_order,
        memory_diagnostics,
        skills,
        skill_clusters,
        tool_schemas,
        assignment,
    })
}

fn normalized_assignment_fragments(
    assignment: &AssignmentContextFacts,
) -> Vec<RuntimeContextFragmentEntry> {
    let mut fragments = assignment
        .fragments
        .iter()
        .filter(|fragment| fragment.runtime_agent_scope)
        .filter(|fragment| {
            agentdash_spi::ASSIGNMENT_CONTEXT_SLOTS.contains(&fragment.slot.as_str())
        })
        .filter(|fragment| !fragment.content.trim().is_empty())
        .collect::<Vec<_>>();
    fragments.sort_by_key(|fragment| fragment.order);
    fragments
        .into_iter()
        .map(|fragment| RuntimeContextFragmentEntry {
            slot: fragment.slot.clone(),
            label: fragment.label.clone(),
            source: fragment.source.clone(),
            content: fragment.content.clone(),
            context_usage_kind: fragment.context_usage_kind.clone(),
        })
        .collect()
}

fn runtime_memory_source_entry(source: &MemorySourceFacts) -> RuntimeMemorySourceEntry {
    RuntimeMemorySourceEntry {
        provider_key: source.provider_key.clone(),
        source_key: source.source_key.clone(),
        display_name: source.display_name.clone(),
        source_uri: source.source_uri.clone(),
        index_uri: source.index_uri.clone(),
        mount_id: source.mount_id.clone(),
        scope: source.scope.clone(),
        index_status: source.index_status.clone(),
        trust_level: source.trust_level.clone(),
        revision: source.revision.clone(),
        summary: source.summary.clone(),
        context_usage_kind: source.context_usage_kind.clone(),
    }
}

fn runtime_memory_diagnostic_entry(
    diagnostic: &MemoryDiagnosticFacts,
) -> RuntimeMemoryDiagnosticEntry {
    RuntimeMemoryDiagnosticEntry {
        provider_key: diagnostic.provider_key.clone(),
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        source_key: diagnostic.source_key.clone(),
        uri: diagnostic.uri.clone(),
        context_usage_kind: diagnostic.context_usage_kind.clone(),
    }
}

fn memory_source_revision(source: &agentdash_spi::DiscoveredMemorySource) -> String {
    let payload = serde_json::to_string(source).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}",
            source.provider_key, source.source_key, source.index_uri, source.index_status as u8
        )
    });
    let mut hash = 0xcbf29ce484222325u64;
    for byte in payload.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn fingerprint(value: &impl Serialize) -> Result<String, String> {
    let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
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
