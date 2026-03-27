use std::path::Path;

use agentdash_domain::context_source::ContextSourceKind;
use serde::Serialize;

/// 寻址空间描述符 — 描述后端当前环境中可用的一种资源引用能力
///
/// 前端通过 `/api/address-spaces` 获取可用空间列表后，
/// 根据 `selector` 字段决定如何呈现引用选择器 UI。
#[derive(Debug, Clone, Serialize)]
pub struct AddressSpaceDescriptor {
    /// 空间唯一标识（如 "workspace_file"、"mcp_resource"）
    pub id: String,
    /// 用户可见的显示名称
    pub label: String,
    /// 映射到的 ContextSourceKind
    pub kind: ContextSourceKind,
    /// 能力提供者标识
    pub provider: String,
    /// 支持的操作（"search" / "browse" / "read"）
    pub supports: Vec<String>,
    /// 前端选择器 hint
    pub selector: Option<SelectorHint>,
}

/// 前端选择器 UI 提示
#[derive(Debug, Clone, Serialize)]
pub struct SelectorHint {
    /// 触发字符（如 "@"）
    pub trigger: Option<String>,
    /// 搜索框占位符
    pub placeholder: String,
    /// 结果条目类型（"file" / "resource" / "entity"）
    pub result_item_type: String,
}

/// 寻址空间能力上下文 — 传递给 Provider 用于决定能力可用性
pub struct AddressSpaceContext<'a> {
    pub workspace_root: Option<&'a Path>,
    pub has_mcp: bool,
}

/// 寻址空间能力提供者
///
/// 每个 Provider 负责一类资源的能力描述、搜索和解析。
/// 注册到 `AddressSpaceRegistry` 后，由 API 层统一暴露。
pub trait AddressSpaceProvider: Send + Sync {
    /// 返回此 Provider 的空间描述符（如果当前环境不支持则返回 None）
    fn descriptor(&self, ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor>;
}

/// 寻址空间注册表 — 持有所有已注册的能力提供者
pub struct AddressSpaceRegistry {
    providers: Vec<Box<dyn AddressSpaceProvider>>,
}

impl AddressSpaceRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: Box<dyn AddressSpaceProvider>) {
        self.providers.push(provider);
    }

    /// 根据当前环境，返回所有可用的寻址空间描述
    pub fn available_spaces(&self, ctx: &AddressSpaceContext<'_>) -> Vec<AddressSpaceDescriptor> {
        self.providers
            .iter()
            .filter_map(|p| p.descriptor(ctx))
            .collect()
    }
}

impl Default for AddressSpaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 内置 Provider ──────────────────────────────────────────

/// 工作空间文件 Provider — 当有工作空间时可用
pub struct WorkspaceFileProvider;

impl AddressSpaceProvider for WorkspaceFileProvider {
    fn descriptor(&self, ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor> {
        ctx.workspace_root?;
        Some(AddressSpaceDescriptor {
            id: "workspace_file".to_string(),
            label: "工作空间文件".to_string(),
            kind: ContextSourceKind::File,
            provider: "workspace".to_string(),
            supports: vec![
                "search".to_string(),
                "browse".to_string(),
                "read".to_string(),
            ],
            selector: Some(SelectorHint {
                trigger: Some("@".to_string()),
                placeholder: "输入文件名或路径".to_string(),
                result_item_type: "file".to_string(),
            }),
        })
    }
}

/// 项目快照 Provider — 当有工作空间时可用
pub struct WorkspaceSnapshotProvider;

impl AddressSpaceProvider for WorkspaceSnapshotProvider {
    fn descriptor(&self, ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor> {
        ctx.workspace_root?;
        Some(AddressSpaceDescriptor {
            id: "workspace_snapshot".to_string(),
            label: "项目结构快照".to_string(),
            kind: ContextSourceKind::ProjectSnapshot,
            provider: "workspace".to_string(),
            supports: vec!["read".to_string()],
            selector: None,
        })
    }
}

/// MCP 资源 Provider — 当有 MCP 服务时可用
pub struct McpResourceProvider;

impl AddressSpaceProvider for McpResourceProvider {
    fn descriptor(&self, ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor> {
        if !ctx.has_mcp {
            return None;
        }
        Some(AddressSpaceDescriptor {
            id: "mcp_resource".to_string(),
            label: "MCP 资源".to_string(),
            kind: ContextSourceKind::McpResource,
            provider: "mcp".to_string(),
            supports: vec!["browse".to_string(), "read".to_string()],
            selector: Some(SelectorHint {
                trigger: None,
                placeholder: "选择 MCP Server 暴露的资源".to_string(),
                result_item_type: "resource".to_string(),
            }),
        })
    }
}

/// 创建包含所有内置 Provider 的注册表
/// Lifecycle 执行记录虚拟挂载 — 由会话在存在活跃 run 时挂载，`lifecycle_vfs` 提供读写浏览能力描述
pub struct LifecycleAddressSpaceProvider;

impl AddressSpaceProvider for LifecycleAddressSpaceProvider {
    fn descriptor(&self, _ctx: &AddressSpaceContext<'_>) -> Option<AddressSpaceDescriptor> {
        Some(AddressSpaceDescriptor {
            id: "lifecycle".to_string(),
            label: "Lifecycle 执行记录".to_string(),
            kind: ContextSourceKind::EntityRef,
            provider: "lifecycle_vfs".to_string(),
            supports: vec!["read".to_string(), "browse".to_string()],
            selector: None,
        })
    }
}

pub fn builtin_address_space_registry() -> AddressSpaceRegistry {
    let mut registry = AddressSpaceRegistry::new();
    registry.register(Box::new(WorkspaceFileProvider));
    registry.register(Box::new(WorkspaceSnapshotProvider));
    registry.register(Box::new(McpResourceProvider));
    registry.register(Box::new(LifecycleAddressSpaceProvider));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_file_available_with_workspace() {
        let registry = builtin_address_space_registry();
        let tmp = tempfile::tempdir().unwrap();
        let ctx = AddressSpaceContext {
            workspace_root: Some(tmp.path()),
            has_mcp: false,
        };
        let spaces = registry.available_spaces(&ctx);
        assert!(spaces.iter().any(|s| s.id == "workspace_file"));
        assert!(spaces.iter().any(|s| s.id == "workspace_snapshot"));
        assert!(!spaces.iter().any(|s| s.id == "mcp_resource"));
    }

    #[test]
    fn mcp_resource_available_with_mcp() {
        let registry = builtin_address_space_registry();
        let ctx = AddressSpaceContext {
            workspace_root: None,
            has_mcp: true,
        };
        let spaces = registry.available_spaces(&ctx);
        assert!(spaces.iter().any(|s| s.id == "mcp_resource"));
        assert!(!spaces.iter().any(|s| s.id == "workspace_file"));
    }

    #[test]
    fn lifecycle_space_always_advertised() {
        let registry = builtin_address_space_registry();
        let ctx = AddressSpaceContext {
            workspace_root: None,
            has_mcp: false,
        };
        let spaces = registry.available_spaces(&ctx);
        assert_eq!(spaces.len(), 1);
        assert!(spaces.iter().any(|s| s.id == "lifecycle"));
    }
}
