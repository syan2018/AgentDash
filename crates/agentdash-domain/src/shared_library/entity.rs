use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;

use super::value_objects::{
    LibraryAssetPayload, LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryAsset {
    pub id: Uuid,
    pub asset_type: LibraryAssetType,
    pub scope: LibraryAssetScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    pub source: LibraryAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub payload_digest: String,
    #[serde(default)]
    pub deprecated: bool,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LibraryAsset {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        asset_type: LibraryAssetType,
        scope: LibraryAssetScope,
        owner_id: Option<String>,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: Option<String>,
        version: impl Into<String>,
        source: LibraryAssetSource,
        source_ref: Option<String>,
        payload_digest: impl Into<String>,
        payload: Value,
    ) -> Result<Self, DomainError> {
        let key = key.into();
        validate_identity(&key, "library_assets.key")?;
        let display_name = display_name.into();
        validate_identity(&display_name, "library_assets.display_name")?;
        let version = version.into();
        validate_identity(&version, "library_assets.version")?;
        let payload_digest = payload_digest.into();
        validate_identity(&payload_digest, "library_assets.payload_digest")?;
        LibraryAssetPayload::validate(asset_type, &payload)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            asset_type,
            scope,
            owner_id,
            key,
            display_name,
            description,
            version,
            source,
            source_ref,
            payload_digest,
            deprecated: false,
            payload,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn typed_payload(&self) -> Result<LibraryAssetPayload, DomainError> {
        LibraryAssetPayload::from_value(self.asset_type, self.payload.clone())
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn mark_deprecated(&mut self) {
        self.deprecated = true;
        self.touch();
    }
}

fn validate_identity(value: &str, field: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        return Err(DomainError::InvalidConfig(format!("{field} 不能为空")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn library_asset_rejects_payload_mismatch() {
        let result = LibraryAsset::new(
            LibraryAssetType::McpServerTemplate,
            LibraryAssetScope::Builtin,
            None,
            "bad",
            "Bad",
            None,
            "1.0.0",
            LibraryAssetSource::Builtin,
            Some("bad".to_string()),
            "digest",
            json!({"system_prompt":"not mcp"}),
        );

        assert!(result.is_err());
    }

    #[test]
    fn library_asset_accepts_agent_template_payload() {
        let asset = LibraryAsset::new(
            LibraryAssetType::AgentTemplate,
            LibraryAssetScope::Builtin,
            None,
            "reviewer",
            "Reviewer",
            None,
            "1.0.0",
            LibraryAssetSource::Builtin,
            Some("reviewer".to_string()),
            "digest",
            json!({
                "config": {
                    "executor": "PI_AGENT",
                    "system_prompt": "你是代码审阅助手",
                    "mcp_slots": [{"key": "repo", "description": "代码仓库"}]
                }
            }),
        )
        .expect("valid asset");

        assert!(matches!(
            asset.typed_payload().expect("typed"),
            LibraryAssetPayload::AgentTemplate(_)
        ));
    }
}
