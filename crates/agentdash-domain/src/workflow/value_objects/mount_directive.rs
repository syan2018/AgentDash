use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{Mount, MountLink};

/// VFS/mount 能力指令。
///
/// 这些指令描述 step/workflow 对资源空间的临时装载、撤销、link 和默认 mount
/// 切换。实际运行时会先继承当前 session 的 VFS，再按顺序应用这些指令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MountDirective {
    AddMount {
        mount: Mount,
    },
    RemoveMount {
        mount_id: String,
    },
    ReplaceMount {
        mount: Mount,
    },
    AddLink {
        link: MountLink,
    },
    RemoveLink {
        from_mount_id: String,
        from_path: String,
    },
    SetDefaultMount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mount_id: Option<String>,
    },
}
