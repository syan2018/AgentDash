//! ToolCatalogService — 前端查询工具目录的统一入口。
//!
//! 合并平台内嵌工具（编译期静态元数据）和 MCP 外部工具（运行时发现），
//! 返回统一的 `ToolDescriptor` 列表供 CapabilitiesEditor 消费。

use agentdash_spi::tool_capability::{self, ToolDescriptor, ToolSource, is_known_key};

/// 按 capability keys 查询工具目录。
///
/// - well-known key（平台 cluster 或平台 MCP scope） → 返回平台工具静态元数据
/// - `mcp:*` → 返回占位描述（具体工具需要运行时通过 MCP tools/list 发现）
///
/// 前端在 workflow editor 中调用此函数（通过 API），
/// 用于展示每个 capability 下属的可选工具列表。
pub fn query_tool_catalog(capability_keys: &[String]) -> Vec<ToolDescriptor> {
    let mut result = Vec::new();

    for key in capability_keys {
        if is_known_key(key) {
            let tools = tool_capability::platform_tools_for_capability(key);
            result.extend(tools);
        } else if key.starts_with("mcp:") {
            let server_name = &key[4..];
            result.push(ToolDescriptor {
                name: format!("mcp:{server_name}"),
                display_name: format!("MCP: {server_name}"),
                description: format!(
                    "MCP 服务器 '{server_name}' 的工具（需运行时发现具体工具列表）"
                ),
                source: ToolSource::Mcp {
                    server_name: server_name.to_string(),
                },
                capability_key: key.clone(),
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_file_read_returns_four_tools() {
        let result = query_tool_catalog(&["file_read".to_string()]);
        assert_eq!(result.len(), 4);
        let names: Vec<&str> = result.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"mounts_list"));
        assert!(names.contains(&"fs_read"));
        assert!(names.contains(&"fs_glob"));
        assert!(names.contains(&"fs_grep"));
    }

    #[test]
    fn query_file_system_alias_no_longer_recognized() {
        // 迁移后 file_system 不再是 well-known key
        let result = query_tool_catalog(&["file_system".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn query_workflow_management_returns_platform_mcp_tools() {
        let result = query_tool_catalog(&["workflow_management".to_string()]);
        assert!(
            !result.is_empty(),
            "workflow_management 应返回平台 MCP 工具"
        );
        let names: Vec<&str> = result.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"list_workflows"));
        assert!(names.contains(&"upsert_workflow_tool"));
    }

    #[test]
    fn query_relay_management_returns_platform_mcp_tools() {
        let result = query_tool_catalog(&["relay_management".to_string()]);
        let names: Vec<&str> = result.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"list_projects"));
        assert!(names.contains(&"create_story"));
    }

    #[test]
    fn query_mcp_returns_placeholder() {
        let result = query_tool_catalog(&["mcp:code_analyzer".to_string()]);
        assert_eq!(result.len(), 1);
        assert!(result[0].description.contains("运行时发现"));
    }

    #[test]
    fn query_canvas_returns_canvas_tools() {
        let result = query_tool_catalog(&["canvas".to_string()]);
        assert_eq!(result.len(), 4);
        let names: Vec<&str> = result.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"canvases_list"));
        assert!(names.contains(&"canvas_start"));
    }
}
