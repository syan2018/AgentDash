//! 为 prompt 路径构造最小的本地工作区 VFS。

use std::path::Path;

use agentdash_spi::{Vfs, Mount, MountCapability};

/// 单一 `local_fs` 挂载，供 relay / 本机后端等在无完整 vfs 服务时注入。
pub fn local_workspace_vfs(root: &Path) -> Vfs {
    let root_ref = root.to_string_lossy().to_string();
    Vfs {
        mounts: vec![Mount {
            id: "workspace".to_string(),
            provider: "local_fs".to_string(),
            backend_id: "local".to_string(),
            root_ref,
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
                MountCapability::Exec,
            ],
            default_write: true,
            display_name: "Workspace".to_string(),
            metadata: serde_json::Value::Null,
        }],
        default_mount_id: Some("workspace".to_string()),
        ..Default::default()
    }
}
