use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path};

use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agentdash_application_shared_library::seed_digest;
use agentdash_domain::DomainError;
use agentdash_domain::extension_package::{
    ExtensionPackageArtifact, ExtensionPackageArtifactOwner, ExtensionPackageArtifactRef,
    validate_sha256_digest,
};
use agentdash_domain::shared_library::{
    ExtensionTemplatePayload, ExtensionWorkspaceTabRendererDeclaration,
    ProjectExtensionInstallation,
};
pub use agentdash_platform_spi::extension_package::{
    ExtensionPackageArtifactStorage, ExtensionPackageArtifactStorageError,
};

use crate::repository_set::RepositorySet;

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
    pub owner: ExtensionPackageArtifactOwner,
    pub storage_ref: String,
    pub archive_bytes: Vec<u8>,
    pub expected_archive_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreExtensionPackageArchiveInput {
    pub project_id: Uuid,
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

#[derive(Debug, thiserror::Error)]
pub enum ExtensionPackageArtifactUseCaseError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Storage(#[from] ExtensionPackageArtifactStorageError),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Integrity(String),
}

#[derive(Debug, Clone)]
pub struct ReadExtensionPackageArchiveInput {
    pub project_id: Uuid,
    pub artifact_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ExtensionPackageArchiveObject {
    pub artifact: ExtensionPackageArtifact,
    pub archive_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ReadExtensionPackageWebviewAssetInput {
    pub project_id: Uuid,
    pub extension_key: String,
    pub asset_path: String,
}

#[derive(Debug, Clone)]
pub struct ExtensionPackageWebviewAsset {
    pub asset_path: String,
    pub bytes: Vec<u8>,
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
    validate_workspace_tab_entries(&files, &manifest)?;
    validate_bundle_entries(&files, &manifest)?;

    Ok(ValidatedExtensionPackageArchive {
        archive_digest,
        manifest_digest,
        manifest,
        byte_size: archive_bytes.len() as i64,
    })
}

pub fn read_extension_package_archive_file(
    archive_bytes: &[u8],
    file_path: &str,
) -> Result<Option<Vec<u8>>, DomainError> {
    let normalized_path = normalize_archive_path(Path::new(file_path))?;
    let mut files = read_tgz_files(archive_bytes)?;
    Ok(files.remove(&normalized_path))
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
        .get_by_owner_and_digest(&input.owner, &validated.archive_digest)
        .await?
    {
        return Ok(existing);
    }

    let artifact = ExtensionPackageArtifact::new(
        input.owner,
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

pub async fn store_extension_package_archive(
    repos: &RepositorySet,
    storage: &dyn ExtensionPackageArtifactStorage,
    input: StoreExtensionPackageArchiveInput,
) -> Result<ExtensionPackageArtifact, ExtensionPackageArtifactUseCaseError> {
    let validated = validate_extension_package_archive(
        &input.archive_bytes,
        input.expected_archive_digest.as_deref(),
    )?;
    let owner = ExtensionPackageArtifactOwner::project(input.project_id);
    let storage_ref = extension_package_archive_storage_ref_for(&owner, &validated.archive_digest)?;
    storage
        .write_archive_object(&storage_ref, &input.archive_bytes)
        .await?;
    Ok(store_extension_package_artifact(
        repos,
        StoreExtensionPackageArtifactInput {
            owner,
            storage_ref,
            archive_bytes: input.archive_bytes,
            expected_archive_digest: input.expected_archive_digest,
        },
    )
    .await?)
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
    if !artifact.owner.is_project(input.project_id) {
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

pub async fn read_extension_package_archive(
    repos: &RepositorySet,
    storage: &dyn ExtensionPackageArtifactStorage,
    input: ReadExtensionPackageArchiveInput,
) -> Result<ExtensionPackageArchiveObject, ExtensionPackageArtifactUseCaseError> {
    let artifact = repos
        .extension_package_artifact_repo
        .get(input.artifact_id)
        .await?
        .ok_or_else(|| {
            ExtensionPackageArtifactUseCaseError::NotFound(
                "Extension package artifact 不存在".to_string(),
            )
        })?;
    if !project_can_access_extension_artifact(repos, input.project_id, &artifact).await? {
        return Err(ExtensionPackageArtifactUseCaseError::NotFound(
            "Extension package artifact 不存在".to_string(),
        ));
    }
    let archive_bytes =
        read_verified_archive_bytes(storage, &artifact.storage_ref, &artifact.archive_digest)
            .await?;

    Ok(ExtensionPackageArchiveObject {
        artifact,
        archive_bytes,
    })
}

async fn project_can_access_extension_artifact(
    repos: &RepositorySet,
    project_id: Uuid,
    artifact: &ExtensionPackageArtifact,
) -> Result<bool, DomainError> {
    if artifact.owner.is_project(project_id) {
        return Ok(true);
    }
    let installations = repos
        .project_extension_installation_repo
        .list_by_project(project_id)
        .await?;
    Ok(installations.iter().any(|installation| {
        installation
            .package_artifact
            .as_ref()
            .is_some_and(|package| package.artifact_id == artifact.id)
    }))
}

pub async fn read_extension_package_webview_asset(
    repos: &RepositorySet,
    storage: &dyn ExtensionPackageArtifactStorage,
    input: ReadExtensionPackageWebviewAssetInput,
) -> Result<ExtensionPackageWebviewAsset, ExtensionPackageArtifactUseCaseError> {
    let asset_path = normalize_webview_asset_path(&input.asset_path)?;
    let installation = repos
        .project_extension_installation_repo
        .get_by_project_and_key(input.project_id, &input.extension_key)
        .await?
        .ok_or_else(|| {
            ExtensionPackageArtifactUseCaseError::NotFound(
                "Extension installation 不存在".to_string(),
            )
        })?;
    if !installation.enabled {
        return Err(ExtensionPackageArtifactUseCaseError::NotFound(
            "Extension installation 不存在".to_string(),
        ));
    }
    if !webview_asset_allowed(&installation.manifest, &asset_path) {
        return Err(ExtensionPackageArtifactUseCaseError::Forbidden(
            "Extension webview asset 不属于已声明 panel 目录".to_string(),
        ));
    }
    let artifact = installation.package_artifact.ok_or_else(|| {
        ExtensionPackageArtifactUseCaseError::Conflict(
            "Extension webview 需要 packaged artifact".to_string(),
        )
    })?;
    let archive_bytes =
        read_verified_archive_bytes(storage, &artifact.storage_ref, &artifact.archive_digest)
            .await?;
    let bytes =
        read_extension_package_archive_file(&archive_bytes, &asset_path)?.ok_or_else(|| {
            ExtensionPackageArtifactUseCaseError::NotFound(
                "Extension webview asset 不存在".to_string(),
            )
        })?;

    Ok(ExtensionPackageWebviewAsset { asset_path, bytes })
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

pub fn extension_package_archive_storage_ref_for(
    owner: &ExtensionPackageArtifactOwner,
    archive_digest: &str,
) -> Result<String, DomainError> {
    owner.validate()?;
    validate_sha256_digest("archive_digest", archive_digest)?;
    let digest = archive_digest
        .strip_prefix("sha256:")
        .ok_or_else(|| DomainError::InvalidConfig("archive_digest 格式非法".to_string()))?;
    Ok(format!(
        "extension-packages/{}/{}/{digest}.agentdash-extension.tgz",
        owner.kind.as_str(),
        owner.id
    ))
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

fn validate_workspace_tab_entries(
    files: &BTreeMap<String, Vec<u8>>,
    manifest: &ExtensionTemplatePayload,
) -> Result<(), DomainError> {
    for tab in &manifest.workspace_tabs {
        let entry = workspace_tab_renderer_entry(&tab.renderer);
        if !files.contains_key(entry) {
            return Err(DomainError::InvalidConfig(format!(
                "extension package archive 缺少 workspace tab renderer entry `{entry}`"
            )));
        }
    }
    Ok(())
}

fn workspace_tab_renderer_entry(renderer: &ExtensionWorkspaceTabRendererDeclaration) -> &str {
    match renderer {
        ExtensionWorkspaceTabRendererDeclaration::Webview { entry }
        | ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { entry } => entry,
    }
}

async fn read_verified_archive_bytes(
    storage: &dyn ExtensionPackageArtifactStorage,
    storage_ref: &str,
    expected_archive_digest: &str,
) -> Result<Vec<u8>, ExtensionPackageArtifactUseCaseError> {
    let archive_bytes = storage.read_archive_object(storage_ref).await?;
    let actual_digest = digest_bytes(&archive_bytes);
    if actual_digest != expected_archive_digest {
        return Err(ExtensionPackageArtifactUseCaseError::Integrity(format!(
            "extension package artifact 存储 digest 不匹配: expected {expected_archive_digest}, actual {actual_digest}"
        )));
    }
    Ok(archive_bytes)
}

fn normalize_webview_asset_path(raw: &str) -> Result<String, ExtensionPackageArtifactUseCaseError> {
    let mut parts = Vec::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => {
                let Some(value) = part.to_str() else {
                    return Err(ExtensionPackageArtifactUseCaseError::BadRequest(
                        "Extension webview asset path 必须是 UTF-8".to_string(),
                    ));
                };
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ExtensionPackageArtifactUseCaseError::BadRequest(
                    "Extension webview asset path 非法".to_string(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(ExtensionPackageArtifactUseCaseError::BadRequest(
            "Extension webview asset path 不能为空".to_string(),
        ));
    }
    Ok(parts.join("/"))
}

fn webview_asset_allowed(manifest: &ExtensionTemplatePayload, asset_path: &str) -> bool {
    manifest.workspace_tabs.iter().any(|tab| {
        let entry = workspace_tab_renderer_entry(&tab.renderer);
        let Ok(entry_path) = normalize_webview_asset_path(entry) else {
            return false;
        };
        if asset_path == entry_path {
            return true;
        }
        let Some((dir, _file)) = entry_path.rsplit_once('/') else {
            return false;
        };
        asset_path.starts_with(&format!("{dir}/"))
    })
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
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            fetch_routes: vec![],
            operation_catalog: vec![],
            backend_services: vec![],
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
    fn storage_ref_uses_archive_digest() {
        let project_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid");
        let owner = ExtensionPackageArtifactOwner::project(project_id);
        let storage_ref = extension_package_archive_storage_ref_for(
            &owner,
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("storage ref");
        assert_eq!(
            storage_ref,
            "extension-packages/project/11111111-1111-1111-1111-111111111111/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.agentdash-extension.tgz"
        );
    }

    fn canvas_panel_manifest() -> Value {
        serde_json::json!({
            "manifest_version": "2",
            "extension_id": "canvas-demo",
            "package": {
                "name": "@agentdash/canvas-demo",
                "version": "0.1.0"
            },
            "asset_version": "0.1.0",
            "workspace_tabs": [{
                "type_id": "canvas-demo.panel",
                "label": "Canvas Demo",
                "uri_scheme": "canvas-demo",
                "renderer": {
                    "kind": "canvas_panel",
                    "entry": "dist/canvas/runtime-snapshot.json"
                }
            }]
        })
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
    fn reads_extension_package_archive_file() {
        let bytes = archive_bytes(vec![(
            "dist/panel/index.html",
            b"<main>hello</main>".to_vec(),
        )]);
        let content = read_extension_package_archive_file(&bytes, "dist/panel/index.html")
            .expect("read archive")
            .expect("file exists");

        assert_eq!(content, b"<main>hello</main>");
    }

    #[test]
    fn webview_asset_path_rejects_traversal() {
        let err = normalize_webview_asset_path("../dist/panel/index.html")
            .expect_err("traversal should fail");
        assert!(err.to_string().contains("path 非法"));
    }

    #[test]
    fn webview_asset_allows_declared_panel_directory() {
        let manifest = serde_json::from_value::<ExtensionTemplatePayload>(serde_json::json!({
            "manifest_version": "2",
            "extension_id": "local-hello",
            "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
            "asset_version": "0.1.0",
            "workspace_tabs": [{
                "type_id": "local-hello.panel",
                "label": "Hello",
                "uri_scheme": "local-hello",
                "renderer": { "kind": "webview", "entry": "dist/panel/index.html" }
            }]
        }))
        .expect("manifest");

        assert!(webview_asset_allowed(&manifest, "dist/panel/index.html"));
        assert!(webview_asset_allowed(&manifest, "dist/panel/app.js"));
        assert!(!webview_asset_allowed(&manifest, "dist/extension.js"));
    }

    #[test]
    fn validates_canvas_panel_renderer_entry() {
        let bytes = archive_bytes(vec![
            (
                EXTENSION_MANIFEST_PATH,
                serde_json::to_vec(&canvas_panel_manifest()).expect("manifest bytes"),
            ),
            (
                PACKAGE_JSON_PATH,
                serde_json::to_vec(&serde_json::json!({
                    "name": "@agentdash/canvas-demo",
                    "version": "0.1.0"
                }))
                .expect("package bytes"),
            ),
            (
                "dist/canvas/runtime-snapshot.json",
                br#"{"canvas_id":"00000000-0000-0000-0000-000000000000"}"#.to_vec(),
            ),
        ]);

        let validated = validate_extension_package_archive(&bytes, None).expect("valid");

        assert_eq!(validated.manifest.extension_id, "canvas-demo");
    }

    #[test]
    fn rejects_missing_workspace_tab_renderer_entry() {
        let bytes = archive_bytes(vec![
            (
                EXTENSION_MANIFEST_PATH,
                serde_json::to_vec(&canvas_panel_manifest()).expect("manifest bytes"),
            ),
            (
                PACKAGE_JSON_PATH,
                serde_json::to_vec(&serde_json::json!({
                    "name": "@agentdash/canvas-demo",
                    "version": "0.1.0"
                }))
                .expect("package bytes"),
            ),
        ]);

        let err = validate_extension_package_archive(&bytes, None).expect_err("missing entry");

        assert!(err.to_string().contains("workspace tab renderer entry"));
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
