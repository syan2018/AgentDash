//! `skill_asset_fs` mount：把项目级 SkillAsset 只读投影为 `skills/<key>/...`。

use agentdash_diagnostics::{diag, Subsystem};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agentdash_domain::common::StoredFileContent;
use agentdash_domain::skill_asset::{
    SkillAsset, SkillAssetFile, SkillAssetFileKind, SkillAssetRepository,
};
use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use super::mount::{
    PROVIDER_SKILL_ASSET_FS, SKILL_ASSET_KEYS_METADATA_KEY, SKILL_ASSET_PROJECT_ID_METADATA_KEY,
};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountEditCapabilities, MountError, MountOperationContext, MountProvider, SearchMatch,
    SearchQuery, SearchResult,
};
use super::types::{
    BinaryReadResult, ExecRequest, ExecResult, ListOptions, ListResult, ReadResult,
};
use agentdash_domain::common::Mount;
use agentdash_spi::platform::mount::RuntimeFileEntry;

const MAX_SKILL_KEY_LENGTH: usize = 64;
const MAX_SKILL_DESCRIPTION_LENGTH: usize = 1024;

#[derive(Debug, Clone)]
pub(crate) struct ProjectedSkillAssetFile {
    content: StoredFileContent,
    size_bytes: u64,
    kind: String,
    /// Asset 级 updated_at 毫秒时间戳。同一 asset 下所有文件共享此值（悲观 token 策略，
    /// 与 canvas 一致）—— skill asset 没有 file 级版本号，asset 任意修改都触发 token 失效。
    asset_updated_at_ms: i64,
}

fn map_mount_err(error: String) -> MountError {
    MountError::OperationFailed(error)
}

fn map_domain_err(error: agentdash_domain::DomainError) -> MountError {
    MountError::OperationFailed(error.to_string())
}

fn parse_projected_skill_path(path: &str) -> Result<(String, String), MountError> {
    let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
    let rest = path.strip_prefix("skills/").ok_or_else(|| {
        MountError::OperationFailed(format!("SkillAsset 路径必须位于 skills/: {path}"))
    })?;
    let (key, relative_path) = rest
        .split_once('/')
        .ok_or_else(|| MountError::OperationFailed(format!("SkillAsset 路径缺少文件名: {path}")))?;
    if key.trim().is_empty() || relative_path.trim().is_empty() {
        return Err(MountError::OperationFailed(format!(
            "SkillAsset 路径非法: {path}"
        )));
    }
    Ok((key.to_string(), relative_path.to_string()))
}

pub(crate) fn parse_skill_asset_mount_metadata(
    mount: &Mount,
) -> Result<(Uuid, Vec<String>), MountError> {
    let project_id = mount
        .metadata
        .get(SKILL_ASSET_PROJECT_ID_METADATA_KEY)
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            MountError::OperationFailed(format!(
                "mount {} 缺少 {}",
                mount.id, SKILL_ASSET_PROJECT_ID_METADATA_KEY
            ))
        })?;
    let project_id = Uuid::parse_str(project_id)
        .map_err(|error| MountError::OperationFailed(format!("project_id 无效: {error}")))?;
    let keys = mount
        .metadata
        .get(SKILL_ASSET_KEYS_METADATA_KEY)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok((project_id, keys))
}

pub(crate) async fn load_projected_skill_files(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
) -> Result<BTreeMap<String, ProjectedSkillAssetFile>, MountError> {
    let (project_id, keys) = parse_skill_asset_mount_metadata(mount)?;
    let mut files = BTreeMap::new();
    for key in keys {
        let Some(asset) = repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
        else {
            diag!(Warn, Subsystem::Skill,
        
                project_id = %project_id,
                skill_asset_key = %key,
                "Agent preset 引用了不存在的 SkillAsset，VFS projection 已跳过"
            );
            continue;
        };
        let asset_updated_at_ms = asset.updated_at.timestamp_millis();
        for file in asset.files {
            files.insert(
                format!("skills/{}/{}", asset.key, file.path),
                ProjectedSkillAssetFile {
                    content: file.content,
                    size_bytes: file.size_bytes,
                    kind: file.kind.tag().to_string(),
                    asset_updated_at_ms,
                },
            );
        }
    }
    Ok(files)
}

pub(crate) async fn read_projected_skill_file(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
    path: &str,
) -> Result<ReadResult, MountError> {
    let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
    let files = load_projected_skill_files(repo, mount).await?;
    let file = files
        .get(&path)
        .ok_or_else(|| MountError::NotFound(format!("SkillAsset 文件不存在: {path}")))?;
    let content = file
        .content
        .text_content()
        .ok_or_else(|| MountError::NotSupported(format!("SkillAsset 文件不是文本文件: {path}")))?;
    let updated_at_ms = file.asset_updated_at_ms;
    Ok(ReadResult::new(path, content.to_string())
        .with_version_token(format!("skill:{}", updated_at_ms))
        .with_modified_at(updated_at_ms))
}

pub(crate) async fn read_projected_skill_file_binary(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
    path: &str,
) -> Result<BinaryReadResult, MountError> {
    let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
    let files = load_projected_skill_files(repo, mount).await?;
    let file = files
        .get(&path)
        .ok_or_else(|| MountError::NotFound(format!("SkillAsset 文件不存在: {path}")))?;
    let bytes = file
        .content
        .binary_content()
        .ok_or_else(|| MountError::NotSupported(format!("SkillAsset 文件不是二进制文件: {path}")))?
        .to_vec();
    let mime_type = file
        .content
        .mime_type()
        .ok_or_else(|| MountError::OperationFailed(format!("二进制文件缺少 MIME: {path}")))?
        .to_string();
    Ok(BinaryReadResult::new(path, bytes, mime_type).with_attributes(skill_file_attributes(file)))
}

pub(crate) async fn list_projected_skill_files(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
    options: &ListOptions,
) -> Result<ListResult, MountError> {
    let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
    let files = load_projected_skill_files(repo, mount).await?;
    let mut entries =
        list_projected_entries(&files, &path, options.pattern.as_deref(), options.recursive);
    for entry in &mut entries {
        entry.is_virtual = true;
    }
    Ok(ListResult { entries })
}

fn list_projected_entries(
    files: &BTreeMap<String, ProjectedSkillAssetFile>,
    base_path: &str,
    pattern: Option<&str>,
    recursive: bool,
) -> Vec<crate::RuntimeFileEntry> {
    let normalized_base = base_path.trim_matches('/');
    let mut dirs = BTreeSet::new();
    let mut file_entries = BTreeMap::new();

    for (path, file) in files {
        let matches_base = if normalized_base.is_empty() {
            true
        } else {
            path == normalized_base
                || path
                    .strip_prefix(normalized_base)
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        if !matches_base {
            continue;
        }

        let relative = if normalized_base.is_empty() {
            path.as_str()
        } else if path == normalized_base {
            ""
        } else {
            path.strip_prefix(normalized_base)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or("")
        };

        if relative.is_empty() {
            file_entries.insert(path.clone(), file.clone());
            continue;
        }

        let parts = relative.split('/').collect::<Vec<_>>();
        if recursive {
            let full_parts = path.split('/').collect::<Vec<_>>();
            for depth in 1..full_parts.len() {
                dirs.insert(full_parts[..depth].join("/"));
            }
            file_entries.insert(path.clone(), file.clone());
        } else if parts.len() == 1 {
            file_entries.insert(path.clone(), file.clone());
        } else {
            let dir_path = if normalized_base.is_empty() {
                parts[0].to_string()
            } else {
                format!("{}/{}", normalized_base, parts[0])
            };
            dirs.insert(dir_path);
        }
    }

    let normalized_pattern = pattern.map(str::trim).filter(|value| !value.is_empty());
    let mut entries = Vec::new();
    for dir in dirs {
        if projected_path_matches_pattern(&dir, normalized_pattern) {
            entries.push(RuntimeFileEntry {
                path: dir,
                size: None,
                modified_at: None,
                is_dir: true,
                is_virtual: false,
                attributes: None,
            });
        }
    }
    for (path, file) in file_entries {
        if projected_path_matches_pattern(&path, normalized_pattern) {
            entries.push(RuntimeFileEntry {
                path,
                size: Some(file.size_bytes),
                modified_at: None,
                is_dir: false,
                is_virtual: false,
                attributes: Some(skill_file_attributes(&file)),
            });
        }
    }
    entries
}

fn projected_path_matches_pattern(path: &str, pattern: Option<&str>) -> bool {
    match pattern {
        None => true,
        Some(pat)
            if pat.contains('*') || pat.contains('?') || pat.contains('[') || pat.contains('{') =>
        {
            globset::Glob::new(pat)
                .ok()
                .map(|g| g.compile_matcher().is_match(path))
                .unwrap_or(false)
        }
        Some(pat) => path.contains(pat),
    }
}

#[derive(Debug, Clone)]
struct ParsedSkillMetadata {
    name: String,
    description: String,
    disable_model_invocation: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "disable-model-invocation", default)]
    disable_model_invocation: bool,
}

fn parse_skill_metadata(content: &str) -> Result<ParsedSkillMetadata, MountError> {
    let frontmatter = parse_skill_file_frontmatter(content).ok_or_else(|| {
        MountError::OperationFailed("SKILL.md 必须包含 YAML frontmatter".to_string())
    })?;
    let name = frontmatter
        .name
        .as_deref()
        .map(validate_skill_key)
        .transpose()?
        .ok_or_else(|| MountError::OperationFailed("SKILL.md 缺少 name".to_string()))?;
    let description = frontmatter
        .description
        .as_deref()
        .ok_or_else(|| MountError::OperationFailed("SKILL.md 缺少 description".to_string()))?
        .trim()
        .to_string();
    validate_description(&description)?;
    Ok(ParsedSkillMetadata {
        name,
        description,
        disable_model_invocation: frontmatter.disable_model_invocation,
    })
}

fn parse_skill_file_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let content = content.trim_start_matches('\u{feff}');
    if !content.starts_with("---") {
        return None;
    }
    let after_open = &content[3..];
    let close_pos = after_open.find("\n---")?;
    serde_yaml::from_str(&after_open[..close_pos]).ok()
}

fn validate_skill_key(key: &str) -> Result<String, MountError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(MountError::OperationFailed(
            "skill key 不能为空".to_string(),
        ));
    }
    if key.len() > MAX_SKILL_KEY_LENGTH {
        return Err(MountError::OperationFailed(format!(
            "skill key 不能超过 {MAX_SKILL_KEY_LENGTH} 字符"
        )));
    }
    if !key
        .chars()
        .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '-'))
    {
        return Err(MountError::OperationFailed(
            "skill key 只能包含小写字母、数字和连字符".to_string(),
        ));
    }
    Ok(key.to_string())
}

fn validate_description(description: &str) -> Result<(), MountError> {
    let value = description.trim();
    if value.is_empty() {
        return Err(MountError::OperationFailed(
            "description 不能为空".to_string(),
        ));
    }
    if value.len() > MAX_SKILL_DESCRIPTION_LENGTH {
        return Err(MountError::OperationFailed(format!(
            "description 不能超过 {MAX_SKILL_DESCRIPTION_LENGTH} 字符"
        )));
    }
    Ok(())
}

async fn ensure_skill_key_available(
    repo: &dyn SkillAssetRepository,
    project_id: Uuid,
    key: &str,
    current_asset_id: Uuid,
) -> Result<(), MountError> {
    let existing = repo
        .get_by_project_and_key(project_id, key)
        .await
        .map_err(map_domain_err)?;
    if existing.is_some_and(|asset| asset.id != current_asset_id) {
        return Err(MountError::OperationFailed(format!(
            "Project SkillAsset key 已存在: {key}"
        )));
    }
    Ok(())
}

fn set_skill_file(
    files: &mut Vec<SkillAssetFile>,
    skill_asset_id: Uuid,
    relative_path: String,
    content: StoredFileContent,
) {
    if let Some(existing) = files.iter_mut().find(|file| file.path == relative_path) {
        existing.content = content;
        existing.kind = SkillAssetFileKind::from_path(&existing.path);
        existing.size_bytes = existing.content.size_bytes();
        existing.touch();
        return;
    }
    files.push(SkillAssetFile::new_with_content(
        skill_asset_id,
        relative_path,
        content,
        SkillAssetFileKind::Reference,
    ));
    if let Some(created) = files.last_mut() {
        created.kind = SkillAssetFileKind::from_path(&created.path);
    }
}

fn remove_skill_file(files: &mut Vec<SkillAssetFile>, relative_path: &str) -> bool {
    let before = files.len();
    files.retain(|file| file.path != relative_path);
    files.len() != before
}

fn rename_skill_file(
    files: &mut [SkillAssetFile],
    from_relative_path: &str,
    to_relative_path: String,
) -> bool {
    let Some(file) = files
        .iter_mut()
        .find(|file| file.path == from_relative_path)
    else {
        return false;
    };
    file.path = to_relative_path;
    file.kind = SkillAssetFileKind::from_path(&file.path);
    file.touch();
    true
}

async fn persist_skill_asset(
    repo: &dyn SkillAssetRepository,
    mut asset: SkillAsset,
    metadata: Option<ParsedSkillMetadata>,
    files: Vec<SkillAssetFile>,
) -> Result<(), MountError> {
    if let Some(metadata) = metadata {
        if metadata.name != asset.key {
            ensure_skill_key_available(repo, asset.project_id, &metadata.name, asset.id).await?;
            asset.key = metadata.name;
        }
        asset.description = metadata.description;
        asset.disable_model_invocation = metadata.disable_model_invocation;
    }
    asset.files = files;
    asset.touch();
    repo.update(&asset).await.map_err(map_domain_err)
}

fn skill_file_attributes(
    file: &ProjectedSkillAssetFile,
) -> serde_json::Map<String, serde_json::Value> {
    let mut attributes = serde_json::Map::new();
    attributes.insert(
        "content_kind".to_string(),
        serde_json::Value::String(file.content.kind().as_str().to_string()),
    );
    if let Some(mime_type) = file.content.mime_type() {
        attributes.insert(
            "mime_type".to_string(),
            serde_json::Value::String(mime_type.to_string()),
        );
    }
    attributes.insert(
        "skill_asset_file_kind".to_string(),
        serde_json::Value::String(file.kind.clone()),
    );
    attributes
}

pub(crate) async fn search_projected_skill_files(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
    query: &SearchQuery,
) -> Result<SearchResult, MountError> {
    let files = load_projected_skill_files(repo, mount).await?;
    let base_path = match &query.path {
        Some(path) => normalize_mount_relative_path(path, true).map_err(map_mount_err)?,
        None => String::new(),
    };
    let max_results = query.max_results.unwrap_or(usize::MAX);
    let mut matches = Vec::new();
    let pattern = if query.case_sensitive {
        query.pattern.clone()
    } else {
        query.pattern.to_lowercase()
    };

    for (file_path, file) in files {
        if !base_path.is_empty()
            && file_path != base_path
            && !file_path
                .strip_prefix(&base_path)
                .is_some_and(|rest| rest.starts_with('/'))
        {
            continue;
        }
        let Some(content) = file.content.text_content() else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            let haystack = if query.case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            if haystack.contains(&pattern) {
                matches.push(SearchMatch {
                    path: file_path.clone(),
                    line: Some((idx + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max_results {
                    return Ok(SearchResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }
    }

    Ok(SearchResult {
        matches,
        truncated: false,
    })
}

pub struct SkillAssetFsMountProvider {
    skill_asset_repo: Arc<dyn SkillAssetRepository>,
}

impl SkillAssetFsMountProvider {
    pub fn new(skill_asset_repo: Arc<dyn SkillAssetRepository>) -> Self {
        Self { skill_asset_repo }
    }
}

#[async_trait]
impl MountProvider for SkillAssetFsMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_SKILL_ASSET_FS
    }

    fn edit_capabilities(&self, mount: &Mount) -> MountEditCapabilities {
        if mount.supports(agentdash_spi::MountCapability::Write) {
            MountEditCapabilities {
                create: true,
                delete: true,
                rename: true,
            }
        } else {
            MountEditCapabilities::default()
        }
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        read_projected_skill_file(self.skill_asset_repo.as_ref(), mount, path).await
    }

    async fn read_binary(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<BinaryReadResult, MountError> {
        read_projected_skill_file_binary(self.skill_asset_repo.as_ref(), mount, path).await
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let (project_id, _keys) = parse_skill_asset_mount_metadata(mount)?;
        let (key, relative_path) = parse_projected_skill_path(path)?;
        let asset = self
            .skill_asset_repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("SkillAsset 不存在: {key}")))?;
        let mut files = asset.files.clone();
        set_skill_file(
            &mut files,
            asset.id,
            relative_path.clone(),
            StoredFileContent::text(content),
        );
        let metadata = if relative_path == "SKILL.md" {
            Some(parse_skill_metadata(content)?)
        } else {
            None
        };
        persist_skill_asset(self.skill_asset_repo.as_ref(), asset, metadata, files).await
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let (project_id, _keys) = parse_skill_asset_mount_metadata(mount)?;
        let (key, relative_path) = parse_projected_skill_path(path)?;
        if relative_path == "SKILL.md" {
            return Err(MountError::OperationFailed(
                "不能通过文件 primitive 删除 Skill 主文档".to_string(),
            ));
        }
        let asset = self
            .skill_asset_repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("SkillAsset 不存在: {key}")))?;
        let mut files = asset.files.clone();
        if !remove_skill_file(&mut files, &relative_path) {
            return Err(MountError::NotFound(format!(
                "SkillAsset 文件不存在: skills/{key}/{relative_path}"
            )));
        }
        persist_skill_asset(self.skill_asset_repo.as_ref(), asset, None, files).await
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let (project_id, _keys) = parse_skill_asset_mount_metadata(mount)?;
        let (from_key, from_relative_path) = parse_projected_skill_path(from_path)?;
        let (to_key, to_relative_path) = parse_projected_skill_path(to_path)?;
        if from_key != to_key {
            return Err(MountError::OperationFailed(
                "SkillAsset 文件重命名不能跨 Skill key".to_string(),
            ));
        }
        if from_relative_path == "SKILL.md" || to_relative_path == "SKILL.md" {
            return Err(MountError::OperationFailed(
                "不能通过文件 primitive 重命名 Skill 主文档".to_string(),
            ));
        }
        let asset = self
            .skill_asset_repo
            .get_by_project_and_key(project_id, &from_key)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("SkillAsset 不存在: {from_key}")))?;
        if asset.files.iter().any(|file| file.path == to_relative_path) {
            return Err(MountError::OperationFailed(format!(
                "目标文件已存在: skills/{to_key}/{to_relative_path}"
            )));
        }
        let mut files = asset.files.clone();
        if !rename_skill_file(&mut files, &from_relative_path, to_relative_path.clone()) {
            return Err(MountError::NotFound(format!(
                "SkillAsset 文件不存在: skills/{from_key}/{from_relative_path}"
            )));
        }
        persist_skill_asset(self.skill_asset_repo.as_ref(), asset, None, files).await
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        list_projected_skill_files(self.skill_asset_repo.as_ref(), mount, options).await
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        search_projected_skill_files(self.skill_asset_repo.as_ref(), mount, query).await
    }

    async fn exec(
        &self,
        _mount: &Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "skill_asset_fs 不支持 exec".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::load_skills_from_vfs;
    use crate::{MountProviderRegistry, VfsService, build_project_skill_asset_management_mount};
    use agentdash_domain::DomainError;
    use agentdash_domain::common::MountCapability;
    use agentdash_domain::skill_asset::{SkillAsset, SkillAssetFile};
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemorySkillAssetRepo {
        assets: Mutex<Vec<SkillAsset>>,
    }

    #[async_trait::async_trait]
    impl SkillAssetRepository for InMemorySkillAssetRepo {
        async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            self.assets.lock().unwrap().push(asset.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.id == id)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.project_id == project_id && asset.key == key)
                .cloned())
        }

        async fn get_by_project_and_builtin_key(
            &self,
            _project_id: Uuid,
            _builtin_key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .filter(|asset| asset.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            let mut assets = self.assets.lock().unwrap();
            let Some(existing) = assets.iter_mut().find(|existing| existing.id == asset.id) else {
                return Err(DomainError::NotFound {
                    entity: "skill_asset",
                    id: asset.id.to_string(),
                });
            };
            *existing = asset.clone();
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn repo_with_skill(project_id: Uuid) -> Arc<InMemorySkillAssetRepo> {
        let repo = Arc::new(InMemorySkillAssetRepo::default());
        let mut asset = SkillAsset::new_user(project_id, "writer", "Writer", "写作辅助", true);
        asset.files = vec![
            SkillAssetFile::new(
                asset.id,
                "SKILL.md",
                "---\nname: writer\ndescription: \"写作辅助\"\ndisable-model-invocation: true\n---\n# Writer\n",
                agentdash_domain::skill_asset::SkillAssetFileKind::Skill,
            ),
            SkillAssetFile::new(
                asset.id,
                "references/style.md",
                "style",
                agentdash_domain::skill_asset::SkillAssetFileKind::Reference,
            ),
            SkillAssetFile::new_binary(
                asset.id,
                "assets/logo.png",
                vec![0, 1, 2, 3],
                "image/png",
                agentdash_domain::skill_asset::SkillAssetFileKind::Asset,
            ),
        ];
        repo.assets.lock().unwrap().push(asset);
        repo
    }

    #[tokio::test]
    async fn skill_asset_projection_is_discoverable_by_existing_skill_loader() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(SkillAssetFsMountProvider::new(repo)));
        let service = VfsService::new(Arc::new(registry));
        let vfs = agentdash_spi::Vfs {
            mounts: vec![build_project_skill_asset_management_mount(
                project_id,
                &["writer".to_string()],
            )],
            default_mount_id: None,
            source_project_id: Some(project_id.to_string()),
            source_story_id: None,
            links: Vec::new(),
        };

        let result = load_skills_from_vfs(&service, &vfs).await;

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "writer");
        assert!(result.skills[0].disable_model_invocation);
        assert_eq!(
            result.skills[0].file_path.to_string_lossy(),
            "skill-assets://skills/writer/SKILL.md"
        );
    }

    #[tokio::test]
    async fn skill_asset_mount_lists_only_selected_keys() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo);
        let mut mount =
            build_project_skill_asset_management_mount(project_id, &["writer".to_string()]);
        mount.capabilities = vec![MountCapability::Read, MountCapability::List];
        let entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "skills".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("list")
            .entries;

        assert!(entries.iter().any(|entry| entry.path == "skills/writer"));
    }

    #[tokio::test]
    async fn skill_asset_mount_exposes_binary_metadata_and_skips_text_read_search() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo);
        let mut mount =
            build_project_skill_asset_management_mount(project_id, &["writer".to_string()]);
        mount.capabilities = vec![
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ];
        let entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "skills/writer".to_string(),
                    pattern: None,
                    recursive: true,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("list")
            .entries;
        let logo = entries
            .iter()
            .find(|entry| entry.path == "skills/writer/assets/logo.png")
            .expect("logo entry");
        assert_eq!(logo.size, Some(4));
        assert_eq!(
            logo.attributes
                .as_ref()
                .and_then(|attrs| attrs.get("content_kind"))
                .and_then(|value| value.as_str()),
            Some("binary")
        );
        assert_eq!(
            logo.attributes
                .as_ref()
                .and_then(|attrs| attrs.get("mime_type"))
                .and_then(|value| value.as_str()),
            Some("image/png")
        );

        let read_result = provider
            .read_text(
                &mount,
                "skills/writer/assets/logo.png",
                &MountOperationContext::default(),
            )
            .await;
        assert!(matches!(read_result, Err(MountError::NotSupported(_))));

        let search = provider
            .search_text(
                &mount,
                &SearchQuery {
                    path: Some("skills/writer".to_string()),
                    pattern: "PNG".to_string(),
                    case_sensitive: false,
                    ..Default::default()
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search");
        assert!(search.matches.is_empty());
    }

    #[tokio::test]
    async fn skill_asset_mount_reads_binary_file() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo);
        let mut mount =
            build_project_skill_asset_management_mount(project_id, &["writer".to_string()]);
        mount.capabilities = vec![MountCapability::Read, MountCapability::List];

        let result = provider
            .read_binary(
                &mount,
                "skills/writer/assets/logo.png",
                &MountOperationContext::default(),
            )
            .await
            .expect("read binary");

        assert_eq!(result.path, "skills/writer/assets/logo.png");
        assert_eq!(result.data, vec![0, 1, 2, 3]);
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(
            result
                .attributes
                .as_ref()
                .and_then(|attrs| attrs.get("skill_asset_file_kind"))
                .and_then(|value| value.as_str()),
            Some("asset")
        );
    }

    #[tokio::test]
    async fn writable_skill_asset_mount_updates_extra_files_through_primitives() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo.clone());
        let mut mount =
            build_project_skill_asset_management_mount(project_id, &["writer".to_string()]);
        mount.capabilities = vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ];
        mount.default_write = true;

        provider
            .write_text(
                &mount,
                "skills/writer/references/new.md",
                "new content",
                &MountOperationContext::default(),
            )
            .await
            .expect("write extra file");
        provider
            .rename_text(
                &mount,
                "skills/writer/references/new.md",
                "skills/writer/references/renamed.md",
                &MountOperationContext::default(),
            )
            .await
            .expect("rename extra file");
        provider
            .delete_text(
                &mount,
                "skills/writer/references/renamed.md",
                &MountOperationContext::default(),
            )
            .await
            .expect("delete extra file");

        let asset = repo
            .get_by_project_and_key(project_id, "writer")
            .await
            .expect("repo query")
            .expect("asset");
        assert!(
            asset
                .files
                .iter()
                .all(|file| file.path != "references/renamed.md")
        );

        provider
            .write_text(
                &mount,
                "skills/writer/SKILL.md",
                "---\nname: writer\ndescription: \"更新后的描述\"\ndisable-model-invocation: true\n---\n# Writer\n",
                &MountOperationContext::default(),
            )
            .await
            .expect("write skill file");
        let asset = repo
            .get_by_project_and_key(project_id, "writer")
            .await
            .expect("repo query")
            .expect("asset");
        assert_eq!(asset.description, "更新后的描述");
        assert!(asset.disable_model_invocation);
    }

    #[tokio::test]
    async fn writable_skill_asset_mount_rejects_skill_document_delete_and_rename() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo.clone());
        let mut mount =
            build_project_skill_asset_management_mount(project_id, &["writer".to_string()]);
        mount.capabilities = vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
        ];
        mount.default_write = true;

        let delete_result = provider
            .delete_text(
                &mount,
                "skills/writer/SKILL.md",
                &MountOperationContext::default(),
            )
            .await;
        assert!(matches!(delete_result, Err(MountError::OperationFailed(_))));

        let rename_from_result = provider
            .rename_text(
                &mount,
                "skills/writer/SKILL.md",
                "skills/writer/SKILL.old.md",
                &MountOperationContext::default(),
            )
            .await;
        assert!(matches!(
            rename_from_result,
            Err(MountError::OperationFailed(_))
        ));

        let rename_to_result = provider
            .rename_text(
                &mount,
                "skills/writer/references/style.md",
                "skills/writer/SKILL.md",
                &MountOperationContext::default(),
            )
            .await;
        assert!(matches!(
            rename_to_result,
            Err(MountError::OperationFailed(_))
        ));

        let asset = repo
            .get_by_project_and_key(project_id, "writer")
            .await
            .expect("repo query")
            .expect("asset");
        assert!(asset.files.iter().any(|file| file.path == "SKILL.md"));
    }
}
