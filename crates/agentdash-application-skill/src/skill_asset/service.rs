use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::common::StoredFileContent;
use agentdash_domain::embedded_skill::EmbeddedSkillBundle;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetRepository, LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
    SkillTemplateFilePayload, SkillTemplatePayload, seed_digest,
};
use agentdash_domain::skill_asset::{
    SkillAsset, SkillAssetFile, SkillAssetFileKind, SkillAssetRepository, SkillAssetSource,
};
use agentdash_spi::{
    RemoteSkillFetch, RemoteSkillFile, RemoteSkillFileBody, RemoteSkillKind, RemoteSkillSource,
    RemoteSkillSourceError,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::skill::parse_skill_file;

use super::definition::{
    BuiltinSkillAssetTemplate, get_builtin_skill_asset_template, list_builtin_skill_asset_templates,
};
use super::error::SkillAssetApplicationError;

const MAX_SKILL_KEY_LENGTH: usize = 64;
const MAX_SKILL_DESCRIPTION_LENGTH: usize = 1024;

#[derive(Debug, Clone)]
pub struct SkillAssetFileInput {
    pub path: String,
    pub content: StoredFileContent,
}

impl SkillAssetFileInput {
    pub fn text(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: StoredFileContent::text(content),
        }
    }

    pub fn binary(path: impl Into<String>, bytes: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: StoredFileContent::binary(bytes, mime_type),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateSkillAssetInput {
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub disable_model_invocation: bool,
    pub files: Vec<SkillAssetFileInput>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateSkillAssetInput {
    pub key: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub disable_model_invocation: Option<bool>,
    pub files: Option<Vec<SkillAssetFileInput>>,
}

#[derive(Debug, Clone)]
pub struct ImportRemoteSkillAssetInput {
    pub project_id: Uuid,
    pub owner_id: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct RemoteSkillTemplateInput {
    pub source_kind: RemoteSkillKind,
    pub normalized_url: String,
    pub files: Vec<RemoteSkillFile>,
}

#[derive(Debug, Clone)]
pub struct MaterializedSkillTemplate {
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: String,
    pub source_ref: String,
    pub remote_digest: String,
    pub payload: SkillTemplatePayload,
}

pub struct SkillAssetService<'a, R: ?Sized> {
    repo: &'a R,
}

impl<'a, R: ?Sized> SkillAssetService<'a, R>
where
    R: SkillAssetRepository,
{
    pub fn new(repo: &'a R) -> Self {
        Self { repo }
    }

    pub async fn list(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        Ok(self.repo.list_by_project(project_id).await?)
    }

    pub async fn get(&self, id: Uuid) -> Result<SkillAsset, SkillAssetApplicationError> {
        self.repo.get(id).await?.ok_or_else(|| {
            SkillAssetApplicationError::NotFound(format!("skill_asset 不存在: {id}"))
        })
    }

    pub async fn create(
        &self,
        input: CreateSkillAssetInput,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let key = validate_skill_key(&input.key)?;
        validate_display_name(&input.display_name)?;
        validate_description(&input.description)?;
        self.ensure_key_available(input.project_id, &key, None)
            .await?;

        let files = build_files(Uuid::nil(), input.files)?;
        validate_skill_files(
            &key,
            &input.description,
            input.disable_model_invocation,
            &files,
        )?;

        let mut asset = SkillAsset::new_user(
            input.project_id,
            key,
            input.display_name.trim(),
            input.description.trim(),
            input.disable_model_invocation,
        );
        asset.files = files
            .into_iter()
            .map(|mut file| {
                file.skill_asset_id = asset.id;
                file
            })
            .collect();

        self.repo.create(&asset).await?;
        Ok(asset)
    }

    pub async fn update(
        &self,
        id: Uuid,
        input: UpdateSkillAssetInput,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let mut asset = self.get(id).await?;
        if let Some(key) = input.key {
            let key = validate_skill_key(&key)?;
            if key != asset.key {
                self.ensure_key_available(asset.project_id, &key, Some(asset.id))
                    .await?;
            }
            asset.key = key;
        }
        if let Some(display_name) = input.display_name {
            validate_display_name(&display_name)?;
            asset.display_name = display_name.trim().to_string();
        }
        if let Some(description) = input.description {
            validate_description(&description)?;
            asset.description = description.trim().to_string();
        }
        if let Some(disable_model_invocation) = input.disable_model_invocation {
            asset.disable_model_invocation = disable_model_invocation;
        }
        if let Some(files) = input.files {
            asset.files = build_files(asset.id, files)?;
        }
        validate_skill_files(
            &asset.key,
            &asset.description,
            asset.disable_model_invocation,
            &asset.files,
        )?;
        asset.touch();
        self.repo.update(&asset).await?;
        Ok(asset)
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), SkillAssetApplicationError> {
        self.repo.delete(id).await?;
        Ok(())
    }

    pub async fn bootstrap_builtins(
        &self,
        project_id: Uuid,
        builtin_key: Option<&str>,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        let templates = match builtin_key.map(str::trim).filter(|value| !value.is_empty()) {
            Some(key) => vec![get_builtin_skill_asset_template(key).ok_or_else(|| {
                SkillAssetApplicationError::NotFound(format!("内嵌 Skill 模板不存在: {key}"))
            })?],
            None => list_builtin_skill_asset_templates(),
        };

        let mut created_or_existing = Vec::with_capacity(templates.len());
        for template in templates {
            if let Some(existing) = self
                .repo
                .get_by_project_and_builtin_key(project_id, template.builtin_key)
                .await?
            {
                created_or_existing.push(
                    self.sync_existing_from_builtin_template(existing, template)
                        .await?,
                );
                continue;
            }
            if let Some(existing) = self
                .repo
                .get_by_project_and_key(project_id, template.bundle.name)
                .await?
            {
                created_or_existing.push(
                    self.sync_existing_from_builtin_template(existing, template)
                        .await?,
                );
                continue;
            }
            created_or_existing.push(
                self.create_from_builtin_template(project_id, template)
                    .await?,
            );
        }
        Ok(created_or_existing)
    }

    pub async fn reset_from_builtin(
        &self,
        id: Uuid,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let mut asset = self.get(id).await?;
        let builtin_key = asset
            .source
            .builtin_key()
            .map(ToString::to_string)
            .ok_or_else(|| {
                SkillAssetApplicationError::BadRequest(
                    "只有 builtin_seed Skill 可以 reset".to_string(),
                )
            })?;
        let template = get_builtin_skill_asset_template(&builtin_key).ok_or_else(|| {
            SkillAssetApplicationError::NotFound(format!("内嵌 Skill 模板不存在: {builtin_key}"))
        })?;
        let (description, disable_model_invocation, files) =
            files_from_embedded_bundle(asset.id, template.bundle)?;
        asset.key = template.bundle.name.to_string();
        asset.display_name = template.display_name.to_string();
        asset.description = description;
        asset.disable_model_invocation = disable_model_invocation;
        asset.files = files;
        asset.touch();
        self.repo.update(&asset).await?;
        Ok(asset)
    }

    pub async fn import_uploaded_files(
        &self,
        project_id: Uuid,
        files: Vec<SkillAssetFileInput>,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        let grouped = group_uploaded_skill_files(files)?;
        let mut results = Vec::new();
        for (key, input_files) in grouped {
            let skill_md = input_files
                .iter()
                .find(|file| file.path == "SKILL.md")
                .ok_or_else(|| {
                    SkillAssetApplicationError::BadRequest(format!("Skill `{key}` 缺少 SKILL.md"))
                })?;
            let meta = parse_skill_metadata(skill_md_text(&skill_md.content)?)?;
            let description = meta.description;
            let disable_model_invocation = meta.disable_model_invocation;
            if let Some(mut existing) = self.repo.get_by_project_and_key(project_id, &key).await? {
                existing.display_name = key.clone();
                existing.description = description.clone();
                existing.disable_model_invocation = disable_model_invocation;
                existing.files = build_files(existing.id, input_files)?;
                validate_skill_files(
                    &existing.key,
                    &existing.description,
                    existing.disable_model_invocation,
                    &existing.files,
                )?;
                existing.touch();
                self.repo.update(&existing).await?;
                results.push(existing);
            } else {
                let created = self
                    .create(CreateSkillAssetInput {
                        project_id,
                        key: key.clone(),
                        display_name: key,
                        description,
                        disable_model_invocation,
                        files: input_files,
                    })
                    .await?;
                results.push(created);
            }
        }
        Ok(results)
    }

    async fn create_from_builtin_template(
        &self,
        project_id: Uuid,
        template: BuiltinSkillAssetTemplate,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let (description, disable_model_invocation, files) =
            files_from_embedded_bundle(Uuid::nil(), template.bundle)?;
        self.ensure_key_available(project_id, template.bundle.name, None)
            .await?;
        let mut asset = SkillAsset::new_builtin_seed(
            project_id,
            template.builtin_key,
            template.bundle.name,
            template.display_name,
            description,
            disable_model_invocation,
        );
        asset.files = files
            .into_iter()
            .map(|mut file| {
                file.skill_asset_id = asset.id;
                file
            })
            .collect();
        self.repo.create(&asset).await?;
        Ok(asset)
    }

    async fn sync_existing_from_builtin_template(
        &self,
        mut asset: SkillAsset,
        template: BuiltinSkillAssetTemplate,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        if apply_builtin_template(&mut asset, template)? {
            self.repo.update(&asset).await?;
        }
        Ok(asset)
    }

    async fn ensure_key_available(
        &self,
        project_id: Uuid,
        key: &str,
        allow_id: Option<Uuid>,
    ) -> Result<(), SkillAssetApplicationError> {
        if let Some(existing) = self.repo.get_by_project_and_key(project_id, key).await?
            && Some(existing.id) != allow_id
        {
            return Err(SkillAssetApplicationError::Conflict(format!(
                "skill_asset key 已存在: {key}"
            )));
        }
        Ok(())
    }
}

// ─── Remote import ───────────────────────────────────────────────────────────

pub async fn prepare_remote_skill_import(
    skill_repo: &dyn SkillAssetRepository,
    library_repo: &dyn LibraryAssetRepository,
    input: &ImportRemoteSkillAssetInput,
    source: &dyn RemoteSkillSource,
) -> Result<LibraryAsset, SkillAssetApplicationError> {
    let owner_id = normalize_remote_import_owner_id(&input.owner_id)?;
    let fetched = source
        .fetch(&input.url)
        .await
        .map_err(map_remote_skill_source_error)?;
    let materialized = materialize_remote_skill_template(remote_template_input(fetched))?;
    ensure_remote_import_target_available(
        skill_repo,
        input.project_id,
        &materialized.key,
        &materialized.source_ref,
    )
    .await?;
    upsert_remote_imported_skill_template(library_repo, owner_id, materialized).await
}

pub fn materialize_remote_skill_template(
    input: RemoteSkillTemplateInput,
) -> Result<MaterializedSkillTemplate, SkillAssetApplicationError> {
    let normalized_url = input.normalized_url.trim();
    if normalized_url.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "远端 Skill normalized_url 不能为空".to_string(),
        ));
    }

    let source_ref = remote_skill_url_source_ref(input.source_kind, normalized_url);
    let input_files = input
        .files
        .into_iter()
        .map(remote_skill_file_to_input)
        .collect::<Result<Vec<_>, _>>()?;
    let skill_md = input_files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            SkillAssetApplicationError::BadRequest("远端 Skill 缺少根目录 SKILL.md".to_string())
        })?;
    let meta = parse_skill_metadata(skill_md_text(&skill_md.content)?)?;
    let files = build_files(Uuid::nil(), input_files)?;
    validate_skill_files(
        &meta.name,
        &meta.description,
        meta.disable_model_invocation,
        &files,
    )?;

    let remote_digest = digest_skill_files(&files);
    let payload_files = files
        .into_iter()
        .map(|file| {
            let content = file
                .text_content()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    SkillAssetApplicationError::BadRequest(format!(
                        "SkillTemplate 暂不支持二进制文件: {}",
                        file.path
                    ))
                })?;
            Ok(SkillTemplateFilePayload {
                path: file.path,
                content,
                kind: file.kind,
            })
        })
        .collect::<Result<Vec<_>, SkillAssetApplicationError>>()?;

    Ok(MaterializedSkillTemplate {
        key: meta.name.clone(),
        display_name: meta.name,
        description: Some(meta.description),
        version: remote_digest.clone(),
        source_ref,
        remote_digest,
        payload: SkillTemplatePayload {
            files: payload_files,
            disable_model_invocation: meta.disable_model_invocation,
        },
    })
}

pub fn remote_skill_url_source_ref(kind: RemoteSkillKind, normalized_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized_url.trim().as_bytes());
    format!(
        "market:skill-url:{}:{:x}",
        remote_skill_kind_key(kind),
        hasher.finalize()
    )
}

async fn upsert_remote_imported_skill_template(
    repo: &dyn LibraryAssetRepository,
    owner_id: String,
    materialized: MaterializedSkillTemplate,
) -> Result<LibraryAsset, SkillAssetApplicationError> {
    let payload = serde_json::to_value(&materialized.payload)
        .map_err(|error| SkillAssetApplicationError::BadRequest(error.to_string()))?;
    let payload_digest = seed_digest(&payload).map_err(map_shared_library_domain_error)?;
    let asset = LibraryAsset::new(
        LibraryAssetType::SkillTemplate,
        LibraryAssetScope::User,
        Some(owner_id.clone()),
        materialized.key.clone(),
        materialized.display_name,
        materialized.description,
        materialized.version,
        LibraryAssetSource::RemoteImported,
        Some(materialized.source_ref.clone()),
        payload_digest,
        payload,
    )
    .map_err(map_shared_library_domain_error)?;

    let existing = repo
        .find_by_identity(
            asset.asset_type,
            asset.scope,
            Some(owner_id.as_str()),
            &asset.key,
        )
        .await
        .map_err(map_shared_library_domain_error)?;

    match existing {
        Some(existing)
            if existing.source == LibraryAssetSource::RemoteImported
                && existing.source_ref.as_deref() == Some(materialized.source_ref.as_str()) =>
        {
            let mut updated = asset;
            updated.id = existing.id;
            updated.created_at = existing.created_at;
            updated.updated_at = chrono::Utc::now();
            repo.update(&updated)
                .await
                .map_err(map_shared_library_domain_error)?;
            Ok(updated)
        }
        Some(existing) => Err(SkillAssetApplicationError::Conflict(format!(
            "LibraryAsset identity 已被其它来源占用: {}:{} source={:?} source_ref={:?}",
            existing.asset_type.as_str(),
            existing.key,
            existing.source,
            existing.source_ref
        ))),
        None => {
            repo.create(&asset)
                .await
                .map_err(map_shared_library_domain_error)?;
            Ok(asset)
        }
    }
}

fn remote_template_input(fetched: RemoteSkillFetch) -> RemoteSkillTemplateInput {
    RemoteSkillTemplateInput {
        source_kind: fetched.kind,
        normalized_url: fetched.normalized_url,
        files: fetched.files,
    }
}

fn remote_skill_kind_key(kind: RemoteSkillKind) -> &'static str {
    match kind {
        RemoteSkillKind::Github => "github",
        RemoteSkillKind::Clawhub => "clawhub",
        RemoteSkillKind::SkillsSh => "skills_sh",
    }
}

fn normalize_remote_import_owner_id(owner_id: &str) -> Result<String, SkillAssetApplicationError> {
    let owner_id = owner_id.trim();
    if owner_id.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "远端 Skill 导入 owner_id 不能为空".to_string(),
        ));
    }
    Ok(owner_id.to_string())
}

pub fn map_shared_library_domain_error(
    error: agentdash_domain::DomainError,
) -> SkillAssetApplicationError {
    match error {
        agentdash_domain::DomainError::NotFound { .. } => {
            SkillAssetApplicationError::NotFound(error.to_string())
        }
        agentdash_domain::DomainError::Conflict { .. }
        | agentdash_domain::DomainError::InvalidTransition { .. } => {
            SkillAssetApplicationError::Conflict(error.to_string())
        }
        agentdash_domain::DomainError::Forbidden { .. } => {
            SkillAssetApplicationError::BadRequest(error.to_string())
        }
        agentdash_domain::DomainError::InvalidConfig(message) => {
            SkillAssetApplicationError::BadRequest(message)
        }
        agentdash_domain::DomainError::Serialization(error) => {
            SkillAssetApplicationError::BadRequest(error.to_string())
        }
        agentdash_domain::DomainError::Database { .. } => {
            SkillAssetApplicationError::Internal("内部数据库错误".to_string())
        }
    }
}

async fn ensure_remote_import_target_available(
    repo: &dyn SkillAssetRepository,
    project_id: Uuid,
    key: &str,
    source_ref: &str,
) -> Result<(), SkillAssetApplicationError> {
    if let Some(existing) = repo
        .list_by_project(project_id)
        .await
        .map_err(map_shared_library_domain_error)?
        .into_iter()
        .find(|asset| {
            asset
                .installed_source
                .as_ref()
                .is_some_and(|source| source.source_ref == source_ref)
                && asset.key != key
        })
    {
        return Err(SkillAssetApplicationError::Conflict(format!(
            "远端 Skill source_ref `{source_ref}` 已安装为 key `{}`",
            existing.key
        )));
    }

    let Some(existing) = repo
        .get_by_project_and_key(project_id, key)
        .await
        .map_err(map_shared_library_domain_error)?
    else {
        return Ok(());
    };
    let same_installed_source = existing
        .installed_source
        .as_ref()
        .is_some_and(|source| source.source_ref == source_ref);
    if same_installed_source {
        return Ok(());
    }
    Err(SkillAssetApplicationError::Conflict(format!(
        "skill_asset key 已存在: {key}"
    )))
}

/// 将 SPI 层的 [`RemoteSkillSourceError`] 映射到应用层错误。
fn map_remote_skill_source_error(error: RemoteSkillSourceError) -> SkillAssetApplicationError {
    match error {
        RemoteSkillSourceError::BadRequest(message) => {
            SkillAssetApplicationError::BadRequest(message)
        }
        RemoteSkillSourceError::Internal(message) => SkillAssetApplicationError::Internal(message),
    }
}

/// 将 SPI 层的远端文件原始体按应用层内容定型规则转换为 [`SkillAssetFileInput`]。
fn remote_skill_file_to_input(
    file: RemoteSkillFile,
) -> Result<SkillAssetFileInput, SkillAssetApplicationError> {
    let content = match file.body {
        RemoteSkillFileBody::Text(text) => StoredFileContent::text(text),
        RemoteSkillFileBody::Bytes(bytes) => content_from_bytes(&file.path, bytes, None)?,
    };
    Ok(SkillAssetFileInput {
        path: file.path,
        content,
    })
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn digest_skill_files(files: &[SkillAssetFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.content_kind_str().as_bytes());
        hasher.update([0]);
        if let Some(mime_type) = file.mime_type() {
            hasher.update(mime_type.as_bytes());
        }
        hasher.update([0]);
        match &file.content {
            StoredFileContent::Text { content } => hasher.update(content.as_bytes()),
            StoredFileContent::Binary { bytes, .. } => hasher.update(bytes),
        }
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn files_from_embedded_bundle(
    asset_id: Uuid,
    bundle: &EmbeddedSkillBundle,
) -> Result<(String, bool, Vec<SkillAssetFile>), SkillAssetApplicationError> {
    bundle
        .validate()
        .map_err(|error| SkillAssetApplicationError::Internal(error.to_string()))?;
    let files = bundle
        .files
        .iter()
        .map(|file| SkillAssetFileInput::text(file.relative_path, file.content))
        .collect::<Vec<_>>();
    let files = build_files(asset_id, files)?;
    let skill_md = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            SkillAssetApplicationError::Internal("内嵌 Skill 缺少 SKILL.md".to_string())
        })?;
    let meta = parse_skill_metadata(skill_md_text(&skill_md.content)?)?;
    validate_skill_files(
        bundle.name,
        &meta.description,
        meta.disable_model_invocation,
        &files,
    )?;
    Ok((meta.description, meta.disable_model_invocation, files))
}

fn apply_builtin_template(
    asset: &mut SkillAsset,
    template: BuiltinSkillAssetTemplate,
) -> Result<bool, SkillAssetApplicationError> {
    let (description, disable_model_invocation, files) =
        files_from_embedded_bundle(asset.id, template.bundle)?;
    let target_source = SkillAssetSource::BuiltinSeed {
        key: template.builtin_key.to_string(),
    };
    let changed = asset.key != template.bundle.name
        || asset.display_name != template.display_name
        || asset.description != description
        || asset.disable_model_invocation != disable_model_invocation
        || asset.source != target_source
        || asset.installed_source.is_some()
        || digest_skill_files(&asset.files) != digest_skill_files(&files);

    if changed {
        asset.key = template.bundle.name.to_string();
        asset.display_name = template.display_name.to_string();
        asset.description = description;
        asset.disable_model_invocation = disable_model_invocation;
        asset.source = target_source;
        asset.installed_source = None;
        asset.files = files;
        asset.touch();
    }

    Ok(changed)
}

fn build_files(
    asset_id: Uuid,
    files: Vec<SkillAssetFileInput>,
) -> Result<Vec<SkillAssetFile>, SkillAssetApplicationError> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::with_capacity(files.len());
    for file in files {
        let path = normalize_skill_file_path(&file.path)?;
        if !seen.insert(path.clone()) {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill 文件路径重复: {path}"
            )));
        }
        result.push(SkillAssetFile::new_with_content(
            asset_id,
            path.clone(),
            file.content,
            SkillAssetFileKind::from_path(&path),
        ));
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn validate_skill_files(
    key: &str,
    description: &str,
    disable_model_invocation: bool,
    files: &[SkillAssetFile],
) -> Result<(), SkillAssetApplicationError> {
    let skill_md = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            SkillAssetApplicationError::BadRequest("Skill 必须包含 SKILL.md".to_string())
        })?;
    let meta = parse_skill_metadata(skill_md_text(&skill_md.content)?)?;
    if meta.name != key {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "SKILL.md frontmatter name `{}` 必须等于 skill key `{key}`",
            meta.name
        )));
    }
    if meta.description.trim() != description.trim() {
        return Err(SkillAssetApplicationError::BadRequest(
            "SKILL.md frontmatter description 必须与资产描述一致".to_string(),
        ));
    }
    if meta.disable_model_invocation != disable_model_invocation {
        return Err(SkillAssetApplicationError::BadRequest(
            "SKILL.md frontmatter disable-model-invocation 必须与资产设置一致".to_string(),
        ));
    }
    Ok(())
}

pub(crate) struct ParsedSkillMetadata {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) disable_model_invocation: bool,
}

pub(crate) fn parse_skill_metadata(
    content: &str,
) -> Result<ParsedSkillMetadata, SkillAssetApplicationError> {
    let (frontmatter, _body) = parse_skill_file(content);
    let frontmatter = frontmatter.ok_or_else(|| {
        SkillAssetApplicationError::BadRequest("SKILL.md 必须包含 YAML frontmatter".to_string())
    })?;
    let name = frontmatter
        .name
        .as_deref()
        .map(validate_skill_key)
        .transpose()?
        .ok_or_else(|| SkillAssetApplicationError::BadRequest("SKILL.md 缺少 name".to_string()))?;
    let description = frontmatter
        .description
        .as_deref()
        .ok_or_else(|| {
            SkillAssetApplicationError::BadRequest("SKILL.md 缺少 description".to_string())
        })?
        .trim()
        .to_string();
    validate_description(&description)?;
    Ok(ParsedSkillMetadata {
        name,
        description,
        disable_model_invocation: frontmatter.disable_model_invocation,
    })
}

fn skill_md_text(content: &StoredFileContent) -> Result<&str, SkillAssetApplicationError> {
    content.text_content().ok_or_else(|| {
        SkillAssetApplicationError::BadRequest(
            "SKILL.md 必须是 UTF-8 文本文档，不能是二进制文件".to_string(),
        )
    })
}

pub fn content_from_bytes(
    path: &str,
    bytes: Vec<u8>,
    mime_hint: Option<&str>,
) -> Result<StoredFileContent, SkillAssetApplicationError> {
    let mime_type = mime_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| guess_skill_file_mime_type(path));

    if path == "SKILL.md" {
        return String::from_utf8(bytes)
            .map(StoredFileContent::text)
            .map_err(|error| {
                SkillAssetApplicationError::BadRequest(format!(
                    "SKILL.md 必须是 UTF-8 文本文档: {error}"
                ))
            });
    }

    if is_image_mime(&mime_type) || is_image_path(path) {
        return Ok(StoredFileContent::binary(bytes, mime_type));
    }

    if is_text_mime(&mime_type) || is_text_path(path) {
        return String::from_utf8(bytes)
            .map(StoredFileContent::text)
            .map_err(|error| {
                SkillAssetApplicationError::BadRequest(format!(
                    "文本 Skill 文件必须是 UTF-8: {path}: {error}"
                ))
            });
    }

    match String::from_utf8(bytes) {
        Ok(content) => Ok(StoredFileContent::text(content)),
        Err(error) => Ok(StoredFileContent::binary(
            error.into_bytes(),
            if mime_type == "text/plain; charset=utf-8" {
                "application/octet-stream".to_string()
            } else {
                mime_type
            },
        )),
    }
}

fn guess_skill_file_mime_type(path: &str) -> String {
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
    } else if lower.ends_with(".json") {
        "application/json".to_string()
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "application/x-yaml".to_string()
    } else if lower.ends_with(".toml") {
        "application/toml".to_string()
    } else if is_text_path(path) {
        "text/plain; charset=utf-8".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn is_image_mime(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

fn is_text_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/json" | "application/x-yaml" | "application/toml"
        )
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(
        lower.rsplit_once('.').map(|(_, ext)| ext),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "svg")
    )
}

fn is_text_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(
        lower.rsplit_once('.').map(|(_, ext)| ext),
        Some(
            "md" | "txt"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "rs"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "css"
                | "html"
                | "sh"
                | "py"
        )
    )
}

fn validate_skill_key(key: &str) -> Result<String, SkillAssetApplicationError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "skill key 不能为空".to_string(),
        ));
    }
    if key.len() > MAX_SKILL_KEY_LENGTH {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "skill key 不能超过 {MAX_SKILL_KEY_LENGTH} 字符"
        )));
    }
    if !key
        .chars()
        .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '-'))
    {
        return Err(SkillAssetApplicationError::BadRequest(
            "skill key 只能包含小写字母、数字和连字符".to_string(),
        ));
    }
    Ok(key.to_string())
}

fn validate_display_name(display_name: &str) -> Result<(), SkillAssetApplicationError> {
    let value = display_name.trim();
    if value.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "display_name 不能为空".to_string(),
        ));
    }
    if value.len() > 128 {
        return Err(SkillAssetApplicationError::BadRequest(
            "display_name 不能超过 128 字符".to_string(),
        ));
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<(), SkillAssetApplicationError> {
    let value = description.trim();
    if value.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "description 不能为空".to_string(),
        ));
    }
    if value.len() > MAX_SKILL_DESCRIPTION_LENGTH {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "description 不能超过 {MAX_SKILL_DESCRIPTION_LENGTH} 字符"
        )));
    }
    Ok(())
}

fn normalize_skill_file_path(path: &str) -> Result<String, SkillAssetApplicationError> {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_matches('/').to_string();
    if normalized.is_empty() || normalized.contains(':') {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "Skill 文件路径非法: {path}"
        )));
    }
    for segment in normalized.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill 文件路径非法: {path}"
            )));
        }
    }
    Ok(normalized)
}

fn group_uploaded_skill_files(
    files: Vec<SkillAssetFileInput>,
) -> Result<BTreeMap<String, Vec<SkillAssetFileInput>>, SkillAssetApplicationError> {
    if files.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "上传内容为空".to_string(),
        ));
    }

    let normalized = files
        .into_iter()
        .map(|file| Ok((normalize_skill_file_path(&file.path)?, file.content)))
        .collect::<Result<Vec<_>, SkillAssetApplicationError>>()?;

    let root_skill_count = normalized
        .iter()
        .filter(|(path, _)| path == "SKILL.md")
        .count();
    if root_skill_count > 1 {
        return Err(SkillAssetApplicationError::BadRequest(
            "上传内容包含重复的根 SKILL.md".to_string(),
        ));
    }

    if root_skill_count == 1 {
        let skill_md = normalized
            .iter()
            .find(|(path, _)| path == "SKILL.md")
            .expect("counted root skill");
        let key = parse_skill_metadata(skill_md_text(&skill_md.1)?)?.name;
        return Ok(BTreeMap::from([(
            key,
            normalized
                .into_iter()
                .map(|(path, content)| SkillAssetFileInput { path, content })
                .collect(),
        )]));
    }

    let mut grouped: BTreeMap<String, Vec<SkillAssetFileInput>> = BTreeMap::new();
    for (path, content) in normalized {
        let (key, relative_path) = path.split_once('/').ok_or_else(|| {
            SkillAssetApplicationError::BadRequest(format!(
                "多 Skill 上传时文件必须位于 <skill-key>/... 目录下: {path}"
            ))
        })?;
        let key = validate_skill_key(key)?;
        grouped.entry(key).or_default().push(SkillAssetFileInput {
            path: relative_path.to_string(),
            content,
        });
    }
    for (key, files) in &grouped {
        if !files.iter().any(|file| file.path == "SKILL.md") {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill `{key}` 缺少 SKILL.md"
            )));
        }
    }
    Ok(grouped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_test_support::shared_library::MemoryLibraryAssetRepository;
    use agentdash_test_support::skill::MemorySkillAssetRepository;

    fn skill_file(key: &str, description: &str) -> SkillAssetFileInput {
        SkillAssetFileInput::text(
            "SKILL.md",
            format!("---\nname: {key}\ndescription: \"{description}\"\n---\n# Body\n"),
        )
    }

    #[tokio::test]
    async fn create_rejects_missing_skill_md_and_mismatched_frontmatter() {
        let repo = MemorySkillAssetRepository::default();
        let service = SkillAssetService::new(&repo);
        let project_id = Uuid::new_v4();

        let missing = service
            .create(CreateSkillAssetInput {
                project_id,
                key: "writer".to_string(),
                display_name: "Writer".to_string(),
                description: "写作辅助".to_string(),
                disable_model_invocation: false,
                files: vec![SkillAssetFileInput::text("references/style.md", "style")],
            })
            .await;
        assert!(matches!(
            missing,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));

        let mismatch = service
            .create(CreateSkillAssetInput {
                project_id,
                key: "writer".to_string(),
                display_name: "Writer".to_string(),
                description: "写作辅助".to_string(),
                disable_model_invocation: false,
                files: vec![skill_file("other", "写作辅助")],
            })
            .await;
        assert!(matches!(
            mismatch,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn builtin_bootstrap_syncs_embedded_template() {
        let repo = MemorySkillAssetRepository::default();
        let service = SkillAssetService::new(&repo);
        let project_id = Uuid::new_v4();

        let first = service
            .bootstrap_builtins(project_id, Some("canvas-system"))
            .await
            .expect("bootstrap");
        assert_eq!(first.len(), 1);
        let asset = first[0].clone();

        let edited_description = "用户编辑后的描述";
        let edited = service
            .update(
                asset.id,
                UpdateSkillAssetInput {
                    description: Some(edited_description.to_string()),
                    files: Some(vec![skill_file(&asset.key, edited_description)]),
                    ..Default::default()
                },
            )
            .await
            .expect("edit builtin seed");
        assert_eq!(edited.description, edited_description);

        let second = service
            .bootstrap_builtins(project_id, Some("canvas-system"))
            .await
            .expect("bootstrap again");
        assert_ne!(second[0].description, edited_description);
        assert!(second[0].files.iter().any(|file| file.path == "SKILL.md"));

        let reset = service
            .reset_from_builtin(asset.id)
            .await
            .expect("reset builtin seed");
        assert_ne!(reset.description, edited_description);
        assert!(reset.files.iter().any(|file| file.path == "SKILL.md"));
    }

    #[tokio::test]
    async fn builtin_bootstrap_converges_same_key_user_snapshot() {
        let repo = MemorySkillAssetRepository::default();
        let service = SkillAssetService::new(&repo);
        let project_id = Uuid::new_v4();
        let mut snapshot = SkillAsset::new_user(
            project_id,
            "companion-system",
            "Companion System",
            "同步前市场安装快照",
            false,
        );
        snapshot.files = build_files(
            snapshot.id,
            vec![skill_file("companion-system", "同步前市场安装快照")],
        )
        .expect("snapshot files");
        let snapshot_id = snapshot.id;
        repo.create(&snapshot).await.expect("create snapshot");

        let synced = service
            .bootstrap_builtins(project_id, Some("companion-system"))
            .await
            .expect("bootstrap");

        assert_eq!(synced.len(), 1);
        assert_eq!(synced[0].id, snapshot_id);
        assert_eq!(
            synced[0].source,
            SkillAssetSource::BuiltinSeed {
                key: "companion-system".to_string()
            }
        );
        assert_eq!(synced[0].installed_source, None);
        assert_ne!(synced[0].description, "同步前市场安装快照");
        assert!(
            synced[0]
                .files
                .iter()
                .any(|file| file.path == "references/payload-envelope.md")
        );
    }

    #[test]
    fn upload_grouping_accepts_root_skill_and_multi_skill_directory() {
        let root = group_uploaded_skill_files(vec![SkillAssetFileInput::text(
            "SKILL.md",
            "---\nname: writer\ndescription: \"写作辅助\"\n---\n",
        )])
        .expect("root skill");
        assert!(root.contains_key("writer"));

        let grouped = group_uploaded_skill_files(vec![
            SkillAssetFileInput::text(
                "writer/SKILL.md",
                "---\nname: writer\ndescription: \"写作辅助\"\n---\n",
            ),
            SkillAssetFileInput::text(
                "research/SKILL.md",
                "---\nname: research\ndescription: \"调研\"\n---\n",
            ),
        ])
        .expect("multi skill");
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn upload_grouping_accepts_binary_assets_and_rejects_binary_skill_md() {
        let grouped = group_uploaded_skill_files(vec![
            SkillAssetFileInput::text(
                "SKILL.md",
                "---\nname: writer\ndescription: \"写作辅助\"\n---\n",
            ),
            SkillAssetFileInput::binary("assets/logo.png", vec![0, 1, 2, 3], "image/png"),
        ])
        .expect("root skill with binary asset");
        let files = grouped.get("writer").expect("writer group");
        let logo = files
            .iter()
            .find(|file| file.path == "assets/logo.png")
            .expect("logo file");
        assert_eq!(logo.content.mime_type(), Some("image/png"));
        assert!(logo.content.binary_content().is_some());

        let binary_skill = group_uploaded_skill_files(vec![SkillAssetFileInput::binary(
            "SKILL.md",
            vec![0, 159, 146, 150],
            "application/octet-stream",
        )]);
        assert!(matches!(
            binary_skill,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));
    }

    #[test]
    fn remote_files_materialize_skill_template_with_stable_source_ref() {
        let normalized_url = "https://github.com/acme/skills/tree/main/writer";
        let materialized = materialize_remote_skill_template(RemoteSkillTemplateInput {
            source_kind: RemoteSkillKind::Github,
            normalized_url: normalized_url.to_string(),
            files: vec![
                RemoteSkillFile {
                    path: "SKILL.md".to_string(),
                    body: RemoteSkillFileBody::Text(
                        "---\nname: writer\ndescription: \"写作辅助\"\n---\n# Body\n".to_string(),
                    ),
                },
                RemoteSkillFile {
                    path: "references/style.md".to_string(),
                    body: RemoteSkillFileBody::Text("保持简洁".to_string()),
                },
            ],
        })
        .expect("materialize");

        assert_eq!(materialized.key, "writer");
        assert_eq!(materialized.display_name, "writer");
        assert_eq!(materialized.description.as_deref(), Some("写作辅助"));
        assert!(materialized.remote_digest.starts_with("sha256:"));
        assert_eq!(materialized.version, materialized.remote_digest);
        assert_eq!(
            materialized.source_ref,
            remote_skill_url_source_ref(RemoteSkillKind::Github, normalized_url)
        );
        assert!(
            materialized
                .source_ref
                .starts_with("market:skill-url:github:")
        );
        assert_eq!(materialized.payload.files.len(), 2);
        assert!(!materialized.payload.disable_model_invocation);
    }

    #[test]
    fn remote_materializer_rejects_missing_skill_md_and_binary_payload() {
        let missing = materialize_remote_skill_template(RemoteSkillTemplateInput {
            source_kind: RemoteSkillKind::Clawhub,
            normalized_url: "https://clawhub.ai/skills/writer".to_string(),
            files: vec![RemoteSkillFile {
                path: "references/style.md".to_string(),
                body: RemoteSkillFileBody::Text("保持简洁".to_string()),
            }],
        });
        assert!(matches!(
            missing,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));

        let binary = materialize_remote_skill_template(RemoteSkillTemplateInput {
            source_kind: RemoteSkillKind::SkillsSh,
            normalized_url: "https://skills.sh/writer".to_string(),
            files: vec![
                RemoteSkillFile {
                    path: "SKILL.md".to_string(),
                    body: RemoteSkillFileBody::Text(
                        "---\nname: writer\ndescription: \"写作辅助\"\n---\n# Body\n".to_string(),
                    ),
                },
                RemoteSkillFile {
                    path: "assets/logo.png".to_string(),
                    body: RemoteSkillFileBody::Bytes(vec![0, 1, 2, 3]),
                },
            ],
        });
        assert!(matches!(
            binary,
            Err(SkillAssetApplicationError::BadRequest(message))
                if message.contains("二进制文件")
        ));
    }

    #[tokio::test]
    async fn remote_import_target_guard_only_allows_same_installed_source() {
        let repo = MemorySkillAssetRepository::default();
        let project_id = Uuid::new_v4();

        let user_skill = SkillAsset::new_user(project_id, "writer", "writer", "写作辅助", false);
        repo.create(&user_skill).await.expect("create user skill");
        let conflict = ensure_remote_import_target_available(
            &repo,
            project_id,
            "writer",
            "market:skill-url:github:abc",
        )
        .await;
        assert!(matches!(
            conflict,
            Err(SkillAssetApplicationError::Conflict(_))
        ));

        let source_ref = "market:skill-url:github:def";
        let mut installed =
            SkillAsset::new_user(project_id, "research", "research", "调研辅助", false);
        installed.installed_source =
            Some(agentdash_domain::shared_library::InstalledAssetSource::new(
                Uuid::new_v4(),
                source_ref,
                "sha256:version",
                "sha256:digest",
            ));
        repo.create(&installed)
            .await
            .expect("create installed skill");

        ensure_remote_import_target_available(&repo, project_id, "research", source_ref)
            .await
            .expect("same source can overwrite");
        let other_source = ensure_remote_import_target_available(
            &repo,
            project_id,
            "research",
            "market:skill-url:github:other",
        )
        .await;
        assert!(matches!(
            other_source,
            Err(SkillAssetApplicationError::Conflict(_))
        ));

        let same_source_different_key =
            ensure_remote_import_target_available(&repo, project_id, "renamed", source_ref).await;
        assert!(matches!(
            same_source_different_key,
            Err(SkillAssetApplicationError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn remote_import_upserts_remote_imported_library_asset() {
        let repo = MemoryLibraryAssetRepository::default();
        let first = materialize_remote_skill_template(RemoteSkillTemplateInput {
            source_kind: RemoteSkillKind::Github,
            normalized_url: "https://github.com/acme/skills/tree/main/writer".to_string(),
            files: vec![RemoteSkillFile {
                path: "SKILL.md".to_string(),
                body: RemoteSkillFileBody::Text(
                    "---\nname: writer\ndescription: \"写作辅助\"\n---\n# Body\n".to_string(),
                ),
            }],
        })
        .expect("first materialize");
        let source_ref = first.source_ref.clone();
        let first_asset = upsert_remote_imported_skill_template(&repo, "user-1".to_string(), first)
            .await
            .expect("first upsert");

        let second = materialize_remote_skill_template(RemoteSkillTemplateInput {
            source_kind: RemoteSkillKind::Github,
            normalized_url: "https://github.com/acme/skills/tree/main/writer".to_string(),
            files: vec![RemoteSkillFile {
                path: "SKILL.md".to_string(),
                body: RemoteSkillFileBody::Text(
                    "---\nname: writer\ndescription: \"更新后的描述\"\n---\n# Body\n".to_string(),
                ),
            }],
        })
        .expect("second materialize");
        let second_asset =
            upsert_remote_imported_skill_template(&repo, "user-1".to_string(), second)
                .await
                .expect("second upsert");

        assert_eq!(second_asset.id, first_asset.id);
        assert_eq!(second_asset.source, LibraryAssetSource::RemoteImported);
        assert_eq!(
            second_asset.source_ref.as_deref(),
            Some(source_ref.as_str())
        );
        assert_ne!(second_asset.payload_digest, first_asset.payload_digest);
    }
}
