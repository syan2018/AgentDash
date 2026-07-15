use agentdash_agent_protocol::ContextFrameSection;

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::{
    NormalizedContextSurfaceDelta, NormalizedContextSurfaceState,
};

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    target: &NormalizedContextSurfaceState,
    phase_node: &str,
) -> Option<ProjectedSurfaceDimension> {
    if delta.capability_keys.is_empty() {
        return None;
    }
    let effective = target.capability_keys.iter().cloned().collect::<Vec<_>>();
    let mut sections = vec![format!(
        "## Capability State Update — Step Transition: {phase_node}"
    )];
    if !delta.capability_keys.added.is_empty() {
        let mut block = vec!["### Added Capabilities".to_string()];
        for key in &delta.capability_keys.added {
            let description = capability_description(key);
            if description.is_empty() {
                block.push(format!("- **{key}**"));
            } else {
                block.push(format!("- **{key}**: {description}"));
            }
        }
        sections.push(block.join("\n"));
    }
    if !delta.capability_keys.removed.is_empty() {
        let mut block = vec!["### Removed Capabilities".to_string()];
        for key in &delta.capability_keys.removed {
            let description = capability_description(key);
            if description.is_empty() {
                block.push(format!("- **{key}**（不再可用）"));
            } else {
                block.push(format!("- **{key}**: {description}（不再可用）"));
            }
        }
        sections.push(block.join("\n"));
    }
    let effective_block = if effective.is_empty() {
        "- （无）".to_string()
    } else {
        effective
            .iter()
            .map(|key| format!("- `{key}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("### Effective Capabilities\n{effective_block}"));
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::CapabilityKeyDelta {
            added_capabilities: delta.capability_keys.added.clone(),
            removed_capabilities: delta.capability_keys.removed.clone(),
            effective_capabilities: effective,
        },
        rendered_text: sections.join("\n\n"),
    })
}

fn capability_description(key: &str) -> &'static str {
    match key {
        "file_read" => "文件读取（mounts_list / fs_read / fs_glob / fs_grep）",
        "file_write" => "文件写入（fs_apply_patch）",
        "shell_execute" => "Shell 命令执行",
        "workspace_module" => "Workspace Module 创建、调用与展示（含 Canvas）",
        "workflow" => "Lifecycle 推进与产物上报",
        "collaboration" => "结构化协作请求、回应与活动回传",
        "task" => "Task 读取与维护（task_read / task_write）",
        "story_management" => "Story 上下文管理、Task 创建与批量拆解、状态推进",
        "relay_management" => "项目管理、Story 创建与状态变更",
        "workflow_management" => "Workflow / Lifecycle 定义的查看、创建与编辑",
        value if value.starts_with("mcp:") => "外部自定义 MCP 工具集",
        _ => "",
    }
}
