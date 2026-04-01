use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::address_space::mount::{
    CONTEXT_OWNER_SCOPE_METADATA_KEY, CONTEXT_OWNER_SCOPE_PROJECT, CONTEXT_OWNER_SCOPE_STORY,
};
use crate::address_space::normalize_mount_relative_path;
use crate::runtime::{AddressSpace, Mount};
use async_trait::async_trait;

// ─── Inline Content Persistence ─────────────────────────────

/// 内联文件写入持久化接口。
/// 实现方负责将 inline_fs mount 的文件修改写回到对应的
/// Project/Story container 配置中。
#[async_trait]
pub trait InlineContentPersister: Send + Sync {
    /// 将文件内容持久化到归属的 container 定义。
    /// `source_project_id` / `source_story_id` 标识来源 owner，
    /// `container_id` 从 mount.root_ref 中解析（`context://inline/{id}`），
    /// `path` 为归一化后的文件路径。
    async fn persist_write(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String>;

    async fn persist_delete(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
    ) -> Result<(), String>;
}

type InlineOverrideMap = HashMap<(String, String), Option<String>>;

/// Per-session 的内联文件写入覆盖层。
///
/// 设计目标：
/// - 同一 session 内 write 后立即可 read（write-through cache）
/// - 写入同时通过 `InlineContentPersister` 持久化到 DB
/// - 多个 Agent 工具共享同一个 overlay（`Arc<InlineContentOverlay>`）
pub struct InlineContentOverlay {
    overrides: tokio::sync::RwLock<InlineOverrideMap>,
    persister: Arc<dyn InlineContentPersister>,
}

impl InlineContentOverlay {
    pub fn new(persister: Arc<dyn InlineContentPersister>) -> Self {
        Self {
            overrides: Default::default(),
            persister,
        }
    }

    pub async fn read_override(&self, mount_id: &str, path: &str) -> Option<Option<String>> {
        self.overrides
            .read()
            .await
            .get(&(mount_id.to_string(), path.to_string()))
            .cloned()
    }

    pub async fn has_override(&self, mount_id: &str, path: &str) -> bool {
        self.overrides
            .read()
            .await
            .contains_key(&(mount_id.to_string(), path.to_string()))
    }

    /// 返回指定 mount 下所有被覆盖的文件（用于 list 时合并新增文件）
    pub async fn apply_to_files(&self, mount_id: &str, files: &mut BTreeMap<String, String>) {
        let overrides = self.overrides.read().await;
        for ((mid, path), content) in overrides.iter() {
            if mid != mount_id {
                continue;
            }
            match content {
                Some(content) => {
                    files.insert(path.clone(), content.clone());
                }
                None => {
                    files.remove(path);
                }
            }
        }
    }

    pub async fn write(
        &self,
        address_space: &AddressSpace,
        mount: &Mount,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let container_id = mount
            .root_ref
            .strip_prefix("context://inline/")
            .ok_or_else(|| format!("无法从 root_ref 解析 container_id: {}", mount.root_ref))?;

        let project_id = address_space
            .source_project_id
            .as_deref()
            .ok_or("address space 缺少 source_project_id，无法持久化 inline 写入")?;
        let story_scope = story_scope_for_mount(address_space, mount)?;

        // 1. 写入本地覆盖缓存（立即可读）
        self.overrides.write().await.insert(
            (mount.id.clone(), path.to_string()),
            Some(content.to_string()),
        );

        // 2. 持久化到 DB
        self.persister
            .persist_write(project_id, story_scope, container_id, path, content)
            .await
    }

    pub async fn delete(
        &self,
        address_space: &AddressSpace,
        mount: &Mount,
        path: &str,
    ) -> Result<(), String> {
        let container_id = mount
            .root_ref
            .strip_prefix("context://inline/")
            .ok_or_else(|| format!("无法从 root_ref 解析 container_id: {}", mount.root_ref))?;

        let project_id = address_space
            .source_project_id
            .as_deref()
            .ok_or("address space 缺少 source_project_id，无法持久化 inline 删除")?;
        let story_scope = story_scope_for_mount(address_space, mount)?;

        self.overrides
            .write()
            .await
            .insert((mount.id.clone(), path.to_string()), None);

        self.persister
            .persist_delete(project_id, story_scope, container_id, path)
            .await
    }

    pub async fn sync_files(
        &self,
        address_space: &AddressSpace,
        mount: &Mount,
        before: &BTreeMap<String, String>,
        after: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for (path, content) in after {
            if before.get(path) != Some(content) {
                self.write(address_space, mount, path, content).await?;
            }
        }

        for path in before.keys() {
            if !after.contains_key(path) {
                self.delete(address_space, mount, path).await?;
            }
        }

        Ok(())
    }
}

fn story_scope_for_mount<'a>(
    address_space: &'a AddressSpace,
    mount: &Mount,
) -> Result<Option<&'a str>, String> {
    match mount
        .metadata
        .get(CONTEXT_OWNER_SCOPE_METADATA_KEY)
        .and_then(serde_json::Value::as_str)
    {
        Some(CONTEXT_OWNER_SCOPE_PROJECT) => Ok(None),
        Some(CONTEXT_OWNER_SCOPE_STORY) => address_space
            .source_story_id
            .as_deref()
            .map(Some)
            .ok_or(format!(
                "mount {} 标记为 story 级容器，但 address space 缺少 source_story_id",
                mount.id
            )),
        Some(other) => Err(format!(
            "mount {} 的 {} 值无效: {}",
            mount.id, CONTEXT_OWNER_SCOPE_METADATA_KEY, other
        )),
        None => {
            if address_space.source_story_id.is_some() {
                Err(format!(
                    "mount {} 缺少 {}，拒绝在 story 上下文中猜测 inline 容器归属",
                    mount.id, CONTEXT_OWNER_SCOPE_METADATA_KEY
                ))
            } else {
                Ok(None)
            }
        }
    }
}

// ─── DB Inline Content Persister ────────────────────────────

/// 基于 Project / Story Repository 的 InlineContentPersister 实现。
///
/// 将 inline_fs 的文件写入持久化到对应的 ContextContainerDefinition
/// (project.config.context_containers 或 story.context.context_containers)。
pub struct DbInlineContentPersister {
    project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
    story_repo: Arc<dyn agentdash_domain::story::StoryRepository>,
}

impl DbInlineContentPersister {
    pub fn new(
        project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
        story_repo: Arc<dyn agentdash_domain::story::StoryRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
        }
    }

    fn upsert_inline_file(
        containers: &mut [agentdash_domain::context_container::ContextContainerDefinition],
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let container = containers
            .iter_mut()
            .find(|c| c.id.trim() == container_id)
            .ok_or_else(|| format!("容器 {} 不存在", container_id))?;

        match &mut container.provider {
            agentdash_domain::context_container::ContextContainerProvider::InlineFiles {
                files,
            } => {
                if let Some(file) = files.iter_mut().find(|f| {
                    normalize_mount_relative_path(&f.path, false).unwrap_or_default() == path
                }) {
                    file.content = content.to_string();
                } else {
                    files.push(agentdash_domain::context_container::ContextContainerFile {
                        path: path.to_string(),
                        content: content.to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(format!("容器 {} 不是 inline_files 类型", container_id)),
        }
    }

    fn delete_inline_file(
        containers: &mut [agentdash_domain::context_container::ContextContainerDefinition],
        container_id: &str,
        path: &str,
    ) -> Result<(), String> {
        let container = containers
            .iter_mut()
            .find(|c| c.id.trim() == container_id)
            .ok_or_else(|| format!("容器 {} 不存在", container_id))?;

        match &mut container.provider {
            agentdash_domain::context_container::ContextContainerProvider::InlineFiles {
                files,
            } => {
                let before = files.len();
                files.retain(|file| {
                    normalize_mount_relative_path(&file.path, false).unwrap_or_default() != path
                });
                if files.len() == before {
                    return Err(format!("文件 {} 不存在于容器 {}", path, container_id));
                }
                Ok(())
            }
            _ => Err(format!("容器 {} 不是 inline_files 类型", container_id)),
        }
    }
}

#[async_trait]
impl InlineContentPersister for DbInlineContentPersister {
    async fn persist_write(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let project_uuid = uuid::Uuid::parse_str(source_project_id)
            .map_err(|e| format!("无效的 project_id: {e}"))?;

        if let Some(story_id_str) = source_story_id {
            let story_uuid =
                uuid::Uuid::parse_str(story_id_str).map_err(|e| format!("无效的 story_id: {e}"))?;
            let mut story = self
                .story_repo
                .get_by_id(story_uuid)
                .await
                .map_err(|e| format!("加载 story 失败: {e}"))?
                .ok_or_else(|| format!("story {} 不存在", story_id_str))?;
            if !story
                .context
                .context_containers
                .iter()
                .any(|c| c.id.trim() == container_id)
            {
                return Err(format!(
                    "story {} 中不存在容器 {}，拒绝回退到 project",
                    story_id_str, container_id
                ));
            }
            Self::upsert_inline_file(
                &mut story.context.context_containers,
                container_id,
                path,
                content,
            )?;
            self.story_repo
                .update(&story)
                .await
                .map_err(|e| format!("保存 story 失败: {e}"))?;
            return Ok(());
        }

        let mut project = self
            .project_repo
            .get_by_id(project_uuid)
            .await
            .map_err(|e| format!("加载 project 失败: {e}"))?
            .ok_or_else(|| format!("project {} 不存在", source_project_id))?;

        Self::upsert_inline_file(
            &mut project.config.context_containers,
            container_id,
            path,
            content,
        )?;
        self.project_repo
            .update(&project)
            .await
            .map_err(|e| format!("保存 project 失败: {e}"))?;

        Ok(())
    }

    async fn persist_delete(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
    ) -> Result<(), String> {
        let project_uuid = uuid::Uuid::parse_str(source_project_id)
            .map_err(|e| format!("无效的 project_id: {e}"))?;

        if let Some(story_id_str) = source_story_id {
            let story_uuid =
                uuid::Uuid::parse_str(story_id_str).map_err(|e| format!("无效的 story_id: {e}"))?;
            let mut story = self
                .story_repo
                .get_by_id(story_uuid)
                .await
                .map_err(|e| format!("加载 story 失败: {e}"))?
                .ok_or_else(|| format!("story {} 不存在", story_id_str))?;
            if !story
                .context
                .context_containers
                .iter()
                .any(|c| c.id.trim() == container_id)
            {
                return Err(format!(
                    "story {} 中不存在容器 {}，拒绝回退到 project",
                    story_id_str, container_id
                ));
            }
            Self::delete_inline_file(&mut story.context.context_containers, container_id, path)?;
            self.story_repo
                .update(&story)
                .await
                .map_err(|e| format!("保存 story 失败: {e}"))?;
            return Ok(());
        }

        let mut project = self
            .project_repo
            .get_by_id(project_uuid)
            .await
            .map_err(|e| format!("加载 project 失败: {e}"))?
            .ok_or_else(|| format!("project {} 不存在", source_project_id))?;

        Self::delete_inline_file(&mut project.config.context_containers, container_id, path)?;
        self.project_repo
            .update(&project)
            .await
            .map_err(|e| format!("保存 project 失败: {e}"))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::context_container::{
        ContextContainerDefinition, ContextContainerExposure, ContextContainerFile,
        ContextContainerProvider,
    };
    use agentdash_domain::project::{
        Project, ProjectRepository, ProjectSubjectGrant, ProjectSubjectType,
    };
    use agentdash_domain::story::{Story, StoryRepository};
    use std::sync::Mutex;

    #[derive(Clone)]
    struct TestProjectRepo {
        project: Arc<Mutex<Project>>,
    }

    #[async_trait]
    impl ProjectRepository for TestProjectRepo {
        async fn create(&self, _project: &Project) -> Result<(), DomainError> {
            unreachable!()
        }

        async fn get_by_id(&self, _id: uuid::Uuid) -> Result<Option<Project>, DomainError> {
            Ok(Some(self.project.lock().expect("lock").clone()))
        }

        async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
            unreachable!()
        }

        async fn update(&self, project: &Project) -> Result<(), DomainError> {
            *self.project.lock().expect("lock") = project.clone();
            Ok(())
        }

        async fn delete(&self, _id: uuid::Uuid) -> Result<(), DomainError> {
            unreachable!()
        }

        async fn list_subject_grants(
            &self,
            _project_id: uuid::Uuid,
        ) -> Result<Vec<ProjectSubjectGrant>, DomainError> {
            Ok(vec![])
        }

        async fn upsert_subject_grant(
            &self,
            _grant: &ProjectSubjectGrant,
        ) -> Result<(), DomainError> {
            unreachable!()
        }

        async fn delete_subject_grant(
            &self,
            _project_id: uuid::Uuid,
            _subject_type: ProjectSubjectType,
            _subject_id: &str,
        ) -> Result<(), DomainError> {
            unreachable!()
        }
    }

    #[derive(Clone)]
    struct TestStoryRepo {
        story: Arc<Mutex<Story>>,
    }

    #[async_trait]
    impl StoryRepository for TestStoryRepo {
        async fn create(&self, _story: &Story) -> Result<(), DomainError> {
            unreachable!()
        }

        async fn get_by_id(&self, _id: uuid::Uuid) -> Result<Option<Story>, DomainError> {
            Ok(Some(self.story.lock().expect("lock").clone()))
        }

        async fn list_by_project(
            &self,
            _project_id: uuid::Uuid,
        ) -> Result<Vec<Story>, DomainError> {
            Ok(vec![])
        }

        async fn update(&self, story: &Story) -> Result<(), DomainError> {
            *self.story.lock().expect("lock") = story.clone();
            Ok(())
        }

        async fn delete(&self, _id: uuid::Uuid) -> Result<(), DomainError> {
            unreachable!()
        }
    }

    fn inline_container(id: &str, path: &str) -> ContextContainerDefinition {
        ContextContainerDefinition {
            id: id.to_string(),
            mount_id: id.to_string(),
            display_name: id.to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: "seed".to_string(),
                }],
            },
            capabilities: vec![],
            default_write: true,
            exposure: ContextContainerExposure::default(),
        }
    }

    #[tokio::test]
    async fn story_scope_write_does_not_fallback_to_project() {
        let mut project = Project::new("proj".to_string(), "desc".to_string());
        project.config.context_containers = vec![inline_container("project-only", "project.md")];

        let story = Story::new(project.id, "story".to_string(), "desc".to_string());
        let story_id = story.id;
        let persister = DbInlineContentPersister::new(
            Arc::new(TestProjectRepo {
                project: Arc::new(Mutex::new(project)),
            }),
            Arc::new(TestStoryRepo {
                story: Arc::new(Mutex::new(story)),
            }),
        );

        let err = persister
            .persist_write(
                &uuid::Uuid::new_v4().to_string(),
                Some(&story_id.to_string()),
                "project-only",
                "project.md",
                "updated",
            )
            .await
            .expect_err("story scope should not fallback to project");
        assert!(err.contains("拒绝回退到 project"));
    }

    #[test]
    fn story_context_requires_explicit_mount_owner_scope() {
        let address_space = AddressSpace {
            source_project_id: Some(uuid::Uuid::new_v4().to_string()),
            source_story_id: Some(uuid::Uuid::new_v4().to_string()),
            ..Default::default()
        };
        let mount = Mount {
            id: "brief".to_string(),
            provider: "inline_fs".to_string(),
            backend_id: String::new(),
            root_ref: "context://inline/brief".to_string(),
            capabilities: vec![],
            default_write: true,
            display_name: "brief".to_string(),
            metadata: serde_json::json!({ "files": { "brief.md": "hello" } }),
        };

        let err = story_scope_for_mount(&address_space, &mount)
            .expect_err("story scope should require explicit owner metadata");
        assert!(err.contains(CONTEXT_OWNER_SCOPE_METADATA_KEY));
    }
}
