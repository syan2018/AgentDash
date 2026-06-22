use std::collections::{HashMap, HashSet};

use agentdash_domain::backend::RuntimeBackendAnchor;
use agentdash_spi::{McpEnvVar, McpHttpHeader, RuntimeMcpServer, Vfs};
use thiserror::Error;
use uuid::Uuid;

use agentdash_domain::mcp_preset::{
    McpPreset, McpPresetRepository, McpRuntimeBindingConfig, McpRuntimeBindingRule,
    McpRuntimeBindingSource, McpRuntimeBindingTarget, McpTransportConfig,
};

pub fn preset_to_runtime_mcp_server(preset: &McpPreset) -> RuntimeMcpServer {
    resolve_preset_mcp_server(preset, None).unwrap_or_else(|_| RuntimeMcpServer {
        name: preset.key.clone(),
        transport: preset.transport.clone(),
        uses_relay: preset_uses_relay(preset),
    })
}

#[derive(Debug, Clone, Copy)]
pub struct McpRuntimeBindingContext<'a> {
    pub vfs: Option<&'a Vfs>,
    pub backend_anchor: Option<&'a RuntimeBackendAnchor>,
}

#[derive(Debug, Error)]
pub enum McpRuntimeBindingError {
    #[error("MCP preset `{preset_key}` runtime_binding 需要 MCP runtime binding context")]
    MissingRuntimeBindingContext { preset_key: String },
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
        "MCP preset `{preset_key}` runtime_binding[{rule_index}] 缺少 runtime backend anchor source {source_path}"
    )]
    MissingRuntimeBackendAnchor {
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
    #[error(
        "MCP preset `{preset_key}` runtime_binding[{rule_index}] target {target_path} 无效: {message}"
    )]
    InvalidTarget {
        preset_key: String,
        rule_index: usize,
        target_path: String,
        message: String,
    },
}

pub fn resolve_preset_mcp_server(
    preset: &McpPreset,
    context: Option<&McpRuntimeBindingContext<'_>>,
) -> Result<RuntimeMcpServer, McpRuntimeBindingError> {
    let mut transport = preset.transport.clone();
    if let Some(binding) = &preset.runtime_binding {
        apply_runtime_binding(&preset.key, &mut transport, binding, context)?;
    }
    Ok(RuntimeMcpServer {
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
    context: Option<&McpRuntimeBindingContext<'_>>,
) -> Result<(), McpRuntimeBindingError> {
    let context = context.ok_or_else(|| McpRuntimeBindingError::MissingRuntimeBindingContext {
        preset_key: preset_key.to_string(),
    })?;
    let vfs = context
        .vfs
        .ok_or_else(|| McpRuntimeBindingError::MissingRuntimeBindingContext {
            preset_key: preset_key.to_string(),
        })?;
    let mount_id = binding.mount_id.as_deref().unwrap_or("main");
    let mount = vfs
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| McpRuntimeBindingError::MissingMount {
            preset_key: preset_key.to_string(),
            mount_id: mount_id.to_string(),
        })?;

    for (rule_index, rule) in binding.bindings.iter().enumerate() {
        let source_path = source_path(&rule.source);
        let source_value = match read_source_value(context, mount, &rule.source) {
            Ok(value) => value,
            Err(SourceReadError::MissingRuntimeBackendAnchor) if !rule.required => None,
            Err(SourceReadError::MissingRuntimeBackendAnchor) => {
                return Err(McpRuntimeBindingError::MissingRuntimeBackendAnchor {
                    preset_key: preset_key.to_string(),
                    rule_index,
                    source_path: source_path.clone(),
                });
            }
            Err(SourceReadError::InvalidScalar) => {
                return Err(McpRuntimeBindingError::InvalidSourceValue {
                    preset_key: preset_key.to_string(),
                    rule_index,
                    source_path: source_path.clone(),
                });
            }
        };
        let Some(value) = source_value else {
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
    context: &McpRuntimeBindingContext<'_>,
    mount: &agentdash_spi::Mount,
    source: &McpRuntimeBindingSource,
) -> Result<Option<String>, SourceReadError> {
    match source {
        McpRuntimeBindingSource::VfsRootRef => Ok(non_empty_value(&mount.root_ref)),
        McpRuntimeBindingSource::RuntimeBackendAnchorBackendId => context
            .backend_anchor
            .map(|anchor| non_empty_value(anchor.backend_id()))
            .ok_or(SourceReadError::MissingRuntimeBackendAnchor),
        McpRuntimeBindingSource::WorkspaceId => {
            scalar_json_to_string(mount.metadata.get("workspace_id"))
        }
        McpRuntimeBindingSource::WorkspaceBindingId => {
            scalar_json_to_string(mount.metadata.get("workspace_binding_id"))
        }
        McpRuntimeBindingSource::WorkspaceIdentity { path } => scalar_json_to_string(
            read_json_path(mount.metadata.get("workspace_identity_payload"), path),
        ),
        McpRuntimeBindingSource::WorkspaceDetectedFact { path } => scalar_json_to_string(
            read_json_path(mount.metadata.get("workspace_detected_facts"), path),
        ),
    }
}

enum SourceReadError {
    InvalidScalar,
    MissingRuntimeBackendAnchor,
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

fn scalar_json_to_string(
    value: Option<&serde_json::Value>,
) -> Result<Option<String>, SourceReadError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(text) => Ok(non_empty_value(text)),
        serde_json::Value::Number(number) => Ok(Some(number.to_string())),
        serde_json::Value::Bool(flag) => Ok(Some(flag.to_string())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Err(SourceReadError::InvalidScalar)
        }
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
        (
            McpRuntimeBindingTarget::HttpHeader { name },
            McpTransportConfig::Http { headers, .. },
        )
        | (McpRuntimeBindingTarget::HttpHeader { name }, McpTransportConfig::Sse { headers, .. }) => {
            apply_http_header(preset_key, rule_index, name, headers, value)
        }
        (McpRuntimeBindingTarget::StdioEnv { name }, McpTransportConfig::Stdio { env, .. }) => {
            apply_stdio_env(preset_key, rule_index, name, env, value)
        }
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
    let mut parsed =
        url::Url::parse(url).map_err(|error| McpRuntimeBindingError::InvalidTarget {
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
        McpRuntimeBindingSource::RuntimeBackendAnchorBackendId => {
            "runtime_backend_anchor.backend_id".to_string()
        }
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

/// 从 preset key 列表解析出对应的 `RuntimeMcpServer` 列表。
pub async fn resolve_preset_mcp_server_refs(
    repo: &dyn McpPresetRepository,
    project_id: Uuid,
    keys: &[String],
) -> Result<Vec<RuntimeMcpServer>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::mcp_preset::{McpRoutePolicy, McpTransportConfig};
    use serde_json::json;

    fn test_vfs() -> Vfs {
        Vfs {
            mounts: vec![agentdash_spi::Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "main://workspace".to_string(),
                capabilities: vec![],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: json!({
                    "workspace_id": "workspace-1",
                    "workspace_binding_id": "binding-1",
                    "workspace_identity_payload": {
                        "kind": "p4_workspace",
                        "name": "identity-main"
                    },
                    "workspace_detected_facts": {
                        "p4": {
                            "client_name": "p4-client-main",
                            "server_address": "ssl:p4.example:1666",
                            "stream": "//stream/main",
                            "workspace_root": "F:/work/main",
                            "user_name": "dev-user"
                        }
                    }
                }),
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: vec![],
        }
    }

    fn runtime_context(vfs: &Vfs) -> McpRuntimeBindingContext<'_> {
        McpRuntimeBindingContext {
            vfs: Some(vfs),
            backend_anchor: None,
        }
    }

    fn http_preset(binding: McpRuntimeBindingConfig) -> McpPreset {
        McpPreset::new_user(
            Uuid::new_v4(),
            "p4-local",
            "P4 Local",
            None,
            McpTransportConfig::Http {
                url: "http://127.0.0.1:7357/mcp?p4_client=old".to_string(),
                headers: vec![],
            },
            McpRoutePolicy::Direct,
        )
        .with_runtime_binding(Some(binding))
    }

    #[test]
    fn runtime_binding_applies_http_query_and_header() {
        let vfs = test_vfs();
        let context = runtime_context(&vfs);
        let preset = http_preset(McpRuntimeBindingConfig {
            mount_id: None,
            bindings: vec![
                McpRuntimeBindingRule {
                    source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                        path: vec!["p4".to_string(), "client_name".to_string()],
                    },
                    target: McpRuntimeBindingTarget::HttpQuery {
                        name: "p4_client".to_string(),
                    },
                    required: true,
                },
                McpRuntimeBindingRule {
                    source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                        path: vec!["p4".to_string(), "workspace_root".to_string()],
                    },
                    target: McpRuntimeBindingTarget::HttpHeader {
                        name: "x-workspace-root".to_string(),
                    },
                    required: true,
                },
            ],
        });

        let server =
            resolve_preset_mcp_server(&preset, Some(&context)).expect("runtime binding resolves");

        let McpTransportConfig::Http { url, headers } = server.transport else {
            panic!("expected http transport");
        };
        let parsed = url::Url::parse(&url).expect("resolved url");
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "p4_client")
                .map(|(_, value)| value.into_owned()),
            Some("p4-client-main".to_string())
        );
        assert_eq!(
            headers
                .iter()
                .find(|header| header.name == "x-workspace-root")
                .map(|header| header.value.as_str()),
            Some("F:/work/main")
        );
    }

    #[test]
    fn runtime_binding_applies_stdio_env_and_cwd() {
        let vfs = test_vfs();
        let context = runtime_context(&vfs);
        let preset = McpPreset::new_user(
            Uuid::new_v4(),
            "p4-stdio",
            "P4 Stdio",
            None,
            McpTransportConfig::Stdio {
                command: "p4-mcp".to_string(),
                args: vec![],
                env: vec![],
                cwd: None,
            },
            McpRoutePolicy::Direct,
        )
        .with_runtime_binding(Some(McpRuntimeBindingConfig {
            mount_id: None,
            bindings: vec![
                McpRuntimeBindingRule {
                    source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                        path: vec!["p4".to_string(), "client_name".to_string()],
                    },
                    target: McpRuntimeBindingTarget::StdioEnv {
                        name: "P4CLIENT".to_string(),
                    },
                    required: true,
                },
                McpRuntimeBindingRule {
                    source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                        path: vec!["p4".to_string(), "workspace_root".to_string()],
                    },
                    target: McpRuntimeBindingTarget::StdioCwd,
                    required: true,
                },
            ],
        }));

        let server =
            resolve_preset_mcp_server(&preset, Some(&context)).expect("runtime binding resolves");

        let McpTransportConfig::Stdio { env, cwd, .. } = server.transport else {
            panic!("expected stdio transport");
        };
        assert_eq!(
            env.iter()
                .find(|var| var.name == "P4CLIENT")
                .map(|var| var.value.as_str()),
            Some("p4-client-main")
        );
        assert_eq!(cwd.as_deref(), Some("F:/work/main"));
    }

    #[test]
    fn missing_required_runtime_source_fails_with_diagnostic() {
        let vfs = test_vfs();
        let context = runtime_context(&vfs);
        let preset = http_preset(McpRuntimeBindingConfig {
            mount_id: None,
            bindings: vec![McpRuntimeBindingRule {
                source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                    path: vec!["p4".to_string(), "missing".to_string()],
                },
                target: McpRuntimeBindingTarget::HttpQuery {
                    name: "p4_missing".to_string(),
                },
                required: true,
            }],
        });

        let error = resolve_preset_mcp_server(&preset, Some(&context)).expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("p4-local"));
        assert!(message.contains("workspace.detected_facts.p4.missing"));
    }

    #[test]
    fn runtime_binding_backend_source_reads_runtime_anchor_not_vfs_mount() {
        let mut vfs = test_vfs();
        vfs.mounts[0].backend_id = "stale-vfs-backend".to_string();
        let anchor = RuntimeBackendAnchor::new(
            "anchor-backend",
            agentdash_domain::backend::RuntimeBackendAnchorSource::System,
        )
        .expect("anchor");
        let context = McpRuntimeBindingContext {
            vfs: Some(&vfs),
            backend_anchor: Some(&anchor),
        };
        let preset = http_preset(McpRuntimeBindingConfig {
            mount_id: None,
            bindings: vec![McpRuntimeBindingRule {
                source: McpRuntimeBindingSource::RuntimeBackendAnchorBackendId,
                target: McpRuntimeBindingTarget::HttpHeader {
                    name: "x-runtime-backend".to_string(),
                },
                required: true,
            }],
        });

        let server =
            resolve_preset_mcp_server(&preset, Some(&context)).expect("runtime binding resolves");

        let McpTransportConfig::Http { headers, .. } = server.transport else {
            panic!("expected http transport");
        };
        assert_eq!(
            headers
                .iter()
                .find(|header| header.name == "x-runtime-backend")
                .map(|header| header.value.as_str()),
            Some("anchor-backend")
        );
    }

    #[test]
    fn required_runtime_anchor_source_fails_without_anchor() {
        let vfs = test_vfs();
        let context = runtime_context(&vfs);
        let preset = http_preset(McpRuntimeBindingConfig {
            mount_id: None,
            bindings: vec![McpRuntimeBindingRule {
                source: McpRuntimeBindingSource::RuntimeBackendAnchorBackendId,
                target: McpRuntimeBindingTarget::HttpHeader {
                    name: "x-runtime-backend".to_string(),
                },
                required: true,
            }],
        });

        let error = resolve_preset_mcp_server(&preset, Some(&context)).expect_err("must fail");

        assert!(matches!(
            error,
            McpRuntimeBindingError::MissingRuntimeBackendAnchor { .. }
        ));
    }
}
