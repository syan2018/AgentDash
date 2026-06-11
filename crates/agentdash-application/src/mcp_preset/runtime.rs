use std::collections::{HashMap, HashSet};

use agentdash_spi::{McpEnvVar, McpHttpHeader, SessionMcpServer, Vfs};
use thiserror::Error;
use uuid::Uuid;

use agentdash_domain::mcp_preset::{
    McpPreset, McpPresetRepository, McpRuntimeBindingConfig, McpRuntimeBindingRule,
    McpRuntimeBindingSource, McpRuntimeBindingTarget, McpTransportConfig,
};

pub fn preset_to_session_mcp_server(preset: &McpPreset) -> SessionMcpServer {
    resolve_preset_mcp_server(preset, None).unwrap_or_else(|_| SessionMcpServer {
        name: preset.key.clone(),
        transport: preset.transport.clone(),
        uses_relay: preset_uses_relay(preset),
    })
}

#[derive(Debug, Clone, Copy)]
pub struct SessionRuntimeMcpContext<'a> {
    pub vfs: Option<&'a Vfs>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionRuntimeMcpContextOwned {
    pub vfs: Option<Vfs>,
}

impl SessionRuntimeMcpContextOwned {
    pub fn as_ref(&self) -> SessionRuntimeMcpContext<'_> {
        SessionRuntimeMcpContext {
            vfs: self.vfs.as_ref(),
        }
    }
}

#[derive(Debug, Error)]
pub enum McpRuntimeBindingError {
    #[error("MCP preset `{preset_key}` runtime_binding 需要 session context")]
    MissingSessionContext { preset_key: String },
    #[error("MCP preset `{preset_key}` runtime_binding 找不到 mount `{mount_id}`")]
    MissingMount {
        preset_key: String,
        mount_id: String,
    },
    #[error(
        "MCP preset `{preset_key}` runtime_binding[{rule_index}] 缺少 required source {source_path}"
    )]
    MissingRequiredSource {
        preset_key: String,
        rule_index: usize,
        source_path: String,
    },
    #[error(
        "MCP preset `{preset_key}` runtime_binding[{rule_index}] source {source_path} 不是可绑定的标量值"
    )]
    InvalidSourceValue {
        preset_key: String,
        rule_index: usize,
        source_path: String,
    },
    #[error(
        "MCP preset `{preset_key}` runtime_binding[{rule_index}] target {target_path} 与 transport `{transport_kind}` 不匹配"
    )]
    TransportMismatch {
        preset_key: String,
        rule_index: usize,
        target_path: String,
        transport_kind: String,
    },
    #[error("MCP preset `{preset_key}` runtime_binding[{rule_index}] target {target_path} 无效: {message}")]
    InvalidTarget {
        preset_key: String,
        rule_index: usize,
        target_path: String,
        message: String,
    },
}

pub fn resolve_preset_mcp_server(
    preset: &McpPreset,
    context: Option<&SessionRuntimeMcpContext<'_>>,
) -> Result<SessionMcpServer, McpRuntimeBindingError> {
    let mut transport = preset.transport.clone();
    if let Some(binding) = &preset.runtime_binding {
        apply_runtime_binding(&preset.key, &mut transport, binding, context)?;
    }
    Ok(SessionMcpServer {
        name: preset.key.clone(),
        uses_relay: preset.route_policy.uses_relay(&transport),
        transport,
    })
}

pub fn preset_uses_relay(preset: &McpPreset) -> bool {
    preset.route_policy.uses_relay(&preset.transport)
}

fn apply_runtime_binding(
    preset_key: &str,
    transport: &mut McpTransportConfig,
    binding: &McpRuntimeBindingConfig,
    context: Option<&SessionRuntimeMcpContext<'_>>,
) -> Result<(), McpRuntimeBindingError> {
    let context =
        context.ok_or_else(|| McpRuntimeBindingError::MissingSessionContext {
            preset_key: preset_key.to_string(),
        })?;
    let vfs = context
        .vfs
        .ok_or_else(|| McpRuntimeBindingError::MissingSessionContext {
            preset_key: preset_key.to_string(),
        })?;
    let mount_id = binding.mount_id.as_deref().unwrap_or("main");
    let mount = vfs.mounts.iter().find(|mount| mount.id == mount_id).ok_or_else(|| {
        McpRuntimeBindingError::MissingMount {
            preset_key: preset_key.to_string(),
            mount_id: mount_id.to_string(),
        }
    })?;

    for (rule_index, rule) in binding.bindings.iter().enumerate() {
        let source_path = source_path(&rule.source);
        let Some(value) = read_source_value(mount, &rule.source).map_err(|_| {
            McpRuntimeBindingError::InvalidSourceValue {
                preset_key: preset_key.to_string(),
                rule_index,
                source_path: source_path.clone(),
            }
        })?
        else {
            if rule.required {
                return Err(McpRuntimeBindingError::MissingRequiredSource {
                    preset_key: preset_key.to_string(),
                    rule_index,
                    source_path,
                });
            }
            continue;
        };
        apply_rule_target(preset_key, transport, rule_index, rule, value)?;
    }

    Ok(())
}

fn read_source_value(
    mount: &agentdash_spi::Mount,
    source: &McpRuntimeBindingSource,
) -> Result<Option<String>, ()> {
    match source {
        McpRuntimeBindingSource::VfsRootRef => Ok(non_empty_value(&mount.root_ref)),
        McpRuntimeBindingSource::VfsBackendId => Ok(non_empty_value(&mount.backend_id)),
        McpRuntimeBindingSource::WorkspaceId => {
            scalar_json_to_string(mount.metadata.get("workspace_id"))
        }
        McpRuntimeBindingSource::WorkspaceBindingId => {
            scalar_json_to_string(mount.metadata.get("workspace_binding_id"))
        }
        McpRuntimeBindingSource::WorkspaceIdentity { path } => {
            scalar_json_to_string(read_json_path(
                mount.metadata.get("workspace_identity_payload"),
                path,
            ))
        }
        McpRuntimeBindingSource::WorkspaceDetectedFact { path } => {
            scalar_json_to_string(read_json_path(
                mount.metadata.get("workspace_detected_facts"),
                path,
            ))
        }
    }
}

fn non_empty_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_json_path<'a>(
    root: Option<&'a serde_json::Value>,
    path: &[String],
) -> Option<&'a serde_json::Value> {
    let mut current = root?;
    for segment in path {
        if segment.trim().is_empty() {
            return None;
        }
        current = current.get(segment)?;
    }
    Some(current)
}

fn scalar_json_to_string(value: Option<&serde_json::Value>) -> Result<Option<String>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(text) => Ok(non_empty_value(text)),
        serde_json::Value::Number(number) => Ok(Some(number.to_string())),
        serde_json::Value::Bool(flag) => Ok(Some(flag.to_string())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Err(()),
    }
}

fn apply_rule_target(
    preset_key: &str,
    transport: &mut McpTransportConfig,
    rule_index: usize,
    rule: &McpRuntimeBindingRule,
    value: String,
) -> Result<(), McpRuntimeBindingError> {
    let target_path = target_path(&rule.target);
    let transport_kind = transport.transport_kind().to_string();
    match (&rule.target, transport) {
        (McpRuntimeBindingTarget::HttpQuery { name }, McpTransportConfig::Http { url, .. })
        | (McpRuntimeBindingTarget::HttpQuery { name }, McpTransportConfig::Sse { url, .. }) => {
            apply_http_query(preset_key, rule_index, name, url, &value)
        }
        (McpRuntimeBindingTarget::HttpHeader { name }, McpTransportConfig::Http { headers, .. })
        | (McpRuntimeBindingTarget::HttpHeader { name }, McpTransportConfig::Sse { headers, .. }) => {
            apply_http_header(preset_key, rule_index, name, headers, value)
        }
        (
            McpRuntimeBindingTarget::StdioEnv { name },
            McpTransportConfig::Stdio { env, .. },
        ) => apply_stdio_env(preset_key, rule_index, name, env, value),
        (McpRuntimeBindingTarget::StdioCwd, McpTransportConfig::Stdio { cwd, .. }) => {
            if value.trim().is_empty() {
                return Err(McpRuntimeBindingError::InvalidTarget {
                    preset_key: preset_key.to_string(),
                    rule_index,
                    target_path,
                    message: "cwd 不能为空".to_string(),
                });
            }
            *cwd = Some(value);
            Ok(())
        }
        _ => Err(McpRuntimeBindingError::TransportMismatch {
            preset_key: preset_key.to_string(),
            rule_index,
            target_path,
            transport_kind,
        }),
    }
}

fn apply_http_query(
    preset_key: &str,
    rule_index: usize,
    name: &str,
    url: &mut String,
    value: &str,
) -> Result<(), McpRuntimeBindingError> {
    let name = validate_target_name(preset_key, rule_index, "http_query", name)?;
    let mut parsed = url::Url::parse(url).map_err(|error| McpRuntimeBindingError::InvalidTarget {
        preset_key: preset_key.to_string(),
        rule_index,
        target_path: format!("http_query.{name}"),
        message: format!("URL 无效: {error}"),
    })?;
    let existing = parsed
        .query_pairs()
        .filter(|(key, _)| key.as_ref() != name)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    parsed.set_query(None);
    {
        let mut pairs = parsed.query_pairs_mut();
        for (key, value) in existing {
            pairs.append_pair(&key, &value);
        }
        pairs.append_pair(name, value);
    }
    *url = parsed.to_string();
    Ok(())
}

fn apply_http_header(
    preset_key: &str,
    rule_index: usize,
    name: &str,
    headers: &mut Vec<McpHttpHeader>,
    value: String,
) -> Result<(), McpRuntimeBindingError> {
    let name = validate_target_name(preset_key, rule_index, "http_header", name)?.to_string();
    headers.retain(|header| !header.name.eq_ignore_ascii_case(&name));
    headers.push(McpHttpHeader { name, value });
    Ok(())
}

fn apply_stdio_env(
    preset_key: &str,
    rule_index: usize,
    name: &str,
    env: &mut Vec<McpEnvVar>,
    value: String,
) -> Result<(), McpRuntimeBindingError> {
    let name = validate_target_name(preset_key, rule_index, "stdio_env", name)?.to_string();
    env.retain(|var| var.name != name);
    env.push(McpEnvVar { name, value });
    Ok(())
}

fn validate_target_name<'a>(
    preset_key: &str,
    rule_index: usize,
    target_kind: &'static str,
    name: &'a str,
) -> Result<&'a str, McpRuntimeBindingError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(McpRuntimeBindingError::InvalidTarget {
            preset_key: preset_key.to_string(),
            rule_index,
            target_path: target_kind.to_string(),
            message: "name 不能为空".to_string(),
        });
    }
    Ok(trimmed)
}

fn source_path(source: &McpRuntimeBindingSource) -> String {
    match source {
        McpRuntimeBindingSource::VfsRootRef => "vfs.main.root_ref".to_string(),
        McpRuntimeBindingSource::VfsBackendId => "vfs.main.backend_id".to_string(),
        McpRuntimeBindingSource::WorkspaceId => "workspace.id".to_string(),
        McpRuntimeBindingSource::WorkspaceBindingId => "workspace.binding_id".to_string(),
        McpRuntimeBindingSource::WorkspaceIdentity { path } => {
            format!("workspace.identity.{}", path.join("."))
        }
        McpRuntimeBindingSource::WorkspaceDetectedFact { path } => {
            format!("workspace.detected_facts.{}", path.join("."))
        }
    }
}

fn target_path(target: &McpRuntimeBindingTarget) -> String {
    match target {
        McpRuntimeBindingTarget::HttpQuery { name } => format!("http_query.{name}"),
        McpRuntimeBindingTarget::HttpHeader { name } => format!("http_header.{name}"),
        McpRuntimeBindingTarget::StdioEnv { name } => format!("stdio_env.{name}"),
        McpRuntimeBindingTarget::StdioCwd => "stdio_cwd".to_string(),
    }
}

/// 从 preset key 列表解析出对应的 `SessionMcpServer` 列表。
pub async fn resolve_preset_mcp_refs(
    repo: &dyn McpPresetRepository,
    project_id: Uuid,
    keys: &[String],
) -> Result<Vec<SessionMcpServer>, String> {
    let presets = resolve_preset_mcp_presets(repo, project_id, keys).await?;
    presets
        .iter()
        .map(|preset| {
            resolve_preset_mcp_server(preset, None)
                .map_err(|error| format!("mcp_preset `{}` 解析失败: {error}", preset.key))
        })
        .collect()
}

/// 从 preset key 列表解析出对应的 `McpPreset` 列表，保留到 final VFS 后再绑定。
pub async fn resolve_preset_mcp_presets(
    repo: &dyn McpPresetRepository,
    project_id: Uuid,
    keys: &[String],
) -> Result<Vec<McpPreset>, String> {
    if keys.is_empty() {
        return Ok(vec![]);
    }

    let presets = repo
        .list_by_project(project_id)
        .await
        .map_err(|error| format!("加载 project MCP Preset 列表失败: {error}"))?;
    let preset_map: HashMap<String, McpPreset> = presets
        .into_iter()
        .map(|preset| (preset.key.clone(), preset))
        .collect();

    let mut selected_presets = Vec::new();
    let mut seen = HashSet::new();

    for (index, key) in keys.iter().enumerate() {
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("mcp_preset_keys[{index}] 不能为空字符串"));
        }
        let preset = preset_map
            .get(key)
            .ok_or_else(|| format!("mcp_preset_keys[{index}] 引用了不存在的 preset: {key}"))?;
        if !seen.insert(preset.key.clone()) {
            continue;
        }
        selected_presets.push(preset.clone());
    }

    Ok(selected_presets)
}
