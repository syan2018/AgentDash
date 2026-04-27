use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::story::StoryRepository;

use agentdash_spi::{HookDiagnosticEntry, HookError, HookOwnerSummary};

use super::snapshot_helpers::ResolvedOwnerSummary;

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 SessionBinding 反查 project/story/task 实体，构建 HookOwnerSummary。
///
/// M1-b：Task 查询经 Story aggregate 完成（`story_repo.find_by_task_id`）。
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

    pub async fn resolve(
        &self,
        binding: &SessionBinding,
    ) -> Result<ResolvedOwnerSummary, HookError> {
        let mut summary = HookOwnerSummary {
            owner_type: binding.owner_type,
            owner_id: binding.owner_id.to_string(),
            label: None,
            project_id: None,
            story_id: None,
            task_id: None,
        };
        let mut diagnostics = vec![HookDiagnosticEntry {
            code: "session_binding_found".to_string(),
            message: format!(
                "命中会话绑定：{} {}（label={}）",
                binding.owner_type, binding.owner_id, binding.label
            ),
        }];

        match binding.owner_type {
            SessionOwnerType::Project => {
                let project = self
                    .project_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(project) = project {
                    summary.label = Some(project.name);
                    summary.project_id = Some(project.id.to_string());
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        message: "会话绑定引用的 Project 已不存在".to_string(),
                    });
                }
            }
            SessionOwnerType::Story => {
                let story = self
                    .story_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(story) = story {
                    summary.label = Some(story.title);
                    summary.project_id = Some(story.project_id.to_string());
                    summary.story_id = Some(story.id.to_string());
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        message: "会话绑定引用的 Story 已不存在".to_string(),
                    });
                }
            }
            SessionOwnerType::Task => {
                // M1-b：task 查询通过 Story aggregate 一次性拿到 task + story（含 project_id）
                let story = self
                    .story_repo
                    .find_by_task_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(story) = story {
                    if let Some(task) = story.find_task(binding.owner_id) {
                        summary.label = Some(task.title.clone());
                        summary.task_id = Some(task.id.to_string());
                        summary.story_id = Some(task.story_id.to_string());
                        summary.project_id = Some(story.project_id.to_string());
                    } else {
                        diagnostics.push(HookDiagnosticEntry {
                            code: "session_binding_owner_missing".to_string(),
                            message: "会话绑定引用的 Task 已不存在".to_string(),
                        });
                    }
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        message: "会话绑定引用的 Task 已不存在".to_string(),
                    });
                }
            }
        }

        Ok(ResolvedOwnerSummary {
            summary,
            diagnostics,
        })
    }
}
