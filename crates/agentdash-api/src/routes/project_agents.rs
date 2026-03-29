use std::sync::Arc;

use agent_client_protocol::{HttpHeader, McpServer, McpServerHttp, McpServerSse};
use agentdash_application::address_space::{
    SessionMountTarget, container_visible_for_target, effective_context_containers,
};
use agentdash_domain::{
    agent::{Agent, ProjectAgentLink},
    context_container::ContextContainerCapability,
    project::Project,
    session_binding::{SessionBinding, SessionOwnerType},
    workspace::Workspace,
};
use agentdash_spi::AgentConfig;
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
};

#[derive(Debug, Clone)]
pub(crate) struct ProjectAgentBridge {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor_config: AgentConfig,
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
    pub thinking_level: Option<agentdash_spi::ThinkingLevel>,
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

    let links = state
        .repos
        .agent_link_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut response = Vec::with_capacity(links.len());
    for link in &links {
        let Some(agent) = state
            .repos
            .agent_repo
            .get_by_id(link.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        else {
            continue;
        };
        let bridge = build_agent_bridge(&agent, link);
        let session = find_project_agent_session(&state, project.id, &bridge.key).await?;
        response.push(build_project_agent_summary(&project, &bridge, session));
    }

    response.sort_by(|a, b| a.display_name.cmp(&b.display_name));
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

    let agent_id = parse_agent_id(&agent_key)?;
    let agent_entity = state
        .repos
        .agent_repo
        .get_by_id(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Agent `{agent_key}` 不存在")))?;
    let link = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("该 Agent 未关联到此项目".into()))?;
    let agent = build_agent_bridge(&agent_entity, &link);

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
                .session_hub
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
            .session_hub
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
        .session_hub
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

    // 自动启动 Lifecycle Run（如果 Agent Link 配置了 default_lifecycle_key）
    if let Some(lifecycle_key) =
        resolve_agent_default_lifecycle(&state, project.id, &agent_key).await
        && let Err(err) = auto_start_lifecycle_run(&state, project.id, &lifecycle_key).await
    {
        tracing::warn!(
            project_id = %project.id,
            agent_key = %agent_key,
            lifecycle_key = %lifecycle_key,
            error = %err,
            "自动启动 Lifecycle Run 失败（不阻塞 session 创建）"
        );
    }

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

/// 从 agent_key（UUID 字符串）异步解析 ProjectAgentBridge
pub(crate) async fn resolve_project_agent_bridge_async(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_key: &str,
) -> Option<ProjectAgentBridge> {
    let agent_id = Uuid::parse_str(agent_key).ok()?;
    let agent = state.repos.agent_repo.get_by_id(agent_id).await.ok()??;
    let link = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .ok()??;
    Some(build_agent_bridge(&agent, &link))
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
    executor_config: &AgentConfig,
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
        .session_hub
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
            .session_hub
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

/// Parse `mcp_servers` from agent config JSON value.
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

fn parse_agent_id(agent_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(agent_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 agent_id: {agent_id}")))
}

// ─── Project-Agent Link API ───

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentLinkResponse {
    pub id: String,
    pub project_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub agent_type: String,
    pub merged_config: serde_json::Value,
    pub config_override: Option<serde_json::Value>,
    pub default_lifecycle_key: Option<String>,
    pub is_default_for_story: bool,
    pub is_default_for_task: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn build_link_response(agent: &Agent, link: &ProjectAgentLink) -> ProjectAgentLinkResponse {
    ProjectAgentLinkResponse {
        id: link.id.to_string(),
        project_id: link.project_id.to_string(),
        agent_id: link.agent_id.to_string(),
        agent_name: agent.name.clone(),
        agent_type: agent.agent_type.clone(),
        merged_config: link.merged_config(&agent.base_config),
        config_override: link.config_override.clone(),
        default_lifecycle_key: link.default_lifecycle_key.clone(),
        is_default_for_story: link.is_default_for_story,
        is_default_for_task: link.is_default_for_task,
        created_at: link.created_at.to_rfc3339(),
        updated_at: link.updated_at.to_rfc3339(),
    }
}

/// GET /projects/{id}/agent-links — 列出项目关联的所有 Agent（新模型）
pub async fn list_project_agent_links(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentLinkResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let links = state
        .repos
        .agent_link_repo
        .list_by_project(project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut response = Vec::with_capacity(links.len());
    for link in &links {
        if let Some(agent) = state
            .repos
            .agent_repo
            .get_by_id(link.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            response.push(build_link_response(&agent, link));
        }
    }
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectAgentLinkRequest {
    pub agent_id: String,
    #[serde(default)]
    pub config_override: Option<serde_json::Value>,
    #[serde(default)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    pub is_default_for_story: bool,
    #[serde(default)]
    pub is_default_for_task: bool,
}

/// POST /projects/{id}/agent-links — 将 Agent 关联到项目
pub async fn create_project_agent_link(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateProjectAgentLinkRequest>,
) -> Result<Json<ProjectAgentLinkResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let agent_id = parse_agent_id(&req.agent_id)?;
    let agent = state
        .repos
        .agent_repo
        .get_by_id(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Agent {agent_id} 不存在")))?;

    let existing = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if existing.is_some() {
        return Err(ApiError::Conflict("该 Agent 已关联到此项目".into()));
    }

    let lifecycle_key =
        resolve_lifecycle_key_for_link(&state, req.default_lifecycle_key, req.default_workflow_key)
            .await?;

    let mut link = ProjectAgentLink::new(project_id, agent_id);
    link.config_override = req.config_override;
    link.default_lifecycle_key = lifecycle_key;
    link.is_default_for_story = req.is_default_for_story;
    link.is_default_for_task = req.is_default_for_task;

    state
        .repos
        .agent_link_repo
        .create(&link)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(build_link_response(&agent, &link)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectAgentLinkRequest {
    #[serde(default)]
    pub config_override: Option<serde_json::Value>,
    #[serde(default)]
    pub default_lifecycle_key: Option<String>,
    #[serde(default)]
    pub default_workflow_key: Option<String>,
    #[serde(default)]
    pub is_default_for_story: Option<bool>,
    #[serde(default)]
    pub is_default_for_task: Option<bool>,
}

/// PUT /projects/{id}/agent-links/{agent_id} — 更新项目-Agent 关联
pub async fn update_project_agent_link(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_id)): Path<(String, String)>,
    Json(req): Json<UpdateProjectAgentLinkRequest>,
) -> Result<Json<ProjectAgentLinkResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let agent_id = parse_agent_id(&agent_id)?;

    let agent = state
        .repos
        .agent_repo
        .get_by_id(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Agent {agent_id} 不存在")))?;

    let mut link = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("该 Agent 未关联到此项目".into()))?;

    if req.config_override.is_some() {
        link.config_override = req.config_override;
    }
    if req.default_lifecycle_key.is_some() || req.default_workflow_key.is_some() {
        link.default_lifecycle_key = resolve_lifecycle_key_for_link(
            &state,
            req.default_lifecycle_key,
            req.default_workflow_key,
        )
        .await?;
    }
    if let Some(v) = req.is_default_for_story {
        link.is_default_for_story = v;
    }
    if let Some(v) = req.is_default_for_task {
        link.is_default_for_task = v;
    }
    link.updated_at = chrono::Utc::now();

    state
        .repos
        .agent_link_repo
        .update(&link)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(build_link_response(&agent, &link)))
}

/// DELETE /projects/{id}/agent-links/{agent_id} — 解除项目-Agent 关联
pub async fn delete_project_agent_link(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let agent_id = parse_agent_id(&agent_id)?;

    state
        .repos
        .agent_link_repo
        .delete_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// 统一处理 lifecycle_key / workflow_key 的解析
///
/// 如果用户指定了 `default_workflow_key`（单个 workflow），
/// 自动创建一个单步 lifecycle 包装它。
async fn resolve_lifecycle_key_for_link(
    state: &Arc<AppState>,
    lifecycle_key: Option<String>,
    workflow_key: Option<String>,
) -> Result<Option<String>, ApiError> {
    if let Some(lk) = lifecycle_key {
        let trimmed = lk.trim().to_string();
        return Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        });
    }

    if let Some(wk) = workflow_key {
        let wk = wk.trim().to_string();
        if wk.is_empty() {
            return Ok(None);
        }

        let _wf = state
            .repos
            .workflow_definition_repo
            .get_by_key(&wk)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Workflow `{wk}` 不存在")))?;

        let auto_key = format!("auto:{wk}");

        let existing = state
            .repos
            .lifecycle_definition_repo
            .get_by_key(&auto_key)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        if existing.is_none() {
            use agentdash_domain::workflow::{
                LifecycleDefinition, LifecycleStepDefinition, WorkflowBindingKind,
                WorkflowDefinitionSource, WorkflowDefinitionStatus,
            };
            let lifecycle = LifecycleDefinition {
                id: Uuid::new_v4(),
                key: auto_key.clone(),
                name: format!("Auto: {wk}"),
                description: format!("自动创建：包装单个 workflow `{wk}`"),
                binding_kind: WorkflowBindingKind::Project,
                recommended_binding_roles: vec![],
                source: WorkflowDefinitionSource::UserAuthored,
                status: WorkflowDefinitionStatus::Active,
                version: 1,
                steps: vec![LifecycleStepDefinition {
                    key: "main".to_string(),
                    description: String::new(),
                    workflow_key: Some(wk),
                }],
                entry_step_key: "main".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            state
                .repos
                .lifecycle_definition_repo
                .create(&lifecycle)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }

        return Ok(Some(auto_key));
    }

    Ok(None)
}

/// 从 Agent + Link 构建 ProjectAgentBridge（新模型）
pub(crate) fn build_agent_bridge(agent: &Agent, link: &ProjectAgentLink) -> ProjectAgentBridge {
    let merged_config = link.merged_config(&agent.base_config);
    let executor_config = executor_config_from_agent_config(&agent.agent_type, &merged_config);

    let display_name = merged_config
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&agent.name)
        .to_string();

    let description = merged_config
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Agent `{}`，执行器 {}。", agent.name, agent.agent_type));

    let (preset_mcp_servers, preset_stdio_mcp_decls) = parse_preset_mcp_servers(&merged_config);

    ProjectAgentBridge {
        key: agent.id.to_string(),
        display_name,
        description,
        executor_config,
        preset_name: Some(agent.name.clone()),
        source: format!("agents[{}]", agent.id),
        preset_mcp_servers,
        preset_stdio_mcp_decls,
    }
}

fn executor_config_from_agent_config(agent_type: &str, config: &serde_json::Value) -> AgentConfig {
    let mut ec = AgentConfig::new(agent_type.to_string());
    if let Some(v) = config.get("variant").and_then(|v| v.as_str()) {
        ec.variant = Some(v.to_string());
    }
    if let Some(v) = config.get("provider_id").and_then(|v| v.as_str()) {
        ec.provider_id = Some(v.to_string());
    }
    if let Some(v) = config.get("model_id").and_then(|v| v.as_str()) {
        ec.model_id = Some(v.to_string());
    }
    if let Some(v) = config.get("agent_id").and_then(|v| v.as_str()) {
        ec.agent_id = Some(v.to_string());
    }
    if let Some(v) = config.get("permission_policy").and_then(|v| v.as_str()) {
        ec.permission_policy = Some(v.to_string());
    }
    ec
}

/// 从新模型的 agent link 或旧模型的 AgentPreset 查找默认 lifecycle key
async fn resolve_agent_default_lifecycle(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_key: &str,
) -> Option<String> {
    // 尝试按 UUID 解析 — 如果 agent_key 是 UUID 则查 agent_link
    if let Ok(agent_id) = Uuid::parse_str(agent_key)
        && let Ok(Some(link)) = state
            .repos
            .agent_link_repo
            .find_by_project_and_agent(project_id, agent_id)
            .await
    {
        return link.default_lifecycle_key;
    }

    // 旧模型 preset 不支持 lifecycle 绑定
    None
}

/// 自动启动 lifecycle run（首步含 workflow_key 时同时激活首步）
async fn auto_start_lifecycle_run(
    state: &Arc<AppState>,
    project_id: Uuid,
    lifecycle_key: &str,
) -> Result<(), String> {
    use agentdash_application::workflow::{LifecycleRunService, StartLifecycleRunCommand};
    use agentdash_domain::workflow::WorkflowBindingKind;

    let service = LifecycleRunService::new(
        state.repos.workflow_definition_repo.as_ref(),
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );

    let cmd = StartLifecycleRunCommand {
        project_id,
        lifecycle_id: None,
        lifecycle_key: Some(lifecycle_key.to_string()),
        binding_kind: WorkflowBindingKind::Project,
        binding_id: project_id,
    };

    let run = service
        .start_run(cmd)
        .await
        .map_err(|e| format!("start_run 失败: {e}"))?;

    // 自动激活首步
    if let Some(step_key) = run.current_step_key.as_deref() {
        use agentdash_application::workflow::ActivateLifecycleStepCommand;
        let activate_cmd = ActivateLifecycleStepCommand {
            run_id: run.id,
            step_key: step_key.to_string(),
        };
        if let Err(e) = service.activate_step(activate_cmd).await {
            tracing::warn!(run_id = %run.id, step_key = %step_key, error = %e, "自动激活首步失败");
        }
    }

    Ok(())
}
