use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use agentdash_application::mcp_preset::ProbeResult;
use agentdash_domain::mcp_preset::{
    McpPreset, McpPresetSource, McpRoutePolicy, McpTransportConfig,
};

/// MCP Preset HTTP 响应 DTO。
///
/// - `source` 序列化为字符串 `"builtin" | "user"`（前端友好）
/// - `builtin_key` 仅 `source == "builtin"` 时非空
/// - `key` 同时是 preset 引用 key 与 agent-facing server name
/// - `transport` 仅描述连接参数；`route_policy` 描述应用层路由策略
#[derive(Debug, Serialize)]
pub struct McpPresetResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub transport: McpTransportConfig,
    pub route_policy: McpRoutePolicy,
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
            key: preset.key,
            display_name: preset.display_name,
            description: preset.description,
            transport: preset.transport,
            route_policy: preset.route_policy,
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
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub route_policy: McpRoutePolicy,
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
    pub key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub transport: Option<McpTransportConfig>,
    #[serde(default)]
    pub route_policy: Option<McpRoutePolicy>,
}

/// 复制 Preset 为 user 副本的请求体。
///
/// `key` 为空时 handler 层会回退到 `"<原 key>-copy"` 默认命名；
/// `display_name` 为空时回退到 `"<原 display_name> (copy)"`。
#[derive(Debug, Deserialize, Default)]
pub struct CloneMcpPresetRequest {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
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

/// Probe 响应 DTO——直接复用 application 层的 `ProbeResult`。
///
/// 序列化形状（通过 tagged enum `#[serde(tag = "status")]`）：
/// - `{ "status": "ok", "latency_ms": 123, "tools": [...] }`
/// - `{ "status": "error", "error": "..." }`
/// - `{ "status": "unsupported", "reason": "..." }`
pub type ProbeMcpPresetResponse = ProbeResult;

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
    use agentdash_domain::mcp_preset::{McpRoutePolicy, McpTransportConfig};

    fn http_transport() -> McpTransportConfig {
        McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
        }
    }

    #[test]
    fn response_serializes_user_preset_without_builtin_key() {
        let preset = McpPreset::new_user(
            Uuid::new_v4(),
            "my",
            "My",
            Some("desc".to_string()),
            http_transport(),
            McpRoutePolicy::Direct,
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
            "Filesystem",
            None,
            http_transport(),
            McpRoutePolicy::Auto,
        );
        let resp = McpPresetResponse::from(preset);
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["source"], "builtin");
        assert_eq!(json["builtin_key"], "filesystem");
    }

    #[test]
    fn update_request_description_triple_state_missing() {
        // 不传 description
        let raw = r#"{"key":"new-key"}"#;
        let parsed: UpdateMcpPresetRequest = serde_json::from_str(raw).expect("parse missing");
        assert!(parsed.description.is_none(), "缺失字段应为 None");
        assert_eq!(parsed.key.as_deref(), Some("new-key"));
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
        assert!(parsed.key.is_none());
        assert!(parsed.display_name.is_none());
        assert!(parsed.description.is_none());
        assert!(parsed.transport.is_none());
        assert!(parsed.route_policy.is_none());
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
