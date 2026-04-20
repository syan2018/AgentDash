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

/// MCP Server 声明——对齐前端 `McpServerDecl` 联合体，支持 http / sse / stdio 三种 transport。
///
/// 此类型故意与 `agent_client_protocol::McpServer` 解耦：Preset 是「配置模板」，
/// 运行时展开后再由 agent 组装路径转换到 ACP McpServer。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerDecl {
    Http {
        name: String,
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
        /// 是否强制通过本机 relay 调用。默认 None 时由 connector 按策略决定。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<bool>,
    },
    Sse {
        name: String,
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        headers: Vec<McpHttpHeader>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<bool>,
    },
    Stdio {
        name: String,
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<McpEnvVar>,
        /// stdio transport 默认通过本机 relay；显式置 false 仅用于本机直连场景。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<bool>,
    },
}

impl McpServerDecl {
    /// 返回 MCP server 声明中的 `name` 字段。
    pub fn server_name(&self) -> &str {
        match self {
            Self::Http { name, .. } | Self::Sse { name, .. } | Self::Stdio { name, .. } => name,
        }
    }

    /// 返回 transport 类型标签——用于日志、只读预览卡摘要等场景。
    pub fn transport_kind(&self) -> &'static str {
        match self {
            Self::Http { .. } => "http",
            Self::Sse { .. } => "sse",
            Self::Stdio { .. } => "stdio",
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
    fn server_decl_roundtrip_http() {
        let decl = McpServerDecl::Http {
            name: "fetch".to_string(),
            url: "https://example.com/mcp".to_string(),
            headers: vec![McpHttpHeader {
                name: "Authorization".to_string(),
                value: "Bearer x".to_string(),
            }],
            relay: Some(false),
        };
        let json = serde_json::to_string(&decl).expect("serialize http");
        let back: McpServerDecl = serde_json::from_str(&json).expect("deserialize http");
        assert_eq!(back, decl);
        assert_eq!(back.server_name(), "fetch");
        assert_eq!(back.transport_kind(), "http");
    }

    #[test]
    fn server_decl_roundtrip_stdio() {
        let decl = McpServerDecl::Stdio {
            name: "filesystem".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
            ],
            env: vec![McpEnvVar {
                name: "ROOT".to_string(),
                value: ".".to_string(),
            }],
            relay: None,
        };
        let json = serde_json::to_string(&decl).expect("serialize stdio");
        let back: McpServerDecl = serde_json::from_str(&json).expect("deserialize stdio");
        assert_eq!(back, decl);
        assert_eq!(back.transport_kind(), "stdio");
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
