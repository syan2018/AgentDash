use thiserror::Error;

use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository, LibraryAssetScope,
    LibraryAssetSource, LibraryAssetType,
};
use agentdash_spi::{MarketplaceAssetListing, MarketplaceFetchedAsset};

use super::seed_digest;

pub const UPSERT_LIBRARY_ASSET_IMPORT_MODE: &str = "upsert_library_asset";

#[derive(Debug, Clone)]
pub struct ImportExternalMarketplaceAssetInput {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: LibraryAssetType,
    pub import_mode: String,
    pub scope: LibraryAssetScope,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshExternalMarketplaceAssetInput {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: LibraryAssetType,
    pub scope: LibraryAssetScope,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalMarketplaceRefreshStatus {
    UpToDate,
    UpdateAvailable,
    SourceMissing,
    NotImported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshExternalMarketplaceAssetOutput {
    pub source_ref: String,
    pub remote_version: Option<String>,
    pub remote_digest: Option<String>,
    pub local_version: Option<String>,
    pub local_digest: Option<String>,
    pub status: ExternalMarketplaceRefreshStatus,
}

#[derive(Debug, Error)]
pub enum ExternalMarketplaceLibraryError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

pub async fn import_external_marketplace_asset(
    repo: &dyn LibraryAssetRepository,
    input: ImportExternalMarketplaceAssetInput,
    fetched: MarketplaceFetchedAsset,
) -> Result<LibraryAsset, ExternalMarketplaceLibraryError> {
    validate_import_mode(&input.import_mode)?;
    ensure_supported_external_asset_type(input.asset_type)?;

    let source_key = normalize_required("source_key", &input.source_key)?;
    let external_id = normalize_required("external_id", &input.external_id)?;
    let fetched_payload = fetched.payload();
    if fetched.asset_type() != input.asset_type {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider fetched asset_type 不匹配: request={} fetched={}",
            input.asset_type.as_str(),
            fetched.asset_type().as_str()
        )));
    }
    if fetched_payload.source_key != source_key {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider fetched source_key 不匹配: request={source_key} fetched={}",
            fetched_payload.source_key
        )));
    }
    if fetched_payload.external_id != external_id {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider fetched external_id 不匹配: request={external_id} fetched={}",
            fetched_payload.external_id
        )));
    }

    let source_ref = external_marketplace_source_ref(&source_key, input.asset_type, &external_id);
    let payload_digest = seed_digest(&fetched_payload.payload)?;
    let owner_id = normalize_optional_owner_id(input.owner_id);
    let asset = LibraryAsset::new(
        input.asset_type,
        input.scope,
        owner_id.clone(),
        fetched_payload.key.clone(),
        fetched_payload.display_name.clone(),
        fetched_payload.description.clone(),
        fetched_payload.version.clone(),
        LibraryAssetSource::RemoteImported,
        Some(source_ref.clone()),
        payload_digest,
        fetched_payload.payload.clone(),
    )?;

    let existing = repo
        .find_by_identity(
            asset.asset_type,
            asset.scope,
            owner_id.as_deref(),
            &asset.key,
        )
        .await?;

    match existing {
        Some(existing)
            if existing.source == LibraryAssetSource::RemoteImported
                && existing.source_ref.as_deref() == Some(source_ref.as_str()) =>
        {
            let mut updated = asset;
            updated.id = existing.id;
            updated.created_at = existing.created_at;
            updated.updated_at = chrono::Utc::now();
            repo.update(&updated).await?;
            Ok(updated)
        }
        Some(existing) => Err(ExternalMarketplaceLibraryError::Conflict(format!(
            "LibraryAsset identity 已被其它来源占用: {}:{} source={:?} source_ref={:?}",
            existing.asset_type.as_str(),
            existing.key,
            existing.source,
            existing.source_ref
        ))),
        None => {
            repo.create(&asset).await?;
            Ok(asset)
        }
    }
}

pub async fn refresh_external_marketplace_asset(
    repo: &dyn LibraryAssetRepository,
    input: RefreshExternalMarketplaceAssetInput,
    remote_listing: Option<MarketplaceAssetListing>,
) -> Result<RefreshExternalMarketplaceAssetOutput, ExternalMarketplaceLibraryError> {
    ensure_supported_external_asset_type(input.asset_type)?;
    let source_key = normalize_required("source_key", &input.source_key)?;
    let external_id = normalize_required("external_id", &input.external_id)?;
    let source_ref = external_marketplace_source_ref(&source_key, input.asset_type, &external_id);
    let owner_id = normalize_optional_owner_id(input.owner_id);

    let remote_listing = match remote_listing {
        Some(listing) => {
            validate_remote_listing(&listing, &source_key, &external_id, input.asset_type)?;
            Some(listing)
        }
        None => None,
    };

    let local_asset = find_imported_asset_by_source_ref(
        repo,
        input.asset_type,
        input.scope,
        owner_id.as_deref(),
        &source_ref,
    )
    .await?;

    let local_version = local_asset.as_ref().map(|asset| asset.version.clone());
    let local_digest = local_asset
        .as_ref()
        .map(|asset| asset.payload_digest.clone());
    let remote_version = remote_listing
        .as_ref()
        .map(|listing| listing.version.clone());
    let remote_digest = remote_listing
        .as_ref()
        .and_then(|listing| listing.digest.clone());

    let status = match (&local_asset, &remote_listing) {
        (_, None) => ExternalMarketplaceRefreshStatus::SourceMissing,
        (None, Some(_)) => ExternalMarketplaceRefreshStatus::NotImported,
        (Some(local), Some(remote)) if remote_matches_local(local, remote) => {
            ExternalMarketplaceRefreshStatus::UpToDate
        }
        (Some(_), Some(_)) => ExternalMarketplaceRefreshStatus::UpdateAvailable,
    };

    Ok(RefreshExternalMarketplaceAssetOutput {
        source_ref,
        remote_version,
        remote_digest,
        local_version,
        local_digest,
        status,
    })
}

pub fn external_marketplace_source_ref(
    source_key: &str,
    asset_type: LibraryAssetType,
    external_id: &str,
) -> String {
    format!("market:{source_key}:{}:{external_id}", asset_type.as_str())
}

pub fn ensure_supported_external_asset_type(
    asset_type: LibraryAssetType,
) -> Result<(), ExternalMarketplaceLibraryError> {
    if matches!(
        asset_type,
        LibraryAssetType::SkillTemplate | LibraryAssetType::McpServerTemplate
    ) {
        return Ok(());
    }
    Err(ExternalMarketplaceLibraryError::BadRequest(format!(
        "external marketplace asset_type 仅支持 skill_template / mcp_server_template: {}",
        asset_type.as_str()
    )))
}

fn validate_import_mode(import_mode: &str) -> Result<(), ExternalMarketplaceLibraryError> {
    if import_mode.trim() == UPSERT_LIBRARY_ASSET_IMPORT_MODE {
        return Ok(());
    }
    Err(ExternalMarketplaceLibraryError::BadRequest(format!(
        "external marketplace import_mode 仅支持 {UPSERT_LIBRARY_ASSET_IMPORT_MODE}"
    )))
}

fn validate_remote_listing(
    listing: &MarketplaceAssetListing,
    source_key: &str,
    external_id: &str,
    asset_type: LibraryAssetType,
) -> Result<(), ExternalMarketplaceLibraryError> {
    if listing.source_key != source_key {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider listing source_key 不匹配: request={source_key} listing={}",
            listing.source_key
        )));
    }
    if listing.external_id != external_id {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider listing external_id 不匹配: request={external_id} listing={}",
            listing.external_id
        )));
    }
    if listing.asset_type != asset_type {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "provider listing asset_type 不匹配: request={} listing={}",
            asset_type.as_str(),
            listing.asset_type.as_str()
        )));
    }
    ensure_supported_external_asset_type(listing.asset_type)
}

async fn find_imported_asset_by_source_ref(
    repo: &dyn LibraryAssetRepository,
    asset_type: LibraryAssetType,
    scope: LibraryAssetScope,
    owner_id: Option<&str>,
    source_ref: &str,
) -> Result<Option<LibraryAsset>, DomainError> {
    let assets = repo
        .list(LibraryAssetListFilter {
            asset_type: Some(asset_type),
            scope: Some(scope),
            owner_id: owner_id.map(ToString::to_string),
            include_deprecated: true,
        })
        .await?;
    Ok(assets.into_iter().find(|asset| {
        asset.source == LibraryAssetSource::RemoteImported
            && asset.source_ref.as_deref() == Some(source_ref)
    }))
}

fn remote_matches_local(local: &LibraryAsset, remote: &MarketplaceAssetListing) -> bool {
    local.version == remote.version
        && remote
            .digest
            .as_deref()
            .is_none_or(|remote_digest| remote_digest == local.payload_digest)
}

fn normalize_required(field: &str, value: &str) -> Result<String, ExternalMarketplaceLibraryError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ExternalMarketplaceLibraryError::BadRequest(format!(
            "{field} 不能为空"
        )));
    }
    Ok(value.to_string())
}

fn normalize_optional_owner_id(owner_id: Option<String>) -> Option<String> {
    owner_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;
    use uuid::Uuid;

    use agentdash_domain::shared_library::{LibraryAssetPayload, SkillTemplatePayload};
    use agentdash_spi::MarketplaceFetchedAssetPayload;

    use super::*;

    #[derive(Default)]
    struct InMemoryLibraryAssetRepository {
        assets: Mutex<Vec<LibraryAsset>>,
    }

    #[async_trait::async_trait]
    impl LibraryAssetRepository for InMemoryLibraryAssetRepository {
        async fn create(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
            asset.typed_payload()?;
            self.assets.lock().expect("lock").push(asset.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .find(|asset| asset.id == id)
                .cloned())
        }

        async fn find_by_identity(
            &self,
            asset_type: LibraryAssetType,
            scope: LibraryAssetScope,
            owner_id: Option<&str>,
            key: &str,
        ) -> Result<Option<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .find(|asset| {
                    asset.asset_type == asset_type
                        && asset.scope == scope
                        && asset.owner_id.as_deref() == owner_id
                        && asset.key == key
                })
                .cloned())
        }

        async fn list(
            &self,
            filter: LibraryAssetListFilter,
        ) -> Result<Vec<LibraryAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .expect("lock")
                .iter()
                .filter(|asset| {
                    filter
                        .asset_type
                        .is_none_or(|value| value == asset.asset_type)
                        && filter.scope.is_none_or(|value| value == asset.scope)
                        && filter
                            .owner_id
                            .as_deref()
                            .is_none_or(|value| asset.owner_id.as_deref() == Some(value))
                        && (filter.include_deprecated || !asset.deprecated)
                })
                .cloned()
                .collect())
        }

        async fn update(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
            asset.typed_payload()?;
            let mut assets = self.assets.lock().expect("lock");
            let existing = assets
                .iter_mut()
                .find(|existing| existing.id == asset.id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "library_asset",
                    id: asset.id.to_string(),
                })?;
            *existing = asset.clone();
            Ok(())
        }

        async fn upsert(&self, asset: &LibraryAsset) -> Result<LibraryAsset, DomainError> {
            if let Some(existing) = self
                .find_by_identity(
                    asset.asset_type,
                    asset.scope,
                    asset.owner_id.as_deref(),
                    &asset.key,
                )
                .await?
            {
                let mut updated = asset.clone();
                updated.id = existing.id;
                updated.created_at = existing.created_at;
                self.update(&updated).await?;
                return Ok(updated);
            }
            self.create(asset).await?;
            Ok(asset.clone())
        }
    }

    #[tokio::test]
    async fn import_external_marketplace_asset_creates_remote_imported_library_asset() {
        let repo = InMemoryLibraryAssetRepository::default();
        let fetched = fetched_skill(json!({
            "files": [{
                "path": "SKILL.md",
                "content": "Use this skill.",
                "kind": "skill"
            }],
            "disable_model_invocation": true
        }));

        let asset = import_external_marketplace_asset(
            &repo,
            ImportExternalMarketplaceAssetInput {
                source_key: "corp.catalog".to_string(),
                external_id: "skill-1".to_string(),
                asset_type: LibraryAssetType::SkillTemplate,
                import_mode: UPSERT_LIBRARY_ASSET_IMPORT_MODE.to_string(),
                scope: LibraryAssetScope::User,
                owner_id: Some("user-1".to_string()),
            },
            fetched,
        )
        .await
        .expect("import succeeds");

        assert_eq!(asset.source, LibraryAssetSource::RemoteImported);
        assert_eq!(
            asset.source_ref.as_deref(),
            Some("market:corp.catalog:skill_template:skill-1")
        );
        assert_eq!(asset.scope, LibraryAssetScope::User);
        assert_eq!(asset.owner_id.as_deref(), Some("user-1"));
        assert!(asset.payload_digest.starts_with("sha256:"));
        assert_ne!(asset.payload_digest, "remote-digest");
        assert!(matches!(
            asset.typed_payload().expect("typed payload"),
            LibraryAssetPayload::SkillTemplate(SkillTemplatePayload { .. })
        ));
    }

    #[tokio::test]
    async fn import_external_marketplace_asset_updates_same_remote_source_ref() {
        let repo = InMemoryLibraryAssetRepository::default();
        let first = import_external_marketplace_asset(
            &repo,
            import_input(),
            fetched_mcp("0.1.0", "https://example.com/mcp"),
        )
        .await
        .expect("first import");

        let second = import_external_marketplace_asset(
            &repo,
            import_input(),
            fetched_mcp("0.2.0", "https://example.com/mcp-v2"),
        )
        .await
        .expect("second import updates");

        assert_eq!(second.id, first.id);
        assert_eq!(second.version, "0.2.0");
        assert_ne!(second.payload_digest, first.payload_digest);
    }

    #[tokio::test]
    async fn import_external_marketplace_asset_rejects_fetched_asset_type_mismatch() {
        let repo = InMemoryLibraryAssetRepository::default();
        let error = import_external_marketplace_asset(
            &repo,
            ImportExternalMarketplaceAssetInput {
                asset_type: LibraryAssetType::McpServerTemplate,
                ..import_input()
            },
            fetched_skill(json!({
                "files": [{
                    "path": "SKILL.md",
                    "content": "Use this skill.",
                    "kind": "skill"
                }],
                "disable_model_invocation": false
            })),
        )
        .await
        .expect_err("mismatch rejected");

        assert!(matches!(
            error,
            ExternalMarketplaceLibraryError::BadRequest(message)
                if message.contains("asset_type 不匹配")
        ));
    }

    #[tokio::test]
    async fn import_external_marketplace_asset_rejects_invalid_payload() {
        let repo = InMemoryLibraryAssetRepository::default();
        let error = import_external_marketplace_asset(
            &repo,
            ImportExternalMarketplaceAssetInput {
                source_key: "corp.catalog".to_string(),
                external_id: "skill-1".to_string(),
                asset_type: LibraryAssetType::SkillTemplate,
                import_mode: UPSERT_LIBRARY_ASSET_IMPORT_MODE.to_string(),
                scope: LibraryAssetScope::User,
                owner_id: Some("user-1".to_string()),
            },
            fetched_skill(json!({"files": "not-array"})),
        )
        .await
        .expect_err("invalid payload rejected");

        assert!(matches!(
            error,
            ExternalMarketplaceLibraryError::Domain(DomainError::InvalidConfig(_))
        ));
    }

    #[tokio::test]
    async fn refresh_external_marketplace_asset_reports_not_imported() {
        let repo = InMemoryLibraryAssetRepository::default();
        let output = refresh_external_marketplace_asset(
            &repo,
            refresh_input(),
            Some(remote_listing("0.1.0", Some("sha256:remote"))),
        )
        .await
        .expect("refresh");

        assert_eq!(output.status, ExternalMarketplaceRefreshStatus::NotImported);
        assert_eq!(output.remote_version.as_deref(), Some("0.1.0"));
        assert!(output.local_version.is_none());
    }

    #[tokio::test]
    async fn refresh_external_marketplace_asset_compares_remote_version_and_digest() {
        let repo = InMemoryLibraryAssetRepository::default();
        let asset = import_external_marketplace_asset(
            &repo,
            import_input(),
            fetched_mcp("0.1.0", "https://example.com/mcp"),
        )
        .await
        .expect("import");

        let up_to_date = refresh_external_marketplace_asset(
            &repo,
            refresh_input(),
            Some(remote_listing("0.1.0", Some(&asset.payload_digest))),
        )
        .await
        .expect("refresh");
        assert_eq!(
            up_to_date.status,
            ExternalMarketplaceRefreshStatus::UpToDate
        );

        let update = refresh_external_marketplace_asset(
            &repo,
            refresh_input(),
            Some(remote_listing("0.2.0", Some(&asset.payload_digest))),
        )
        .await
        .expect("refresh");
        assert_eq!(
            update.status,
            ExternalMarketplaceRefreshStatus::UpdateAvailable
        );
    }

    #[tokio::test]
    async fn refresh_external_marketplace_asset_reports_source_missing_without_remote_listing() {
        let repo = InMemoryLibraryAssetRepository::default();
        let asset = import_external_marketplace_asset(
            &repo,
            import_input(),
            fetched_mcp("0.1.0", "https://example.com/mcp"),
        )
        .await
        .expect("import");

        let output = refresh_external_marketplace_asset(&repo, refresh_input(), None)
            .await
            .expect("refresh");

        assert_eq!(
            output.status,
            ExternalMarketplaceRefreshStatus::SourceMissing
        );
        assert_eq!(
            output.local_version.as_deref(),
            Some(asset.version.as_str())
        );
        assert!(output.remote_version.is_none());
    }

    fn import_input() -> ImportExternalMarketplaceAssetInput {
        ImportExternalMarketplaceAssetInput {
            source_key: "corp.catalog".to_string(),
            external_id: "mcp-1".to_string(),
            asset_type: LibraryAssetType::McpServerTemplate,
            import_mode: UPSERT_LIBRARY_ASSET_IMPORT_MODE.to_string(),
            scope: LibraryAssetScope::User,
            owner_id: Some("user-1".to_string()),
        }
    }

    fn refresh_input() -> RefreshExternalMarketplaceAssetInput {
        RefreshExternalMarketplaceAssetInput {
            source_key: "corp.catalog".to_string(),
            external_id: "mcp-1".to_string(),
            asset_type: LibraryAssetType::McpServerTemplate,
            scope: LibraryAssetScope::User,
            owner_id: Some("user-1".to_string()),
        }
    }

    fn fetched_skill(payload: serde_json::Value) -> MarketplaceFetchedAsset {
        MarketplaceFetchedAsset::SkillTemplate(MarketplaceFetchedAssetPayload {
            source_key: "corp.catalog".to_string(),
            external_id: "skill-1".to_string(),
            key: "corp-skill".to_string(),
            display_name: "Corp Skill".to_string(),
            description: Some("A skill".to_string()),
            version: "0.1.0".to_string(),
            digest: Some("remote-digest".to_string()),
            payload,
        })
    }

    fn fetched_mcp(version: &str, url: &str) -> MarketplaceFetchedAsset {
        MarketplaceFetchedAsset::McpServerTemplate(MarketplaceFetchedAssetPayload {
            source_key: "corp.catalog".to_string(),
            external_id: "mcp-1".to_string(),
            key: "corp-mcp".to_string(),
            display_name: "Corp MCP".to_string(),
            description: Some("An MCP server".to_string()),
            version: version.to_string(),
            digest: Some("remote-digest".to_string()),
            payload: json!({
                "transport_template": {
                    "type": "http",
                    "url_template": url
                },
                "route_policy": "direct",
                "capabilities": ["search"]
            }),
        })
    }

    fn remote_listing(version: &str, digest: Option<&str>) -> MarketplaceAssetListing {
        MarketplaceAssetListing {
            source_key: "corp.catalog".to_string(),
            external_id: "mcp-1".to_string(),
            asset_type: LibraryAssetType::McpServerTemplate,
            key: "corp-mcp".to_string(),
            display_name: "Corp MCP".to_string(),
            description: Some("An MCP server".to_string()),
            version: version.to_string(),
            tags: vec!["corp".to_string()],
            author: Some("AgentDash".to_string()),
            digest: digest.map(ToString::to_string),
            updated_at: None,
            install_requirements: vec![],
        }
    }
}
