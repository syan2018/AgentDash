use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::vfs::{
    SessionMountTarget, append_agent_knowledge_mounts, filter_project_containers_by_whitelist,
};
use agentdash_application::canvas::append_visible_canvas_mounts;
use agentdash_application::session::SessionExecutionState;
use agentdash_application::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use agentdash_application::session::context::SessionContextSnapshot;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::vfs_surfaces::build_surface_summary,
    routes::project_agents::{
        parse_project_agent_session_label, resolve_project_agent_bridge_async,
        resolve_project_workspace,
    },
    rpc::ApiError,
    runtime_bridge::acp_mcp_servers_to_runtime,
};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};

/// 批量加载 project 级 MCP Preset 并展开为 resolver 消费的 map。
/// 查询失败降级为空 map，避免 session 创建被 Preset 读失败阻断。
async fn load_project_presets(
    state: &Arc<AppState>,
    project_id: Uuid,
) -> agentdash_application::capability::AvailableMcpPresets {
    match state.repos.mcp_preset_repo.list_by_project(project_id).await {
        Ok(presets) => presets
            .into_iter()
            .map(|p| (p.name, p.server_decl))
            .collect(),
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "project_sessions: 加载 MCP Preset 列表失败"
            );
            Default::default()
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ProjectSessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<agentdash_spi::Vfs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_surface: Option<agentdash_application::vfs::ResolvedVfsSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug)]
pub(crate) struct BuiltProjectSessionContextResponse {
    pub(crate) vfs: Option<agentdash_spi::Vfs>,
    pub(crate) context_snapshot: Option<SessionContextSnapshot>,
}

pub async fn get_project_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, binding_id)): Path<(String, String)>,
) -> Result<Json<ProjectSessionDetailResponse>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;
    let binding_uuid = Uuid::parse_str(&binding_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_uuid,
        ProjectPermission::View,
    )
    .await?;

    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Project, project_uuid)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let binding = bindings
        .into_iter()
        .find(|item| item.id == binding_uuid)
        .ok_or_else(|| {
            ApiError::NotFound(format!("Project Session binding {binding_id} 不存在"))
        })?;

    let meta = state
        .services
        .session_hub
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let built_context = build_project_session_context_response(
        &state,
        &project,
        &binding.session_id,
        &binding.label,
    )
    .await?;
    let response_session_id = binding.session_id.clone();

    Ok(Json(ProjectSessionDetailResponse {
        binding_id,
        session_id: response_session_id.clone(),
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        vfs: built_context.vfs.clone(),
        runtime_surface: if let Some(space) = built_context.vfs.as_ref() {
            Some(
                build_surface_summary(
                    &state,
                    &agentdash_application::vfs::ResolvedVfsSurfaceSource::SessionRuntime {
                        session_id: response_session_id,
                    },
                    space,
                )
                .await?,
            )
        } else {
            None
        },
        context_snapshot: built_context.context_snapshot,
    }))
}

pub(crate) async fn build_project_session_context_response(
    state: &Arc<AppState>,
    project: &agentdash_domain::project::Project,
    session_id: &str,
    binding_label: &str,
) -> Result<BuiltProjectSessionContextResponse, ApiError> {
    let agent_key = parse_project_agent_session_label(binding_label).ok_or_else(|| {
        ApiError::BadRequest(format!("无效的项目 Agent session label: {binding_label}"))
    })?;
    let project_agent = resolve_project_agent_bridge_async(state, project.id, agent_key)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let workspace = resolve_project_workspace(state, project).await?;
    let session_meta = state
        .services
        .session_hub
        .get_session_meta(session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Session `{session_id}` 不存在")))?;

    let connector_config = session_meta
        .executor_config
        .clone()
        .or_else(|| Some(project_agent.executor_config.clone()));
    let resolved_config = connector_config.clone();
    let use_vfs = connector_config
        .as_ref()
        .is_some_and(|c| c.is_cloud_native());
    // 加载 ProjectAgentLink 用于注入知识容器 mounts
    let agent_uuid = Uuid::parse_str(agent_key).ok();
    let agent_link = if let Some(aid) = agent_uuid {
        state
            .repos
            .agent_link_repo
            .find_by_project_and_agent(project.id, aid)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
    } else {
        None
    };

    let vfs = if use_vfs {
        let mut vfs = state
            .services
            .vfs_service
            .build_vfs(
                project,
                None,
                workspace.as_ref(),
                SessionMountTarget::Project,
                resolved_config.as_ref().map(|c| c.executor.as_str()),
            )
            .map_err(ApiError::BadRequest)?;

        // Agent 级容器管控：白名单过滤 + 知识库注入
        if let Some(link) = &agent_link {
            filter_project_containers_by_whitelist(&mut vfs, link);
            append_agent_knowledge_mounts(&mut vfs, link)
                .map_err(ApiError::Internal)?;
        }

        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            project.id,
            &mut vfs,
            &session_meta.visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        Some(vfs)
    } else {
        None
    };
    let agent_mcp_entries: Vec<agentdash_application::capability::AgentMcpServerEntry> =
        project_agent
            .preset_mcp_servers
            .iter()
            .filter_map(|server| {
                let name = match server {
                    agent_client_protocol::McpServer::Http(http) => http.name.clone(),
                    agent_client_protocol::McpServer::Sse(sse) => sse.name.clone(),
                    agent_client_protocol::McpServer::Stdio(stdio) => stdio.name.clone(),
                    _ => return None,
                };
                Some(agentdash_application::capability::AgentMcpServerEntry {
                    name,
                    server: server.clone(),
                })
            })
            .collect();

    // ── 解析 agent_link 绑定的 lifecycle 上下文（与实际 session 创建保持一致） ──
    let workflow_ctx = if let Some(link) = agent_link.as_ref() {
        agentdash_application::capability::resolve_session_workflow_context(
            agentdash_application::capability::SessionWorkflowRepos {
                agent_link: state.repos.agent_link_repo.as_ref(),
                lifecycle_def: state.repos.lifecycle_definition_repo.as_ref(),
                workflow_def: state.repos.workflow_definition_repo.as_ref(),
            },
            agentdash_application::capability::SessionWorkflowOwner::Project {
                project_id: project.id,
                agent_id: link.agent_id,
            },
        )
        .await
    } else {
        agentdash_application::capability::SessionWorkflowContext::NONE
    };

    // ── CapabilityResolver 统一计算平台 MCP（与实际 session 注入保持一致） ──
    let cap_output = agentdash_application::capability::CapabilityResolver::resolve(
        &agentdash_application::capability::CapabilityResolverInput {
            owner_ctx: agentdash_domain::session_binding::SessionOwnerCtx::Project {
                project_id: project.id,
            },
            agent_declared_capabilities: resolved_config
                .as_ref()
                .and_then(|config| config.tool_clusters.clone()),
            workflow_ctx,
            agent_mcp_servers: agent_mcp_entries,
            available_presets: load_project_presets(state, project.id).await,
            companion_slice_mode: None,
        },
        &state.config.platform_config,
    );
    let mut effective_mcp_servers: Vec<agent_client_protocol::McpServer> = cap_output
        .platform_mcp_configs
        .iter()
        .map(|c| c.to_acp_mcp_server())
        .collect();
    effective_mcp_servers.extend(cap_output.custom_mcp_servers.iter().cloned());
    effective_mcp_servers.extend(project_agent.preset_mcp_servers.iter().cloned());

    let executor_source = if session_meta.executor_config.is_some() {
        "session.meta.executor_config".to_string()
    } else {
        project_agent.source.clone()
    };

    let runtime_vfs = vfs.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project: project.clone(),
        story: None,
        workspace,
        resolved_config,
        vfs: runtime_vfs,
        mcp_servers: acp_mcp_servers_to_runtime(&effective_mcp_servers),
        working_dir: None,
        executor_preset_name: project_agent.preset_name,
        executor_resolution:
            agentdash_application::session::ExecutorResolution::resolved(executor_source),
        owner_variant: BootstrapOwnerVariant::Project {
            agent_key: project_agent.key,
            agent_display_name: project_agent.display_name,
        },
        workflow: None,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Ok(BuiltProjectSessionContextResponse {
        vfs: plan.vfs.clone(),
        context_snapshot: Some(snapshot),
    })
}

// ─── Project Sessions 聚合 API ────────────────────────────────────────────────

/// 项目级 Session 聚合条目
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectSessionEntry {
    pub session_id: String,
    pub session_title: Option<String>,
    /// Unix 时间戳（毫秒），最后活跃时间
    pub last_activity: Option<i64>,

    /// 执行状态: "idle" | "running" | "completed" | "failed" | "interrupted"
    pub execution_status: String,

    /// 归属层级: "project" | "story" | "task"
    pub owner_type: String,
    pub owner_id: String,
    /// owner 实体标题（task: task.title / story: story.title）
    pub owner_title: Option<String>,
    /// 当 owner_type = "task" 时有值，表示所属 Story ID
    pub story_id: Option<String>,
    /// 直接内联的 Story 名（当 owner_type = "task" 时有值）
    pub story_title: Option<String>,

    /// Agent key（project 级从 label 解析；story/task 级从 executor_config 推断）
    pub agent_key: Option<String>,
    /// Agent 显示名称（project 级有值；story/task 级暂为 null）
    pub agent_display_name: Option<String>,

    /// 非 null 表示这是 Companion 子会话
    pub parent_session_id: Option<String>,
}

/// GET /api/projects/{project_id}/sessions 查询参数
#[derive(Debug, Deserialize)]
pub struct ListProjectSessionsQuery {
    /// 逗号分隔状态过滤，如 "running,idle"；不传时返回全部
    pub status: Option<String>,
    /// 最大返回条数（默认 50，上限 500）
    pub limit: Option<i64>,
}

/// GET /api/projects/{project_id}/sessions
///
/// 重构后的实现：
///   1. 一次 SQL UNION 查出项目下所有层级的 bindings + 归属上下文（无 N+1）
///   2. 批量并发读取 session meta（并发 IO）
///   3. 单次内存 lock 批量读执行状态（无 JSONL 扫描）
///   - 复杂度从 O(N×M) 降为 O(1 DB + N parallel IO + 1 lock)
pub async fn list_project_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<ListProjectSessionsQuery>,
) -> Result<Json<Vec<ProjectSessionEntry>>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;

    // ── Step 1: 并发拉取项目信息 + 所有 bindings（一次 SQL UNION）───────────
    let (project_result, bindings_result) = tokio::join!(
        load_project_with_permission(
            state.as_ref(),
            &current_user,
            project_uuid,
            ProjectPermission::View
        ),
        state
            .repos
            .session_binding_repo
            .list_by_project(project_uuid),
    );

    let _project = project_result?;

    let project_bindings = bindings_result.map_err(|e| ApiError::Internal(e.to_string()))?;

    if project_bindings.is_empty() {
        return Ok(Json(vec![]));
    }

    // ── Step 1.5: 构建 agent_id → display_name 映射 ─────────────────────────
    let agent_display_map = {
        let links = state
            .repos
            .agent_link_repo
            .list_by_project(project_uuid)
            .await
            .unwrap_or_default();
        let mut map = HashMap::new();
        for link in &links {
            if let Ok(Some(agent)) = state.repos.agent_repo.get_by_id(link.agent_id).await {
                let merged = link.merged_config(&agent.base_config);
                let name = merged
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .unwrap_or(&agent.name)
                    .to_string();
                map.insert(agent.id.to_string(), name);
            }
        }
        map
    };

    // ── Step 2: 批量并发读取所有 session meta ────────────────────────────────
    let session_ids: Vec<String> = project_bindings
        .iter()
        .map(|pb| pb.binding.session_id.clone())
        .collect();

    let meta_map = state
        .services
        .session_hub
        .get_session_metas_bulk(&session_ids)
        .await
        .map_err(|e| ApiError::Internal(format!("批量读取 session meta 失败: {e}")))?;

    // ── Step 3: 单次 lock 批量读执行状态（内存，不扫 JSONL）─────────────────
    let status_map = state
        .services
        .session_hub
        .inspect_execution_states_bulk(&session_ids)
        .await
        .map_err(|e| ApiError::Internal(format!("批量读取 session 执行状态失败: {e}")))?;

    // ── Step 4: 组装结果 ─────────────────────────────────────────────────────
    let limit = query.limit.unwrap_or(50).clamp(1, 500) as usize;
    let status_filter: Option<Vec<String>> = query.status.as_deref().map(|s| {
        s.split(',')
            .map(|part| part.trim().to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect()
    });

    let mut entries: Vec<ProjectSessionEntry> = project_bindings
        .into_iter()
        .filter_map(|pb| {
            // meta 不存在 → session 已被删除，跳过
            let meta = meta_map.get(&pb.binding.session_id)?;

            let execution_status = execution_state_to_str(status_map.get(&pb.binding.session_id));

            // 状态过滤
            if let Some(filter) = &status_filter
                && !filter.contains(&execution_status.to_string())
            {
                return None;
            }

            let parent_session_id = meta
                .companion_context
                .as_ref()
                .map(|c| c.parent_session_id.clone());

            let (agent_key, agent_display_name) =
                resolve_agent_info(&pb.binding, meta, &agent_display_map);

            let story_id = pb.story_id.map(|id| id.to_string());

            Some(ProjectSessionEntry {
                session_id: pb.binding.session_id.clone(),
                session_title: Some(meta.title.clone()),
                last_activity: Some(meta.updated_at),
                execution_status: execution_status.to_string(),
                owner_type: pb.binding.owner_type.to_string(),
                owner_id: pb.binding.owner_id.to_string(),
                owner_title: pb.owner_title,
                story_id,
                story_title: pb.story_title,
                agent_key,
                agent_display_name,
                parent_session_id,
            })
        })
        .collect();

    // 按 last_activity 倒序，null 排最后
    entries.sort_by(|a, b| match (b.last_activity, a.last_activity) {
        (Some(bt), Some(at)) => bt.cmp(&at),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    entries.truncate(limit);

    Ok(Json(entries))
}

fn execution_state_to_str(state: Option<&SessionExecutionState>) -> &'static str {
    match state {
        Some(SessionExecutionState::Running { .. }) => "running",
        Some(SessionExecutionState::Completed { .. }) => "completed",
        Some(SessionExecutionState::Failed { .. }) => "failed",
        Some(SessionExecutionState::Interrupted { .. }) => "interrupted",
        _ => "idle",
    }
}

/// 从 binding label 或 agent 配置推断 agent_key 和 display_name。
/// `agent_display_map` 由 list_project_sessions 预加载，key 为 agent UUID 字符串。
fn resolve_agent_info(
    binding: &SessionBinding,
    meta: &agentdash_application::session::SessionMeta,
    agent_display_map: &HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    if binding.owner_type == agentdash_domain::session_binding::SessionOwnerType::Project {
        if let Some(agent_key) = parse_project_agent_session_label(&binding.label) {
            let display_name = agent_display_map.get(agent_key).cloned();
            return (Some(agent_key.to_string()), display_name);
        }
        // companion 会话: 用 companion_context.agent_name（真实 agent 名）
        if let Some(ctx) = &meta.companion_context {
            if let Some(name) = &ctx.agent_name {
                return (Some(name.clone()), Some(name.clone()));
            }
        }
    }

    let agent_key = meta.executor_config.as_ref().map(|c| c.executor.clone());
    (agent_key, None)
}

#[cfg(test)]
mod list_project_sessions_tests {
    use super::*;

    #[test]
    fn project_session_entry_serializes_as_snake_case() {
        let value = serde_json::to_value(ProjectSessionEntry {
            session_id: "sess-1".to_string(),
            session_title: Some("Test".to_string()),
            last_activity: Some(1711234567890),
            execution_status: "idle".to_string(),
            owner_type: "task".to_string(),
            owner_id: "task-1".to_string(),
            owner_title: Some("Fix bug".to_string()),
            story_id: Some("story-1".to_string()),
            story_title: Some("My Story".to_string()),
            agent_key: Some("claude-code".to_string()),
            agent_display_name: None,
            parent_session_id: None,
        })
        .expect("serialize ProjectSessionEntry");

        assert!(value.get("session_id").is_some());
        assert!(value.get("execution_status").is_some());
        assert!(value.get("owner_type").is_some());
        assert!(value.get("story_id").is_some());
        assert!(value.get("agent_key").is_some());
        assert!(value.get("parent_session_id").is_some());
        // 确保没有 camelCase 字段
        assert!(value.get("sessionId").is_none());
        assert!(value.get("executionStatus").is_none());
        assert!(value.get("ownerType").is_none());
        assert!(value.get("storyId").is_none());
        assert!(value.get("agentKey").is_none());
        assert!(value.get("parentSessionId").is_none());
    }
}
