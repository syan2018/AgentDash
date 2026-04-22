use serde::{Deserialize, Serialize};

/// MCP HTTP header 键值对。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpHttpHeader {
    pub name: String,
    pub value: String,
}

/// MCP stdio transport 环境变量键值对。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpEnvVar {
    pub name: String,
    pub value: String,
}

/// MCP transport 声明——仅描述如何连接，不包含展示名、路由策略或 server identity。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
    },
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<McpEnvVar>,
    },
}

impl McpTransportConfig {
    /// 返回 transport 类型标签——用于日志、只读预览卡摘要等场景。
    pub fn transport_kind(&self) -> &'static str {
        match self {
            Self::Http { .. } => "http",
            Self::Sse { .. } => "sse",
            Self::Stdio { .. } => "stdio",
        }
    }
}

/// 应用层路由策略——不属于 transport 连接定义。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpRoutePolicy {
    /// 按 transport 默认策略决定：stdio 走 relay，http/sse 走直连。
    Auto,
    /// 强制通过本机 relay 路径访问。
    Relay,
    /// 强制直连。
    Direct,
}

impl Default for McpRoutePolicy {
    fn default() -> Self {
        Self::Auto
    }
}

impl McpRoutePolicy {
    /// 将 route policy 解析为运行时是否走 relay。
    pub fn uses_relay(self, transport: &McpTransportConfig) -> bool {
        match self {
            Self::Relay => true,
            Self::Direct => false,
            Self::Auto => matches!(transport, McpTransportConfig::Stdio { .. }),
        }
    }
}

/// MCP Preset 来源——对齐 Workflow 的 `WorkflowDefinitionSource::BuiltinSeed` / `UserAuthored` 语义。
///
/// - `Builtin { key }`：由平台内置 JSON 装载而来，key 对应 `builtins/*.json` 文件名根。
///   前端以此标识渲染只读态，并允许「复制为 user」生成可编辑副本。
/// - `User`：用户手工创建或从 builtin 复制的副本，完全可编辑。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpPresetSource {
    Builtin { key: String },
    User,
}

impl McpPresetSource {
    /// 返回数据库列存储用的 source 类型字符串（`builtin` / `user`）。
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Builtin { .. } => "builtin",
            Self::User => "user",
        }
    }

    /// 返回 builtin 来源的 key；非 builtin 时返回 None。
    pub fn builtin_key(&self) -> Option<&str> {
        match self {
            Self::Builtin { key } => Some(key.as_str()),
            Self::User => None,
        }
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self, Self::Builtin { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_roundtrip_http() {
        let decl = McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: vec![McpHttpHeader {
                name: "Authorization".to_string(),
                value: "Bearer x".to_string(),
            }],
        };
        let json = serde_json::to_string(&decl).expect("serialize http");
        let back: McpTransportConfig = serde_json::from_str(&json).expect("deserialize http");
        assert_eq!(back, decl);
        assert_eq!(back.transport_kind(), "http");
    }

    #[test]
    fn transport_roundtrip_stdio() {
        let decl = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
            ],
            env: vec![McpEnvVar {
                name: "ROOT".to_string(),
                value: ".".to_string(),
            }],
        };
        let json = serde_json::to_string(&decl).expect("serialize stdio");
        let back: McpTransportConfig = serde_json::from_str(&json).expect("deserialize stdio");
        assert_eq!(back, decl);
        assert_eq!(back.transport_kind(), "stdio");
    }

    #[test]
    fn route_policy_auto_uses_stdio_default() {
        let stdio = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec![],
            env: vec![],
        };
        let http = McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
        };
        assert!(McpRoutePolicy::Auto.uses_relay(&stdio));
        assert!(!McpRoutePolicy::Auto.uses_relay(&http));
        assert!(McpRoutePolicy::Relay.uses_relay(&http));
        assert!(!McpRoutePolicy::Direct.uses_relay(&stdio));
    }

    #[test]
    fn source_builtin_serialization() {
        let builtin = McpPresetSource::Builtin {
            key: "filesystem".to_string(),
        };
        let json = serde_json::to_string(&builtin).expect("serialize builtin");
        assert!(json.contains("\"kind\":\"builtin\""));
        assert!(json.contains("\"key\":\"filesystem\""));
        let back: McpPresetSource = serde_json::from_str(&json).expect("deserialize builtin");
        assert_eq!(back, builtin);
        assert_eq!(back.tag(), "builtin");
        assert_eq!(back.builtin_key(), Some("filesystem"));
        assert!(back.is_builtin());
    }

    #[test]
    fn source_user_serialization() {
        let user = McpPresetSource::User;
        let json = serde_json::to_string(&user).expect("serialize user");
        assert_eq!(json, "{\"kind\":\"user\"}");
        let back: McpPresetSource = serde_json::from_str(&json).expect("deserialize user");
        assert_eq!(back, user);
        assert_eq!(back.tag(), "user");
        assert!(back.builtin_key().is_none());
        assert!(!back.is_builtin());
    }
}
