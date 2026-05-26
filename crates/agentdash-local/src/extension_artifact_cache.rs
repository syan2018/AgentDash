use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use reqwest::header;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ExtensionArtifactDownloadRequest {
    pub api_base_url: String,
    pub access_token: String,
    pub project_id: String,
    pub artifact_id: String,
    pub archive_digest: String,
    pub cache_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionArtifactCacheEntry {
    pub cache_key: String,
    pub archive_path: PathBuf,
    pub unpacked_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum ExtensionArtifactCacheError {
    #[error("extension artifact 请求非法: {0}")]
    InvalidRequest(String),
    #[error("extension artifact 下载失败: {0}")]
    Download(String),
    #[error("extension artifact digest 不匹配: expected {expected}, actual {actual}")]
    DigestMismatch { expected: String, actual: String },
    #[error("extension artifact 缓存 I/O 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("extension artifact archive 解析失败: {0}")]
    Archive(String),
    #[error("extension artifact cache manifest 解析失败: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheManifest {
    artifact_id: String,
    archive_digest: String,
}

pub async fn download_and_cache_extension_artifact(
    request: ExtensionArtifactDownloadRequest,
) -> Result<ExtensionArtifactCacheEntry, ExtensionArtifactCacheError> {
    validate_sha256_digest(&request.archive_digest)?;
    let cache_key = cache_key(&request.artifact_id, &request.archive_digest)?;
    let cache_dir = request
        .cache_root
        .join("extension-artifacts")
        .join(&cache_key);
    let archive_path = cache_dir.join("archive.agentdash-extension.tgz");
    let unpacked_dir = cache_dir.join("package");
    let manifest_path = cache_dir.join(".agentdash-extension-cache.json");

    if cache_manifest_matches(
        &manifest_path,
        &archive_path,
        &unpacked_dir,
        &request.artifact_id,
        &request.archive_digest,
    )
    .await
    {
        return Ok(ExtensionArtifactCacheEntry {
            cache_key,
            archive_path,
            unpacked_dir,
        });
    }

    let bytes = download_archive(&request).await?;
    let actual = digest_bytes(&bytes);
    if actual != request.archive_digest {
        return Err(ExtensionArtifactCacheError::DigestMismatch {
            expected: request.archive_digest,
            actual,
        });
    }

    tokio::fs::create_dir_all(&cache_dir).await?;
    tokio::fs::write(&archive_path, &bytes).await?;
    replace_unpacked_dir(&unpacked_dir).await?;
    unpack_tgz_bytes(&bytes, &unpacked_dir).await?;

    let manifest = CacheManifest {
        artifact_id: request.artifact_id,
        archive_digest: request.archive_digest,
    };
    tokio::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?).await?;

    Ok(ExtensionArtifactCacheEntry {
        cache_key,
        archive_path,
        unpacked_dir,
    })
}

async fn download_archive(
    request: &ExtensionArtifactDownloadRequest,
) -> Result<Vec<u8>, ExtensionArtifactCacheError> {
    let base = request.api_base_url.trim_end_matches('/');
    if base.is_empty() {
        return Err(ExtensionArtifactCacheError::InvalidRequest(
            "api_base_url 不能为空".to_string(),
        ));
    }
    let url = format!(
        "{base}/api/projects/{}/extension-artifacts/{}/archive",
        url_escape(&request.project_id),
        url_escape(&request.artifact_id)
    );
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(&request.access_token)
        .header(header::ACCEPT, "application/vnd.agentdash.extension+gzip")
        .send()
        .await
        .map_err(|error| ExtensionArtifactCacheError::Download(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ExtensionArtifactCacheError::Download(format!(
            "HTTP {status}"
        )));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| ExtensionArtifactCacheError::Download(error.to_string()))
}

async fn cache_manifest_matches(
    manifest_path: &Path,
    archive_path: &Path,
    unpacked_dir: &Path,
    artifact_id: &str,
    archive_digest: &str,
) -> bool {
    if !cache_files_exist(archive_path, unpacked_dir).await {
        return false;
    }
    let Ok(bytes) = tokio::fs::read(manifest_path).await else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<CacheManifest>(&bytes) else {
        return false;
    };
    manifest.artifact_id == artifact_id && manifest.archive_digest == archive_digest
}

async fn cache_files_exist(archive_path: &Path, unpacked_dir: &Path) -> bool {
    let archive_exists = tokio::fs::try_exists(archive_path).await.unwrap_or(false);
    let unpacked_exists = tokio::fs::metadata(unpacked_dir)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    archive_exists && unpacked_exists
}

async fn replace_unpacked_dir(path: &Path) -> Result<(), ExtensionArtifactCacheError> {
    if tokio::fs::try_exists(path).await? {
        tokio::fs::remove_dir_all(path).await?;
    }
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

async fn unpack_tgz_bytes(
    bytes: &[u8],
    target_dir: &Path,
) -> Result<(), ExtensionArtifactCacheError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| ExtensionArtifactCacheError::Archive(error.to_string()))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| ExtensionArtifactCacheError::Archive(error.to_string()))?;
        let path = normalize_archive_path(&entry.path().map_err(|error| {
            ExtensionArtifactCacheError::Archive(format!("条目路径非法: {error}"))
        })?)?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            tokio::fs::create_dir_all(target_dir.join(&path)).await?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(ExtensionArtifactCacheError::Archive(format!(
                "包含非普通文件条目: {}",
                path.display()
            )));
        }
        let destination = target_dir.join(&path);
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = tokio::fs::File::create(&destination).await?;
        let mut std_file = file.into_std().await;
        std::io::copy(&mut entry, &mut std_file)?;
    }
    Ok(())
}

fn normalize_archive_path(path: &Path) -> Result<PathBuf, ExtensionArtifactCacheError> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ExtensionArtifactCacheError::Archive(format!(
                    "不安全路径: {}",
                    path.display()
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(ExtensionArtifactCacheError::Archive(
            "archive 包含空路径".to_string(),
        ));
    }
    Ok(normalized)
}

fn cache_key(
    artifact_id: &str,
    archive_digest: &str,
) -> Result<String, ExtensionArtifactCacheError> {
    if artifact_id.trim().is_empty() {
        return Err(ExtensionArtifactCacheError::InvalidRequest(
            "artifact_id 不能为空".to_string(),
        ));
    }
    let digest = archive_digest.strip_prefix("sha256:").ok_or_else(|| {
        ExtensionArtifactCacheError::InvalidRequest("archive_digest 非法".to_string())
    })?;
    Ok(format!("{}-{digest}", sanitize_path_segment(artifact_id)))
}

fn validate_sha256_digest(value: &str) -> Result<(), ExtensionArtifactCacheError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ExtensionArtifactCacheError::InvalidRequest(
            "archive_digest 必须使用 sha256:<hex> 格式".to_string(),
        ));
    };
    if hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ExtensionArtifactCacheError::InvalidRequest(
            "archive_digest 必须包含 64 位 sha256 十六进制摘要".to_string(),
        ))
    }
}

fn sanitize_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn url_escape(raw: &str) -> String {
    raw.replace('%', "%25")
        .replace('/', "%2F")
        .replace('\\', "%5C")
        .replace('?', "%3F")
        .replace('#', "%23")
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    use super::*;

    fn archive_bytes(path: &str, content: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            let mut header = Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, content)
                .expect("append");
            builder.finish().expect("finish tar");
        }
        encoder.finish().expect("finish gzip")
    }

    #[tokio::test]
    async fn unpacks_safe_archive_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let bytes = archive_bytes("dist/extension.js", b"hello");
        unpack_tgz_bytes(&bytes, temp.path()).await.expect("unpack");
        let unpacked = tokio::fs::read_to_string(temp.path().join("dist/extension.js"))
            .await
            .expect("read");
        assert_eq!(unpacked, "hello");
    }

    #[tokio::test]
    async fn cache_manifest_requires_archive_and_unpacked_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest_path = temp.path().join(".agentdash-extension-cache.json");
        let archive_path = temp.path().join("archive.agentdash-extension.tgz");
        let unpacked_dir = temp.path().join("package");
        tokio::fs::write(
            &manifest_path,
            serde_json::to_vec(&CacheManifest {
                artifact_id: "artifact-1".to_string(),
                archive_digest:
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
            })
            .expect("manifest"),
        )
        .await
        .expect("write manifest");

        assert!(
            !cache_manifest_matches(
                &manifest_path,
                &archive_path,
                &unpacked_dir,
                "artifact-1",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .await
        );

        tokio::fs::write(&archive_path, b"archive")
            .await
            .expect("write archive");
        tokio::fs::create_dir_all(&unpacked_dir)
            .await
            .expect("create package dir");
        assert!(
            cache_manifest_matches(
                &manifest_path,
                &archive_path,
                &unpacked_dir,
                "artifact-1",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .await
        );
    }

    #[test]
    fn rejects_digest_shape() {
        let err = validate_sha256_digest("sha256:bad").expect_err("invalid");
        assert!(err.to_string().contains("64 位"));
    }

    #[test]
    fn cache_key_uses_digest_hex() {
        let key = cache_key(
            "artifact/one",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("key");
        assert_eq!(
            key,
            "artifact_one-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }
}
