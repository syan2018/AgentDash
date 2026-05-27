mod manager;
mod permissions;
mod process;
mod protocol;
mod runner;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use manager::{LocalExtensionHostManager, LocalTsExtensionHostConfig};

#[derive(Debug, Clone)]
pub struct LocalExtensionHostActivation {
    pub extension_key: String,
    pub backend_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostProfile {
    pub username: String,
    pub platform: String,
    pub arch: String,
    pub backend_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_roots: Vec<LocalExtensionHostWorkspaceRoot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostWorkspaceRoot {
    pub index: usize,
    pub name: String,
    pub display_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostHealth {
    pub active: bool,
    pub extension_id: Option<String>,
    pub action_keys: Vec<String>,
    #[serde(default)]
    pub channel_keys: Vec<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Error)]
pub enum LocalExtensionHostError {
    #[error("extension host package 非法: {0}")]
    InvalidPackage(String),
    #[error("extension host 进程失败: {0}")]
    Process(String),
    #[error("extension host protocol 非法: {0}")]
    Protocol(String),
    #[error("extension host 权限拒绝: {0}")]
    PermissionDenied(String),
    #[error("extension host 执行失败: {0}")]
    Host(String),
    #[error("extension host I/O 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("extension host JSON 失败: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests;
