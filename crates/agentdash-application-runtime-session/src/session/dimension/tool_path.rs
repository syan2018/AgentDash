//! 工具路径维度 — 追踪工具级路径的屏蔽 / 恢复 / 白名单变化。

use agentdash_spi::hooks::ContextFrameSection;

use super::DimensionDelta;
use agentdash_spi::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct ToolPathDimensionDelta {
    pub blocked: Vec<String>,
    pub unblocked: Vec<String>,
    pub whitelisted: Vec<String>,
    pub removed_whitelist: Vec<String>,
}

impl ToolPathDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        let delta = Self {
            blocked: state_delta.excluded_tool_paths.added.clone(),
            unblocked: state_delta.excluded_tool_paths.removed.clone(),
            whitelisted: state_delta.included_tool_paths.added.clone(),
            removed_whitelist: state_delta.included_tool_paths.removed.clone(),
        };
        if !delta.has_changes() {
            return None;
        }
        Some(Box::new(delta))
    }
}

impl DimensionDelta for ToolPathDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.blocked.is_empty()
            || !self.unblocked.is_empty()
            || !self.whitelisted.is_empty()
            || !self.removed_whitelist.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::ToolPathDelta {
            blocked_tool_paths: self.blocked.clone(),
            unblocked_tool_paths: self.unblocked.clone(),
            whitelisted_tool_paths: self.whitelisted.clone(),
            removed_whitelist_paths: self.removed_whitelist.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## Tool Path Changes — Step Transition: {node}"),
            None => "## Tool Path Changes".to_string(),
        }];

        append_path_lines(&mut lines, "Blocked tool paths", &self.blocked, "不再暴露");
        append_path_lines(
            &mut lines,
            "Unblocked tool paths",
            &self.unblocked,
            "重新暴露",
        );
        append_path_lines(
            &mut lines,
            "Whitelisted tool paths",
            &self.whitelisted,
            "进入白名单",
        );
        append_path_lines(
            &mut lines,
            "Removed whitelist paths",
            &self.removed_whitelist,
            "移出白名单",
        );

        lines.join("\n")
    }
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
