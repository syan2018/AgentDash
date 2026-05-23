use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::session_binding::SessionOwnerType;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Workflow 可挂载到哪一类 owner。
/// 这里只描述绑定范围，不表达 workflow 自身的业务主语。
///
/// **Model C 收敛（2026-04-27）**：原先的 `Task` 变体已被移除——Task 不再作为独立
/// aggregate，而是 Story aggregate 下的 child entity；task-scope lifecycle
/// definition 统一归到 Story binding。详见
/// `.trellis/spec/backend/story-task-runtime.md`。
///
/// 注意：`SessionOwnerType::Task` 仍然存在（session binding 的 owner 坐标系
/// 不受影响），但当需要把它映射到 `WorkflowBindingKind` 时，会落到 `Story`。
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

impl From<SessionOwnerType> for WorkflowBindingKind {
    /// 将 session owner 类型映射为 workflow binding kind。
    ///
    /// **Model C 决策**：`SessionOwnerType::Task` 映射到 `WorkflowBindingKind::Story`。
    /// 理由：Task 所属的 Story 是 binding 定义的自然归属；task 级的 lifecycle
    /// 统一由 Story-bound lifecycle 承载（一个 Story 下每个 task 激活其对应的
    /// step）。这里会丢掉 task_id 信息——上层若需要区分 task，必须通过
    /// `SessionOwnerCtx::Task { story_id, task_id, .. }` 单独保留，而不是依赖
    /// `WorkflowBindingKind`。
    fn from(value: SessionOwnerType) -> Self {
        match value {
            SessionOwnerType::Project => Self::Project,
            SessionOwnerType::Story | SessionOwnerType::Task => Self::Story,
        }
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
