use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{McpPresetSource, McpRoutePolicy, McpTransportConfig};

/// MCP Preset — Project 级单个 MCP Server 配置模板。
///
/// 每个 Preset 封装一个可复用的 project 级 MCP 引用：
/// - `key`：项目内唯一，也是 agent-facing server name
/// - `display_name`：纯展示名称
/// - `transport`：纯连接参数
/// - `route_policy`：应用层路由策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpPreset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub route_policy: McpRoutePolicy,
    pub source: McpPresetSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl McpPreset {
    /// 创建一个全新的 user-authored Preset。
    pub fn new_user(
        project_id: Uuid,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: Option<String>,
        transport: McpTransportConfig,
        route_policy: McpRoutePolicy,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            key: key.into(),
            display_name: display_name.into(),
            description,
            transport,
            route_policy,
            source: McpPresetSource::User,
            created_at: now,
            updated_at: now,
        }
    }

    /// 根据 builtin 模板创建一个 builtin Preset 实例。
    ///
    /// `builtin_key` 对应 `builtins/<key>.json` 文件名根，同时写入 `source` 字段。
    pub fn new_builtin(
        project_id: Uuid,
        builtin_key: impl Into<String>,
        key: impl Into<String>,
        display_name: impl Into<String>,
        description: Option<String>,
        transport: McpTransportConfig,
        route_policy: McpRoutePolicy,
    ) -> Self {
        let now = Utc::now();
        let source_key = builtin_key.into();
        Self {
            id: Uuid::new_v4(),
            project_id,
            key: key.into(),
            display_name: display_name.into(),
            description,
            transport,
            route_policy,
            source: McpPresetSource::Builtin { key: source_key },
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_builtin(&self) -> bool {
        self.source.is_builtin()
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_preset_has_user_source() {
        let preset = McpPreset::new_user(
            Uuid::new_v4(),
            "my-fetch",
            "My Fetch",
            Some("desc".to_string()),
            McpTransportConfig::Http {
                url: "https://example.com/mcp".to_string(),
                headers: vec![],
            },
            McpRoutePolicy::Direct,
        );
        assert!(!preset.is_builtin());
        assert_eq!(preset.source, McpPresetSource::User);
        assert_eq!(preset.key, "my-fetch");
        assert_eq!(preset.display_name, "My Fetch");
    }

    #[test]
    fn new_builtin_preset_carries_builtin_key() {
        let preset = McpPreset::new_builtin(
            Uuid::new_v4(),
            "filesystem",
            "Filesystem",
            "Filesystem",
            None,
            McpTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: vec![],
            },
            McpRoutePolicy::Auto,
        );
        assert!(preset.is_builtin());
        assert_eq!(preset.source.builtin_key(), Some("filesystem"));
    }
}
