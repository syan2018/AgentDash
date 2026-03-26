use std::sync::Arc;

use agent_client_protocol::{HttpHeader, McpServer, McpServerHttp, McpServerSse};
use agentdash_application::{
    address_space::{
        SessionMountTarget, container_visible_for_target, effective_context_containers,
    },
    task::config::executor_config_from_preset,
};
use agentdash_domain::{
    context_container::ContextContainerCapability,
    project::{AgentPreset, Project},
    session_binding::{SessionBinding, SessionOwnerType},
    workspace::Workspace,
};
use agentdash_executor::AgentDashExecutorConfig;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
    runtime_bridge::runtime_executor_config_to_connector,
    session_context::normalize_optional_string,
};

#[derive(Debug, Clone)]
pub(crate) struct ProjectAgentBridge {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor_config: AgentDashExecutorConfig,
    pub preset_name: Option<String>,
    pub source: String,
    /// Http/SSE MCP servers parsed from preset config — injected into ExecutionContext for cloud agents
    pub preset_mcp_servers: Vec<McpServer>,
    /// Stdio MCP server JSON decls — forwarded as-is in relay CommandPromptPayload
    pub preset_stdio_mcp_decls: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAgentWritebackMode {
    ReadOnly,
    ConfirmBeforeWrite,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentExecutorResponse {
    pub executor: String,
    pub variant: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<agentdash_executor::ThinkingLevel>,
    pub permission_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentMountResponse {
    pub container_id: String,
    pub mount_id: String,
    pub display_name: String,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSessionResponse {
    pub binding_id: String,
    pub session_id: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSummaryResponse {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor: ProjectAgentExecutorResponse,
    pub preset_name: Option<String>,
    pub source: String,
    pub writeback_mode: ProjectAgentWritebackMode,
    pub shared_context_mounts: Vec<ProjectAgentMountResponse>,
    pub session: Option<ProjectAgentSessionResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OpenProjectAgentSessionResponse {
    pub created: bool,
    pub session_id: String,
    pub binding_id: String,
    pub agent: ProjectAgentSummaryResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_agent_summary_response_serializes_as_snake_case() {
        let value = serde_json::to_value(ProjectAgentSummaryResponse {
            key: "default".to_string(),
            display_name: "项目默认 Agent".to_string(),
            description: "desc".to_string(),
            executor: ProjectAgentExecutorResponse {
                executor: "PI_AGENT".to_string(),
                variant: None,
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-5.4".to_string()),
                agent_id: None,
                thinking_level: None,
                permission_policy: Some("AUTO".to_string()),
            },
            preset_name: None,
            source: "project.config.default_agent_type".to_string(),
            writeback_mode: ProjectAgentWritebackMode::ReadOnly,
            shared_context_mounts: vec![ProjectAgentMountResponse {
                container_id: "project-spec".to_string(),
                mount_id: "spec".to_string(),
                display_name: "项目规范".to_string(),
                writable: false,
            }],
            session: Some(ProjectAgentSessionResponse {
                binding_id: "binding-1".to_string(),
                session_id: "sess-1".to_string(),
                session_title: Some("title".to_string()),
                last_activity: Some(1),
            }),
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("shared_context_mounts").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("sharedContextMounts").is_none());
        assert!(value.get("presetName").is_none());
    }
}

pub async fn list_project_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentSummaryResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let mut agents = list_project_agent_bridges(&project);
    agents.sort_by(|left, right| left.display_name.cmp(&right.display_name));

    let mut response = Vec::with_capacity(agents.len());
    for agent in agents {
        let session = find_project_agent_session(&state, project.id, &agent.key).await?;
        response.push(build_project_agent_summary(&project, &agent, session));
    }

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct OpenSessionQuery {
    #[serde(default)]
    pub force_new: bool,
}

pub async fn open_project_agent_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
    Query(query): Query<OpenSessionQuery>,
) -> Result<Json<OpenProjectAgentSessionResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let agent = resolve_project_agent_bridge(&project, &agent_key)
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;

    let label = project_agent_session_label(&agent.key);

    if !query.force_new {
        let existing_binding = state
            .repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Project, project.id, &label)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        if let Some(binding) = existing_binding
            && let Some(meta) = state
                .services
                .executor_hub
                .get_session_meta(&binding.session_id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?
        {
            let session = Some(ProjectAgentSessionResponse {
                binding_id: binding.id.to_string(),
                session_id: binding.session_id,
                session_title: Some(meta.title),
                last_activity: Some(meta.updated_at),
            });
            let summary = build_project_agent_summary(&project, &agent, session);
            return Ok(Json(OpenProjectAgentSessionResponse {
                created: false,
                session_id: summary
                    .session
                    .as_ref()
                    .map(|item| item.session_id.clone())
                    .unwrap_or_default(),
                binding_id: summary
                    .session
                    .as_ref()
                    .map(|item| item.binding_id.clone())
                    .unwrap_or_default(),
                agent: summary,
            }));
        }
    }

    // Clean up stale binding (session gone from executor hub) -- but only the latest one
    if let Some(binding) = state
        .repos
        .session_binding_repo
        .find_by_owner_and_label(SessionOwnerType::Project, project.id, &label)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        let session_alive = state
            .services
            .executor_hub
            .get_session_meta(&binding.session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .is_some();

        if !session_alive {
            state
                .repos
                .session_binding_repo
                .delete(binding.id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
    }

    let title = format!("{} · {}", project.name.trim(), agent.display_name.trim());
    let meta = state
        .services
        .executor_hub
        .create_session(title.trim())
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let binding = SessionBinding::new(
        project.id,
        meta.id.clone(),
        SessionOwnerType::Project,
        project.id,
        label,
    );
    state
        .repos
        .session_binding_repo
        .create(&binding)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let session = Some(ProjectAgentSessionResponse {
        binding_id: binding.id.to_string(),
        session_id: meta.id.clone(),
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
    });
    let summary = build_project_agent_summary(&project, &agent, session);

    Ok(Json(OpenProjectAgentSessionResponse {
        created: true,
        session_id: meta.id,
        binding_id: binding.id.to_string(),
        agent: summary,
    }))
}

pub(crate) fn resolve_project_agent_bridge(
    project: &Project,
    agent_key: &str,
) -> Option<ProjectAgentBridge> {
    let normalized_key = agent_key.trim();
    if normalized_key.eq_ignore_ascii_case("default") {
        let agent_type = normalize_optional_string(project.config.default_agent_type.clone())?;
        return Some(ProjectAgentBridge {
            key: "default".to_string(),
            display_name: "项目默认 Agent".to_string(),
            description: "用于维护 Project 共享上下文，也可作为 Story 会话启动时的默认后备 Agent。"
                .to_string(),
            executor_config: AgentDashExecutorConfig::new(agent_type),
            preset_name: None,
            source: "project.config.default_agent_type".to_string(),
            preset_mcp_servers: vec![],
            preset_stdio_mcp_decls: vec![],
        });
    }

    let preset_name = normalized_key
        .strip_prefix("preset:")
        .unwrap_or(normalized_key);
    let preset = project
        .config
        .agent_presets
        .iter()
        .find(|item| item.name == preset_name)?;

    Some(build_preset_bridge(preset))
}

pub(crate) fn list_project_agent_bridges(project: &Project) -> Vec<ProjectAgentBridge> {
    let mut agents = Vec::new();
    if let Some(default_agent) = resolve_project_agent_bridge(project, "default") {
        agents.push(default_agent);
    }
    for preset in &project.config.agent_presets {
        agents.push(build_preset_bridge(preset));
    }
    agents
}

pub(crate) async fn resolve_project_workspace(
    state: &Arc<AppState>,
    project: &Project,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return state
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()));
    }
    Ok(None)
}

pub(crate) fn project_agent_session_label(agent_key: &str) -> String {
    format!("project_agent:{}", agent_key.trim())
}

fn build_project_agent_summary(
    project: &Project,
    agent: &ProjectAgentBridge,
    session: Option<ProjectAgentSessionResponse>,
) -> ProjectAgentSummaryResponse {
    let visible_containers = build_project_agent_visible_mounts(project, &agent.executor_config);

    let writeback_mode = if visible_containers.iter().any(|item| item.writable) {
        ProjectAgentWritebackMode::ConfirmBeforeWrite
    } else {
        ProjectAgentWritebackMode::ReadOnly
    };

    ProjectAgentSummaryResponse {
        key: agent.key.clone(),
        display_name: agent.display_name.clone(),
        description: agent.description.clone(),
        executor: ProjectAgentExecutorResponse {
            executor: agent.executor_config.executor.clone(),
            variant: agent.executor_config.variant.clone(),
            provider_id: agent.executor_config.provider_id.clone(),
            model_id: agent.executor_config.model_id.clone(),
            agent_id: agent.executor_config.agent_id.clone(),
            thinking_level: agent.executor_config.thinking_level,
            permission_policy: agent.executor_config.permission_policy.clone(),
        },
        preset_name: agent.preset_name.clone(),
        source: agent.source.clone(),
        writeback_mode,
        shared_context_mounts: visible_containers,
        session,
    }
}

pub(crate) fn build_project_agent_visible_mounts(
    project: &Project,
    executor_config: &AgentDashExecutorConfig,
) -> Vec<ProjectAgentMountResponse> {
    effective_context_containers(project, None)
        .into_iter()
        .filter(|container| {
            container_visible_for_target(
                container,
                SessionMountTarget::Project,
                Some(executor_config.executor.as_str()),
            )
        })
        .map(|container| {
            let container_id = container.id;
            let mount_id = container.mount_id;
            let display_name = if container.display_name.trim().is_empty() {
                container_id.clone()
            } else {
                container.display_name
            };
            let writable = container.default_write
                || container
                    .capabilities
                    .iter()
                    .any(|capability| matches!(capability, ContextContainerCapability::Write));

            ProjectAgentMountResponse {
                container_id,
                mount_id,
                display_name,
                writable,
            }
        })
        .collect::<Vec<_>>()
}

async fn find_project_agent_session(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_key: &str,
) -> Result<Option<ProjectAgentSessionResponse>, ApiError> {
    let binding = state
        .repos
        .session_binding_repo
        .find_by_owner_and_label(
            SessionOwnerType::Project,
            project_id,
            &project_agent_session_label(agent_key),
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let Some(binding) = binding else {
        return Ok(None);
    };

    let meta = state
        .services
        .executor_hub
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Some(ProjectAgentSessionResponse {
        binding_id: binding.id.to_string(),
        session_id: binding.session_id,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
    }))
}

pub async fn list_project_agent_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
) -> Result<Json<Vec<ProjectAgentSessionResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let _agent = resolve_project_agent_bridge(&project, &agent_key)
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;

    let label = project_agent_session_label(&agent_key);
    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Project, project.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let matching: Vec<_> = bindings.into_iter().filter(|b| b.label == label).collect();

    let mut sessions = Vec::with_capacity(matching.len());
    for binding in matching {
        let meta = state
            .services
            .executor_hub
            .get_session_meta(&binding.session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        sessions.push(ProjectAgentSessionResponse {
            binding_id: binding.id.to_string(),
            session_id: binding.session_id,
            session_title: meta.as_ref().map(|m| m.title.clone()),
            last_activity: meta.as_ref().map(|m| m.updated_at),
        });
    }

    sessions.sort_by(|a, b| {
        let at = b.last_activity.unwrap_or(0);
        let bt = a.last_activity.unwrap_or(0);
        at.cmp(&bt)
    });

    Ok(Json(sessions))
}

fn build_preset_bridge(preset: &AgentPreset) -> ProjectAgentBridge {
    let executor_config = executor_config_from_preset(preset)
        .map(|config| runtime_executor_config_to_connector(&config))
        .unwrap_or_else(|| AgentDashExecutorConfig::new(preset.agent_type.clone()));

    let display_name = preset
        .config
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&preset.name)
        .to_string();

    let description = preset
        .config
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "来自 Project Agent 预设，底层执行器为 {}。",
                preset.agent_type.trim()
            )
        });

    let (preset_mcp_servers, preset_stdio_mcp_decls) = parse_preset_mcp_servers(&preset.config);

    ProjectAgentBridge {
        key: format!("preset:{}", preset.name),
        display_name,
        description,
        executor_config,
        preset_name: Some(preset.name.clone()),
        source: format!("project.config.agent_presets[{}]", preset.name),
        preset_mcp_servers,
        preset_stdio_mcp_decls,
    }
}

/// Parse `mcp_servers` from an AgentPreset config JSON value.
///
/// Returns (http_sse_servers, stdio_json_decls).
/// - Http/SSE → constructed as `McpServer::Http` / `McpServer::Sse`
/// - Stdio → kept as raw JSON (forwarded to relay as-is; cannot execute on cloud)
///
/// JSON format:
///   Http:  { "type": "http",  "name": "...", "url": "...", "headers": [...] }
///   SSE:   { "type": "sse",   "name": "...", "url": "...", "headers": [...] }
///   Stdio: { "type": "stdio", "name": "...", "command": "...", "args": [...], "env": [...] }
///   Backward compat: missing `type` → has `url` = Http, has `command` = Stdio
fn parse_preset_mcp_servers(
    config: &serde_json::Value,
) -> (Vec<McpServer>, Vec<serde_json::Value>) {
    let raw_list = match config.get("mcp_servers").and_then(|v| v.as_array()) {
        Some(list) => list,
        None => return (vec![], vec![]),
    };

    let mut mcp_servers = Vec::new();
    let mut stdio_decls = Vec::new();

    for entry in raw_list {
        let obj = match entry.as_object() {
            Some(o) => o,
            None => continue,
        };

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Determine transport type
        let transport = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let has_url = obj.contains_key("url");
        let has_command = obj.contains_key("command");

        let effective_type = if transport == "http" {
            "http"
        } else if transport == "sse" {
            "sse"
        } else if transport == "stdio" {
            "stdio"
        } else if has_url {
            // Backward compat: no type, has url → Http
            "http"
        } else if has_command {
            // Backward compat: no type, has command → Stdio
            "stdio"
        } else {
            tracing::warn!(name = %name, "MCP server entry 缺少 type/url/command，跳过");
            continue;
        };

        match effective_type {
            "http" | "sse" => {
                let url = match obj.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u.to_string(),
                    None => {
                        tracing::warn!(name = %name, "MCP Http/SSE server 缺少 url，跳过");
                        continue;
                    }
                };
                let headers: Vec<HttpHeader> = obj
                    .get("headers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|h| {
                                let ho = h.as_object()?;
                                let hname = ho.get("name")?.as_str()?.to_string();
                                let hvalue = ho.get("value")?.as_str()?.to_string();
                                Some(HttpHeader::new(hname, hvalue))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if effective_type == "http" {
                    mcp_servers.push(McpServer::Http(
                        McpServerHttp::new(name, url).headers(headers),
                    ));
                } else {
                    mcp_servers.push(McpServer::Sse(
                        McpServerSse::new(name, url).headers(headers),
                    ));
                }
            }
            "stdio" => {
                // Stdio servers are forwarded as raw JSON to relay for local execution
                // Build a normalized JSON representation
                let command = obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if command.is_empty() {
                    tracing::warn!(name = %name, "MCP Stdio server 缺少 command，跳过");
                    continue;
                }
                let args: Vec<serde_json::Value> = obj
                    .get("args")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let env: Vec<serde_json::Value> = obj
                    .get("env")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                stdio_decls.push(serde_json::json!({
                    "type": "stdio",
                    "name": name,
                    "command": command,
                    "args": args,
                    "env": env,
                }));
            }
            _ => {}
        }
    }

    (mcp_servers, stdio_decls)
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}
