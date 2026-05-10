//! 能力状态更新的维度策略模块
//!
//! 每个维度（capability key / tool path / MCP / VFS / skill / tool schema）
//! 独立实现 `DimensionDelta` trait，自治 delta 计算、section 构造和 rendered_text 生成。
//! `RuntimeContextUpdateFrame` 仅做编排：收集各维度产出并委托渲染。

pub(crate) mod capability_key;
pub(crate) mod mcp_server;
pub(crate) mod skill;
pub(crate) mod tool_path;
pub(crate) mod tool_schema;
pub(crate) mod vfs;

use agentdash_spi::hooks::ContextFrameSection;

/// 各维度 delta 的统一协议。
///
/// 每个维度模块实现此 trait 后，由 `RuntimeContextUpdateFrame` 统一编排。
pub(crate) trait DimensionDelta: std::fmt::Debug + Send + Sync {
    /// 此维度是否存在有意义的变更。
    fn has_changes(&self) -> bool;

    /// 构造该维度对应的 `ContextFrameSection`（协议层类型，序列化到前端）。
    fn to_section(&self) -> ContextFrameSection;

    /// 渲染面向 Agent 的 Markdown 文本块。
    fn render_text(&self, phase_node: Option<&str>) -> String;
}
