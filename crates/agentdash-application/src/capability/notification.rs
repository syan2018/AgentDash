//! 能力变更结构化 Markdown 生成器
//!
//! 设计意图：格式对齐 `McpInjectionConfig::to_context_content()` 的人类可读风格
//! （二级标题 + 列表 + 一句话描述），但发生在 step transition 时，作为 out-of-band
//! 的 user message 注入到 agent 对话尾部 —— KV cache 前缀不受影响。
//!
//! Agent 内置的 `ToolDefinition` 自动化机制仍然承担"告诉 LLM 有哪些工具可用"的职责；
//! 本 Markdown 的作用是让 LLM 在对话层面**显式感知**"能力刚刚变化了"。

use std::collections::BTreeSet;

use agentdash_platform_spi::platform::tool_capability::{
    self, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_RELAY_MANAGEMENT,
    CAP_SHELL_EXECUTE, CAP_STORY_MANAGEMENT, CAP_TASK, CAP_WORKFLOW, CAP_WORKFLOW_MANAGEMENT,
    CAP_WORKSPACE_MODULE, ToolCapability,
};

use crate::agent_run::runtime_capability::{CapabilityStateDelta, SetDelta};

/// 能力 key 的人类可读短描述 —— 与 `McpInjectionConfig::to_context_content` 保持口径一致。
pub fn capability_description(key: &str) -> &'static str {
    match key {
        CAP_FILE_READ => "文件读取（mounts_list / fs_read / fs_glob / fs_grep）",
        CAP_FILE_WRITE => "文件写入（fs_apply_patch）",
        CAP_SHELL_EXECUTE => "Shell 命令执行",
        CAP_WORKSPACE_MODULE => "Workspace Module 创建、调用与展示（含 Canvas）",
        CAP_WORKFLOW => "Lifecycle 推进与产物上报",
        CAP_COLLABORATION => "结构化协作请求、回应与活动回传",
        CAP_TASK => "Task 读取与维护（task_read / task_write）",
        CAP_STORY_MANAGEMENT => "Story 上下文管理、Task 创建与批量拆解、状态推进",
        CAP_RELAY_MANAGEMENT => "项目管理、Story 创建与状态变更",
        CAP_WORKFLOW_MANAGEMENT => "Workflow / Lifecycle 定义的查看、创建与编辑",
        _ => {
            if ToolCapability::new(key).is_custom_mcp() {
                "外部自定义 MCP 工具集"
            } else {
                ""
            }
        }
    }
}

/// 生成 step transition 的结构化 delta Markdown。
///
/// 格式参考 `## Capability Update — Step Transition` PRD 模板，
/// 并在末尾附加当前 effective 能力清单，便于 agent 对齐状态。
pub fn build_capability_delta_markdown(
    phase_node_key: &str,
    delta: &SetDelta,
    effective_caps: &BTreeSet<String>,
    state_delta: Option<&CapabilityStateDelta>,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!(
        "## Capability Update — Step Transition: {phase_node_key}"
    ));

    if !delta.added.is_empty() {
        let mut block = vec!["### Added Capabilities".to_string()];
        for key in &delta.added {
            let desc = capability_description(key);
            if desc.is_empty() {
                block.push(format!("- **{key}**"));
            } else {
                block.push(format!("- **{key}**: {desc}"));
            }
        }
        sections.push(block.join("\n"));
    }

    if !delta.removed.is_empty() {
        let mut block = vec!["### Removed Capabilities".to_string()];
        for key in &delta.removed {
            let desc = capability_description(key);
            if desc.is_empty() {
                block.push(format!("- **{key}** — 不再可用"));
            } else {
                block.push(format!("- **{key}**: {desc}（不再可用）"));
            }
        }
        sections.push(block.join("\n"));
    }

    let caps_block = if effective_caps.is_empty() {
        "- （无）".to_string()
    } else {
        effective_caps
            .iter()
            .map(|k| format!("- `{k}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("### Effective Capabilities\n{caps_block}"));

    if let Some(state_delta) = state_delta
        && let Some(block) = build_tool_state_block(state_delta)
    {
        sections.push(block);
    }

    if delta.is_empty()
        && state_delta.is_none_or(|state_delta| {
            state_delta.excluded_tool_paths.is_empty()
                && state_delta.included_tool_paths.is_empty()
                && state_delta.mcp_servers.is_empty()
        })
    {
        sections.push("> 本次没有 capability key 或工具级状态变化；历史对话未被改写。".to_string());
    } else {
        sections.push(
            "> 工具状态已按上述 capability 与 tool path 更新；历史对话未被改写。".to_string(),
        );
    }

    sections.join("\n\n")
}

fn build_tool_state_block(state_delta: &CapabilityStateDelta) -> Option<String> {
    let mut lines = vec!["### Tool State Changes".to_string()];

    append_path_lines(
        &mut lines,
        "Blocked tool paths",
        &state_delta.excluded_tool_paths.added,
        "不再暴露",
    );
    append_path_lines(
        &mut lines,
        "Unblocked tool paths",
        &state_delta.excluded_tool_paths.removed,
        "重新暴露",
    );
    append_path_lines(
        &mut lines,
        "Whitelisted tool paths",
        &state_delta.included_tool_paths.added,
        "进入白名单",
    );
    append_path_lines(
        &mut lines,
        "Removed whitelist paths",
        &state_delta.included_tool_paths.removed,
        "移出白名单",
    );
    append_path_lines(
        &mut lines,
        "Added MCP servers",
        &state_delta.mcp_servers.added,
        "已注入",
    );
    append_path_lines(
        &mut lines,
        "Removed MCP servers",
        &state_delta.mcp_servers.removed,
        "已移除",
    );

    (lines.len() > 1).then(|| lines.join("\n"))
}

fn append_path_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("- {title}:"));
    for value in values {
        lines.push(format!("  - `{value}` — {suffix}"));
    }
}

/// 防御性检查：key 是否属于 well-known 集合或 `mcp:*` 格式。
/// 主要用于在构造 delta 时过滤无效 key，保证输出稳定。
pub fn is_known_capability_key(key: &str) -> bool {
    let cap = ToolCapability::new(key);
    cap.is_well_known() || cap.is_custom_mcp() || tool_capability::WELL_KNOWN_KEYS.contains(&key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_markdown_covers_added_removed_and_effective() {
        let delta = SetDelta {
            added: vec!["file_read".to_string(), "mcp:code_analyzer".to_string()],
            removed: vec!["workflow".to_string()],
        };
        let effective: BTreeSet<String> =
            ["file_read".to_string(), "mcp:code_analyzer".to_string()]
                .into_iter()
                .collect();

        let md = build_capability_delta_markdown("implement", &delta, &effective, None);

        assert!(md.contains("## Capability Update — Step Transition: implement"));
        assert!(md.contains("### Added Capabilities"));
        assert!(md.contains("**file_read**: 文件读取"));
        assert!(md.contains("**mcp:code_analyzer**: 外部自定义 MCP 工具集"));
        assert!(md.contains("### Removed Capabilities"));
        assert!(md.contains("**workflow**: Lifecycle 推进与产物上报（不再可用）"));
        assert!(md.contains("### Effective Capabilities"));
        assert!(md.contains("`file_read`"));
    }

    #[test]
    fn delta_markdown_handles_empty_effective() {
        let delta = SetDelta {
            added: vec![],
            removed: vec!["workflow".to_string()],
        };
        let effective = BTreeSet::new();

        let md = build_capability_delta_markdown("wrap_up", &delta, &effective, None);

        assert!(md.contains("Removed Capabilities"));
        assert!(!md.contains("### Added Capabilities"));
        assert!(md.contains("### Effective Capabilities\n- （无）"));
    }

    #[test]
    fn delta_markdown_reports_tool_state_changes_without_key_delta() {
        let delta = SetDelta::default();
        let effective: BTreeSet<String> = ["workflow_management".to_string()].into_iter().collect();
        let state_delta = CapabilityStateDelta {
            excluded_tool_paths: crate::agent_run::runtime_capability::SetDelta {
                added: vec![
                    "workflow_management::upsert_workflow_tool".to_string(),
                    "workflow_management::upsert_lifecycle_tool".to_string(),
                ],
                removed: vec![],
            },
            ..Default::default()
        };

        let md = build_capability_delta_markdown("plan", &delta, &effective, Some(&state_delta));

        assert!(md.contains("### Tool State Changes"));
        assert!(md.contains("workflow_management::upsert_workflow_tool"));
        assert!(md.contains("不再暴露"));
        assert!(!md.contains("工具 schema 已同步更新，可直接调用上述能力"));
    }

    #[test]
    fn is_known_capability_key_accepts_well_known_and_mcp() {
        assert!(is_known_capability_key("file_read"));
        assert!(is_known_capability_key("mcp:code_analyzer"));
        assert!(!is_known_capability_key("random_nonsense"));
    }
}
