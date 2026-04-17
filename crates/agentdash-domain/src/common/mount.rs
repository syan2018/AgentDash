use serde::{Deserialize, Serialize};

use super::MountCapability;

/// 统一挂载点定义，被 connector-contract 和 application 直接使用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mount {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl Mount {
    pub fn supports(&self, capability: MountCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// 统一地址空间定义，被 connector-contract 和 application 直接使用。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vfs {
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_story_id: Option<String>,
}

impl Vfs {
    pub fn default_mount(&self) -> Option<&Mount> {
        let default_id = self.default_mount_id.as_deref()?;
        self.mounts.iter().find(|mount| mount.id == default_id)
    }
}
