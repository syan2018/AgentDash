//! 系统级 guidelines 帧。
//!
//! 承载"基础身份之外的系统级指引"：用户偏好（来自 settings）与项目指引
//! （来自 VFS 发现的 AGENTS.md 等）。与 identity 帧同走
//! `connector_context` 系统通道、`system` 角色，由连接器拼进最终 system prompt。
//!
//! 单一真相源约束：`rendered_text` **直接由 `sections()` 派生**（见
//! `render_sections`），不存在第二份手写拷贝，杜绝结构化表达与渲染文本漂移。

use agentdash_spi::DiscoveredGuideline;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, ProjectGuidelineEntry, RuntimeEventSource,
};

use super::context_frame::{self, ContextFramePayload};

/// guidelines 帧的 `kind` 标识。连接器与帧通道路由按此识别。
pub(crate) const SYSTEM_GUIDELINES_FRAME_KIND: &str = "system_guidelines";

pub(crate) struct GuidelinesFrameInput<'a> {
    pub user_preferences: &'a [String],
    pub discovered_guidelines: &'a [DiscoveredGuideline],
}

/// 构建 guidelines 帧。偏好与指引均为空（过滤后）时返回 `None`。
pub(crate) fn build_guidelines_context_frame(
    input: &GuidelinesFrameInput<'_>,
) -> Option<ContextFrame> {
    let preferences: Vec<String> = input
        .user_preferences
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let entries: Vec<ProjectGuidelineEntry> = input
        .discovered_guidelines
        .iter()
        .filter(|g| !g.content.trim().is_empty())
        .map(|g| ProjectGuidelineEntry {
            path: g.path.clone(),
            content: g.content.clone(),
        })
        .collect();

    if preferences.is_empty() && entries.is_empty() {
        return None;
    }

    Some(context_frame::build_context_frame(
        &GuidelinesContextFrame {
            preferences,
            entries,
        },
    ))
}

#[derive(Debug, Clone)]
struct GuidelinesContextFrame {
    preferences: Vec<String>,
    entries: Vec<ProjectGuidelineEntry>,
}

impl ContextFramePayload for GuidelinesContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("system-guidelines-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        SYSTEM_GUIDELINES_FRAME_KIND
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "prepared_for_connector".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "connector_context"
    }

    fn message_role(&self) -> &'static str {
        "system"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        let mut sections = Vec::new();
        if !self.preferences.is_empty() {
            sections.push(ContextFrameSection::UserPreferences {
                title: "User Preferences".to_string(),
                summary: "用户级偏好设置。".to_string(),
                items: self.preferences.clone(),
            });
        }
        if !self.entries.is_empty() {
            sections.push(ContextFrameSection::ProjectGuidelines {
                title: "Project Guidelines".to_string(),
                summary: "工作区中发现的项目级指引文件。".to_string(),
                entries: self.entries.clone(),
            });
        }
        sections
    }

    fn rendered_text(&self) -> String {
        render_sections(&self.sections())
    }
}

/// 由结构化 section 派生 `rendered_text`——guidelines 帧的唯一渲染入口。
fn render_sections(sections: &[ContextFrameSection]) -> String {
    sections
        .iter()
        .filter_map(render_section)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_section(section: &ContextFrameSection) -> Option<String> {
    match section {
        ContextFrameSection::UserPreferences { items, .. } => {
            let body = items
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n");
            (!body.is_empty()).then(|| format!("## User Preferences\n\n{body}"))
        }
        ContextFrameSection::ProjectGuidelines { entries, .. } => {
            let body = entries
                .iter()
                .filter(|entry| !entry.content.trim().is_empty())
                .map(|entry| format!("### {}\n\n{}", entry.path, entry.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            (!body.is_empty()).then(|| format!("## Project Guidelines\n\n{body}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guideline(path: &str, content: &str) -> DiscoveredGuideline {
        DiscoveredGuideline {
            file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
            mount_id: "workspace".to_string(),
            path: path.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn empty_inputs_produce_no_frame() {
        assert!(
            build_guidelines_context_frame(&GuidelinesFrameInput {
                user_preferences: &[],
                discovered_guidelines: &[],
            })
            .is_none()
        );
        // 仅有空白内容也应判定为空
        assert!(
            build_guidelines_context_frame(&GuidelinesFrameInput {
                user_preferences: &["   ".to_string()],
                discovered_guidelines: &[guideline("AGENTS.md", "  \n ")],
            })
            .is_none()
        );
    }

    #[test]
    fn rendered_text_is_derived_from_sections() {
        let frame = build_guidelines_context_frame(&GuidelinesFrameInput {
            user_preferences: &["使用中文".to_string()],
            discovered_guidelines: &[guideline("AGENTS.md", "项目约定")],
        })
        .expect("frame");

        // 单一真相源：rendered_text 必须等于由结构化 section 重新渲染的结果。
        assert_eq!(frame.rendered_text, render_sections(&frame.sections));
        assert!(frame.rendered_text.contains("## User Preferences"));
        assert!(frame.rendered_text.contains("- 使用中文"));
        assert!(frame.rendered_text.contains("## Project Guidelines"));
        assert!(frame.rendered_text.contains("### AGENTS.md"));
        assert!(frame.rendered_text.contains("项目约定"));
        assert_eq!(frame.kind, SYSTEM_GUIDELINES_FRAME_KIND);
        assert_eq!(frame.delivery_channel, "connector_context");
        assert_eq!(frame.message_role, "system");
    }

    #[test]
    fn preferences_only_omits_guidelines_section() {
        let frame = build_guidelines_context_frame(&GuidelinesFrameInput {
            user_preferences: &["偏好A".to_string()],
            discovered_guidelines: &[],
        })
        .expect("frame");
        assert!(frame.rendered_text.contains("## User Preferences"));
        assert!(!frame.rendered_text.contains("## Project Guidelines"));
    }
}
