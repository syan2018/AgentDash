//! Declared source 渲染共享 helper（PR 5c）。
//!
//! 声明式上下文来源（`ContextSourceRef`）的 fragment 构造在历史上有两处独立实现：
//! - `context/source_resolver.rs` 处理 `ManualText` / `HttpFetch` 等非 workspace kind。
//! - `context/workspace_sources.rs` 处理 `File` / `ProjectSnapshot`（需要 VFS）。
//!
//! 二者的 `fragment_slot` / `fragment_label` / `render_source_section` /
//! `display_source_label` / `truncate_text` 五个私有函数几乎逐行重复。
//! 本模块把这些纯渲染工具收拢到单点，两个 resolver 文件都走同一套模板。

use agentdash_domain::context_source::{ContextSlot, ContextSourceKind, ContextSourceRef};

/// Fragment slot 字符串映射（与 `ContextSlot` 枚举一一对应）。
pub fn fragment_slot(slot: &ContextSlot) -> &'static str {
    match slot {
        ContextSlot::Requirements => "requirements",
        ContextSlot::Constraints => "constraints",
        ContextSlot::Codebase => "codebase",
        ContextSlot::References => "references",
        ContextSlot::InstructionAppend => "instruction_append",
    }
}

/// Fragment label 字符串映射（按来源 kind 区分）。
pub fn fragment_label(kind: &ContextSourceKind) -> &'static str {
    match kind {
        ContextSourceKind::ManualText => "declared_manual_text",
        ContextSourceKind::File => "declared_file_source",
        ContextSourceKind::ProjectSnapshot => "declared_project_snapshot",
        ContextSourceKind::HttpFetch => "declared_http_fetch",
        ContextSourceKind::McpResource => "declared_mcp_resource",
        ContextSourceKind::EntityRef => "declared_entity_ref",
    }
}

/// 渲染单个 declared source section —— 统一走 `## 来源: {title}\n{content}` 模板。
pub fn render_source_section(source: &ContextSourceRef, content: String) -> String {
    let title = display_source_label(source);
    format!("## 来源: {title}\n{content}")
}

/// 返回 declared source 的显示名（优先 label，否则 locator）。
pub fn display_source_label(source: &ContextSourceRef) -> &str {
    source.label.as_deref().unwrap_or(source.locator.as_str())
}

/// 按字符数截断文本，超出时附上"内容已截断"提示。
pub fn truncate_text(content: String, max_chars: Option<usize>) -> String {
    const DEFAULT_TRUNCATE_CHARS: usize = 12_000;
    let max = max_chars.unwrap_or(DEFAULT_TRUNCATE_CHARS);
    if content.chars().count() <= max {
        return content;
    }
    let truncated = content.chars().take(max).collect::<String>();
    format!("{truncated}\n\n> 内容已截断")
}
