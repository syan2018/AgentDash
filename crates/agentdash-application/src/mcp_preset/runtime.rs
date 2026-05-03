use std::collections::{HashMap, HashSet};

use agentdash_spi::{McpEnvVar, McpHeader, McpTransportConfig as SpiTransportConfig, SessionMcpServer};
use uuid::Uuid;

use agentdash_domain::mcp_preset::{McpPreset, McpPresetRepository, McpTransportConfig};

pub fn preset_to_session_mcp_server(preset: &McpPreset) -> SessionMcpServer {
    let uses_relay = preset_uses_relay(preset);
    let transport = match &preset.transport {
        McpTransportConfig::Http { url, headers } => SpiTransportConfig::Http {
            url: url.clone(),
            headers: headers.iter().map(|h| McpHeader { name: h.name.clone(), value: h.value.clone() }).collect(),
        },
        McpTransportConfig::Sse { url, headers } => SpiTransportConfig::Sse {
            url: url.clone(),
            headers: headers.iter().map(|h| McpHeader { name: h.name.clone(), value: h.value.clone() }).collect(),
        },
        McpTransportConfig::Stdio { command, args, env } => SpiTransportConfig::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: env.iter().map(|e| McpEnvVar { name: e.name.clone(), value: e.value.clone() }).collect(),
        },
    };
    SessionMcpServer { name: preset.key.clone(), transport, uses_relay }
}

pub fn preset_uses_relay(preset: &McpPreset) -> bool {
    preset.route_policy.uses_relay(&preset.transport)
}

pub async fn resolve_config_mcp_preset_refs(
    repo: &dyn McpPresetRepository,
    project_id: Uuid,
    config: &serde_json::Value,
) -> Result<Vec<SessionMcpServer>, String> {
    let raw_list = match config.get("mcp_preset_keys").and_then(|v| v.as_array()) {
        Some(list) => list,
        None => return Ok(vec![]),
    };

    let presets = repo
        .list_by_project(project_id)
        .await
        .map_err(|error| format!("加载 project MCP Preset 列表失败: {error}"))?;
    let preset_map: HashMap<String, McpPreset> = presets
        .into_iter()
        .map(|preset| (preset.key.clone(), preset))
        .collect();

    let mut mcp_servers = Vec::new();
    let mut seen = HashSet::new();

    for (index, entry) in raw_list.iter().enumerate() {
        let key = entry
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("mcp_preset_keys[{index}] 缺失或不是非空字符串"))?;
        let preset = preset_map
            .get(key)
            .ok_or_else(|| format!("mcp_preset_keys[{index}] 引用了不存在的 preset: {key}"))?;
        if !seen.insert(preset.key.clone()) {
            continue;
        }
        mcp_servers.push(preset_to_session_mcp_server(preset));
    }

    Ok(mcp_servers)
}
