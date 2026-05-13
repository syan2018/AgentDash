use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use agentdash_relay::{
    MaterializationAccessMode, MaterializationCacheScope, MaterializationTargetKind,
    VfsMaterializeContent, VfsMaterializePayload, VfsMaterializeResponse,
};
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::resource_server::ResourceServer;

#[derive(Debug, thiserror::Error)]
pub enum MaterializationError {
    #[error("物化请求没有包含任何资源 entry")]
    EmptyEntries,

    #[error("物化路径非法: {0}")]
    InvalidPath(String),

    #[error("物化内容大小不匹配: {path} expected={expected} actual={actual}")]
    SizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },

    #[error("物化内容 digest 不匹配: {path}")]
    DigestMismatch { path: String },

    #[error("物化内容解码失败: {0}")]
    Decode(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct MaterializationStore {
    readonly_root: PathBuf,
    workdir_root: PathBuf,
    max_entry_bytes: u64,
    max_total_bytes: u64,
    resource_server: ResourceServer,
}

impl MaterializationStore {
    pub fn new(backend_id: impl AsRef<str>) -> Self {
        let backend_key = clean_component(backend_id.as_ref());
        Self {
            readonly_root: std::env::temp_dir()
                .join("agentdash")
                .join("materialized")
                .join(&backend_key),
            workdir_root: local_data_root()
                .join("agentdash")
                .join("materialized-workdirs")
                .join(backend_key),
            max_entry_bytes: 8 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            resource_server: ResourceServer::start()
                .expect("materialized resource server must bind to 127.0.0.1"),
        }
    }

    #[cfg(test)]
    fn new_for_test(readonly_root: PathBuf, workdir_root: PathBuf) -> Self {
        Self {
            readonly_root,
            workdir_root,
            max_entry_bytes: 8 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            resource_server: ResourceServer::start()
                .expect("materialized resource server must bind to 127.0.0.1"),
        }
    }

    pub async fn materialize(
        &self,
        payload: VfsMaterializePayload,
    ) -> Result<VfsMaterializeResponse, MaterializationError> {
        if payload.entries.is_empty() {
            return Err(MaterializationError::EmptyEntries);
        }

        let prepared = prepare_entries(&payload, self.max_entry_bytes, self.max_total_bytes)?;
        let manifest_digest = manifest_digest(&payload, &prepared);
        let local_root = match payload.cache_scope {
            MaterializationCacheScope::Session => self
                .readonly_root
                .join(clean_component(&payload.session_id))
                .join(clean_component(&payload.plan_id))
                .join("content"),
            MaterializationCacheScope::PersistentWorkingCopy => self
                .workdir_root
                .join(resource_key(&payload))
                .join("content"),
        };

        ensure_inside_any_root(&local_root, &[&self.readonly_root, &self.workdir_root])?;
        let manifest_path = local_root
            .parent()
            .unwrap_or(local_root.as_path())
            .join("manifest.json");
        let cache_hit = existing_manifest_matches(&manifest_path, &manifest_digest).await;

        if !cache_hit {
            if let Some(parent) = local_root.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            if local_root.exists() {
                tokio::fs::remove_dir_all(&local_root).await?;
            }
            tokio::fs::create_dir_all(&local_root).await?;

            for entry in &prepared {
                let full_path = local_root.join(&entry.relative_path);
                ensure_inside(&full_path, &local_root)?;
                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&full_path, &entry.bytes).await?;
                set_file_mode(&full_path, payload.access_mode, entry.executable_hint)?;
            }

            let manifest = serde_json::json!({
                "manifest_digest": manifest_digest,
                "source_uri": payload.source_uri,
                "root_uri": payload.root_uri,
                "mount_id": payload.mount_id,
                "provider": payload.provider,
                "plan_kind": payload.plan_kind,
                "target_kind": payload.target_kind,
                "access_mode": payload.access_mode,
                "entry_count": prepared.len(),
                "total_size_bytes": prepared.iter().map(|entry| entry.size_bytes).sum::<u64>(),
                "written_at": chrono::Utc::now().to_rfc3339(),
            });
            tokio::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?).await?;
        }

        let primary_relative = primary_relative_path(&payload)?;
        let primary_local_path = match payload.target_kind {
            MaterializationTargetKind::Directory if primary_relative.as_os_str().is_empty() => {
                local_root.clone()
            }
            _ => local_root.join(primary_relative),
        };
        ensure_inside(&primary_local_path, &local_root)?;
        let primary_local_url = {
            let mut url = self
                .resource_server
                .register_root(resource_token(&manifest_digest), local_root.clone())
                .await;
            url.push_str(&url_path_suffix(&primary_local_path, &local_root)?);
            Some(url)
        };

        let total_size_bytes = prepared.iter().map(|entry| entry.size_bytes).sum();
        tracing::info!(
            source_uri = %payload.source_uri,
            root_uri = %payload.root_uri,
            local_root = %local_root.display(),
            primary_local_path = %primary_local_path.display(),
            entry_count = prepared.len(),
            total_size_bytes,
            cache_hit,
            "VFS 资源已物化到本机"
        );

        Ok(VfsMaterializeResponse {
            source_uri: payload.source_uri,
            local_root_path: local_root.to_string_lossy().to_string(),
            primary_local_path: primary_local_path.to_string_lossy().to_string(),
            primary_local_url,
            access_mode: payload.access_mode,
            manifest_digest,
            total_size_bytes,
            entry_count: prepared.len(),
            dirty: false,
            cache_hit,
        })
    }
}

#[derive(Debug)]
struct PreparedEntry {
    relative_path: PathBuf,
    bytes: Vec<u8>,
    size_bytes: u64,
    executable_hint: bool,
}

fn prepare_entries(
    payload: &VfsMaterializePayload,
    max_entry_bytes: u64,
    max_total_bytes: u64,
) -> Result<Vec<PreparedEntry>, MaterializationError> {
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut entries = Vec::with_capacity(payload.entries.len());

    for entry in &payload.entries {
        let relative_path = safe_relative_path(&entry.relative_path)?;
        let normalized_key = relative_path.to_string_lossy().replace('\\', "/");
        if !seen.insert(normalized_key.clone()) {
            return Err(MaterializationError::InvalidPath(format!(
                "重复 entry: {}",
                entry.relative_path
            )));
        }

        let bytes = match &entry.content {
            VfsMaterializeContent::Utf8Text { text } => text.as_bytes().to_vec(),
            VfsMaterializeContent::Base64Bytes { data } => {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|e| MaterializationError::Decode(e.to_string()))?
            }
        };
        let actual_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_size != entry.size_bytes {
            return Err(MaterializationError::SizeMismatch {
                path: entry.relative_path.clone(),
                expected: entry.size_bytes,
                actual: actual_size,
            });
        }
        if actual_size > max_entry_bytes {
            return Err(MaterializationError::InvalidPath(format!(
                "{} 超过单文件物化大小限制",
                entry.relative_path
            )));
        }
        total = total.saturating_add(actual_size);
        if total > max_total_bytes {
            return Err(MaterializationError::InvalidPath(
                "物化请求超过总大小限制".to_string(),
            ));
        }

        let actual_digest = format!("sha256:{}", sha256_hex(&bytes));
        if normalize_digest(&entry.digest) != actual_digest {
            return Err(MaterializationError::DigestMismatch {
                path: entry.relative_path.clone(),
            });
        }

        entries.push(PreparedEntry {
            relative_path,
            bytes,
            size_bytes: actual_size,
            executable_hint: entry.executable_hint,
        });
    }

    Ok(entries)
}

fn primary_relative_path(payload: &VfsMaterializePayload) -> Result<PathBuf, MaterializationError> {
    let trimmed = payload.primary_relative_path.trim();
    if matches!(payload.target_kind, MaterializationTargetKind::Directory)
        && (trimmed.is_empty() || trimmed == ".")
    {
        return Ok(PathBuf::new());
    }
    safe_relative_path(trimmed)
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, MaterializationError> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed == "." {
        return Err(MaterializationError::InvalidPath(raw.to_string()));
    }
    if trimmed.starts_with('/')
        || trimmed.starts_with("//")
        || looks_like_windows_absolute(&trimmed)
    {
        return Err(MaterializationError::InvalidPath(raw.to_string()));
    }

    let mut path = PathBuf::new();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::Normal(segment) => path.push(segment),
            _ => return Err(MaterializationError::InvalidPath(raw.to_string())),
        }
    }
    if path.as_os_str().is_empty() {
        return Err(MaterializationError::InvalidPath(raw.to_string()));
    }
    Ok(path)
}

fn ensure_inside(path: &Path, root: &Path) -> Result<(), MaterializationError> {
    if !path.starts_with(root) {
        return Err(MaterializationError::InvalidPath(format!(
            "{} 不在 {} 内",
            path.display(),
            root.display()
        )));
    }
    Ok(())
}

fn ensure_inside_any_root(path: &Path, roots: &[&Path]) -> Result<(), MaterializationError> {
    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(MaterializationError::InvalidPath(
            path.display().to_string(),
        ))
    }
}

async fn existing_manifest_matches(path: &Path, digest: &str) -> bool {
    let Ok(bytes) = tokio::fs::read(path).await else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    value
        .get("manifest_digest")
        .and_then(|value| value.as_str())
        .is_some_and(|existing| existing == digest)
}

fn manifest_digest(payload: &VfsMaterializePayload, entries: &[PreparedEntry]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.source_uri.as_bytes());
    hasher.update([0]);
    hasher.update(payload.root_uri.as_bytes());
    hasher.update([0]);
    hasher.update(format!("{:?}", payload.plan_kind).as_bytes());
    hasher.update([0]);
    for entry in entries {
        hasher.update(entry.relative_path.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(sha256_hex(&entry.bytes).as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

fn resource_key(payload: &VfsMaterializePayload) -> String {
    let seed = format!(
        "{}\0{}\0{}\0{}",
        payload.mount_id, payload.provider, payload.root_uri, payload.source_uri
    );
    sha256_hex(seed.as_bytes())
}

fn resource_token(manifest_digest: &str) -> String {
    manifest_digest
        .trim_start_matches("sha256:")
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(32)
        .collect()
}

fn url_path_suffix(path: &Path, root: &Path) -> Result<String, MaterializationError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        MaterializationError::InvalidPath(format!("{} 不在 {} 内", path.display(), root.display()))
    })?;
    if relative.as_os_str().is_empty() {
        return Ok(String::new());
    }
    let mut suffix = String::new();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(MaterializationError::InvalidPath(
                relative.display().to_string(),
            ));
        };
        suffix.push('/');
        suffix.push_str(&url_encode_path_segment(&segment.to_string_lossy()));
    }
    Ok(suffix)
}

fn set_file_mode(
    path: &Path,
    access_mode: MaterializationAccessMode,
    executable: bool,
) -> Result<(), MaterializationError> {
    let mut permissions = std::fs::metadata(path)?.permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = match (access_mode, executable) {
            (MaterializationAccessMode::ReadOnly, true) => 0o555,
            (MaterializationAccessMode::ReadOnly, false) => 0o444,
            (MaterializationAccessMode::WritableLocalCopy, true) => 0o755,
            (MaterializationAccessMode::WritableLocalCopy, false) => 0o644,
        };
        permissions.set_mode(mode);
    }

    #[cfg(not(unix))]
    let _ = executable;

    #[cfg(not(unix))]
    if matches!(access_mode, MaterializationAccessMode::ReadOnly) {
        permissions.set_readonly(true);
    }

    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

fn url_encode_path_segment(input: &str) -> String {
    let mut out = String::new();
    for byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn clean_component(raw: &str) -> String {
    let cleaned = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn local_data_root() -> PathBuf {
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(value);
    }
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(value);
    }
    if let Some(value) = std::env::var_os("HOME") {
        return PathBuf::from(value).join(".local").join("share");
    }
    std::env::temp_dir()
}

fn looks_like_windows_absolute(path: &str) -> bool {
    path.as_bytes()
        .get(1)
        .zip(path.as_bytes().get(2))
        .is_some_and(|(second, third)| *second == b':' && (*third == b'/' || *third == b'\\'))
}

fn normalize_digest(digest: &str) -> String {
    let trimmed = digest.trim();
    if trimmed.starts_with("sha256:") {
        trimmed.to_string()
    } else {
        format!("sha256:{trimmed}")
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_relay::{MaterializationPlanKind, VfsMaterializeEntry, VfsMaterializePayload};

    fn payload() -> VfsMaterializePayload {
        let text = "echo ok\n";
        VfsMaterializePayload {
            session_id: "session-1".to_string(),
            turn_id: None,
            tool_call_id: None,
            plan_id: "plan-1".to_string(),
            plan_kind: MaterializationPlanKind::SingleFile,
            source_uri: "skill-assets://skills/reviewer/scripts/check.sh".to_string(),
            root_uri: "skill-assets://skills/reviewer/scripts".to_string(),
            mount_id: "skill-assets".to_string(),
            provider: "skill_asset_fs".to_string(),
            primary_relative_path: "check.sh".to_string(),
            target_kind: MaterializationTargetKind::File,
            access_mode: MaterializationAccessMode::ReadOnly,
            entries: vec![VfsMaterializeEntry {
                relative_path: "check.sh".to_string(),
                content: VfsMaterializeContent::Utf8Text {
                    text: text.to_string(),
                },
                digest: format!("sha256:{}", sha256_hex(text.as_bytes())),
                size_bytes: text.len() as u64,
                mime_hint: None,
                executable_hint: true,
            }],
            cache_scope: MaterializationCacheScope::Session,
            ttl_ms: None,
        }
    }

    #[tokio::test]
    async fn materialize_writes_verified_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = MaterializationStore::new_for_test(
            temp.path().join("readonly"),
            temp.path().join("workdirs"),
        );

        let response = store.materialize(payload()).await.expect("materialize");
        assert_eq!(response.entry_count, 1);
        assert!(!response.cache_hit);
        assert!(
            response
                .primary_local_url
                .as_deref()
                .is_some_and(|url| url.starts_with("http://127.0.0.1:")),
            "materialized file should expose a localhost URL"
        );
        let content = tokio::fs::read_to_string(&response.primary_local_path)
            .await
            .expect("read materialized file");
        assert_eq!(content, "echo ok\n");

        let second = store.materialize(payload()).await.expect("cache hit");
        assert!(second.cache_hit);
    }

    #[tokio::test]
    async fn materialize_rejects_path_traversal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = MaterializationStore::new_for_test(
            temp.path().join("readonly"),
            temp.path().join("workdirs"),
        );
        let mut payload = payload();
        payload.entries[0].relative_path = "../escape.sh".to_string();

        let err = store
            .materialize(payload)
            .await
            .expect_err("path traversal must fail");
        assert!(matches!(err, MaterializationError::InvalidPath(_)));
    }
}
