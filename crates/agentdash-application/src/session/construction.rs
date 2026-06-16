#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::common::AgentConfig;
use agentdash_domain::task::TaskDispatchPreference;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AuthIdentity, CapabilityState, DiscoveredGuideline, SessionBaselineCapabilities,
    SessionContextBundle, Vfs,
};
use uuid::Uuid;

use agentdash_spi::CapabilityScope;

use super::context::SessionContextSnapshot;
use super::post_turn_handler::TerminalHookEffectBinding;
use super::types::UserPromptInput;

/// 测试 fixture：RuntimeSession trace 的创建来源信息。
#[derive(Debug, Clone)]
pub struct ResolvedSessionOwner {
    pub owner_type: CapabilityScope,
    pub project_id: Option<Uuid>,
    pub trace: OwnerResolutionTrace,
}

#[derive(Debug, Clone, Default)]
pub struct OwnerResolutionTrace {
    pub selected_reason: String,
}

impl ResolvedSessionOwner {
    pub fn project(project_id: Uuid) -> Self {
        Self {
            owner_type: CapabilityScope::Project,
            project_id: Some(project_id),
            trace: OwnerResolutionTrace {
                selected_reason: "project".to_string(),
            },
        }
    }

    pub fn story(project_id: Uuid) -> Self {
        Self {
            owner_type: CapabilityScope::Story,
            project_id: Some(project_id),
            trace: OwnerResolutionTrace {
                selected_reason: "story".to_string(),
            },
        }
    }

    pub fn task(project_id: Uuid) -> Self {
        Self {
            owner_type: CapabilityScope::Task,
            project_id: Some(project_id),
            trace: OwnerResolutionTrace {
                selected_reason: "task".to_string(),
            },
        }
    }
}
use crate::vfs::ResolvedVfsSurface;
use crate::agent_run::frame::surface::FrameSurfaceDraft;

/// 测试 fixture：launch envelope 测试所需的完整投影形态。
#[derive(Debug, Clone)]
pub struct RuntimeContextInspectionPlan {
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
    pub input: Option<Vec<agentdash_agent_protocol::UserInputBlock>>,
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
    /// Construction 到 AgentFrame / FrameLaunchEnvelope 的唯一 surface handoff。
    pub frame_surface_draft: Option<FrameSurfaceDraft>,
    pub session_capabilities: Option<SessionBaselineCapabilities>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
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
    pub dispatch_preference: Option<TaskDispatchPreference>,
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

impl RuntimeContextInspectionPlan {
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
                        CapabilityScope::Task => "task.context_builder".to_string(),
                        CapabilityScope::Story => "story.context_builder".to_string(),
                        CapabilityScope::Project => "project.context_builder".to_string(),
                    },
                },
            ],
        };
        let projections = ConstructionProjections {
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
        plan.prompt.input = user_input.input.clone();
        plan.prompt.environment_variables = user_input.env.clone();
        plan.execution_profile.executor_config = user_input.executor_config.clone();
        plan
    }

    pub fn active_vfs(&self) -> Option<&Vfs> {
        self.projections
            .frame_surface_draft
            .as_ref()
            .and_then(|draft| draft.vfs.as_ref())
            .or_else(|| {
                self.projections
                    .frame_surface_draft
                    .as_ref()
                    .and_then(|draft| draft.capability_state.as_ref())
                    .and_then(|state| state.vfs.active.as_ref())
            })
            .or(self.surface.vfs.as_ref())
            .or(self.context_projection.vfs.as_ref())
    }

    pub fn active_vfs_cloned(&self) -> Option<Vfs> {
        self.active_vfs().cloned()
    }

    pub fn set_active_vfs(&mut self, vfs: Vfs) {
        if let Some(draft) = self.projections.frame_surface_draft.as_mut() {
            draft.vfs = Some(vfs.clone());
            if let Some(capability_state) = draft.capability_state.as_mut() {
                capability_state.vfs.active = Some(vfs.clone());
            }
        }
        self.surface.vfs = Some(vfs.clone());
        self.context_projection.vfs = Some(vfs);
    }

    pub fn sync_vfs_projection_from_capability(&mut self) {
        if let Some(vfs) = self
            .projections
            .frame_surface_draft
            .as_ref()
            .and_then(|draft| {
                draft.vfs.clone().or_else(|| {
                    draft
                        .capability_state
                        .as_ref()
                        .and_then(|state| state.vfs.active.clone())
                })
            })
        {
            self.surface.vfs = Some(vfs.clone());
            self.context_projection.vfs = Some(vfs);
        } else if let Some(vfs) = self.surface.vfs.clone() {
            self.context_projection.vfs = Some(vfs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_context_plan_carries_owner_and_projection_trace() {
        let owner = ResolvedSessionOwner::task(Uuid::new_v4());
        let projection = SessionConstructionContextProjection {
            workspace_id: Some(Uuid::new_v4()),
            ..Default::default()
        };

        let plan = RuntimeContextInspectionPlan::new("sess-construction", owner, projection);

        assert_eq!(plan.session_id, "sess-construction");
        assert_eq!(plan.owner.owner_type, CapabilityScope::Task);
        assert!(plan.context_projection.workspace_id.is_some());
        assert_eq!(plan.trace.entries[0].stage, "owner");
        assert_eq!(plan.trace.entries[1].source, "task.context_builder");
    }

    #[test]
    fn runtime_context_plan_keeps_full_context_bundle() {
        let owner = ResolvedSessionOwner::project(Uuid::new_v4());
        let bundle = SessionContextBundle::new(Uuid::new_v4(), "owner_bootstrap");
        let bundle_id = bundle.bundle_id;

        let mut plan = RuntimeContextInspectionPlan::new(
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
}
