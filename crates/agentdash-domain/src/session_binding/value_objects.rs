use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

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
