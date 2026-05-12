//! `skill_asset_fs` mount：把项目级 SkillAsset 只读投影为 `skills/<key>/...`。

use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_domain::skill_asset::SkillAssetRepository;
use async_trait::async_trait;
use uuid::Uuid;

use crate::skill_asset::{SkillAssetFileInput, SkillAssetService, UpdateSkillAssetInput};

use super::mount::{
    PROVIDER_SKILL_ASSET_FS, SKILL_ASSET_KEYS_METADATA_KEY, SKILL_ASSET_PROJECT_ID_METADATA_KEY,
    list_inline_entries,
};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountEditCapabilities, MountError, MountOperationContext, MountProvider, SearchMatch,
    SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::Mount;

fn map_mount_err(error: String) -> MountError {
    MountError::OperationFailed(error)
}

fn map_domain_err(error: agentdash_domain::DomainError) -> MountError {
    MountError::OperationFailed(error.to_string())
}

fn map_app_err(error: crate::skill_asset::SkillAssetApplicationError) -> MountError {
    use crate::skill_asset::SkillAssetApplicationError as Error;
    match error {
        Error::NotFound(message) => MountError::NotFound(message),
        Error::BadRequest(message) | Error::Conflict(message) | Error::Internal(message) => {
            MountError::OperationFailed(message)
        }
    }
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
) -> Result<BTreeMap<String, String>, MountError> {
    let (project_id, keys) = parse_skill_asset_mount_metadata(mount)?;
    let mut files = BTreeMap::new();
    for key in keys {
        let Some(asset) = repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
        else {
            tracing::warn!(
                project_id = %project_id,
                skill_asset_key = %key,
                "Agent preset 引用了不存在的 SkillAsset，VFS projection 已跳过"
            );
            continue;
        };
        for file in asset.files {
            files.insert(format!("skills/{}/{}", asset.key, file.path), file.content);
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
    let content = files
        .get(&path)
        .cloned()
        .ok_or_else(|| MountError::NotFound(format!("SkillAsset 文件不存在: {path}")))?;
    Ok(ReadResult::new(path, content))
}

pub(crate) async fn list_projected_skill_files(
    repo: &dyn SkillAssetRepository,
    mount: &Mount,
    options: &ListOptions,
) -> Result<ListResult, MountError> {
    let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
    let files = load_projected_skill_files(repo, mount).await?;
    let mut entries =
        list_inline_entries(&files, &path, options.pattern.as_deref(), options.recursive);
    for entry in &mut entries {
        entry.is_virtual = true;
    }
    Ok(ListResult { entries })
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

    for (file_path, content) in files {
        if !base_path.is_empty()
            && file_path != base_path
            && !file_path
                .strip_prefix(&base_path)
                .is_some_and(|rest| rest.starts_with('/'))
        {
            continue;
        }
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
                    return Ok(SearchResult { matches });
                }
            }
        }
    }

    Ok(SearchResult { matches })
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

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let (project_id, _keys) = parse_skill_asset_mount_metadata(mount)?;
        let (key, relative_path) = parse_projected_skill_path(path)?;
        let service = SkillAssetService::new(self.skill_asset_repo.as_ref());
        let asset = self
            .skill_asset_repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("SkillAsset 不存在: {key}")))?;
        let mut files = asset
            .files
            .iter()
            .map(|file| SkillAssetFileInput {
                path: file.path.clone(),
                content: file.content.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(existing) = files.iter_mut().find(|file| file.path == relative_path) {
            existing.content = content.to_string();
        } else {
            files.push(SkillAssetFileInput {
                path: relative_path,
                content: content.to_string(),
            });
        }
        service
            .update(
                asset.id,
                UpdateSkillAssetInput {
                    files: Some(files),
                    ..Default::default()
                },
            )
            .await
            .map_err(map_app_err)?;
        Ok(())
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
        let service = SkillAssetService::new(self.skill_asset_repo.as_ref());
        let asset = self
            .skill_asset_repo
            .get_by_project_and_key(project_id, &key)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("SkillAsset 不存在: {key}")))?;
        if !asset.files.iter().any(|file| file.path == relative_path) {
            return Err(MountError::NotFound(format!(
                "SkillAsset 文件不存在: skills/{key}/{relative_path}"
            )));
        }
        let files = asset
            .files
            .iter()
            .filter(|file| file.path != relative_path)
            .map(|file| SkillAssetFileInput {
                path: file.path.clone(),
                content: file.content.clone(),
            })
            .collect::<Vec<_>>();
        service
            .update(
                asset.id,
                UpdateSkillAssetInput {
                    files: Some(files),
                    ..Default::default()
                },
            )
            .await
            .map_err(map_app_err)?;
        Ok(())
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
        let service = SkillAssetService::new(self.skill_asset_repo.as_ref());
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
        let mut found = false;
        let files = asset
            .files
            .iter()
            .map(|file| {
                if file.path == from_relative_path {
                    found = true;
                    SkillAssetFileInput {
                        path: to_relative_path.clone(),
                        content: file.content.clone(),
                    }
                } else {
                    SkillAssetFileInput {
                        path: file.path.clone(),
                        content: file.content.clone(),
                    }
                }
            })
            .collect::<Vec<_>>();
        if !found {
            return Err(MountError::NotFound(format!(
                "SkillAsset 文件不存在: skills/{from_key}/{from_relative_path}"
            )));
        }
        service
            .update(
                asset.id,
                UpdateSkillAssetInput {
                    files: Some(files),
                    ..Default::default()
                },
            )
            .await
            .map_err(map_app_err)?;
        Ok(())
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
    use crate::vfs::{MountProviderRegistry, RelayVfsService, build_skill_asset_mount};
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
        let service = RelayVfsService::new(Arc::new(registry));
        let vfs = agentdash_spi::Vfs {
            mounts: vec![build_skill_asset_mount(project_id, &["writer".to_string()])],
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
        let mut mount = build_skill_asset_mount(project_id, &["writer".to_string()]);
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
    async fn writable_skill_asset_mount_updates_extra_files_through_primitives() {
        let project_id = Uuid::new_v4();
        let repo = repo_with_skill(project_id);
        let provider = SkillAssetFsMountProvider::new(repo.clone());
        let mut mount = build_skill_asset_mount(project_id, &["writer".to_string()]);
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
    }
}
