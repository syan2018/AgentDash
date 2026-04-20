use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use agentdash_domain::mcp_preset::{McpPreset, McpPresetSource, McpServerDecl};

/// MCP Preset HTTP 响应 DTO。
///
/// - `source` 序列化为字符串 `"builtin" | "user"`（前端友好）
/// - `builtin_key` 仅 `source == "builtin"` 时非空
/// - `server_decl` 透传领域层联合体（`{ type: "http" | "sse" | "stdio", ... }`）
///   避免 DTO 层重复定义 transport 联合体
#[derive(Debug, Serialize)]
pub struct McpPresetResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub server_decl: McpServerDecl,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<McpPreset> for McpPresetResponse {
    fn from(preset: McpPreset) -> Self {
        let source = preset.source.tag();
        let builtin_key = match &preset.source {
            McpPresetSource::Builtin { key } => Some(key.clone()),
            McpPresetSource::User => None,
        };
        Self {
            id: preset.id,
            project_id: preset.project_id,
            name: preset.name,
            description: preset.description,
            server_decl: preset.server_decl,
            source,
            builtin_key,
            created_at: preset.created_at,
            updated_at: preset.updated_at,
        }
    }
}

/// 创建 user MCP Preset 的请求体。
#[derive(Debug, Deserialize)]
pub struct CreateMcpPresetRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub server_decl: McpServerDecl,
}

/// 更新 MCP Preset 的请求体——支持部分字段更新。
///
/// `description` 字段支持三态：
/// - 不传 → `None`（保持原值）
/// - 传 `null` → `Some(None)`（清空 description）
/// - 传字符串 → `Some(Some(s))`（更新为新值）
///
/// 通过 `deserialize_double_option` 自定义反序列化实现。
#[derive(Debug, Deserialize, Default)]
pub struct UpdateMcpPresetRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub server_decl: Option<McpServerDecl>,
}

/// 复制 Preset 为 user 副本的请求体。
///
/// `name` 为空时 handler 层会回退到 `"<原 name> (copy)"` 默认命名。
#[derive(Debug, Deserialize, Default)]
pub struct CloneMcpPresetRequest {
    #[serde(default)]
    pub name: Option<String>,
}

/// 装载 builtin Preset 的请求体。
///
/// - `builtin_key == None` → 装载全部内置模板（幂等）
/// - `builtin_key == Some(key)` → 仅装载指定 key 对应模板
#[derive(Debug, Deserialize, Default)]
pub struct BootstrapMcpPresetRequest {
    #[serde(default)]
    pub builtin_key: Option<String>,
}

/// 列表查询参数——可按来源筛选。
#[derive(Debug, Deserialize, Default)]
pub struct ListMcpPresetQuery {
    /// 期望值：`"user"` / `"builtin"` / `None`（不过滤）
    #[serde(default)]
    pub source: Option<String>,
}

/// 自定义反序列化：把「字段缺失 / 显式 null / 有值」三态映射到 `Option<Option<T>>`。
///
/// - JSON 未提供该 key → serde `#[serde(default)]` 生效 → `None`
/// - JSON 提供了 key 且值为 `null` → `Some(None)`
/// - JSON 提供了 key 且值为具体内容 → `Some(Some(value))`
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::mcp_preset::McpServerDecl;

    fn http_decl() -> McpServerDecl {
        McpServerDecl::Http {
            name: "fetch".to_string(),
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
            relay: None,
        }
    }

    #[test]
    fn response_serializes_user_preset_without_builtin_key() {
        let preset = McpPreset::new_user(
            Uuid::new_v4(),
            "my",
            Some("desc".to_string()),
            http_decl(),
        );
        let resp = McpPresetResponse::from(preset);
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["source"], "user");
        assert!(
            json.get("builtin_key").is_none() || json["builtin_key"].is_null(),
            "user preset 不应输出 builtin_key 字段"
        );
    }

    #[test]
    fn response_serializes_builtin_preset_with_key() {
        let preset = McpPreset::new_builtin(
            Uuid::new_v4(),
            "filesystem",
            "Filesystem",
            None,
            http_decl(),
        );
        let resp = McpPresetResponse::from(preset);
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["source"], "builtin");
        assert_eq!(json["builtin_key"], "filesystem");
    }

    #[test]
    fn update_request_description_triple_state_missing() {
        // 不传 description
        let raw = r#"{"name":"new-name"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse missing");
        assert!(parsed.description.is_none(), "缺失字段应为 None");
        assert_eq!(parsed.name.as_deref(), Some("new-name"));
    }

    #[test]
    fn update_request_description_triple_state_null() {
        // 传 null → Some(None) 代表「清空」
        let raw = r#"{"description":null}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse null");
        assert_eq!(parsed.description, Some(None), "null 应解析为 Some(None)");
    }

    #[test]
    fn update_request_description_triple_state_value() {
        // 传字符串 → Some(Some(s))
        let raw = r#"{"description":"updated"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse value");
        assert_eq!(parsed.description, Some(Some("updated".to_string())));
    }

    #[test]
    fn update_request_empty_body_parses_as_all_none() {
        let parsed: UpdateMcpPresetRequest = serde_json::from_str("{}").expect("parse empty");
        assert!(parsed.name.is_none());
        assert!(parsed.description.is_none());
        assert!(parsed.server_decl.is_none());
    }

    #[test]
    fn bootstrap_request_without_key_parses() {
        let parsed: BootstrapMcpPresetRequest = serde_json::from_str("{}").expect("parse");
        assert!(parsed.builtin_key.is_none());
    }

    #[test]
    fn bootstrap_request_with_key_parses() {
        let parsed: BootstrapMcpPresetRequest =
            serde_json::from_str(r#"{"builtin_key":"fetch"}"#).expect("parse");
        assert_eq!(parsed.builtin_key.as_deref(), Some("fetch"));
    }
}
