use crate::vfs::selected_workspace_binding;
use agentdash_spi::{ContextFragment, MergeStrategy};

use serde_json::{Value, json};

use super::Contribution;

// ─── 文本工具 ────────────────────────────────────────────────

pub(crate) fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}

// ─── Workspace Context Fragment ──────────────────────────────

/// 构建 owner workspace context fragment。
pub(crate) fn workspace_context_fragment(
    workspace: &agentdash_domain::workspace::Workspace,
) -> ContextFragment {
    let binding_summary = selected_workspace_binding(workspace)
        .map(|binding| {
            format!(
                "{} @ {}",
                trim_or_dash(&binding.backend_id),
                trim_or_dash(&binding.root_ref)
            )
        })
        .unwrap_or_else(|| "-".to_string());

    ContextFragment {
        slot: "workspace".to_string(),
        label: "workspace_context".to_string(),
        order: 30,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "context_contributor:workspace".to_string(),
        content: format!(
            "## Workspace\n- id: {}\n- identity_kind: {:?}\n- name: {}\n- binding: {}\n- working_dir: .",
            workspace.id,
            workspace.identity_kind,
            trim_or_dash(&workspace.name),
            binding_summary,
        ),
    }
}

// ─── Owner Context Resource Block ───────────────────────────

/// 将 context markdown 封装为 ACP resource content block。
///
/// 所有 owner 类型（Project / Story / Task）的 context 都需要以
/// `{ "type": "resource", "resource": { uri, mimeType, text } }` 结构
/// 注入到 prompt blocks 中，此函数统一了该构建逻辑。
pub fn build_owner_context_resource_block(uri: &str, markdown: &str) -> Value {
    json!({
        "type": "resource",
        "resource": {
            "uri": uri,
            "mimeType": "text/markdown",
            "text": markdown,
        }
    })
}

/// MCP 能力注入片段 —— 同时把 `McpServerSummary` 声明挂到 `Contribution.mcp_servers`。
pub fn contribute_mcp(config: &agentdash_spi::McpInjectionConfig) -> Contribution {
    let label: &'static str = match config.scope {
        agentdash_spi::ToolScope::Relay => "mcp_relay_tools",
        agentdash_spi::ToolScope::Story => "mcp_story_tools",
        agentdash_spi::ToolScope::Workflow => "mcp_workflow_tools",
    };

    let runtime_mcp_server = config.to_runtime_mcp_server();
    let server_summary = crate::runtime_bridge::runtime_mcp_server_to_summary(&runtime_mcp_server);

    Contribution {
        fragments: vec![ContextFragment {
            slot: "mcp_config".to_string(),
            label: label.to_string(),
            order: 85,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "context_contributor:mcp".to_string(),
            content: config.to_context_content(),
        }],
        mcp_servers: vec![server_summary],
    }
}
