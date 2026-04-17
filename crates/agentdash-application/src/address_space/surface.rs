use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 已解析后的 Address Space 展示面。
///
/// 这是前端浏览/摘要展示/运行时诊断应共享的唯一 mount 真相源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedAddressSpaceSurface {
    pub surface_ref: String,
    pub source: ResolvedAddressSpaceSurfaceSource,
    pub mounts: Vec<ResolvedMountSummary>,
    pub default_mount_id: Option<String>,
}

/// 触发 surface 解析的来源。
///
/// 不同 source 可以有不同的 mount 派生规则，但最终都必须收敛到同一份 surface DTO。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum ResolvedAddressSpaceSurfaceSource {
    ProjectPreview {
        project_id: Uuid,
    },
    StoryPreview {
        project_id: Uuid,
        story_id: Uuid,
    },
    TaskPreview {
        project_id: Uuid,
        task_id: Uuid,
    },
    SessionRuntime {
        session_id: String,
    },
    ProjectAgentKnowledge {
        project_id: Uuid,
        agent_id: Uuid,
        link_id: Uuid,
    },
}

impl ResolvedAddressSpaceSurfaceSource {
    /// 生成稳定的 surface_ref，供前后端后续基于该 surface 做 mount 读写操作。
    pub fn surface_ref(&self) -> String {
        match self {
            Self::ProjectPreview { project_id } => format!("project-preview:{project_id}"),
            Self::StoryPreview {
                project_id,
                story_id,
            } => format!("story-preview:{project_id}:{story_id}"),
            Self::TaskPreview {
                project_id,
                task_id,
            } => format!("task-preview:{project_id}:{task_id}"),
            Self::SessionRuntime { session_id } => {
                format!("session-runtime:{}", session_id.trim())
            }
            Self::ProjectAgentKnowledge {
                project_id,
                agent_id,
                link_id,
            } => format!("project-agent-knowledge:{project_id}:{agent_id}:{link_id}"),
        }
    }

    pub fn parse_surface_ref(surface_ref: &str) -> Result<Self, String> {
        let trimmed = surface_ref.trim();
        if let Some(rest) = trimmed.strip_prefix("project-preview:") {
            let project_id =
                Uuid::parse_str(rest).map_err(|_| format!("无效的 project preview surface_ref: {trimmed}"))?;
            return Ok(Self::ProjectPreview { project_id });
        }
        if let Some(rest) = trimmed.strip_prefix("story-preview:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 story preview surface_ref: {trimmed}"))?;
            let story_id = parts
                .next()
                .ok_or_else(|| format!("无效的 story preview surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 story preview surface_ref: {trimmed}"));
            }
            return Ok(Self::StoryPreview {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 story preview project_id: {project_id}"))?,
                story_id: Uuid::parse_str(story_id)
                    .map_err(|_| format!("无效的 story preview story_id: {story_id}"))?,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("task-preview:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 task preview surface_ref: {trimmed}"))?;
            let task_id = parts
                .next()
                .ok_or_else(|| format!("无效的 task preview surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 task preview surface_ref: {trimmed}"));
            }
            return Ok(Self::TaskPreview {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 task preview project_id: {project_id}"))?,
                task_id: Uuid::parse_str(task_id)
                    .map_err(|_| format!("无效的 task preview task_id: {task_id}"))?,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("session-runtime:") {
            let session_id = rest.trim();
            if session_id.is_empty() {
                return Err(format!("无效的 session runtime surface_ref: {trimmed}"));
            }
            return Ok(Self::SessionRuntime {
                session_id: session_id.to_string(),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("project-agent-knowledge:") {
            let mut parts = rest.split(':');
            let project_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent knowledge surface_ref: {trimmed}"))?;
            let agent_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent knowledge surface_ref: {trimmed}"))?;
            let link_id = parts
                .next()
                .ok_or_else(|| format!("无效的 agent knowledge surface_ref: {trimmed}"))?;
            if parts.next().is_some() {
                return Err(format!("无效的 agent knowledge surface_ref: {trimmed}"));
            }
            return Ok(Self::ProjectAgentKnowledge {
                project_id: Uuid::parse_str(project_id)
                    .map_err(|_| format!("无效的 agent knowledge project_id: {project_id}"))?,
                agent_id: Uuid::parse_str(agent_id)
                    .map_err(|_| format!("无效的 agent knowledge agent_id: {agent_id}"))?,
                link_id: Uuid::parse_str(link_id)
                    .map_err(|_| format!("无效的 agent knowledge link_id: {link_id}"))?,
            });
        }

        Err(format!("未知的 surface_ref: {trimmed}"))
    }
}

/// 已解析 mount 的统一摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedMountSummary {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: Vec<String>,
    pub default_write: bool,
    pub purpose: ResolvedMountPurpose,
    pub owner_kind: ResolvedMountOwnerKind,
    pub owner_id: String,
    pub container_id: Option<String>,
    pub backend_online: Option<bool>,
    pub file_count: Option<usize>,
}

/// mount 在 UI/诊断中的语义用途。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedMountPurpose {
    Workspace,
    ProjectContainer,
    StoryContainer,
    AgentKnowledge,
    Lifecycle,
    Canvas,
    ExternalService,
}

/// mount 所属的领域 owner。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedMountOwnerKind {
    Project,
    Story,
    Task,
    Session,
    ProjectAgentLink,
    Canvas,
    Workspace,
    External,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_agent_knowledge_surface_ref_is_stable() {
        let source = ResolvedAddressSpaceSurfaceSource::ProjectAgentKnowledge {
            project_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                .expect("project uuid"),
            agent_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                .expect("agent uuid"),
            link_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333")
                .expect("link uuid"),
        };

        assert_eq!(
            source.surface_ref(),
            "project-agent-knowledge:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222:33333333-3333-3333-3333-333333333333"
        );
    }

    #[test]
    fn session_runtime_surface_ref_trims_session_id() {
        let source = ResolvedAddressSpaceSurfaceSource::SessionRuntime {
            session_id: "  sess-1  ".to_string(),
        };

        assert_eq!(source.surface_ref(), "session-runtime:sess-1");
    }

    #[test]
    fn parse_story_preview_surface_ref() {
        let parsed = ResolvedAddressSpaceSurfaceSource::parse_surface_ref(
            "story-preview:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222",
        )
        .expect("parse story preview");

        assert_eq!(
            parsed,
            ResolvedAddressSpaceSurfaceSource::StoryPreview {
                project_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                    .expect("project uuid"),
                story_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                    .expect("story uuid"),
            }
        );
    }
}
