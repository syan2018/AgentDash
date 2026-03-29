use std::sync::Arc;

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
}

/// Per-session 的内联文件写入覆盖层。
///
/// 设计目标：
/// - 同一 session 内 write 后立即可 read（write-through cache）
/// - 写入同时通过 `InlineContentPersister` 持久化到 DB
/// - 多个 Agent 工具共享同一个 overlay（`Arc<InlineContentOverlay>`）
pub struct InlineContentOverlay {
    overrides: tokio::sync::RwLock<std::collections::HashMap<(String, String), String>>,
    persister: Arc<dyn InlineContentPersister>,
}

impl InlineContentOverlay {
    pub fn new(persister: Arc<dyn InlineContentPersister>) -> Self {
        Self {
            overrides: Default::default(),
            persister,
        }
    }

    pub async fn read(&self, mount_id: &str, path: &str) -> Option<String> {
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
    pub async fn overridden_files(
        &self,
        mount_id: &str,
    ) -> std::collections::HashMap<String, String> {
        self.overrides
            .read()
            .await
            .iter()
            .filter(|((mid, _), _)| mid == mount_id)
            .map(|((_, path), content)| (path.clone(), content.clone()))
            .collect()
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

        // 1. 写入本地覆盖缓存（立即可读）
        self.overrides
            .write()
            .await
            .insert((mount.id.clone(), path.to_string()), content.to_string());

        // 2. 持久化到 DB
        self.persister
            .persist_write(
                project_id,
                address_space.source_story_id.as_deref(),
                container_id,
                path,
                content,
            )
            .await
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

        // 优先尝试在 story 中查找（story 级 container 覆盖 project 级）
        if let Some(story_id_str) = source_story_id {
            let story_uuid =
                uuid::Uuid::parse_str(story_id_str).map_err(|e| format!("无效的 story_id: {e}"))?;
            if let Some(mut story) = self
                .story_repo
                .get_by_id(story_uuid)
                .await
                .map_err(|e| format!("加载 story 失败: {e}"))?
                && story
                    .context
                    .context_containers
                    .iter()
                    .any(|c| c.id.trim() == container_id)
            {
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
        }

        // 回退到 project
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
}
