use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::MountCapability;
use crate::shared_library::InstalledAssetSource;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectVfsMount {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub content: ProjectVfsMountContent,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectVfsMount {
    pub fn new_inline(
        project_id: Uuid,
        mount_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            mount_id: mount_id.into(),
            display_name: display_name.into(),
            description: None,
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
            ],
            installed_source: None,
            content: ProjectVfsMountContent::Inline,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_external_service(
        project_id: Uuid,
        mount_id: impl Into<String>,
        display_name: impl Into<String>,
        service_id: impl Into<String>,
        root_ref: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            mount_id: mount_id.into(),
            display_name: display_name.into(),
            description: None,
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            installed_source: None,
            content: ProjectVfsMountContent::ExternalService {
                service_id: service_id.into(),
                root_ref: root_ref.into(),
            },
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectVfsMountContent {
    Inline,
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}
