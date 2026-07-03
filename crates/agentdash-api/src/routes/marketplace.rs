//! Enterprise Marketplace HTTP routes.

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use agentdash_application_shared_library::{
    ExternalMarketplaceRefreshStatus as ApplicationRefreshStatus,
    ImportExternalMarketplaceAssetInput, RefreshExternalMarketplaceAssetInput,
    RefreshExternalMarketplaceAssetOutput, UPSERT_LIBRARY_ASSET_IMPORT_MODE,
    ensure_supported_external_asset_type, import_external_marketplace_asset,
    refresh_external_marketplace_asset,
};
use agentdash_contracts::external_marketplace::{
    ExternalMarketplaceAssetDetailDto, ExternalMarketplaceAssetListingDto,
    ExternalMarketplaceAssetPageDto, ExternalMarketplaceInstallRequirementDto,
    ExternalMarketplaceRefreshStatus, ImportExternalMarketplaceAssetRequest,
    ImportExternalMarketplaceAssetResponse, ListExternalMarketplaceAssetsQuery,
    MarketplaceInstallRequirementKindDto, MarketplaceSourceDto, MarketplaceSourceProviderKindDto,
    MarketplaceSourceTrustLevelDto, RefreshExternalMarketplaceAssetRequest,
    RefreshExternalMarketplaceAssetResponse,
};
use agentdash_domain::shared_library::{
    LibraryAssetRepository, LibraryAssetScope, LibraryAssetType,
};
use agentdash_integration_api::{
    MarketplaceAssetDetail, MarketplaceAssetListing, MarketplaceAssetPage, MarketplaceAssetQuery,
    MarketplaceInstallRequirementKind, MarketplaceSourceDescriptor, MarketplaceSourceError,
    MarketplaceSourceProvider, MarketplaceSourceProviderKind, MarketplaceSourceTrustLevel,
};

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::dto::{contract_asset_type, library_asset_response, parse_asset_type};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ExternalMarketplaceAssetPath {
    pub source_key: String,
    pub external_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/marketplace/sources",
            axum::routing::get(list_marketplace_sources),
        )
        .route(
            "/marketplace/external-assets",
            axum::routing::get(list_external_marketplace_assets),
        )
        .route(
            "/marketplace/external-assets/import",
            axum::routing::post(import_external_marketplace_asset_route),
        )
        .route(
            "/marketplace/external-assets/refresh",
            axum::routing::post(refresh_external_marketplace_asset_route),
        )
        .route(
            "/marketplace/external-assets/{source_key}/{external_id}",
            axum::routing::get(get_external_marketplace_asset_detail),
        )
}

/// GET `/api/marketplace/sources`
pub async fn list_marketplace_sources(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
) -> Result<Json<Vec<MarketplaceSourceDto>>, ApiError> {
    Ok(Json(
        state
            .services
            .marketplace_source_providers
            .iter()
            .map(|provider| marketplace_source_response(provider.descriptor()))
            .collect(),
    ))
}

/// GET `/api/marketplace/external-assets`
pub async fn list_external_marketplace_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<ListExternalMarketplaceAssetsQuery>,
) -> Result<Json<ExternalMarketplaceAssetPageDto>, ApiError> {
    Ok(Json(
        list_external_assets_from_providers(&state.services.marketplace_source_providers, query)
            .await?,
    ))
}

/// GET `/api/marketplace/external-assets/:source_key/:external_id`
pub async fn get_external_marketplace_asset_detail(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(path): Path<ExternalMarketplaceAssetPath>,
) -> Result<Json<ExternalMarketplaceAssetDetailDto>, ApiError> {
    Ok(Json(
        get_external_asset_detail_from_provider(
            &state.services.marketplace_source_providers,
            &path.source_key,
            &path.external_id,
        )
        .await?,
    ))
}

/// POST `/api/marketplace/external-assets/import`
pub async fn import_external_marketplace_asset_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<ImportExternalMarketplaceAssetRequest>,
) -> Result<Json<ImportExternalMarketplaceAssetResponse>, ApiError> {
    Ok(Json(
        import_external_marketplace_asset_for_request(
            state.repos.shared_library_repo.as_ref(),
            &state.services.marketplace_source_providers,
            &current_user.user_id,
            req,
        )
        .await?,
    ))
}

async fn import_external_marketplace_asset_for_request(
    repo: &dyn LibraryAssetRepository,
    providers: &[Arc<dyn MarketplaceSourceProvider>],
    current_user_id: &str,
    req: ImportExternalMarketplaceAssetRequest,
) -> Result<ImportExternalMarketplaceAssetResponse, ApiError> {
    if req.import_mode.trim() != UPSERT_LIBRARY_ASSET_IMPORT_MODE {
        return Err(ApiError::BadRequest(format!(
            "external marketplace import_mode 仅支持 {UPSERT_LIBRARY_ASSET_IMPORT_MODE}"
        )));
    }
    let source_key = normalize_required_api("source_key", &req.source_key)?;
    let external_id = normalize_required_api("external_id", &req.external_id)?;
    let asset_type = parse_external_asset_type(&req.asset_type)?;
    let (provider, descriptor) = find_marketplace_provider(providers, &source_key)?;
    ensure_provider_supports_asset_type(&descriptor, asset_type)?;

    let fetched = provider
        .fetch_asset_payload(&external_id)
        .await
        .map_err(provider_error_to_api)?;
    let asset = import_external_marketplace_asset(
        repo,
        ImportExternalMarketplaceAssetInput {
            source_key,
            external_id,
            asset_type,
            import_mode: req.import_mode,
            scope: LibraryAssetScope::User,
            owner_id: Some(current_user_id.to_string()),
        },
        fetched,
    )
    .await?;

    Ok(ImportExternalMarketplaceAssetResponse {
        asset: library_asset_response(asset),
    })
}

/// POST `/api/marketplace/external-assets/refresh`
pub async fn refresh_external_marketplace_asset_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<RefreshExternalMarketplaceAssetRequest>,
) -> Result<Json<RefreshExternalMarketplaceAssetResponse>, ApiError> {
    let source_key = normalize_required_api("source_key", &req.source_key)?;
    let external_id = normalize_required_api("external_id", &req.external_id)?;
    let asset_type = parse_external_asset_type(&req.asset_type)?;
    let (provider, descriptor) =
        find_marketplace_provider(&state.services.marketplace_source_providers, &source_key)?;
    ensure_provider_supports_asset_type(&descriptor, asset_type)?;

    let remote_listing = match provider.get_asset_detail(&external_id).await {
        Ok(detail) => {
            validate_detail_from_provider(&descriptor, &detail, &external_id)?;
            Some(detail.listing)
        }
        Err(MarketplaceSourceError::NotFound { .. }) => None,
        Err(error) => return Err(provider_error_to_api(error)),
    };
    let output = refresh_external_marketplace_asset(
        state.repos.shared_library_repo.as_ref(),
        RefreshExternalMarketplaceAssetInput {
            source_key,
            external_id,
            asset_type,
            scope: LibraryAssetScope::User,
            owner_id: Some(current_user.user_id.clone()),
        },
        remote_listing,
    )
    .await?;

    Ok(Json(refresh_external_marketplace_asset_response(output)))
}

async fn list_external_assets_from_providers(
    providers: &[Arc<dyn MarketplaceSourceProvider>],
    query: ListExternalMarketplaceAssetsQuery,
) -> Result<ExternalMarketplaceAssetPageDto, ApiError> {
    let source_key = normalize_optional(query.source_key);
    let cursor = normalize_optional(query.cursor);
    if source_key.is_none() && cursor.is_some() {
        return Err(ApiError::BadRequest(
            "cursor 分页必须同时指定 source_key".to_string(),
        ));
    }
    let asset_type = parse_optional_external_asset_type(query.asset_type)?;
    let search_query = normalize_optional(query.query);

    if let Some(source_key) = source_key {
        let (provider, descriptor) = find_marketplace_provider(providers, &source_key)?;
        if let Some(asset_type) = asset_type {
            ensure_provider_supports_asset_type(&descriptor, asset_type)?;
        }
        let page = provider
            .list_assets(MarketplaceAssetQuery {
                asset_type,
                query: search_query,
                cursor,
                limit: query.limit,
            })
            .await
            .map_err(provider_error_to_api)?;
        return map_external_asset_page(&descriptor, page, asset_type);
    }

    let mut items = Vec::new();
    for provider in providers {
        let descriptor = provider.descriptor();
        if !descriptor.enabled {
            continue;
        }
        if asset_type
            .is_some_and(|asset_type| !descriptor.supported_asset_types.contains(&asset_type))
        {
            continue;
        }
        let page = provider
            .list_assets(MarketplaceAssetQuery {
                asset_type,
                query: search_query.clone(),
                cursor: None,
                limit: query.limit,
            })
            .await
            .map_err(provider_error_to_api)?;
        let mut page = map_external_asset_page(&descriptor, page, asset_type)?;
        items.append(&mut page.items);
    }

    if let Some(limit) = query.limit {
        items.truncate(limit as usize);
    }

    Ok(ExternalMarketplaceAssetPageDto {
        items,
        next_cursor: None,
    })
}

async fn get_external_asset_detail_from_provider(
    providers: &[Arc<dyn MarketplaceSourceProvider>],
    source_key: &str,
    external_id: &str,
) -> Result<ExternalMarketplaceAssetDetailDto, ApiError> {
    let source_key = normalize_required_api("source_key", source_key)?;
    let external_id = normalize_required_api("external_id", external_id)?;
    let (provider, descriptor) = find_marketplace_provider(providers, &source_key)?;
    let detail = provider
        .get_asset_detail(&external_id)
        .await
        .map_err(provider_error_to_api)?;
    validate_detail_from_provider(&descriptor, &detail, &external_id)?;
    Ok(external_asset_detail_response(detail))
}

fn find_marketplace_provider(
    providers: &[Arc<dyn MarketplaceSourceProvider>],
    source_key: &str,
) -> Result<
    (
        Arc<dyn MarketplaceSourceProvider>,
        MarketplaceSourceDescriptor,
    ),
    ApiError,
> {
    let source_key = source_key.trim();
    providers
        .iter()
        .find_map(|provider| {
            let descriptor = provider.descriptor();
            (descriptor.source_key == source_key).then(|| (provider.clone(), descriptor))
        })
        .ok_or_else(|| ApiError::NotFound(format!("Marketplace source 不存在: {source_key}")))
}

fn map_external_asset_page(
    descriptor: &MarketplaceSourceDescriptor,
    page: MarketplaceAssetPage,
    requested_asset_type: Option<LibraryAssetType>,
) -> Result<ExternalMarketplaceAssetPageDto, ApiError> {
    let mut items = Vec::with_capacity(page.items.len());
    for listing in page.items {
        validate_listing_from_provider(descriptor, &listing, requested_asset_type)?;
        items.push(external_asset_listing_response(listing));
    }
    Ok(ExternalMarketplaceAssetPageDto {
        items,
        next_cursor: page.next_cursor,
    })
}

fn validate_detail_from_provider(
    descriptor: &MarketplaceSourceDescriptor,
    detail: &MarketplaceAssetDetail,
    external_id: &str,
) -> Result<(), ApiError> {
    validate_listing_from_provider(descriptor, &detail.listing, None)?;
    if detail.listing.external_id != external_id {
        return Err(ApiError::BadRequest(format!(
            "provider detail external_id 不匹配: request={external_id} listing={}",
            detail.listing.external_id
        )));
    }
    Ok(())
}

fn validate_listing_from_provider(
    descriptor: &MarketplaceSourceDescriptor,
    listing: &MarketplaceAssetListing,
    requested_asset_type: Option<LibraryAssetType>,
) -> Result<(), ApiError> {
    if listing.source_key != descriptor.source_key {
        return Err(ApiError::BadRequest(format!(
            "provider listing source_key 不匹配: provider={} listing={}",
            descriptor.source_key, listing.source_key
        )));
    }
    ensure_supported_external_asset_type(listing.asset_type).map_err(ApiError::from)?;
    if !descriptor
        .supported_asset_types
        .contains(&listing.asset_type)
    {
        return Err(ApiError::BadRequest(format!(
            "provider listing asset_type `{}` 不在 source `{}` descriptor supported_asset_types 内",
            listing.asset_type.as_str(),
            descriptor.source_key
        )));
    }
    if requested_asset_type.is_some_and(|asset_type| asset_type != listing.asset_type) {
        return Err(ApiError::BadRequest(format!(
            "provider listing asset_type 不匹配: request={} listing={}",
            requested_asset_type.expect("checked").as_str(),
            listing.asset_type.as_str()
        )));
    }
    for (field, value) in [
        ("external_id", listing.external_id.as_str()),
        ("key", listing.key.as_str()),
        ("display_name", listing.display_name.as_str()),
        ("version", listing.version.as_str()),
    ] {
        normalize_required_api(field, value)?;
    }
    Ok(())
}

fn ensure_provider_supports_asset_type(
    descriptor: &MarketplaceSourceDescriptor,
    asset_type: LibraryAssetType,
) -> Result<(), ApiError> {
    ensure_supported_external_asset_type(asset_type).map_err(ApiError::from)?;
    if descriptor.supported_asset_types.contains(&asset_type) {
        return Ok(());
    }
    Err(ApiError::BadRequest(format!(
        "Marketplace source `{}` 不支持 asset_type `{}`",
        descriptor.source_key,
        asset_type.as_str()
    )))
}

fn parse_optional_external_asset_type(
    raw: Option<String>,
) -> Result<Option<LibraryAssetType>, ApiError> {
    raw.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_external_asset_type)
        .transpose()
}

fn parse_external_asset_type(raw: &str) -> Result<LibraryAssetType, ApiError> {
    let asset_type = parse_asset_type(raw.trim()).map_err(ApiError::BadRequest)?;
    ensure_supported_external_asset_type(asset_type).map_err(ApiError::from)?;
    Ok(asset_type)
}

fn marketplace_source_response(descriptor: MarketplaceSourceDescriptor) -> MarketplaceSourceDto {
    MarketplaceSourceDto {
        source_key: descriptor.source_key,
        display_name: descriptor.display_name,
        description: descriptor.description,
        provider_kind: match descriptor.provider_kind {
            MarketplaceSourceProviderKind::Integration => {
                MarketplaceSourceProviderKindDto::Integration
            }
            MarketplaceSourceProviderKind::Builtin => MarketplaceSourceProviderKindDto::Builtin,
        },
        supported_asset_types: descriptor
            .supported_asset_types
            .into_iter()
            .map(contract_asset_type)
            .collect(),
        trust_level: match descriptor.trust_level {
            MarketplaceSourceTrustLevel::Curated => MarketplaceSourceTrustLevelDto::Curated,
            MarketplaceSourceTrustLevel::Organization => {
                MarketplaceSourceTrustLevelDto::Organization
            }
            MarketplaceSourceTrustLevel::PublicIndex => MarketplaceSourceTrustLevelDto::PublicIndex,
        },
        enabled: descriptor.enabled,
    }
}

fn external_asset_detail_response(
    detail: MarketplaceAssetDetail,
) -> ExternalMarketplaceAssetDetailDto {
    ExternalMarketplaceAssetDetailDto {
        listing: external_asset_listing_response(detail.listing),
        detail_markdown: detail.detail_markdown,
        homepage_url: detail.homepage_url,
        repository_url: detail.repository_url,
    }
}

fn external_asset_listing_response(
    listing: MarketplaceAssetListing,
) -> ExternalMarketplaceAssetListingDto {
    ExternalMarketplaceAssetListingDto {
        source_key: listing.source_key,
        external_id: listing.external_id,
        asset_type: contract_asset_type(listing.asset_type),
        key: listing.key,
        display_name: listing.display_name,
        description: listing.description,
        version: listing.version,
        tags: listing.tags,
        author: listing.author,
        digest: listing.digest,
        updated_at: listing.updated_at.map(|value| value.to_rfc3339()),
        install_requirements: listing
            .install_requirements
            .into_iter()
            .map(|requirement| ExternalMarketplaceInstallRequirementDto {
                kind: match requirement.kind {
                    MarketplaceInstallRequirementKind::EnvVar => {
                        MarketplaceInstallRequirementKindDto::EnvVar
                    }
                    MarketplaceInstallRequirementKind::Secret => {
                        MarketplaceInstallRequirementKindDto::Secret
                    }
                    MarketplaceInstallRequirementKind::Permission => {
                        MarketplaceInstallRequirementKindDto::Permission
                    }
                    MarketplaceInstallRequirementKind::AssetDependency => {
                        MarketplaceInstallRequirementKindDto::AssetDependency
                    }
                },
                key: requirement.key,
                display_name: requirement.display_name,
                description: requirement.description,
                required: requirement.required,
            })
            .collect(),
    }
}

fn refresh_external_marketplace_asset_response(
    output: RefreshExternalMarketplaceAssetOutput,
) -> RefreshExternalMarketplaceAssetResponse {
    RefreshExternalMarketplaceAssetResponse {
        source_ref: output.source_ref,
        remote_version: output.remote_version,
        remote_digest: output.remote_digest,
        local_version: output.local_version,
        local_digest: output.local_digest,
        status: match output.status {
            ApplicationRefreshStatus::UpToDate => ExternalMarketplaceRefreshStatus::UpToDate,
            ApplicationRefreshStatus::UpdateAvailable => {
                ExternalMarketplaceRefreshStatus::UpdateAvailable
            }
            ApplicationRefreshStatus::SourceMissing => {
                ExternalMarketplaceRefreshStatus::SourceMissing
            }
            ApplicationRefreshStatus::NotImported => ExternalMarketplaceRefreshStatus::NotImported,
        },
    }
}

fn provider_error_to_api(error: MarketplaceSourceError) -> ApiError {
    match error {
        MarketplaceSourceError::BadRequest(message) => ApiError::BadRequest(message),
        MarketplaceSourceError::NotFound { .. } => ApiError::NotFound(error.to_string()),
        MarketplaceSourceError::Unavailable(message) => ApiError::ServiceUnavailable(message),
        MarketplaceSourceError::Internal(err) => {
            let context =
                DiagnosticErrorContext::new("marketplace_source.route", "provider_internal");
            diag_error!(
                Error,
                Subsystem::Api,
                context = &context,
                error = &err,
                route = "marketplace_source",
                "marketplace source provider internal error"
            );
            ApiError::Internal("Marketplace source 内部错误".to_string())
        }
    }
}

fn normalize_optional(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_required_api(field: &str, value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::shared_library::{
        LibraryAsset, LibraryAssetListFilter, LibraryAssetPayload, LibraryAssetSource,
        SkillTemplatePayload,
    };
    use agentdash_integration_api::{MarketplaceFetchedAsset, MarketplaceFetchedAssetPayload};
    use async_trait::async_trait;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct InMemoryLibraryAssetRepository {
        assets: Mutex<Vec<LibraryAsset>>,
    }

    #[async_trait]
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

    struct TestMarketplaceSourceProvider {
        descriptor: MarketplaceSourceDescriptor,
        list_result: Mutex<Result<MarketplaceAssetPage, MarketplaceSourceError>>,
        detail_result: Mutex<Result<MarketplaceAssetDetail, MarketplaceSourceError>>,
        fetch_result: Mutex<Result<MarketplaceFetchedAsset, MarketplaceSourceError>>,
        last_query: Mutex<Option<MarketplaceAssetQuery>>,
    }

    impl TestMarketplaceSourceProvider {
        fn new(descriptor: MarketplaceSourceDescriptor) -> Self {
            let source_key = descriptor.source_key.clone();
            Self {
                descriptor,
                list_result: Mutex::new(Ok(MarketplaceAssetPage {
                    items: vec![skill_listing()],
                    next_cursor: Some("next-page".to_string()),
                })),
                detail_result: Mutex::new(Ok(MarketplaceAssetDetail {
                    listing: skill_listing(),
                    detail_markdown: Some("details".to_string()),
                    homepage_url: None,
                    repository_url: None,
                })),
                fetch_result: Mutex::new(Err(MarketplaceSourceError::NotFound {
                    source_key,
                    external_id: "missing".to_string(),
                })),
                last_query: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl MarketplaceSourceProvider for TestMarketplaceSourceProvider {
        fn descriptor(&self) -> MarketplaceSourceDescriptor {
            self.descriptor.clone()
        }

        async fn list_assets(
            &self,
            query: MarketplaceAssetQuery,
        ) -> Result<MarketplaceAssetPage, MarketplaceSourceError> {
            *self.last_query.lock().expect("lock") = Some(query);
            self.list_result.lock().expect("lock").clone()
        }

        async fn get_asset_detail(
            &self,
            _external_id: &str,
        ) -> Result<MarketplaceAssetDetail, MarketplaceSourceError> {
            self.detail_result.lock().expect("lock").clone()
        }

        async fn fetch_asset_payload(
            &self,
            _external_id: &str,
        ) -> Result<MarketplaceFetchedAsset, MarketplaceSourceError> {
            self.fetch_result.lock().expect("lock").clone()
        }
    }

    #[tokio::test]
    async fn marketplace_list_external_assets_passes_query_to_source_provider() {
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider.clone()];

        let page = list_external_assets_from_providers(
            &providers,
            ListExternalMarketplaceAssetsQuery {
                source_key: Some("test.source".to_string()),
                asset_type: Some("skill_template".to_string()),
                query: Some(" search ".to_string()),
                cursor: Some("cursor-1".to_string()),
                limit: Some(25),
            },
        )
        .await
        .expect("list succeeds");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.next_cursor.as_deref(), Some("next-page"));
        let query = provider
            .last_query
            .lock()
            .expect("lock")
            .clone()
            .expect("query captured");
        assert_eq!(query.asset_type, Some(LibraryAssetType::SkillTemplate));
        assert_eq!(query.query.as_deref(), Some("search"));
        assert_eq!(query.cursor.as_deref(), Some("cursor-1"));
        assert_eq!(query.limit, Some(25));
    }

    #[tokio::test]
    async fn marketplace_list_external_assets_rejects_cursor_without_source_key() {
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let error = list_external_assets_from_providers(
            &providers,
            ListExternalMarketplaceAssetsQuery {
                cursor: Some("cursor-1".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("cursor without source rejected");

        assert!(matches!(error, ApiError::BadRequest(_)));
    }

    #[tokio::test]
    async fn marketplace_list_external_assets_maps_unknown_source_to_not_found() {
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let error = list_external_assets_from_providers(
            &providers,
            ListExternalMarketplaceAssetsQuery {
                source_key: Some("missing.source".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("unknown source rejected");

        assert!(matches!(error, ApiError::NotFound(_)));
    }

    #[tokio::test]
    async fn marketplace_detail_maps_provider_not_found_to_not_found() {
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        *provider.detail_result.lock().expect("lock") = Err(MarketplaceSourceError::NotFound {
            source_key: "test.source".to_string(),
            external_id: "missing".to_string(),
        });
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let error = get_external_asset_detail_from_provider(&providers, "test.source", "missing")
            .await
            .expect_err("provider not found maps to 404");

        assert!(matches!(error, ApiError::NotFound(_)));
    }

    #[tokio::test]
    async fn marketplace_list_external_assets_rejects_provider_asset_type_mismatch() {
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        *provider.list_result.lock().expect("lock") = Ok(MarketplaceAssetPage {
            items: vec![MarketplaceAssetListing {
                asset_type: LibraryAssetType::McpServerTemplate,
                ..skill_listing()
            }],
            next_cursor: None,
        });
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let error = list_external_assets_from_providers(
            &providers,
            ListExternalMarketplaceAssetsQuery {
                source_key: Some("test.source".to_string()),
                asset_type: Some("skill_template".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("mismatch rejected");

        assert!(matches!(error, ApiError::BadRequest(_)));
    }

    #[tokio::test]
    async fn marketplace_import_external_asset_writes_remote_imported_library_asset() {
        let repo = InMemoryLibraryAssetRepository::default();
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        *provider.fetch_result.lock().expect("lock") =
            Ok(fetched_skill(valid_skill_template_payload()));
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let response = import_external_marketplace_asset_for_request(
            &repo,
            &providers,
            "user-1",
            import_request(),
        )
        .await
        .expect("import succeeds");

        assert_eq!(
            response.asset.source_ref.as_deref(),
            Some("market:test.source:skill_template:skill-1")
        );
        let assets = repo.assets.lock().expect("lock");
        assert_eq!(assets.len(), 1);
        let asset = &assets[0];
        assert_eq!(asset.source, LibraryAssetSource::RemoteImported);
        assert_eq!(
            asset.source_ref.as_deref(),
            Some("market:test.source:skill_template:skill-1")
        );
        assert_eq!(asset.scope, LibraryAssetScope::User);
        assert_eq!(asset.owner_id.as_deref(), Some("user-1"));
        assert!(asset.payload_digest.starts_with("sha256:"));
        assert!(matches!(
            asset.typed_payload().expect("typed payload"),
            LibraryAssetPayload::SkillTemplate(SkillTemplatePayload { .. })
        ));
    }

    #[tokio::test]
    async fn marketplace_import_external_asset_maps_invalid_payload_to_bad_request() {
        let repo = InMemoryLibraryAssetRepository::default();
        let provider = Arc::new(TestMarketplaceSourceProvider::new(test_descriptor()));
        *provider.fetch_result.lock().expect("lock") =
            Ok(fetched_skill(json!({"files": "not-array"})));
        let providers: Vec<Arc<dyn MarketplaceSourceProvider>> = vec![provider];

        let error = import_external_marketplace_asset_for_request(
            &repo,
            &providers,
            "user-1",
            import_request(),
        )
        .await
        .expect_err("invalid payload maps to bad request");

        assert!(matches!(error, ApiError::BadRequest(message) if message.contains("payload")));
    }

    fn test_descriptor() -> MarketplaceSourceDescriptor {
        MarketplaceSourceDescriptor {
            source_key: "test.source".to_string(),
            display_name: "Test Source".to_string(),
            description: Some("Test marketplace source".to_string()),
            provider_kind: MarketplaceSourceProviderKind::Integration,
            supported_asset_types: vec![
                LibraryAssetType::SkillTemplate,
                LibraryAssetType::McpServerTemplate,
            ],
            trust_level: MarketplaceSourceTrustLevel::Organization,
            enabled: true,
        }
    }

    fn skill_listing() -> MarketplaceAssetListing {
        MarketplaceAssetListing {
            source_key: "test.source".to_string(),
            external_id: "skill-1".to_string(),
            asset_type: LibraryAssetType::SkillTemplate,
            key: "skill-1".to_string(),
            display_name: "Skill One".to_string(),
            description: Some("A skill".to_string()),
            version: "0.1.0".to_string(),
            tags: vec!["skill".to_string()],
            author: Some("AgentDash".to_string()),
            digest: Some("sha256:remote".to_string()),
            updated_at: None,
            install_requirements: vec![],
        }
    }

    fn import_request() -> ImportExternalMarketplaceAssetRequest {
        ImportExternalMarketplaceAssetRequest {
            source_key: "test.source".to_string(),
            external_id: "skill-1".to_string(),
            asset_type: "skill_template".to_string(),
            import_mode: UPSERT_LIBRARY_ASSET_IMPORT_MODE.to_string(),
        }
    }

    fn fetched_skill(payload: serde_json::Value) -> MarketplaceFetchedAsset {
        MarketplaceFetchedAsset::SkillTemplate(MarketplaceFetchedAssetPayload {
            source_key: "test.source".to_string(),
            external_id: "skill-1".to_string(),
            key: "skill-1".to_string(),
            display_name: "Skill One".to_string(),
            description: Some("A skill".to_string()),
            version: "0.1.0".to_string(),
            digest: Some("sha256:remote".to_string()),
            payload,
        })
    }

    fn valid_skill_template_payload() -> serde_json::Value {
        json!({
            "files": [{
                "path": "SKILL.md",
                "content": "Use this skill.",
                "kind": "skill"
            }],
            "disable_model_invocation": true
        })
    }
}
