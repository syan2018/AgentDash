use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{McpPresetSource, McpServerDecl};

/// MCP Preset — Project 级单个 MCP Server 配置模板。
///
/// 每个 Preset 封装一个 MCP server 声明（http / sse / stdio），
/// 供 Agent 组装时作为可复用模板引用。name 在 project 内唯一。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpPreset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub server_decl: McpServerDecl,
    pub source: McpPresetSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl McpPreset {
    /// 创建一个全新的 user-authored Preset。
    pub fn new_user(
        project_id: Uuid,
        name: impl Into<String>,
        description: Option<String>,
        server_decl: McpServerDecl,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description,
            server_decl,
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
        name: impl Into<String>,
        description: Option<String>,
        server_decl: McpServerDecl,
    ) -> Self {
        let now = Utc::now();
        let key = builtin_key.into();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description,
            server_decl,
            source: McpPresetSource::Builtin { key },
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
            Some("desc".to_string()),
            McpServerDecl::Http {
                name: "fetch".to_string(),
                url: "https://example.com/mcp".to_string(),
                headers: vec![],
                relay: None,
            },
        );
        assert!(!preset.is_builtin());
        assert_eq!(preset.source, McpPresetSource::User);
        assert_eq!(preset.server_decl.server_name(), "fetch");
    }

    #[test]
    fn new_builtin_preset_carries_builtin_key() {
        let preset = McpPreset::new_builtin(
            Uuid::new_v4(),
            "filesystem",
            "Filesystem",
            None,
            McpServerDecl::Stdio {
                name: "filesystem".to_string(),
                command: "npx".to_string(),
                args: vec![],
                env: vec![],
                relay: None,
            },
        );
        assert!(preset.is_builtin());
        assert_eq!(preset.source.builtin_key(), Some("filesystem"));
    }
}
