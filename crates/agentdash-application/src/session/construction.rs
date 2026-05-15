use agentdash_domain::task::AgentBinding;
use agentdash_spi::{SessionBaselineCapabilities, Vfs};
use uuid::Uuid;

use super::context::SessionContextSnapshot;
use super::ownership::ResolvedSessionOwner;
use crate::vfs::ResolvedVfsSurface;

#[derive(Debug, Clone)]
pub struct SessionConstructionPlan {
    pub session_id: String,
    pub owner: ResolvedSessionOwner,
    pub context_projection: SessionConstructionContextProjection,
    pub trace: SessionConstructionTrace,
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
        Self {
            session_id: session_id.into(),
            owner,
            context_projection,
            trace,
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
}
