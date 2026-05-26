use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path};

use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::{
    ExtensionPackageArtifact, ExtensionPackageArtifactRef, validate_sha256_digest,
};
use agentdash_domain::shared_library::{ExtensionTemplatePayload, ProjectExtensionInstallation};

use crate::repository_set::RepositorySet;
use crate::shared_library::seed_digest;

const EXTENSION_MANIFEST_PATH: &str = "agentdash.extension.json";
const PACKAGE_JSON_PATH: &str = "package.json";
const INSTALL_LIFECYCLE_SCRIPTS: &[&str] = &["preinstall", "install", "postinstall", "prepare"];

#[derive(Debug, Clone)]
pub struct ValidatedExtensionPackageArchive {
    pub archive_digest: String,
    pub manifest_digest: String,
    pub manifest: ExtensionTemplatePayload,
    pub byte_size: i64,
}

#[derive(Debug, Clone)]
pub struct StoreExtensionPackageArtifactInput {
    pub project_id: Uuid,
    pub storage_ref: String,
    pub archive_bytes: Vec<u8>,
    pub expected_archive_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallExtensionPackageArtifactInput {
    pub project_id: Uuid,
    pub artifact_id: Uuid,
    pub extension_key: Option<String>,
    pub display_name: Option<String>,
    pub overwrite: bool,
}

pub fn validate_extension_package_archive(
    archive_bytes: &[u8],
    expected_archive_digest: Option<&str>,
) -> Result<ValidatedExtensionPackageArchive, DomainError> {
    if archive_bytes.is_empty() {
        return Err(DomainError::InvalidConfig(
            "extension package archive 不能为空".to_string(),
        ));
    }

    let archive_digest = digest_bytes(archive_bytes);
    if let Some(expected) = expected_archive_digest {
        validate_sha256_digest("expected_archive_digest", expected)?;
        if expected.trim() != archive_digest {
            return Err(DomainError::InvalidConfig(format!(
                "extension package archive digest 不匹配: expected {expected}, actual {archive_digest}"
            )));
        }
    }

    let files = read_tgz_files(archive_bytes)?;
    let manifest_value = parse_json_file(&files, EXTENSION_MANIFEST_PATH)?;
    let manifest_digest = seed_digest(&manifest_value)?;
    let manifest: ExtensionTemplatePayload =
        serde_json::from_value(manifest_value).map_err(DomainError::Serialization)?;
    manifest.validate()?;

    let package_json = parse_json_file(&files, PACKAGE_JSON_PATH)?;
    validate_package_json(&package_json, &manifest)?;
    validate_bundle_entries(&files, &manifest)?;

    Ok(ValidatedExtensionPackageArchive {
        archive_digest,
        manifest_digest,
        manifest,
        byte_size: archive_bytes.len() as i64,
    })
}

pub async fn store_extension_package_artifact(
    repos: &RepositorySet,
    input: StoreExtensionPackageArtifactInput,
) -> Result<ExtensionPackageArtifact, DomainError> {
    let validated = validate_extension_package_archive(
        &input.archive_bytes,
        input.expected_archive_digest.as_deref(),
    )?;
    if let Some(existing) = repos
        .extension_package_artifact_repo
        .get_by_project_and_digest(input.project_id, &validated.archive_digest)
        .await?
    {
        return Ok(existing);
    }

    let artifact = ExtensionPackageArtifact::new(
        input.project_id,
        input.storage_ref,
        validated.archive_digest,
        validated.manifest_digest,
        validated.manifest,
        validated.byte_size,
    )?;
    repos
        .extension_package_artifact_repo
        .create(&artifact)
        .await?;
    Ok(artifact)
}

pub async fn install_extension_package_artifact(
    repos: &RepositorySet,
    input: InstallExtensionPackageArtifactInput,
) -> Result<ProjectExtensionInstallation, DomainError> {
    let artifact = repos
        .extension_package_artifact_repo
        .get(input.artifact_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "extension_package_artifact",
            id: input.artifact_id.to_string(),
        })?;
    if artifact.project_id != input.project_id {
        return Err(DomainError::NotFound {
            entity: "extension_package_artifact",
            id: input.artifact_id.to_string(),
        });
    }

    let extension_key = input
        .extension_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&artifact.extension_id)
        .to_string();
    let display_name = input
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&artifact.extension_id)
        .to_string();
    let installation = ProjectExtensionInstallation::new_packaged(
        input.project_id,
        extension_key,
        display_name,
        artifact.manifest.clone(),
        ExtensionPackageArtifactRef::from_artifact(&artifact),
    )?;

    upsert_extension_installation(repos, installation, input.overwrite).await
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn read_tgz_files(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, DomainError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut files = BTreeMap::new();
    let entries = archive.entries().map_err(|error| {
        DomainError::InvalidConfig(format!("extension package archive 解析失败: {error}"))
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            DomainError::InvalidConfig(format!("extension package archive 条目读取失败: {error}"))
        })?;
        let path = normalize_archive_path(&entry.path().map_err(|error| {
            DomainError::InvalidConfig(format!("extension package archive 条目路径非法: {error}"))
        })?)?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            continue;
        }
        if !entry_type.is_file() {
            return Err(DomainError::InvalidConfig(format!(
                "extension package archive 包含非普通文件条目: {path}"
            )));
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|error| {
            DomainError::InvalidConfig(format!(
                "extension package archive 条目读取失败 `{path}`: {error}"
            ))
        })?;
        files.insert(path, content);
    }
    Ok(files)
}

fn normalize_archive_path(path: &Path) -> Result<String, DomainError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(value) = part.to_str() else {
                    return Err(DomainError::InvalidConfig(
                        "extension package archive 路径必须是 UTF-8".to_string(),
                    ));
                };
                if value.is_empty() {
                    continue;
                }
                parts.push(value.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DomainError::InvalidConfig(format!(
                    "extension package archive 包含不安全路径: {}",
                    path.display()
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(DomainError::InvalidConfig(
            "extension package archive 包含空路径".to_string(),
        ));
    }
    Ok(parts.join("/"))
}

fn parse_json_file(files: &BTreeMap<String, Vec<u8>>, path: &str) -> Result<Value, DomainError> {
    let bytes = files.get(path).ok_or_else(|| {
        DomainError::InvalidConfig(format!("extension package archive 缺少 `{path}`"))
    })?;
    serde_json::from_slice(bytes).map_err(|error| {
        DomainError::InvalidConfig(format!(
            "extension package archive `{path}` JSON 非法: {error}"
        ))
    })
}

fn validate_package_json(
    package_json: &Value,
    manifest: &ExtensionTemplatePayload,
) -> Result<(), DomainError> {
    let package = package_json.as_object().ok_or_else(|| {
        DomainError::InvalidConfig("extension package `package.json` 必须是对象".to_string())
    })?;
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainError::InvalidConfig("package.json.name 不能为空".to_string()))?;
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainError::InvalidConfig("package.json.version 不能为空".to_string()))?;
    if name != manifest.package.name {
        return Err(DomainError::InvalidConfig(format!(
            "package.json.name `{name}` 与 manifest package.name `{}` 不一致",
            manifest.package.name
        )));
    }
    if version != manifest.package.version {
        return Err(DomainError::InvalidConfig(format!(
            "package.json.version `{version}` 与 manifest package.version `{}` 不一致",
            manifest.package.version
        )));
    }

    if let Some(scripts) = package.get("scripts").and_then(Value::as_object) {
        for key in INSTALL_LIFECYCLE_SCRIPTS {
            if scripts.contains_key(*key) {
                return Err(DomainError::InvalidConfig(format!(
                    "extension package 不允许声明安装生命周期脚本: scripts.{key}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_bundle_entries(
    files: &BTreeMap<String, Vec<u8>>,
    manifest: &ExtensionTemplatePayload,
) -> Result<(), DomainError> {
    for bundle in &manifest.bundles {
        let bytes = files.get(&bundle.entry).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "extension package archive 缺少 bundle 文件 `{}`",
                bundle.entry
            ))
        })?;
        let actual = digest_bytes(bytes);
        if actual != bundle.digest {
            return Err(DomainError::InvalidConfig(format!(
                "extension package bundle `{}` digest 不匹配: expected {}, actual {}",
                bundle.entry, bundle.digest, actual
            )));
        }
    }
    Ok(())
}

async fn upsert_extension_installation(
    repos: &RepositorySet,
    installation: ProjectExtensionInstallation,
    overwrite: bool,
) -> Result<ProjectExtensionInstallation, DomainError> {
    if let Some(existing) = repos
        .project_extension_installation_repo
        .get_by_project_and_key(installation.project_id, &installation.extension_key)
        .await?
    {
        if !overwrite {
            return Err(DomainError::InvalidConfig(format!(
                "Project Extension key 已存在: {}",
                installation.extension_key
            )));
        }
        let mut merged = installation;
        merged.id = existing.id;
        merged.created_at = existing.created_at;
        merged.updated_at = chrono::Utc::now();
        repos
            .project_extension_installation_repo
            .update(&merged)
            .await?;
        return Ok(merged);
    }

    repos
        .project_extension_installation_repo
        .create(&installation)
        .await?;
    Ok(installation)
}

#[cfg(test)]
mod tests {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionTemplatePayload,
    };

    use super::*;

    fn archive_bytes(files: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            for (path, content) in files {
                let mut header = Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, path, content.as_slice())
                    .expect("append file");
            }
            builder.finish().expect("finish tar");
        }
        encoder.finish().expect("finish gzip")
    }

    fn manifest(bundle_digest: String) -> Value {
        serde_json::to_value(ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "local-hello".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/local-hello".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: bundle_digest,
            }],
        })
        .expect("manifest json")
    }

    fn valid_archive() -> Vec<u8> {
        let bundle = b"console.log('hello');".to_vec();
        let bundle_digest = digest_bytes(&bundle);
        archive_bytes(vec![
            (
                EXTENSION_MANIFEST_PATH,
                serde_json::to_vec(&manifest(bundle_digest)).expect("manifest bytes"),
            ),
            (
                PACKAGE_JSON_PATH,
                serde_json::to_vec(&serde_json::json!({
                    "name": "@agentdash/local-hello",
                    "version": "0.1.0"
                }))
                .expect("package bytes"),
            ),
            ("dist/extension.js", bundle),
        ])
    }

    #[test]
    fn validates_extension_package_archive() {
        let bytes = valid_archive();
        let validated =
            validate_extension_package_archive(&bytes, Some(&digest_bytes(&bytes))).expect("valid");
        assert_eq!(validated.manifest.extension_id, "local-hello");
        assert_eq!(validated.manifest.package.name, "@agentdash/local-hello");
        assert!(validated.archive_digest.starts_with("sha256:"));
        assert!(validated.manifest_digest.starts_with("sha256:"));
    }

    #[test]
    fn rejects_archive_digest_mismatch() {
        let bytes = valid_archive();
        let err = validate_extension_package_archive(
            &bytes,
            Some("sha256:0000000000000000000000000000000000000000000000000000000000000000"),
        )
        .expect_err("digest mismatch");
        assert!(err.to_string().contains("digest 不匹配"));
    }

    #[test]
    fn rejects_bundle_digest_mismatch() {
        let bundle = b"console.log('hello');".to_vec();
        let bytes = archive_bytes(vec![
            (
                EXTENSION_MANIFEST_PATH,
                serde_json::to_vec(&manifest(
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                ))
                .expect("manifest bytes"),
            ),
            (
                PACKAGE_JSON_PATH,
                serde_json::to_vec(&serde_json::json!({
                    "name": "@agentdash/local-hello",
                    "version": "0.1.0"
                }))
                .expect("package bytes"),
            ),
            ("dist/extension.js", bundle),
        ]);
        let err = validate_extension_package_archive(&bytes, None).expect_err("bundle mismatch");
        assert!(
            err.to_string()
                .contains("bundle `dist/extension.js` digest 不匹配")
        );
    }

    #[test]
    fn rejects_install_lifecycle_scripts() {
        let bundle = b"console.log('hello');".to_vec();
        let bundle_digest = digest_bytes(&bundle);
        let bytes = archive_bytes(vec![
            (
                EXTENSION_MANIFEST_PATH,
                serde_json::to_vec(&manifest(bundle_digest)).expect("manifest bytes"),
            ),
            (
                PACKAGE_JSON_PATH,
                serde_json::to_vec(&serde_json::json!({
                    "name": "@agentdash/local-hello",
                    "version": "0.1.0",
                    "scripts": { "postinstall": "node download.js" }
                }))
                .expect("package bytes"),
            ),
            ("dist/extension.js", bundle),
        ]);
        let err = validate_extension_package_archive(&bytes, None).expect_err("lifecycle");
        assert!(err.to_string().contains("scripts.postinstall"));
    }
}
