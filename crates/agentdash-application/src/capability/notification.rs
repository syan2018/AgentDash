//! 能力变更结构化 Markdown 生成器
//!
//! 设计意图：格式对齐 `McpInjectionConfig::to_context_content()` 的人类可读风格
//! （二级标题 + 列表 + 一句话描述），但发生在 step transition 时，作为 out-of-band
//! 的 user message 注入到 agent 对话尾部 —— KV cache 前缀不受影响。
//!
//! Agent 内置的 `ToolDefinition` 自动化机制仍然承担"告诉 LLM 有哪些工具可用"的职责；
//! 本 Markdown 的作用是让 LLM 在对话层面**显式感知**"能力刚刚变化了"。

use std::collections::BTreeSet;

use agentdash_spi::hooks::CapabilityDelta;
use agentdash_spi::tool_capability::{
    self, CAP_CANVAS, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_RELAY_MANAGEMENT,
    CAP_SHELL_EXECUTE, CAP_STORY_MANAGEMENT, CAP_TASK_MANAGEMENT, CAP_WORKFLOW,
    CAP_WORKFLOW_MANAGEMENT, ToolCapability,
};

/// 能力 key 的人类可读短描述 —— 与 `McpInjectionConfig::to_context_content` 保持口径一致。
pub fn capability_description(key: &str) -> &'static str {
    match key {
        CAP_FILE_READ => "文件读取（mounts_list / fs_read / fs_glob / fs_grep）",
        CAP_FILE_WRITE => "文件写入（fs_apply_patch）",
        CAP_SHELL_EXECUTE => "Shell 命令执行",
        CAP_CANVAS => "Canvas 绘制与展示",
        CAP_WORKFLOW => "Lifecycle 推进与产物上报",
        CAP_COLLABORATION => "Companion 协作 + Hook action 解析",
        CAP_STORY_MANAGEMENT => "Story 上下文管理、Task 创建与批量拆解、状态推进",
        CAP_TASK_MANAGEMENT => "Task 状态更新、执行产物上报、兄弟 Task 查看、Story 上下文读取",
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
    delta: &CapabilityDelta,
    effective_caps: &BTreeSet<String>,
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

    sections.push("> 工具 schema 已同步更新，可直接调用上述能力；历史对话未被改写。".to_string());

    sections.join("\n\n")
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
        let delta = CapabilityDelta {
            added: vec!["file_read".to_string(), "mcp:code_analyzer".to_string()],
            removed: vec!["canvas".to_string()],
        };
        let effective: BTreeSet<String> =
            ["file_read".to_string(), "mcp:code_analyzer".to_string()]
                .into_iter()
                .collect();

        let md = build_capability_delta_markdown("implement", &delta, &effective);

        assert!(md.contains("## Capability Update — Step Transition: implement"));
        assert!(md.contains("### Added Capabilities"));
        assert!(md.contains("**file_read**: 文件读取"));
        assert!(md.contains("**mcp:code_analyzer**: 外部自定义 MCP 工具集"));
        assert!(md.contains("### Removed Capabilities"));
        assert!(md.contains("**canvas**: Canvas 绘制与展示（不再可用）"));
        assert!(md.contains("### Effective Capabilities"));
        assert!(md.contains("`file_read`"));
    }

    #[test]
    fn delta_markdown_handles_empty_effective() {
        let delta = CapabilityDelta {
            added: vec![],
            removed: vec!["workflow".to_string()],
        };
        let effective = BTreeSet::new();

        let md = build_capability_delta_markdown("wrap_up", &delta, &effective);

        assert!(md.contains("Removed Capabilities"));
        assert!(!md.contains("### Added Capabilities"));
        assert!(md.contains("### Effective Capabilities\n- （无）"));
    }

    #[test]
    fn is_known_capability_key_accepts_well_known_and_mcp() {
        assert!(is_known_capability_key("file_read"));
        assert!(is_known_capability_key("mcp:code_analyzer"));
        assert!(!is_known_capability_key("random_nonsense"));
    }
}
