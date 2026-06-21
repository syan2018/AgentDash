//! ToolCatalogService — 前端查询 capability / tool catalog 的统一入口。
//!
//! 合并平台内嵌工具（编译期静态元数据）和 MCP 外部工具（运行时发现），
//! 返回 application read model 供 API adapter 投影为 browser-facing contract DTO。

use agentdash_spi::connector::ToolCluster as SpiToolCluster;
use agentdash_spi::platform::tool_capability::{
    self, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_RELAY_MANAGEMENT,
    CAP_SHELL_EXECUTE, CAP_STORY_MANAGEMENT, CAP_TASK, CAP_WORKFLOW, CAP_WORKFLOW_MANAGEMENT,
    CAP_WORKSPACE_MODULE, CapabilityScope as SpiCapabilityScope,
    PlatformMcpScope as SpiPlatformMcpScope, ToolDescriptor, ToolSource, WELL_KNOWN_KEYS,
    default_visibility_rules, is_known_key,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCatalog {
    pub capabilities: Vec<CapabilityCatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCatalogEntry {
    pub key: String,
    pub label: String,
    pub description: String,
    pub allowed_scopes: Vec<CapabilityCatalogScope>,
    pub auto_granted: bool,
    pub agent_can_grant: bool,
    pub workflow_can_grant: bool,
    pub tools: Vec<ToolCatalogDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityCatalogScope {
    Project,
    Story,
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCatalogDescriptor {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub source: ToolCatalogSource,
    pub capability_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCatalogSource {
    Platform { cluster: ToolCatalogCluster },
    PlatformMcp { scope: ToolCatalogPlatformMcpScope },
    Mcp { server_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCatalogCluster {
    Read,
    Write,
    Execute,
    Workflow,
    Collaboration,
    Task,
    WorkspaceModule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCatalogPlatformMcpScope {
    Relay,
    Story,
    Workflow,
}

/// 按 capability keys 查询工具目录。
///
/// - well-known key（平台 cluster 或平台 MCP scope） → 返回平台工具静态元数据
/// - `mcp:*` → 返回占位描述（具体工具需要运行时通过 MCP tools/list 发现）
///
/// 前端在 workflow editor 中调用此函数（通过 API），用于展示每个 capability 下属的可选工具列表。
pub fn query_tool_catalog(capability_keys: &[String]) -> Vec<ToolCatalogDescriptor> {
    let mut result = Vec::new();

    for key in capability_keys {
        if is_known_key(key) {
            let tools = tool_capability::platform_tools_for_capability(key);
            result.extend(tools.into_iter().map(tool_descriptor_to_read_model));
        } else if let Some(server_name) = key.strip_prefix("mcp:") {
            result.push(tool_descriptor_to_read_model(ToolDescriptor {
                name: format!("mcp:{server_name}"),
                display_name: format!("MCP: {server_name}"),
                description: format!(
                    "MCP 服务器 '{server_name}' 的工具（需运行时发现具体工具列表）"
                ),
                source: ToolSource::Mcp {
                    server_name: server_name.to_string(),
                },
                capability_key: key.clone(),
            }));
        }
    }

    result
}

/// 查询 capability catalog。`capability_keys=None` 时返回全部平台 well-known capability。
pub fn query_capability_catalog(capability_keys: Option<&[String]>) -> CapabilityCatalog {
    let keys = catalog_keys(capability_keys);
    let capabilities = keys
        .into_iter()
        .filter_map(|key| capability_catalog_entry(&key))
        .collect();
    CapabilityCatalog { capabilities }
}

fn catalog_keys(capability_keys: Option<&[String]>) -> Vec<String> {
    let source: Vec<String> = match capability_keys {
        Some(keys) if !keys.is_empty() => keys.to_vec(),
        _ => WELL_KNOWN_KEYS
            .iter()
            .map(|key| (*key).to_string())
            .collect(),
    };
    let mut result = Vec::new();
    for key in source {
        if !result.contains(&key) {
            result.push(key);
        }
    }
    result
}

fn capability_catalog_entry(key: &str) -> Option<CapabilityCatalogEntry> {
    if let Some(rule) = default_visibility_rules()
        .iter()
        .find(|rule| rule.key == key)
    {
        let (label, description) = capability_metadata(key);
        return Some(CapabilityCatalogEntry {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            allowed_scopes: rule
                .allowed_scopes
                .iter()
                .copied()
                .map(capability_scope_to_read_model)
                .collect(),
            auto_granted: rule.auto_granted,
            agent_can_grant: rule.agent_can_grant,
            workflow_can_grant: rule.workflow_can_grant,
            tools: query_tool_catalog(&[key.to_string()]),
        });
    }

    key.strip_prefix("mcp:")
        .map(|server_name| CapabilityCatalogEntry {
            key: key.to_string(),
            label: format!("MCP: {server_name}"),
            description:
                "用户自定义 MCP Preset 引用。由后端按 preset key 展开为运行时 MCP server。"
                    .to_string(),
            allowed_scopes: vec![
                CapabilityCatalogScope::Project,
                CapabilityCatalogScope::Story,
                CapabilityCatalogScope::Task,
            ],
            auto_granted: false,
            agent_can_grant: true,
            workflow_can_grant: true,
            tools: query_tool_catalog(&[key.to_string()]),
        })
}

fn capability_metadata(key: &str) -> (&'static str, &'static str) {
    match key {
        CAP_FILE_READ => (
            "文件读取",
            "只读文件系统访问（fs_read、fs_glob、fs_grep 等）",
        ),
        CAP_FILE_WRITE => ("文件写入", "文件写入操作（fs_apply_patch）"),
        CAP_SHELL_EXECUTE => ("Shell 执行", "执行 shell 命令（shell_exec）"),
        CAP_WORKSPACE_MODULE => ("Workspace Module", "模块创建、调用与展示，包含 Canvas"),
        CAP_WORKFLOW => ("工作流", "工作流汇报与推进"),
        CAP_COLLABORATION => ("协作", "多 agent 协作通道"),
        CAP_STORY_MANAGEMENT => ("Story 管理", "创建 / 调整 Story"),
        CAP_TASK => ("Task", "读取 / 维护 run-scoped Task"),
        CAP_RELAY_MANAGEMENT => ("Relay 管理", "Relay 后端管理"),
        CAP_WORKFLOW_MANAGEMENT => ("工作流管理", "MCP workflow 管理工具"),
        _ => ("未知能力", "未登记的 capability key"),
    }
}

fn tool_descriptor_to_read_model(descriptor: ToolDescriptor) -> ToolCatalogDescriptor {
    ToolCatalogDescriptor {
        name: descriptor.name,
        display_name: descriptor.display_name,
        description: descriptor.description,
        source: tool_source_to_read_model(descriptor.source),
        capability_key: descriptor.capability_key,
    }
}

fn tool_source_to_read_model(source: ToolSource) -> ToolCatalogSource {
    match source {
        ToolSource::Platform { cluster } => ToolCatalogSource::Platform {
            cluster: tool_cluster_to_read_model(cluster),
        },
        ToolSource::PlatformMcp { scope } => ToolCatalogSource::PlatformMcp {
            scope: platform_mcp_scope_to_read_model(scope),
        },
        ToolSource::Mcp { server_name } => ToolCatalogSource::Mcp { server_name },
    }
}

fn tool_cluster_to_read_model(cluster: SpiToolCluster) -> ToolCatalogCluster {
    match cluster {
        SpiToolCluster::Read => ToolCatalogCluster::Read,
        SpiToolCluster::Write => ToolCatalogCluster::Write,
        SpiToolCluster::Execute => ToolCatalogCluster::Execute,
        SpiToolCluster::Workflow => ToolCatalogCluster::Workflow,
        SpiToolCluster::Collaboration => ToolCatalogCluster::Collaboration,
        SpiToolCluster::Task => ToolCatalogCluster::Task,
        SpiToolCluster::WorkspaceModule => ToolCatalogCluster::WorkspaceModule,
    }
}

fn platform_mcp_scope_to_read_model(scope: SpiPlatformMcpScope) -> ToolCatalogPlatformMcpScope {
    match scope {
        SpiPlatformMcpScope::Relay => ToolCatalogPlatformMcpScope::Relay,
        SpiPlatformMcpScope::Story => ToolCatalogPlatformMcpScope::Story,
        SpiPlatformMcpScope::Workflow => ToolCatalogPlatformMcpScope::Workflow,
    }
}

fn capability_scope_to_read_model(scope: SpiCapabilityScope) -> CapabilityCatalogScope {
    match scope {
        SpiCapabilityScope::Project => CapabilityCatalogScope::Project,
        SpiCapabilityScope::Story => CapabilityCatalogScope::Story,
        SpiCapabilityScope::Task => CapabilityCatalogScope::Task,
    }
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
        assert!(result.iter().any(|tool| matches!(
            &tool.source,
            ToolCatalogSource::PlatformMcp {
                scope: ToolCatalogPlatformMcpScope::Workflow
            }
        )));
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
        assert!(matches!(
            &result[0].source,
            ToolCatalogSource::Mcp { server_name } if server_name == "code_analyzer"
        ));
    }

    #[test]
    fn query_workspace_module_returns_canvas_module_tools() {
        let result = query_tool_catalog(&["workspace_module".to_string()]);
        assert_eq!(result.len(), 5);
        let names: Vec<&str> = result.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"workspace_module_list"));
        assert!(names.contains(&"workspace_module_describe"));
        assert!(names.contains(&"workspace_module_create"));
        assert!(names.contains(&"workspace_module_invoke"));
        assert!(names.contains(&"workspace_module_present"));
    }

    #[test]
    fn capability_catalog_projects_visibility_rules() {
        let catalog = query_capability_catalog(None);
        let workspace_module = catalog
            .capabilities
            .iter()
            .find(|entry| entry.key == "workspace_module")
            .expect("workspace_module entry");
        assert_eq!(workspace_module.label, "Workspace Module");
        assert!(workspace_module.auto_granted);
        assert!(
            workspace_module
                .allowed_scopes
                .contains(&CapabilityCatalogScope::Project)
        );
        assert!(
            workspace_module
                .allowed_scopes
                .contains(&CapabilityCatalogScope::Story)
        );
        assert!(
            workspace_module
                .allowed_scopes
                .contains(&CapabilityCatalogScope::Task)
        );
        assert_eq!(workspace_module.tools.len(), 5);
    }
}
