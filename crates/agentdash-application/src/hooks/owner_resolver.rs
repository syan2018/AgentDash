use std::sync::Arc;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;

use agentdash_connector_contract::{HookDiagnosticEntry, HookError, HookOwnerSummary};

use super::snapshot_helpers::{ResolvedOwnerSummary, task_status_tag};

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

fn session_binding_source_ref(binding: &SessionBinding) -> agentdash_connector_contract::HookSourceRef {
    agentdash_connector_contract::HookSourceRef {
        layer: agentdash_connector_contract::HookSourceLayer::Session,
        key: format!("binding:{}", binding.id),
        label: format!("Session Binding / {}", binding.label),
        priority: 500,
    }
}

fn task_source_ref(task_id: uuid::Uuid) -> agentdash_connector_contract::HookSourceRef {
    agentdash_connector_contract::HookSourceRef {
        layer: agentdash_connector_contract::HookSourceLayer::Task,
        key: task_id.to_string(),
        label: format!("Task / {task_id}"),
        priority: 400,
    }
}

fn source_summary_from_refs(source_refs: &[agentdash_connector_contract::HookSourceRef]) -> Vec<String> {
    source_refs
        .iter()
        .map(|source| {
            format!(
                "{}:{}",
                super::source_layer_tag(source.layer),
                source.key
            )
        })
        .collect()
}

/// 根据 SessionBinding 反查 project/story/task 实体，构建 HookOwnerSummary。
pub struct SessionOwnerResolver {
    project_repo: Arc<dyn ProjectRepository>,
    story_repo: Arc<dyn StoryRepository>,
    task_repo: Arc<dyn TaskRepository>,
}

impl SessionOwnerResolver {
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        story_repo: Arc<dyn StoryRepository>,
        task_repo: Arc<dyn TaskRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
            task_repo,
        }
    }

    pub async fn resolve(
        &self,
        binding: &SessionBinding,
    ) -> Result<ResolvedOwnerSummary, HookError> {
        let binding_source_refs = vec![session_binding_source_ref(binding)];
        let binding_source_summary = source_summary_from_refs(&binding_source_refs);
        let mut summary = HookOwnerSummary {
            owner_type: binding.owner_type.to_string(),
            owner_id: binding.owner_id.to_string(),
            label: None,
            project_id: None,
            story_id: None,
            task_id: None,
        };
        let mut diagnostics = vec![HookDiagnosticEntry {
            code: "session_binding_found".to_string(),
            summary: format!(
                "命中会话绑定：{} {}（label={}）",
                binding.owner_type, binding.owner_id, binding.label
            ),
            detail: None,
            source_summary: binding_source_summary.clone(),
            source_refs: binding_source_refs.clone(),
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
                        summary: "会话绑定引用的 Project 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: binding_source_summary.clone(),
                        source_refs: binding_source_refs.clone(),
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
                        summary: "会话绑定引用的 Story 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: binding_source_summary.clone(),
                        source_refs: binding_source_refs.clone(),
                    });
                }
            }
            SessionOwnerType::Task => {
                let task = self
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?;
                if let Some(task) = task {
                    summary.label = Some(task.title);
                    summary.task_id = Some(task.id.to_string());
                    summary.story_id = Some(task.story_id.to_string());

                    let story = self
                        .story_repo
                        .get_by_id(task.story_id)
                        .await
                        .map_err(map_hook_error)?;
                    if let Some(story) = story {
                        summary.project_id = Some(story.project_id.to_string());
                    } else {
                        diagnostics.push(HookDiagnosticEntry {
                            code: "task_story_missing".to_string(),
                            summary: "Task 对应的 Story 已不存在，无法补全 project_id".to_string(),
                            detail: Some(task.story_id.to_string()),
                            source_summary: source_summary_from_refs(&[task_source_ref(task.id)]),
                            source_refs: vec![task_source_ref(task.id)],
                        });
                    }
                } else {
                    diagnostics.push(HookDiagnosticEntry {
                        code: "session_binding_owner_missing".to_string(),
                        summary: "会话绑定引用的 Task 已不存在".to_string(),
                        detail: Some(binding.owner_id.to_string()),
                        source_summary: binding_source_summary,
                        source_refs: binding_source_refs,
                    });
                }
            }
        }

        Ok(ResolvedOwnerSummary {
            summary,
            diagnostics,
            task_status: match binding.owner_type {
                SessionOwnerType::Task => self
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(map_hook_error)?
                    .map(|task| task_status_tag(task.status).to_string()),
                SessionOwnerType::Project | SessionOwnerType::Story => None,
            },
        })
    }
}
