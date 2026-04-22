use std::collections::{HashMap, HashSet};

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use uuid::Uuid;

use agentdash_domain::mcp_preset::{McpPreset, McpPresetRepository, McpTransportConfig};

pub fn preset_to_acp_server(preset: &McpPreset) -> McpServer {
    match &preset.transport {
        McpTransportConfig::Http { url, headers } => {
            let mapped_headers: Vec<HttpHeader> = headers
                .iter()
                .map(|h| HttpHeader::new(h.name.clone(), h.value.clone()))
                .collect();
            McpServer::Http(
                McpServerHttp::new(preset.key.clone(), url.clone()).headers(mapped_headers),
            )
        }
        McpTransportConfig::Sse { url, headers } => {
            let mapped_headers: Vec<HttpHeader> = headers
                .iter()
                .map(|h| HttpHeader::new(h.name.clone(), h.value.clone()))
                .collect();
            McpServer::Sse(
                McpServerSse::new(preset.key.clone(), url.clone()).headers(mapped_headers),
            )
        }
        McpTransportConfig::Stdio { command, args, env } => {
            let mapped_env: Vec<EnvVariable> = env
                .iter()
                .map(|e| EnvVariable::new(e.name.clone(), e.value.clone()))
                .collect();
            McpServer::Stdio(
                McpServerStdio::new(preset.key.clone(), command.clone())
                    .args(args.clone())
                    .env(mapped_env),
            )
        }
    }
}

pub fn preset_uses_relay(preset: &McpPreset) -> bool {
    preset.route_policy.uses_relay(&preset.transport)
}

pub async fn resolve_config_mcp_preset_refs(
    repo: &dyn McpPresetRepository,
    project_id: Uuid,
    config: &serde_json::Value,
) -> Result<(Vec<McpServer>, HashSet<String>), String> {
    let raw_list = match config.get("mcp_preset_keys").and_then(|v| v.as_array()) {
        Some(list) => list,
        None => return Ok((vec![], HashSet::new())),
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
    let mut relay_names = HashSet::new();
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
        if preset_uses_relay(preset) {
            relay_names.insert(preset.key.clone());
        }
        mcp_servers.push(preset_to_acp_server(preset));
    }

    Ok((mcp_servers, relay_names))
}
