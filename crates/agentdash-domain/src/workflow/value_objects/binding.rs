use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
/// Workflow 可挂载到哪一类 scope。
///
/// **Model C 收敛（2026-04-27）**：原先的 `Task` 变体已被移除——Task 不再作为独立
/// aggregate，而是 Story aggregate 下的 child entity；task-scope lifecycle
/// definition 统一归到 Story binding。
pub enum WorkflowBindingKind {
    Project,
    Story,
}

impl WorkflowBindingKind {
    pub fn binding_scope_key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
        }
    }

    pub fn from_binding_scope(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "project" => Some(Self::Project),
            "story" => Some(Self::Story),
            _ => None,
        }
    }

    pub fn from_owner_type(raw: &str) -> Option<Self> {
        Self::from_binding_scope(raw)
    }
}

pub fn normalize_workflow_binding_kinds(
    kinds: Vec<WorkflowBindingKind>,
) -> Result<Vec<WorkflowBindingKind>, String> {
    let mut normalized = Vec::new();
    for candidate in [WorkflowBindingKind::Project, WorkflowBindingKind::Story] {
        if kinds.contains(&candidate) {
            normalized.push(candidate);
        }
    }
    if normalized.is_empty() {
        return Err("workflow binding_kinds 至少需要一个挂载类型".to_string());
    }
    Ok(normalized)
}

pub fn workflow_binding_kinds_cover(
    required: &[WorkflowBindingKind],
    available: &[WorkflowBindingKind],
) -> bool {
    required.iter().all(|kind| available.contains(kind))
}
