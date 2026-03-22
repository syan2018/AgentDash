use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 挂载点上可声明的资源能力，被 ContextContainer 和 ExecutionMount 共用
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MountCapability {
    Read,
    Write,
    List,
    Search,
    Exec,
}
