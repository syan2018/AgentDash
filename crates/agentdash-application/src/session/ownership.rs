use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSessionOwner {
    pub project_id: Uuid,
    pub owner_type: SessionOwnerType,
    pub owner_id: Uuid,
    pub label: String,
    pub trace: SessionOwnerResolutionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOwnerResolutionTrace {
    pub priority: Vec<SessionOwnerType>,
    pub candidates: Vec<SessionOwnerCandidateTrace>,
    pub selected_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOwnerCandidateTrace {
    pub owner_type: SessionOwnerType,
    pub owner_id: Uuid,
    pub label: String,
}

pub struct SessionOwnerResolver;

impl SessionOwnerResolver {
    pub const PRIORITY: [SessionOwnerType; 3] = [
        SessionOwnerType::Task,
        SessionOwnerType::Story,
        SessionOwnerType::Project,
    ];

    pub fn select_primary_binding(bindings: &[SessionBinding]) -> Option<&SessionBinding> {
        Self::PRIORITY
            .iter()
            .find_map(|owner_type| {
                bindings
                    .iter()
                    .find(|binding| binding.owner_type == *owner_type)
            })
            .or_else(|| bindings.first())
    }

    pub fn resolve_primary(bindings: &[SessionBinding]) -> Option<ResolvedSessionOwner> {
        let selected = Self::select_primary_binding(bindings)?;
        let selected_priority = Self::PRIORITY
            .iter()
            .position(|owner_type| *owner_type == selected.owner_type)
            .map(|index| format!("priority[{index}]={}", selected.owner_type))
            .unwrap_or_else(|| "fallback:first-binding".to_string());

        Some(ResolvedSessionOwner {
            project_id: selected.project_id,
            owner_type: selected.owner_type,
            owner_id: selected.owner_id,
            label: selected.label.clone(),
            trace: SessionOwnerResolutionTrace {
                priority: Self::PRIORITY.to_vec(),
                candidates: bindings
                    .iter()
                    .map(|binding| SessionOwnerCandidateTrace {
                        owner_type: binding.owner_type,
                        owner_id: binding.owner_id,
                        label: binding.label.clone(),
                    })
                    .collect(),
                selected_reason: selected_priority,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(owner_type: SessionOwnerType, label: &str) -> SessionBinding {
        SessionBinding::new(
            Uuid::new_v4(),
            "sess-owner".to_string(),
            owner_type,
            Uuid::new_v4(),
            label,
        )
    }

    #[test]
    fn resolves_task_before_story_and_project() {
        let project = binding(SessionOwnerType::Project, "project");
        let story = binding(SessionOwnerType::Story, "story");
        let task = binding(SessionOwnerType::Task, "task");
        let bindings = vec![project, story, task.clone()];

        let resolved = SessionOwnerResolver::resolve_primary(&bindings).expect("owner");

        assert_eq!(resolved.owner_type, SessionOwnerType::Task);
        assert_eq!(resolved.owner_id, task.owner_id);
        assert_eq!(resolved.label, "task");
        assert_eq!(
            resolved.trace.priority,
            vec![
                SessionOwnerType::Task,
                SessionOwnerType::Story,
                SessionOwnerType::Project
            ]
        );
        assert_eq!(resolved.trace.selected_reason, "priority[0]=task");
    }

    #[test]
    fn resolves_story_before_project_when_task_missing() {
        let project = binding(SessionOwnerType::Project, "project");
        let story = binding(SessionOwnerType::Story, "story");
        let bindings = vec![project, story.clone()];

        let resolved = SessionOwnerResolver::resolve_primary(&bindings).expect("owner");

        assert_eq!(resolved.owner_type, SessionOwnerType::Story);
        assert_eq!(resolved.owner_id, story.owner_id);
        assert_eq!(resolved.trace.selected_reason, "priority[1]=story");
    }

    #[test]
    fn resolves_project_when_it_is_the_only_owner() {
        let project = binding(SessionOwnerType::Project, "project");
        let bindings = vec![project.clone()];

        let resolved = SessionOwnerResolver::resolve_primary(&bindings).expect("owner");

        assert_eq!(resolved.owner_type, SessionOwnerType::Project);
        assert_eq!(resolved.owner_id, project.owner_id);
        assert_eq!(resolved.trace.selected_reason, "priority[2]=project");
    }

    #[test]
    fn returns_none_for_empty_bindings() {
        assert!(SessionOwnerResolver::resolve_primary(&[]).is_none());
        assert!(SessionOwnerResolver::select_primary_binding(&[]).is_none());
    }
}
