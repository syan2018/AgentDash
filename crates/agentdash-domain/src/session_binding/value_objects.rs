use serde::{Deserialize, Serialize};
use std::fmt;

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

impl SessionOwnerType {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "project" => Some(Self::Project),
            "story" => Some(Self::Story),
            "task" => Some(Self::Task),
            _ => None,
        }
    }
}
