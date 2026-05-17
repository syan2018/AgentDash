//! Shared Library HTTP 路由——公共资产查询与 builtin seed 入口。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::shared_library::{SeedBuiltinLibraryAssetsInput, SharedLibraryService};
use agentdash_domain::shared_library::{
    LibraryAssetListFilter, LibraryAssetScope, LibraryAssetType,
};

use crate::app_state::AppState;
use crate::auth::CurrentUser;
use crate::dto::{
    LibraryAssetResponse, ListLibraryAssetsQuery, SeedBuiltinLibraryAssetsRequest,
    parse_asset_scope, parse_asset_type,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct LibraryAssetPath {
    pub id: String,
}

/// GET `/api/shared-library/assets`
pub async fn list_library_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Query(query): Query<ListLibraryAssetsQuery>,
) -> Result<Json<Vec<LibraryAssetResponse>>, ApiError> {
    let filter = LibraryAssetListFilter {
        asset_type: parse_optional_asset_type(query.asset_type)?,
        scope: parse_optional_scope(query.scope)?,
        owner_id: query.owner_id.filter(|value| !value.trim().is_empty()),
        include_deprecated: query.include_deprecated,
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.list(filter).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

/// GET `/api/shared-library/assets/:id`
pub async fn get_library_asset(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Path(path): Path<LibraryAssetPath>,
) -> Result<Json<LibraryAssetResponse>, ApiError> {
    let id = parse_library_asset_id(&path.id)?;
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    Ok(Json(service.get(id).await?.into()))
}

/// POST `/api/shared-library/assets/seed-builtin`
pub async fn seed_builtin_library_assets(
    State(state): State<Arc<AppState>>,
    CurrentUser(_current_user): CurrentUser,
    Json(req): Json<SeedBuiltinLibraryAssetsRequest>,
) -> Result<Json<Vec<LibraryAssetResponse>>, ApiError> {
    let input = SeedBuiltinLibraryAssetsInput {
        asset_type: parse_optional_asset_type(req.asset_type)?,
        key: req.key.filter(|value| !value.trim().is_empty()),
    };
    let service = SharedLibraryService::new(state.repos.shared_library_repo.as_ref());
    let assets = service.seed_builtin_assets(input).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

fn parse_library_asset_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 LibraryAsset ID".into()))
}

fn parse_optional_asset_type(raw: Option<String>) -> Result<Option<LibraryAssetType>, ApiError> {
    raw.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_asset_type)
        .transpose()
        .map_err(ApiError::BadRequest)
}

fn parse_optional_scope(raw: Option<String>) -> Result<Option<LibraryAssetScope>, ApiError> {
    raw.as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_asset_scope)
        .transpose()
        .map_err(ApiError::BadRequest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_library_asset_id_rejects_invalid() {
        let err = parse_library_asset_id("bad").expect_err("invalid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn parse_optional_asset_type_accepts_known_type() {
        let parsed = parse_optional_asset_type(Some("agent_template".to_string()))
            .expect("parse")
            .expect("some");
        assert_eq!(parsed, LibraryAssetType::AgentTemplate);
    }

    #[test]
    fn parse_optional_asset_type_rejects_unknown_type() {
        let err = parse_optional_asset_type(Some("catalog".to_string())).expect_err("invalid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
