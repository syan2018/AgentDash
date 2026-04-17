use serde::{Deserialize, Serialize};

use super::MountCapability;

/// 统一挂载点定义，被 connector-contract 和 application 直接使用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mount {
    pub id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl Mount {
    pub fn supports(&self, capability: MountCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// Mount 级别的路径引用（声明式，在 VFS 构建时定义）。
///
/// 类比 Unix 符号链接，但为避免通用 symlink 的复杂性（循环检测、权限穿透），
/// 仅支持 mount 级别的声明式 alias：从 `(from_mount_id, from_path)` 透明重定向到
/// `(to_mount_id, to_path)`。路径解析层在命中时会自动跳转。
///
/// 典型场景：
/// - Workflow step input 声明为 link → 上游 step output mount 的某路径
/// - Agent knowledge 引用 project 级别的共享文档（不复制）
/// - Canvas 引用 workspace 文件（只读视图）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountLink {
    /// 来源：用户看到的路径。
    pub from_mount_id: String,
    pub from_path: String,
    /// 目标：实际读取的位置。
    pub to_mount_id: String,
    pub to_path: String,
}

/// 统一虚拟文件系统（VFS）定义，被 connector-contract 和 application 直接使用。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vfs {
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_story_id: Option<String>,
    /// mount 级引用（声明式 symlink alias）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MountLink>,
}

impl Vfs {
    pub fn default_mount(&self) -> Option<&Mount> {
        let default_id = self.default_mount_id.as_deref()?;
        self.mounts.iter().find(|mount| mount.id == default_id)
    }

    /// 查找匹配 `(mount_id, path)` 的第一条 link。
    ///
    /// 匹配规则：link.from_mount_id 必须相等，并且 path 等于 from_path
    /// 或以 `from_path/` 为前缀（目录级别的透明重定向）。
    pub fn find_link(&self, mount_id: &str, path: &str) -> Option<&MountLink> {
        self.links.iter().find(|link| {
            link.from_mount_id == mount_id
                && (link.from_path == path
                    || (link.from_path.is_empty() && !path.is_empty())
                    || path.starts_with(&format!("{}/", link.from_path)))
        })
    }
}
