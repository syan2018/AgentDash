use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::address_space::SessionMountTarget;
use agentdash_application::canvas::append_visible_canvas_mounts;
use agentdash_application::session::SessionExecutionState;
use agentdash_application::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use agentdash_application::session::context::{SessionContextSnapshot, SharedContextMount};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::project_agents::{
        build_project_agent_visible_mounts, parse_project_agent_session_label,
        resolve_project_agent_bridge_async, resolve_project_workspace,
    },
    rpc::ApiError,
    runtime_bridge::acp_mcp_servers_to_runtime,
};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_mcp::injection::McpInjectionConfig;
#[derive(Debug, Serialize)]
pub struct ProjectSessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_spi::AddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug)]
pub(crate) struct BuiltProjectSessionContextResponse {
    pub(crate) address_space: Option<agentdash_spi::AddressSpace>,
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

    Ok(Json(ProjectSessionDetailResponse {
        binding_id,
        session_id: binding.session_id,
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        address_space: built_context.address_space.clone(),
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
    let use_address_space = connector_config
        .as_ref()
        .is_some_and(|c| c.is_cloud_native());
    let address_space = if use_address_space {
        let mut address_space = state
            .services
            .address_space_service
            .build_address_space(
                project,
                None,
                workspace.as_ref(),
                SessionMountTarget::Project,
                resolved_config.as_ref().map(|c| c.executor.as_str()),
            )
            .map_err(ApiError::BadRequest)?;
        append_visible_canvas_mounts(
            state.repos.canvas_repo.as_ref(),
            project.id,
            &mut address_space,
            &session_meta.visible_canvas_mount_ids,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        Some(address_space)
    } else {
        None
    };
    let mut effective_mcp_servers = state
        .config
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            vec![McpInjectionConfig::for_relay(base_url.clone(), project.id).to_acp_mcp_server()]
        })
        .unwrap_or_default();
    effective_mcp_servers.extend(project_agent.preset_mcp_servers.iter().cloned());

    let executor_source = if session_meta.executor_config.is_some() {
        "session.meta.executor_config".to_string()
    } else {
        project_agent.source.clone()
    };

    let shared_mounts: Vec<SharedContextMount> = resolved_config
        .as_ref()
        .map(|config| {
            build_project_agent_visible_mounts(project, config)
                .into_iter()
                .map(|m| SharedContextMount {
                    container_id: m.container_id,
                    mount_id: m.mount_id,
                    display_name: m.display_name,
                    writable: m.writable,
                })
                .collect()
        })
        .unwrap_or_default();
    let runtime_address_space = address_space.clone();

    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project: project.clone(),
        story: None,
        workspace,
        resolved_config,
        address_space: runtime_address_space,
        mcp_servers: acp_mcp_servers_to_runtime(&effective_mcp_servers),
        working_dir: None,
        executor_preset_name: project_agent.preset_name,
        executor_source,
        executor_resolution_error: None,
        owner_variant: BootstrapOwnerVariant::Project {
            agent_key: project_agent.key,
            agent_display_name: project_agent.display_name,
            shared_context_mounts: shared_mounts,
        },
        workflow: None,
    });

    let snapshot = derive_session_context_snapshot(&plan);

    Ok(BuiltProjectSessionContextResponse {
        address_space: plan.address_space.clone(),
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

    let project = project_result?;

    let project_bindings = bindings_result.map_err(|e| ApiError::Internal(e.to_string()))?;

    if project_bindings.is_empty() {
        return Ok(Json(vec![]));
    }

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

            let (agent_key, agent_display_name) = resolve_agent_info(&pb.binding, &project, meta);

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

/// 从 binding label 或 session meta 推断 agent_key 和 display_name
fn resolve_agent_info(
    binding: &SessionBinding,
    _project: &agentdash_domain::project::Project,
    meta: &agentdash_application::session::SessionMeta,
) -> (Option<String>, Option<String>) {
    if binding.owner_type == agentdash_domain::session_binding::SessionOwnerType::Project {
        if let Some(agent_key) = parse_project_agent_session_label(&binding.label) {
            let display_name = Some(meta.title.clone());
            return (Some(agent_key.to_string()), display_name);
        }
        // companion 会话: 优先用 companion_context.agent_name
        if let Some(ctx) = &meta.companion_context {
            if let Some(name) = &ctx.agent_name {
                return (Some(name.clone()), Some(meta.title.clone()));
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
