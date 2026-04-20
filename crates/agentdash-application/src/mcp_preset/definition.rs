use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::mcp_preset::{McpPreset, McpServerDecl};

/// 首批内置 MCP Preset key——与 `builtins/<key>.json` 文件名一一对应。
///
/// 约定：`builtins/` 下 JSON 文件的文件名根即为 `builtin_key`，
/// 也会写入到 `McpPresetSource::Builtin { key }` 和数据库 `builtin_key` 列。
pub const BUILTIN_MCP_PRESET_FILESYSTEM_KEY: &str = "filesystem";
pub const BUILTIN_MCP_PRESET_FETCH_KEY: &str = "fetch";

/// Builtin Preset 模板——从 JSON 反序列化，用于在运行时为某个 project 生成 `McpPreset` 实例。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinMcpPresetTemplate {
    /// builtin key，对齐文件名根（如 `filesystem`、`fetch`）。
    pub key: String,
    /// Preset 默认名称（project 内唯一——复制为 user 时前端可重命名）。
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MCP server 声明本体。
    pub server_decl: McpServerDecl,
}

impl BuiltinMcpPresetTemplate {
    /// 在给定 project 下生成一个 builtin-sourced Preset 实例。
    pub fn instantiate(&self, project_id: Uuid) -> McpPreset {
        McpPreset::new_builtin(
            project_id,
            self.key.clone(),
            self.name.clone(),
            self.description.clone(),
            self.server_decl.clone(),
        )
    }
}

/// 装载首批内置 MCP Preset 模板。
///
/// 对齐 `workflow::definition::list_builtin_workflow_templates` 的模式——
/// 模板 JSON 通过 `include_str!` 在编译期嵌入 binary，保证单测与运行时一致。
pub fn list_builtin_mcp_preset_templates() -> Result<Vec<BuiltinMcpPresetTemplate>, String> {
    [
        include_str!("builtins/filesystem.json"),
        include_str!("builtins/fetch.json"),
    ]
    .into_iter()
    .map(parse_builtin_mcp_preset_template)
    .collect()
}

/// 按 key 获取单个 builtin 模板。
pub fn get_builtin_mcp_preset_template(
    builtin_key: &str,
) -> Result<Option<BuiltinMcpPresetTemplate>, String> {
    let templates = list_builtin_mcp_preset_templates()?;
    Ok(templates.into_iter().find(|item| item.key == builtin_key))
}

fn parse_builtin_mcp_preset_template(raw: &str) -> Result<BuiltinMcpPresetTemplate, String> {
    serde_json::from_str::<BuiltinMcpPresetTemplate>(raw)
        .map_err(|error| format!("解析 builtin MCP Preset 模板失败: {error}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn builtin_mcp_preset_templates_load_cleanly() {
        let templates = list_builtin_mcp_preset_templates().expect("load templates");
        assert!(!templates.is_empty(), "至少应有一条 builtin 模板");

        // key 唯一性
        let keys: BTreeSet<&str> = templates.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys.len(),
            templates.len(),
            "builtin MCP Preset key 必须在模板间唯一"
        );
        assert!(keys.contains(BUILTIN_MCP_PRESET_FILESYSTEM_KEY));
        assert!(keys.contains(BUILTIN_MCP_PRESET_FETCH_KEY));
    }

    #[test]
    fn builtin_mcp_preset_template_instantiate_preserves_key() {
        let template =
            get_builtin_mcp_preset_template(BUILTIN_MCP_PRESET_FILESYSTEM_KEY)
                .expect("load")
                .expect("filesystem template exists");
        let project_id = Uuid::new_v4();
        let preset = template.instantiate(project_id);

        assert_eq!(preset.project_id, project_id);
        assert_eq!(preset.source.builtin_key(), Some(template.key.as_str()));
        assert_eq!(preset.name, template.name);
        assert!(preset.is_builtin());
    }
}
