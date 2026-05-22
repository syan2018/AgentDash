use std::sync::Arc;

use agentdash_application::vfs::{PROVIDER_INLINE_FS, RuntimeFileEntry, VfsMutationError};
use agentdash_spi::Vfs;

use crate::{app_state::AppState, rpc::ApiError};

use super::dto::SurfaceStatFileResponse;

pub(super) fn ensure_not_skill_asset_document_path(
    source: &agentdash_application::vfs::ResolvedVfsSurfaceSource,
    path: &str,
    operation: &str,
) -> Result<(), ApiError> {
    if matches!(
        source,
        agentdash_application::vfs::ResolvedVfsSurfaceSource::ProjectSkillAssets { .. }
    ) && skill_asset_relative_path(path) == Some("SKILL.md")
    {
        return Err(ApiError::BadRequest(format!(
            "不能通过 VFS {operation} Skill 主文档 SKILL.md"
        )));
    }
    Ok(())
}

fn skill_asset_relative_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("skills/")?;
    let (_key, relative_path) = rest.split_once('/')?;
    Some(relative_path)
}

pub(super) fn api_error_from_vfs_mutation(error: VfsMutationError) -> ApiError {
    match error {
        VfsMutationError::BadRequest(message) | VfsMutationError::InvalidMount(message) => {
            ApiError::BadRequest(message)
        }
        VfsMutationError::NotFound(message) => ApiError::NotFound(message),
        VfsMutationError::Conflict(message) => ApiError::Conflict(message),
        VfsMutationError::Provider(message) | VfsMutationError::Internal(message) => {
            ApiError::Internal(message)
        }
    }
}

pub(super) fn surface_stat_response(
    surface_ref: String,
    mount_id: String,
    entry: RuntimeFileEntry,
) -> SurfaceStatFileResponse {
    SurfaceStatFileResponse {
        surface_ref,
        mount_id,
        content_kind: entry_content_kind(&entry),
        mime_type: entry_mime_type(&entry),
        path: entry.path,
        entry_type: if entry.is_dir {
            "directory".to_string()
        } else {
            "file".to_string()
        },
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
    }
}

pub(super) fn entry_content_kind(entry: &RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("content_kind"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

pub(super) fn entry_mime_type(entry: &RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("mime_type"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

pub(super) fn guess_image_mime_type(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

pub(super) async fn check_mount_available(
    state: &Arc<AppState>,
    vfs: &Vfs,
    mount_id: &str,
) -> Result<(), ApiError> {
    if let Some(mount) = vfs.mounts.iter().find(|mount| mount.id == mount_id)
        && let Some(provider) = state.services.mount_provider_registry.get(&mount.provider)
        && mount.provider != PROVIDER_INLINE_FS
        && !provider.is_available(mount).await
    {
        return Err(ApiError::ServiceUnavailable(format!(
            "挂载点 \"{}\" 的 Backend 当前不在线（{}），无法浏览文件。请确认 Backend 已连接。",
            mount.display_name, mount.backend_id,
        )));
    }
    Ok(())
}
