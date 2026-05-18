use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::common::AgentConfig;
use agentdash_domain::shared_library::{
    ExtensionCommandHandler, ExtensionFlagType, ExtensionRendererDeclaration, InstalledAssetSource,
};
use agentdash_domain::task::AgentBinding;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AuthIdentity, CapabilityState, DiscoveredGuideline, SessionBaselineCapabilities,
    SessionContextBundle, SessionMcpServer, Vfs,
};
use uuid::Uuid;

use super::context::SessionContextSnapshot;
use super::ownership::ResolvedSessionOwner;
use super::post_turn_handler::TerminalHookEffectBinding;
use super::types::UserPromptInput;
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
    pub prompt: ConstructionPromptPlan,
    pub identity: IdentityPlan,
    pub effects: ConstructionEffectPlan,
    pub projections: ConstructionProjections,
    pub resolution: ConstructionResolutionPlan,
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
    pub continuation_context_frame: Option<ContextFrame>,
    pub context_snapshot: Option<SessionContextSnapshot>,
    pub bootstrap_fragment_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructionPromptPlan {
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub environment_variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct IdentityPlan {
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructionEffectPlan {
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructionProjections {
    pub context: SessionConstructionContextProjection,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub capability_state: Option<CapabilityState>,
    pub session_capabilities: Option<SessionBaselineCapabilities>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub extension_runtime: Option<ExtensionRuntimeProjection>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtensionRuntimeProjection {
    pub installations: Vec<ExtensionInstallationProjection>,
    pub commands: Vec<ExtensionCommandProjection>,
    pub flags: Vec<ExtensionFlagProjection>,
    pub message_renderers: Vec<ExtensionMessageRendererProjection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionInstallationProjection {
    pub installation_id: Uuid,
    pub extension_key: String,
    pub extension_id: String,
    pub display_name: String,
    pub installed_source: InstalledAssetSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCommandProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub description: String,
    pub handler: ExtensionCommandHandler,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionFlagProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub flag_type: ExtensionFlagType,
    pub default: serde_json::Value,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionMessageRendererProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub custom_type: String,
    pub renderer: ExtensionRendererDeclaration,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructionResolutionPlan {
    pub vfs_source: Option<String>,
    pub mcp_source: Option<String>,
    pub capability_source: Option<String>,
    pub executor_source: Option<String>,
    pub working_directory_source: Option<String>,
    pub pending_overlay_applied: bool,
    pub runtime_base_capability_state: Option<CapabilityState>,
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
            prompt: ConstructionPromptPlan::default(),
            identity: IdentityPlan::default(),
            effects: ConstructionEffectPlan::default(),
            projections,
            resolution: ConstructionResolutionPlan::default(),
            context_projection,
            trace,
        }
    }

    pub fn from_source_input(
        session_id: impl Into<String>,
        owner: ResolvedSessionOwner,
        user_input: &UserPromptInput,
    ) -> Self {
        let mut plan = Self::new(
            session_id,
            owner,
            SessionConstructionContextProjection::default(),
        );
        plan.prompt.prompt_blocks = user_input.prompt_blocks.clone();
        plan.prompt.environment_variables = user_input.env.clone();
        plan.execution_profile.executor_config = user_input.executor_config.clone();
        plan
    }

    pub fn validate_for_launch(&self) -> Result<(), String> {
        if self.workspace.working_directory.is_none() {
            return Err(
                "SessionConstructionPlan.workspace.working_directory 必须在 launch 前解析"
                    .to_string(),
            );
        }
        if self.execution_profile.executor_config.is_none() {
            return Err(
                "SessionConstructionPlan.execution_profile.executor_config 必须在 launch 前解析"
                    .to_string(),
            );
        }
        let Some(vfs) = self.surface.vfs.as_ref() else {
            return Err("SessionConstructionPlan.surface.vfs 必须在 launch 前解析".to_string());
        };
        let Some(capability_state) = self.projections.capability_state.as_ref() else {
            return Err(
                "SessionConstructionPlan.projections.capability_state 必须在 launch 前解析"
                    .to_string(),
            );
        };
        if capability_state.vfs.active.as_ref() != Some(vfs) {
            return Err(
                "SessionConstructionPlan capability_state.vfs.active 必须等于 surface.vfs"
                    .to_string(),
            );
        }
        if capability_state.tool.mcp_servers != self.projections.mcp_servers {
            return Err(
                "SessionConstructionPlan capability_state.tool.mcp_servers 必须等于 projections.mcp_servers"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
    use agentdash_spi::Vfs;

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

        let mut plan = SessionConstructionPlan::new(
            "sess-launch-construction",
            owner,
            SessionConstructionContextProjection::default(),
        );
        plan.context.bundle = Some(bundle);
        plan.context.bundle_id = plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id);

        assert_eq!(
            plan.context.bundle.as_ref().map(|bundle| bundle.bundle_id),
            Some(bundle_id)
        );
        assert_eq!(plan.context.bundle_id, Some(bundle_id));
    }

    #[test]
    fn validate_for_launch_requires_final_execution_facts() {
        let binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-invalid-construction".to_string(),
            SessionOwnerType::Project,
            Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let plan = SessionConstructionPlan::new(
            "sess-invalid-construction",
            owner,
            SessionConstructionContextProjection::default(),
        );

        assert!(
            plan.validate_for_launch()
                .expect_err("partial construction must be rejected")
                .contains("working_directory")
        );
    }

    #[test]
    fn validate_for_launch_rejects_capability_surface_drift() {
        let binding = SessionBinding::new(
            Uuid::new_v4(),
            "sess-drift-construction".to_string(),
            SessionOwnerType::Project,
            Uuid::new_v4(),
            "execution",
        );
        let owner = SessionOwnerResolver::resolve_primary(&[binding]).expect("owner");
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut plan = SessionConstructionPlan::new(
            "sess-drift-construction",
            owner,
            SessionConstructionContextProjection::default(),
        );
        plan.workspace.working_directory = Some(PathBuf::from("/workspace"));
        plan.execution_profile.executor_config = Some(AgentConfig::new("PI_AGENT"));
        plan.surface.vfs = Some(vfs);
        plan.projections.capability_state = Some(CapabilityState::default());

        assert!(
            plan.validate_for_launch()
                .expect_err("capability/vfs drift must be rejected")
                .contains("capability_state.vfs.active")
        );
    }
}
