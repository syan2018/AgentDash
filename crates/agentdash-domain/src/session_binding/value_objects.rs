use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Session 归属实体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOwnerType {
    Project,
    Story,
    Task,
}

impl fmt::Display for SessionOwnerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project => write!(f, "project"),
            Self::Story => write!(f, "story"),
            Self::Task => write!(f, "task"),
        }
    }
}

impl FromStr for SessionOwnerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "project" => Ok(Self::Project),
            "story" => Ok(Self::Story),
            "task" => Ok(Self::Task),
            _ => Err(format!("无效的 SessionOwnerType: {s}")),
        }
    }
}

/// Session 归属上下文 —— 合法的 owner 组合 sum type。
///
/// 替代 `(owner_type, project_id, story_id, task_id)` 四字段并列结构。
/// 三种变体对应三种合法组合;不合法组合(如 `owner_type=Task, task_id=None`)
/// 在类型层面被排除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionOwnerCtx {
    Project {
        project_id: Uuid,
    },
    Story {
        project_id: Uuid,
        story_id: Uuid,
    },
    Task {
        project_id: Uuid,
        story_id: Uuid,
        task_id: Uuid,
    },
}

impl SessionOwnerCtx {
    /// 对应的 owner 类型标签。
    pub fn owner_type(&self) -> SessionOwnerType {
        match self {
            Self::Project { .. } => SessionOwnerType::Project,
            Self::Story { .. } => SessionOwnerType::Story,
            Self::Task { .. } => SessionOwnerType::Task,
        }
    }

    /// Project ID — 三种变体均持有。
    pub fn project_id(&self) -> Uuid {
        match self {
            Self::Project { project_id }
            | Self::Story { project_id, .. }
            | Self::Task { project_id, .. } => *project_id,
        }
    }

    /// Story ID — Story / Task 变体有值,Project 返回 None。
    pub fn story_id(&self) -> Option<Uuid> {
        match self {
            Self::Story { story_id, .. } | Self::Task { story_id, .. } => Some(*story_id),
            Self::Project { .. } => None,
        }
    }

    /// Task ID — 仅 Task 变体有值。
    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            Self::Task { task_id, .. } => Some(*task_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ctx_accessors() {
        let pid = Uuid::new_v4();
        let ctx = SessionOwnerCtx::Project { project_id: pid };
        assert_eq!(ctx.owner_type(), SessionOwnerType::Project);
        assert_eq!(ctx.project_id(), pid);
        assert_eq!(ctx.story_id(), None);
        assert_eq!(ctx.task_id(), None);
    }

    #[test]
    fn story_ctx_accessors() {
        let pid = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let ctx = SessionOwnerCtx::Story {
            project_id: pid,
            story_id: sid,
        };
        assert_eq!(ctx.owner_type(), SessionOwnerType::Story);
        assert_eq!(ctx.project_id(), pid);
        assert_eq!(ctx.story_id(), Some(sid));
        assert_eq!(ctx.task_id(), None);
    }

    #[test]
    fn task_ctx_accessors() {
        let pid = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let ctx = SessionOwnerCtx::Task {
            project_id: pid,
            story_id: sid,
            task_id: tid,
        };
        assert_eq!(ctx.owner_type(), SessionOwnerType::Task);
        assert_eq!(ctx.project_id(), pid);
        assert_eq!(ctx.story_id(), Some(sid));
        assert_eq!(ctx.task_id(), Some(tid));
    }

    #[test]
    fn serializes_with_kind_tag() {
        let pid = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let ctx = SessionOwnerCtx::Task {
            project_id: pid,
            story_id: sid,
            task_id: tid,
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["kind"], "task");
        assert_eq!(json["task_id"], serde_json::Value::String(tid.to_string()));
    }

    #[test]
    fn roundtrip_json() {
        let original = SessionOwnerCtx::Story {
            project_id: Uuid::new_v4(),
            story_id: Uuid::new_v4(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: SessionOwnerCtx = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }
}
