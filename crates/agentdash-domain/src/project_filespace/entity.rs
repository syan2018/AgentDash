use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::MountCapability;
use crate::shared_library::InstalledAssetSource;

pub const PROJECT_FILESPACE_CONTAINER_ID: &str = "files";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectFilespace {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectFilespace {
    pub fn new(project_id: Uuid, key: impl Into<String>, display_name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            key: key.into(),
            display_name: display_name.into(),
            description: None,
            installed_source: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectVfsMountBinding {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub source: ProjectVfsMountSource,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub default_write: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectVfsMountBinding {
    pub fn new_filespace(
        project_id: Uuid,
        mount_id: impl Into<String>,
        display_name: impl Into<String>,
        filespace_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            mount_id: mount_id.into(),
            display_name: display_name.into(),
            source: ProjectVfsMountSource::Filespace { filespace_id },
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: true,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectVfsMountSource {
    Filespace {
        filespace_id: Uuid,
    },
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}
