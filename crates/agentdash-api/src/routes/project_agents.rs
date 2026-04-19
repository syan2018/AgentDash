use std::sync::Arc;

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use agentdash_domain::{
    agent::{Agent, ProjectAgentLink},
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
    /// MCP servers parsed from preset config — injected into ExecutionContext for project-agent sessions
    pub preset_mcp_servers: Vec<McpServer>,
    /// 配置中显式标记为 relay 的 MCP server name 集合
    pub relay_mcp_server_names: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentExecutorResponse {
    pub executor: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<agentdash_spi::ThinkingLevel>,
    pub permission_policy: Option<String>,
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
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-5.4".to_string()),
                agent_id: None,
                thinking_level: None,
                permission_policy: Some("AUTO".to_string()),
            },
            preset_name: None,
            source: "project.config.default_agent_type".to_string(),
            session: Some(ProjectAgentSessionResponse {
                binding_id: "binding-1".to_string(),
                session_id: "sess-1".to_string(),
                session_title: Some("title".to_string()),
                last_activity: Some(1),
            }),
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("presetName").is_none());
    }

    #[test]
    fn parse_project_agent_session_label_requires_expected_prefix() {
        assert_eq!(
            parse_project_agent_session_label("project_agent:agent-1"),
            Some("agent-1")
        );
        assert_eq!(parse_project_agent_session_label("agent-1"), None);
        assert_eq!(parse_project_agent_session_label("project_agent:   "), None);
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
        let agent = state
            .repos
            .agent_repo
            .get_by_id(link.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| {
                ApiError::Internal(format!(
                    "Project Agent Link `{}` 指向不存在的 Agent `{}`",
                    link.id, link.agent_id
                ))
            })?;
        let bridge = build_agent_bridge(&agent, link)?;
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
    let agent = build_agent_bridge(&agent_entity, &link)?;

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
            let session_id = binding.session_id.clone();
            let binding_id = binding.id.to_string();
            let session = Some(ProjectAgentSessionResponse {
                binding_id: binding_id.clone(),
                session_id: session_id.clone(),
                session_title: Some(meta.title),
                last_activity: Some(meta.updated_at),
            });
            let summary = build_project_agent_summary(&project, &agent, session);
            return Ok(Json(OpenProjectAgentSessionResponse {
                created: false,
                session_id,
                binding_id,
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

    let meta = state
        .services
        .session_hub
        .create_session("")
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
    state
        .services
        .session_hub
        .mark_owner_bootstrap_pending(&meta.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    // 自动启动 Lifecycle Run（如果 Agent Link 配置了 default_lifecycle_key）
    if let Some(lifecycle_key) = resolve_agent_default_lifecycle(&state, project.id, agent_id).await
        && let Err(err) =
            auto_start_lifecycle_run(&state, project.id, &meta.id, &lifecycle_key).await
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
) -> Result<Option<ProjectAgentBridge>, ApiError> {
    let agent_id = match Uuid::parse_str(agent_key) {
        Ok(agent_id) => agent_id,
        Err(_) => return Ok(None),
    };
    let agent = state
        .repos
        .agent_repo
        .get_by_id(agent_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let Some(agent) = agent else {
        return Ok(None);
    };
    let link = state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let Some(link) = link else {
        return Ok(None);
    };
    Ok(Some(build_agent_bridge(&agent, &link)?))
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

pub(crate) const PROJECT_AGENT_SESSION_LABEL_PREFIX: &str = "project_agent:";

pub(crate) fn project_agent_session_label(agent_key: &str) -> String {
    format!("{PROJECT_AGENT_SESSION_LABEL_PREFIX}{}", agent_key.trim())
}

pub(crate) fn parse_project_agent_session_label(label: &str) -> Option<&str> {
    let agent_key = label
        .trim()
        .strip_prefix(PROJECT_AGENT_SESSION_LABEL_PREFIX)?;
    if agent_key.trim().is_empty() {
        return None;
    }
    Some(agent_key)
}

fn build_project_agent_summary(
    _project: &Project,
    agent: &ProjectAgentBridge,
    session: Option<ProjectAgentSessionResponse>,
) -> ProjectAgentSummaryResponse {
    ProjectAgentSummaryResponse {
        key: agent.key.clone(),
        display_name: agent.display_name.clone(),
        description: agent.description.clone(),
        executor: ProjectAgentExecutorResponse {
            executor: agent.executor_config.executor.clone(),
            provider_id: agent.executor_config.provider_id.clone(),
            model_id: agent.executor_config.model_id.clone(),
            agent_id: agent.executor_config.agent_id.clone(),
            thinking_level: agent.executor_config.thinking_level,
            permission_policy: agent.executor_config.permission_policy.clone(),
        },
        preset_name: agent.preset_name.clone(),
        source: agent.source.clone(),
        session,
    }
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
fn parse_preset_mcp_servers(
    config: &serde_json::Value,
) -> Result<(Vec<McpServer>, std::collections::HashSet<String>), String> {
    let raw_list = match config.get("mcp_servers").and_then(|v| v.as_array()) {
        Some(list) => list,
        None => return Ok((vec![], Default::default())),
    };

    let mut mcp_servers = Vec::new();
    let mut relay_names = std::collections::HashSet::new();

    for (index, entry) in raw_list.iter().enumerate() {
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("mcp_servers[{index}] 必须是对象"))?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("mcp_servers[{index}].name 缺失或为空"))?
            .to_string();

        let effective_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("mcp_servers[{index}].type 缺失或不是字符串"))?;

        // relay 标记：stdio 默认 true，http/sse 默认 false，显式 relay 字段可覆盖
        let is_relay = obj
            .get("relay")
            .and_then(|v| v.as_bool())
            .unwrap_or(effective_type == "stdio");
        if is_relay {
            relay_names.insert(name.clone());
        }

        match effective_type {
            "http" | "sse" => {
                let url = obj
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("mcp_servers[{index}].url 缺失或为空"))?
                    .to_string();
                let headers = match obj.get("headers") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].headers 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(header_index, header)| {
                            let header_obj = header.as_object().ok_or_else(|| {
                                format!(
                                    "mcp_servers[{index}].headers[{header_index}] 必须是对象"
                                )
                            })?;
                            let header_name = header_obj
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].headers[{header_index}].name 缺失或为空"
                                    )
                                })?;
                            let header_value = header_obj
                                .get("value")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].headers[{header_index}].value 缺失或不是字符串"
                                    )
                                })?;
                            Ok::<HttpHeader, String>(HttpHeader::new(
                                header_name.to_string(),
                                header_value.to_string(),
                            ))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };

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
                let command = obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("mcp_servers[{index}].command 缺失或为空"))?
                    .to_string();
                let args = match obj.get("args") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].args 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(arg_index, arg)| {
                            arg.as_str().map(String::from).ok_or_else(|| {
                                format!("mcp_servers[{index}].args[{arg_index}] 必须是字符串")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };
                let env = match obj.get("env") {
                    Some(value) => value
                        .as_array()
                        .ok_or_else(|| format!("mcp_servers[{index}].env 必须是数组"))?
                        .iter()
                        .enumerate()
                        .map(|(env_index, entry)| {
                            let env_obj = entry.as_object().ok_or_else(|| {
                                format!("mcp_servers[{index}].env[{env_index}] 必须是对象")
                            })?;
                            let env_name = env_obj
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].env[{env_index}].name 缺失或为空"
                                    )
                                })?;
                            let env_value = env_obj
                                .get("value")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    format!(
                                        "mcp_servers[{index}].env[{env_index}].value 缺失或不是字符串"
                                    )
                                })?;
                            Ok::<EnvVariable, String>(EnvVariable::new(
                                env_name.to_string(),
                                env_value.to_string(),
                            ))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => Vec::new(),
                };
                mcp_servers.push(McpServer::Stdio(
                    McpServerStdio::new(name, command).args(args).env(env),
                ));
            }
            other => {
                return Err(format!("mcp_servers[{index}].type 非法，不支持 `{other}`"));
            }
        }
    }

    Ok((mcp_servers, relay_names))
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
    pub knowledge_enabled: bool,
    pub project_container_ids: Vec<String>,
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
        knowledge_enabled: link.knowledge_enabled,
        project_container_ids: link.project_container_ids.clone(),
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
        let agent = state
            .repos
            .agent_repo
            .get_by_id(link.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| {
                ApiError::Internal(format!(
                    "Project Agent Link `{}` 指向不存在的 Agent `{}`",
                    link.id, link.agent_id
                ))
            })?;
        response.push(build_link_response(&agent, link));
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
    #[serde(default)]
    pub knowledge_enabled: Option<bool>,
    #[serde(default)]
    pub project_container_ids: Option<Vec<String>>,
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
    if let Some(v) = req.knowledge_enabled {
        link.knowledge_enabled = v;
    }
    if let Some(ids) = req.project_container_ids {
        link.project_container_ids = ids;
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
                    node_type: Default::default(),
                    output_ports: vec![],
                    input_ports: vec![],
                }],
                edges: vec![],
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
pub(crate) fn build_agent_bridge(
    agent: &Agent,
    link: &ProjectAgentLink,
) -> Result<ProjectAgentBridge, ApiError> {
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

    let (preset_mcp_servers, relay_mcp_server_names) = parse_preset_mcp_servers(&merged_config)
        .map_err(|error| {
            ApiError::Internal(format!(
                "Agent `{}` 的 mcp_servers 配置非法: {error}",
                agent.id
            ))
        })?;

    Ok(ProjectAgentBridge {
        key: agent.id.to_string(),
        display_name,
        description,
        executor_config,
        preset_name: Some(agent.name.clone()),
        source: format!("agents[{}]", agent.id),
        preset_mcp_servers,
        relay_mcp_server_names,
    })
}

fn executor_config_from_agent_config(agent_type: &str, config: &serde_json::Value) -> AgentConfig {
    let mut ec = AgentConfig::new(agent_type.to_string());
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
    if let Some(v) = config
        .get("thinking_level")
        .and_then(|v| serde_json::from_value::<agentdash_spi::ThinkingLevel>(v.clone()).ok())
    {
        ec.thinking_level = Some(v);
    }
    if let Some(arr) = config.get("tool_clusters").and_then(|v| v.as_array()) {
        let clusters: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !clusters.is_empty() {
            ec.tool_clusters = Some(clusters);
        }
    }
    if let Some(v) = config.get("system_prompt").and_then(|v| v.as_str()) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            ec.system_prompt = Some(trimmed.to_string());
        }
    }
    if let Some(v) = config
        .get("system_prompt_mode")
        .and_then(|v| serde_json::from_value::<agentdash_spi::SystemPromptMode>(v.clone()).ok())
    {
        ec.system_prompt_mode = Some(v);
    }
    ec
}

async fn resolve_agent_default_lifecycle(
    state: &Arc<AppState>,
    project_id: Uuid,
    agent_id: Uuid,
) -> Option<String> {
    state
        .repos
        .agent_link_repo
        .find_by_project_and_agent(project_id, agent_id)
        .await
        .ok()
        .flatten()
        .and_then(|link| link.default_lifecycle_key)
}

/// 自动启动 lifecycle run（首步含 workflow_key 时同时激活首步）
async fn auto_start_lifecycle_run(
    state: &Arc<AppState>,
    project_id: Uuid,
    session_id: &str,
    lifecycle_key: &str,
) -> Result<(), String> {
    use agentdash_application::workflow::{LifecycleRunService, StartLifecycleRunCommand};

    let service = LifecycleRunService::new(
        state.repos.lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
    );

    let cmd = StartLifecycleRunCommand {
        project_id,
        lifecycle_id: None,
        lifecycle_key: Some(lifecycle_key.to_string()),
        session_id: session_id.to_string(),
    };

    let run = service
        .start_run(cmd)
        .await
        .map_err(|e| format!("start_run 失败: {e}"))?;

    // 自动激活首步
    if let Some(step_key) = run.current_step_key() {
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
