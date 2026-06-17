//! ToolCatalogService — 前端查询 capability / tool catalog 的统一入口。
//!
//! 合并平台内嵌工具（编译期静态元数据）和 MCP 外部工具（运行时发现），
//! 返回 contract DTO 供 CapabilitiesEditor 消费。

use agentdash_contracts::workflow::{
    CapabilityCatalogEntryDto, CapabilityCatalogResponse, CapabilityScopeDto, PlatformMcpScopeDto,
    ToolClusterDto, ToolDescriptorDto, ToolSourceDto,
};
use agentdash_spi::connector::ToolCluster;
use agentdash_spi::platform::tool_capability::{
    self, CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_RELAY_MANAGEMENT,
    CAP_SHELL_EXECUTE, CAP_STORY_MANAGEMENT, CAP_TASK, CAP_WORKFLOW, CAP_WORKFLOW_MANAGEMENT,
    CAP_WORKSPACE_MODULE, CapabilityScope, PlatformMcpScope, ToolDescriptor, ToolSource,
    WELL_KNOWN_KEYS, default_visibility_rules, is_known_key,
};

/// 按 capability keys 查询工具目录。
///
/// - well-known key（平台 cluster 或平台 MCP scope） → 返回平台工具静态元数据
/// - `mcp:*` → 返回占位描述（具体工具需要运行时通过 MCP tools/list 发现）
///
/// 前端在 workflow editor 中调用此函数（通过 API），用于展示每个 capability 下属的可选工具列表。
pub fn query_tool_catalog(capability_keys: &[String]) -> Vec<ToolDescriptorDto> {
    let mut result = Vec::new();

    for key in capability_keys {
        if is_known_key(key) {
            let tools = tool_capability::platform_tools_for_capability(key);
            result.extend(tools.into_iter().map(tool_descriptor_to_dto));
        } else if let Some(server_name) = key.strip_prefix("mcp:") {
            result.push(tool_descriptor_to_dto(ToolDescriptor {
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
pub fn query_capability_catalog(capability_keys: Option<&[String]>) -> CapabilityCatalogResponse {
    let keys = catalog_keys(capability_keys);
    let capabilities = keys
        .into_iter()
        .filter_map(|key| capability_catalog_entry(&key))
        .collect();
    CapabilityCatalogResponse { capabilities }
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

fn capability_catalog_entry(key: &str) -> Option<CapabilityCatalogEntryDto> {
    if let Some(rule) = default_visibility_rules()
        .iter()
        .find(|rule| rule.key == key)
    {
        let (label, description) = capability_metadata(key);
        return Some(CapabilityCatalogEntryDto {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            allowed_scopes: rule
                .allowed_scopes
                .iter()
                .copied()
                .map(capability_scope_to_dto)
                .collect(),
            auto_granted: rule.auto_granted,
            agent_can_grant: rule.agent_can_grant,
            workflow_can_grant: rule.workflow_can_grant,
            tools: query_tool_catalog(&[key.to_string()]),
        });
    }

    key.strip_prefix("mcp:")
        .map(|server_name| CapabilityCatalogEntryDto {
            key: key.to_string(),
            label: format!("MCP: {server_name}"),
            description:
                "用户自定义 MCP Preset 引用。由后端按 preset key 展开为运行时 MCP server。"
                    .to_string(),
            allowed_scopes: vec![
                CapabilityScopeDto::Project,
                CapabilityScopeDto::Story,
                CapabilityScopeDto::Task,
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

fn tool_descriptor_to_dto(descriptor: ToolDescriptor) -> ToolDescriptorDto {
    ToolDescriptorDto {
        name: descriptor.name,
        display_name: descriptor.display_name,
        description: descriptor.description,
        source: tool_source_to_dto(descriptor.source),
        capability_key: descriptor.capability_key,
    }
}

fn tool_source_to_dto(source: ToolSource) -> ToolSourceDto {
    match source {
        ToolSource::Platform { cluster } => ToolSourceDto::Platform {
            cluster: tool_cluster_to_dto(cluster),
        },
        ToolSource::PlatformMcp { scope } => ToolSourceDto::PlatformMcp {
            scope: platform_mcp_scope_to_dto(scope),
        },
        ToolSource::Mcp { server_name } => ToolSourceDto::Mcp { server_name },
    }
}

fn tool_cluster_to_dto(cluster: ToolCluster) -> ToolClusterDto {
    match cluster {
        ToolCluster::Read => ToolClusterDto::Read,
        ToolCluster::Write => ToolClusterDto::Write,
        ToolCluster::Execute => ToolClusterDto::Execute,
        ToolCluster::Workflow => ToolClusterDto::Workflow,
        ToolCluster::Collaboration => ToolClusterDto::Collaboration,
        ToolCluster::Task => ToolClusterDto::Task,
        ToolCluster::WorkspaceModule => ToolClusterDto::WorkspaceModule,
    }
}

fn platform_mcp_scope_to_dto(scope: PlatformMcpScope) -> PlatformMcpScopeDto {
    match scope {
        PlatformMcpScope::Relay => PlatformMcpScopeDto::Relay,
        PlatformMcpScope::Story => PlatformMcpScopeDto::Story,
        PlatformMcpScope::Workflow => PlatformMcpScopeDto::Workflow,
    }
}

fn capability_scope_to_dto(scope: CapabilityScope) -> CapabilityScopeDto {
    match scope {
        CapabilityScope::Project => CapabilityScopeDto::Project,
        CapabilityScope::Story => CapabilityScopeDto::Story,
        CapabilityScope::Task => CapabilityScopeDto::Task,
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
                .contains(&CapabilityScopeDto::Project)
        );
        assert!(
            workspace_module
                .allowed_scopes
                .contains(&CapabilityScopeDto::Story)
        );
        assert!(
            workspace_module
                .allowed_scopes
                .contains(&CapabilityScopeDto::Task)
        );
        assert_eq!(workspace_module.tools.len(), 5);
    }
}
