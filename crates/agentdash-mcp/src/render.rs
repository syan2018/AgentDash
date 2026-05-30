//! MCP `Content` 片段 → 文本的统一渲染。
//!
//! direct 与 local 历史上各自实现了一份近似的 `render_content`（rmcp `Content`
//! 片段 → 文本），是**唯一真正重复**的逻辑，收敛到此。
//!
//! 注意：曾经的 `MCP tool: <name>` 头、`structured_content:`/`content:` 分段标签
//! 属各调用点自己的取舍（且原本就不一致：local 不加头/不分段），不在此强求统一——
//! 那是把个别站点的习惯当成项目事实。各站点保留自己的成帧方式，只共享片段渲染。

use rmcp::model::{Content, ResourceContents};

/// 把单个 MCP `Content` 片段渲染为文本（文本 / 资源文本 / 兜底 pretty JSON）。
pub fn render_content(content: &Content) -> String {
    if let Some(text) = content.raw.as_text() {
        return text.text.clone();
    }

    if let Some(resource) = content.raw.as_resource() {
        return match &resource.resource {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            other => serde_json::to_string_pretty(other)
                .unwrap_or_else(|_| "<无法解析 MCP 资源内容>".to_string()),
        };
    }

    serde_json::to_string_pretty(content).unwrap_or_else(|_| "<无法解析 MCP 内容>".to_string())
}
