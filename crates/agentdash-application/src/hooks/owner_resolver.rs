use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_spi::hooks::SessionRunContext;
use agentdash_spi::CapabilityScope;
use uuid::Uuid;

/// 从 LifecycleRunLink 反查 project/story/task 实体，构建 SessionRunContext。
///
/// 当前阶段为过渡 stub：仅根据传入 project_id 构建最小 context，
/// 后续需接入 LifecycleRunLink 完整查询。
pub struct SessionOwnerResolver {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
}

impl SessionOwnerResolver {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
        }
    }

    pub fn story_repo(&self) -> &dyn StoryRepository {
        self.story_repo.as_ref()
    }

    pub fn project_repo(&self) -> &dyn ProjectRepository {
        self.project_repo.as_ref()
    }

    /// 根据 project_id 构建默认的 SessionRunContext stub。
    /// TODO: migrate to LifecycleRunLink query for full context resolution
    pub fn build_default_run_context(&self, project_id: Uuid) -> SessionRunContext {
        SessionRunContext {
            project_id,
            story_id: None,
            task_id: None,
            story_title: None,
            task_title: None,
            scope: CapabilityScope::Project,
        }
    }

    /// 根据已知的 story/task 信息构建完整 SessionRunContext。
    pub fn build_run_context(
        &self,
        project_id: Uuid,
        story_id: Option<Uuid>,
        task_id: Option<Uuid>,
        story_title: Option<String>,
        task_title: Option<String>,
    ) -> SessionRunContext {
        let scope = if task_id.is_some() {
            CapabilityScope::Task
        } else if story_id.is_some() {
            CapabilityScope::Story
        } else {
            CapabilityScope::Project
        };
        SessionRunContext {
            project_id,
            story_id,
            task_id,
            story_title,
            task_title,
            scope,
        }
    }
}
