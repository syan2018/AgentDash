use std::path::PathBuf;

use agentdash_domain::common::AgentConfig;
use agentdash_domain::task::AgentBinding;
use agentdash_spi::{
    AuthIdentity, CapabilityState, SessionBaselineCapabilities, SessionContextBundle,
    SessionMcpServer, Vfs,
};
use uuid::Uuid;

use super::context::SessionContextSnapshot;
use super::ownership::ResolvedSessionOwner;
use crate::vfs::ResolvedVfsSurface;

#[derive(Debug, Clone)]
pub struct SessionConstructionPlan {
    pub session_id: String,
    pub owner: ResolvedSessionOwner,
    pub session: SessionIdentityPlan,
    pub source: SourceContractPlan,
    pub workspace: WorkspacePlan,
    pub execution_profile: ExecutionProfilePlan,
    pub surface: SessionSurfacePlan,
    pub context: ContextPlan,
    pub identity: IdentityPlan,
    pub projections: ConstructionProjections,
    pub context_projection: SessionConstructionContextProjection,
    pub trace: SessionConstructionTrace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionIdentityPlan {
    pub session_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceContractPlan {
    pub launch_source: Option<String>,
    pub preparation: Option<String>,
    pub strictness: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspacePlan {
    pub workspace_id: Option<Uuid>,
    pub working_dir_input: Option<String>,
    pub working_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionProfilePlan {
    pub executor_config: Option<AgentConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionSurfacePlan {
    pub vfs: Option<Vfs>,
    pub runtime_surface: Option<ResolvedVfsSurface>,
}

#[derive(Debug, Clone, Default)]
pub struct ContextPlan {
    pub bundle: Option<SessionContextBundle>,
    pub bundle_id: Option<Uuid>,
    pub context_snapshot: Option<SessionContextSnapshot>,
    pub bootstrap_fragment_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct IdentityPlan {
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructionProjections {
    pub context: SessionConstructionContextProjection,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub capability_state: Option<CapabilityState>,
    pub session_capabilities: Option<SessionBaselineCapabilities>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionConstructionContextProjection {
    pub workspace_id: Option<Uuid>,
    pub agent_binding: Option<AgentBinding>,
    pub vfs: Option<Vfs>,
    pub runtime_surface: Option<ResolvedVfsSurface>,
    pub context_snapshot: Option<SessionContextSnapshot>,
    pub session_capabilities: Option<SessionBaselineCapabilities>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionConstructionTrace {
    pub entries: Vec<SessionConstructionTraceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConstructionTraceEntry {
    pub stage: &'static str,
    pub source: String,
}

pub struct SessionConstructionLaunchInput {
    pub session_id: String,
    pub owner: ResolvedSessionOwner,
    pub source: SourceContractPlan,
    pub workspace_id: Option<Uuid>,
    pub working_dir_input: Option<String>,
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub vfs: Option<Vfs>,
    pub runtime_surface: Option<ResolvedVfsSurface>,
    pub context_bundle: Option<SessionContextBundle>,
    pub context_snapshot: Option<SessionContextSnapshot>,
    pub identity: Option<AuthIdentity>,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub capability_state: CapabilityState,
    pub session_capabilities: Option<SessionBaselineCapabilities>,
    pub trace_entries: Vec<SessionConstructionTraceEntry>,
}

impl SessionConstructionPlan {
    pub fn new(
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        context_projection: SessionConstructionContextProjection,
    ) -> Self {
        let session_id = session_id.into();
        let trace = SessionConstructionTrace {
            entries: vec![
                SessionConstructionTraceEntry {
                    stage: "owner",
                    source: owner.trace.selected_reason.clone(),
                },
                SessionConstructionTraceEntry {
                    stage: "context_projection",
                    source: match owner.owner_type {
                        agentdash_domain::session_binding::SessionOwnerType::Task => {
                            "task.context_builder".to_string()
                        }
                        agentdash_domain::session_binding::SessionOwnerType::Story => {
                            "story.context_builder".to_string()
                        }
                        agentdash_domain::session_binding::SessionOwnerType::Project => {
                            "project.context_builder".to_string()
                        }
                    },
                },
            ],
        };
        let projections = ConstructionProjections {
            context: context_projection.clone(),
            session_capabilities: context_projection.session_capabilities.clone(),
            ..Default::default()
        };
        Self {
            session_id: session_id.clone(),
            owner,
            session: SessionIdentityPlan { session_id },
            source: SourceContractPlan::default(),
            workspace: WorkspacePlan {
                workspace_id: context_projection.workspace_id,
                ..Default::default()
            },
            execution_profile: ExecutionProfilePlan::default(),
            surface: SessionSurfacePlan {
                vfs: context_projection.vfs.clone(),
                runtime_surface: context_projection.runtime_surface.clone(),
            },
            context: ContextPlan {
                bundle: None,
                context_snapshot: context_projection.context_snapshot.clone(),
                ..Default::default()
            },
            identity: IdentityPlan::default(),
            projections,
            context_projection,
            trace,
        }
    }

    pub fn from_launch(input: SessionConstructionLaunchInput) -> Self {
        let context_projection = SessionConstructionContextProjection {
            workspace_id: input.workspace_id,
            agent_binding: None,
            vfs: input.vfs.clone(),
            runtime_surface: input.runtime_surface.clone(),
            context_snapshot: input.context_snapshot.clone(),
            session_capabilities: input.session_capabilities.clone(),
        };
        let bootstrap_fragment_count = input
            .context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.len())
            .unwrap_or_default();
        let mut trace_entries = vec![
            SessionConstructionTraceEntry {
                stage: "owner",
                source: input.owner.trace.selected_reason.clone(),
            },
            SessionConstructionTraceEntry {
                stage: "source",
                source: input
                    .source
                    .launch_source
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            },
        ];
        trace_entries.extend(input.trace_entries);
        Self {
            session_id: input.session_id.clone(),
            owner: input.owner,
            session: SessionIdentityPlan {
                session_id: input.session_id,
            },
            source: input.source,
            workspace: WorkspacePlan {
                workspace_id: input.workspace_id,
                working_dir_input: input.working_dir_input,
                working_directory: Some(input.working_directory),
            },
            execution_profile: ExecutionProfilePlan {
                executor_config: Some(input.executor_config),
            },
            surface: SessionSurfacePlan {
                vfs: input.vfs.clone(),
                runtime_surface: input.runtime_surface,
            },
            context: ContextPlan {
                bundle: input.context_bundle.clone(),
                bundle_id: input.context_bundle.as_ref().map(|bundle| bundle.bundle_id),
                context_snapshot: input.context_snapshot,
                bootstrap_fragment_count,
            },
            identity: IdentityPlan {
                identity: input.identity,
            },
            projections: ConstructionProjections {
                context: context_projection.clone(),
                mcp_servers: input.mcp_servers,
                capability_state: Some(input.capability_state),
                session_capabilities: input.session_capabilities,
            },
            context_projection,
            trace: SessionConstructionTrace {
                entries: trace_entries,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

    use super::*;
    use crate::session::ownership::SessionOwnerResolver;

    #[test]
    fn construction_plan_carries_owner_and_projection_trace() {
        let binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-construction".to_string(),
            SessionOwnerType::Task,
            Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let projection = SessionConstructionContextProjection {
            workspace_id: Some(Uuid::new_v4()),
            ..Default::default()
        };

        let plan = SessionConstructionPlan::new("sess-construction", owner, projection);

        assert_eq!(plan.session_id, "sess-construction");
        assert_eq!(plan.owner.owner_type, SessionOwnerType::Task);
        assert!(plan.context_projection.workspace_id.is_some());
        assert_eq!(plan.trace.entries[0].stage, "owner");
        assert_eq!(plan.trace.entries[0].source, "priority[0]=task");
        assert_eq!(plan.trace.entries[1].source, "task.context_builder");
    }

    #[test]
    fn launch_construction_plan_keeps_full_context_bundle() {
        let binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-launch-construction".to_string(),
            SessionOwnerType::Project,
            Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let bundle = SessionContextBundle::new(Uuid::new_v4(), "owner_bootstrap");
        let bundle_id = bundle.bundle_id;

        let plan = SessionConstructionPlan::from_launch(SessionConstructionLaunchInput {
            session_id: "sess-launch-construction".to_string(),
            owner,
            source: SourceContractPlan {
                launch_source: Some("http_prompt".to_string()),
                preparation: None,
                strictness: Some("strict".to_string()),
            },
            workspace_id: None,
            working_dir_input: Some("workspace".to_string()),
            working_directory: PathBuf::from("/workspace"),
            executor_config: AgentConfig::new("PI_AGENT"),
            vfs: None,
            runtime_surface: None,
            context_bundle: Some(bundle),
            context_snapshot: None,
            identity: None,
            mcp_servers: Vec::new(),
            capability_state: CapabilityState::default(),
            session_capabilities: Some(SessionBaselineCapabilities::default()),
            trace_entries: Vec::new(),
        });

        assert_eq!(
            plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id),
            Some(bundle_id)
        );
        assert_eq!(plan.context.bundle_id, Some(bundle_id));
    }
}
