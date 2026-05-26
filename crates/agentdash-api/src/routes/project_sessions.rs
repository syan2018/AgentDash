use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_application::session::SessionExecutionState;
use agentdash_application::session::construction_planner::SessionConstructionPlanner;
use agentdash_application::session::context::SessionContextSnapshot;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
    session_use_cases::context_query::build_session_context_plan,
};
use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
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

pub async fn get_project_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, binding_id)): Path<(String, String)>,
) -> Result<Json<ProjectSessionDetailResponse>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;
    let binding_uuid = Uuid::parse_str(&binding_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    let _project = load_project_with_permission(
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
        .session_core
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let context_bindings = state
        .repos
        .session_binding_repo
        .list_by_session(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let context_projection = build_session_context_plan(
        &state,
        &current_user,
        &binding.session_id,
        &context_bindings,
    )
    .await?
    .map(|plan| plan.context_projection);
    let response_session_id = binding.session_id.clone();

    Ok(Json(ProjectSessionDetailResponse {
        binding_id,
        session_id: response_session_id.clone(),
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        vfs: context_projection
            .as_ref()
            .and_then(|projection| projection.vfs.clone()),
        runtime_surface: context_projection
            .as_ref()
            .and_then(|projection| projection.runtime_surface.clone()),
        context_snapshot: context_projection.and_then(|projection| projection.context_snapshot),
    }))
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
    /// 父子关系类型：fork / companion / spawned_agent / rollback_branch
    pub parent_relation_kind: Option<String>,
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

    // ── Step 1.5: 构建 project_agent_id → display_name 映射 ─────────────────
    let agent_display_map = {
        let agents = state
            .repos
            .project_agent_repo
            .list_by_project(project_uuid)
            .await
            .unwrap_or_default();
        let mut map = HashMap::new();
        for agent in &agents {
            let preset = agent
                .preset_config()
                .map_err(|error| ApiError::BadRequest(error.to_string()))?;
            let name = preset
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or(&agent.name)
                .to_string();
            map.insert(agent.id.to_string(), name.clone());
            map.insert(agent.name.clone(), name);
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
        .session_core
        .get_session_metas_bulk(&session_ids)
        .await
        .map_err(|e| ApiError::Internal(format!("批量读取 session meta 失败: {e}")))?;

    // ── Step 3: 单次 lock 批量读执行状态（内存，不扫 JSONL）─────────────────
    let status_map = state
        .services
        .session_core
        .inspect_execution_states_bulk(&session_ids)
        .await
        .map_err(|e| ApiError::Internal(format!("批量读取 session 执行状态失败: {e}")))?;
    let lineage_pairs = futures::future::join_all(session_ids.iter().map(|session_id| {
        let session_id = session_id.clone();
        let branching = state.services.session_branching.clone();
        async move {
            branching
                .lineage_parent(&session_id)
                .await
                .map(|lineage| (session_id, lineage))
        }
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| ApiError::Internal(format!("批量读取 session lineage 失败: {e}")))?;
    let lineage_map: HashMap<String, Option<agentdash_application::session::SessionLineageRecord>> =
        lineage_pairs.into_iter().collect();

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

            let lineage = lineage_map
                .get(&pb.binding.session_id)
                .and_then(|record| record.as_ref());
            let companion_parent_session_id = meta
                .companion_context
                .as_ref()
                .map(|c| c.parent_session_id.clone());
            let (parent_session_id, parent_relation_kind) = if let Some(lineage) = lineage {
                (
                    Some(lineage.parent_session_id.clone()),
                    Some(lineage.relation_kind.as_str().to_string()),
                )
            } else if let Some(parent_session_id) = companion_parent_session_id {
                (Some(parent_session_id), Some("companion".to_string()))
            } else {
                (None, None)
            };

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
                parent_relation_kind,
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
    if let Some(ctx) = &meta.companion_context {
        let label = normalized_agent_label(ctx.agent_name.as_deref())
            .or_else(|| normalized_agent_label(Some(&ctx.companion_label)));
        let display_name = label
            .as_deref()
            .and_then(|value| agent_display_map.get(value).cloned())
            .or_else(|| label.clone());
        let key = label.clone().or_else(|| {
            meta.executor_config
                .as_ref()
                .map(|config| config.executor.clone())
        });
        return (key, display_name);
    }

    if binding.owner_type == agentdash_domain::session_binding::SessionOwnerType::Project {
        if let Some(agent_key) =
            SessionConstructionPlanner::parse_project_agent_session_label(&binding.label)
        {
            let display_name = agent_display_map.get(agent_key).cloned();
            return (Some(agent_key.to_string()), display_name);
        }
    }

    let agent_key = meta.executor_config.as_ref().map(|c| c.executor.clone());
    (agent_key, None)
}

fn normalized_agent_label(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod list_project_sessions_tests {
    use super::*;
    use agentdash_application::session::{
        CompanionSessionContext, ExecutionStatus, SessionBootstrapState, SessionMeta, TitleSource,
    };
    use agentdash_spi::AgentConfig;

    fn test_binding(owner_type: SessionOwnerType) -> SessionBinding {
        SessionBinding::new(
            Uuid::new_v4(),
            "sess-test".to_string(),
            owner_type,
            Uuid::new_v4(),
            "label".to_string(),
        )
    }

    fn test_meta() -> SessionMeta {
        SessionMeta {
            id: "sess-test".to_string(),
            title: "Test Session".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: Some(AgentConfig::new("PI_AGENT")),
            executor_session_id: None,
            companion_context: None,
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        }
    }

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
            parent_relation_kind: None,
        })
        .expect("serialize ProjectSessionEntry");

        assert!(value.get("session_id").is_some());
        assert!(value.get("execution_status").is_some());
        assert!(value.get("owner_type").is_some());
        assert!(value.get("story_id").is_some());
        assert!(value.get("agent_key").is_some());
        assert!(value.get("parent_session_id").is_some());
        assert!(value.get("parent_relation_kind").is_some());
        // 确保没有 camelCase 字段
        assert!(value.get("sessionId").is_none());
        assert!(value.get("executionStatus").is_none());
        assert!(value.get("ownerType").is_none());
        assert!(value.get("storyId").is_none());
        assert!(value.get("agentKey").is_none());
        assert!(value.get("parentSessionId").is_none());
        assert!(value.get("parentRelationKind").is_none());
    }

    #[test]
    fn companion_agent_info_uses_agent_name_for_any_owner_type() {
        let binding = test_binding(SessionOwnerType::Task);
        let mut meta = test_meta();
        meta.companion_context = Some(CompanionSessionContext {
            dispatch_id: "dispatch-1".to_string(),
            parent_session_id: "parent-session".to_string(),
            parent_turn_id: "turn-1".to_string(),
            companion_label: "review".to_string(),
            slice_mode: "focused".to_string(),
            adoption_mode: "manual".to_string(),
            request_type: None,
            inherited_fragment_labels: Vec::new(),
            inherited_constraint_keys: Vec::new(),
            agent_name: Some("reviewer".to_string()),
        });

        let (agent_key, agent_display_name) = resolve_agent_info(&binding, &meta, &HashMap::new());

        assert_eq!(agent_key.as_deref(), Some("reviewer"));
        assert_eq!(agent_display_name.as_deref(), Some("reviewer"));
    }

    #[test]
    fn companion_agent_info_prefers_resolved_display_name() {
        let binding = test_binding(SessionOwnerType::Story);
        let mut meta = test_meta();
        meta.companion_context = Some(CompanionSessionContext {
            dispatch_id: "dispatch-1".to_string(),
            parent_session_id: "parent-session".to_string(),
            parent_turn_id: "turn-1".to_string(),
            companion_label: "review".to_string(),
            slice_mode: "focused".to_string(),
            adoption_mode: "manual".to_string(),
            request_type: None,
            inherited_fragment_labels: Vec::new(),
            inherited_constraint_keys: Vec::new(),
            agent_name: Some("reviewer".to_string()),
        });
        let mut display_map = HashMap::new();
        display_map.insert("reviewer".to_string(), "Code Reviewer".to_string());

        let (agent_key, agent_display_name) = resolve_agent_info(&binding, &meta, &display_map);

        assert_eq!(agent_key.as_deref(), Some("reviewer"));
        assert_eq!(agent_display_name.as_deref(), Some("Code Reviewer"));
    }

    #[test]
    fn companion_agent_info_falls_back_to_companion_label() {
        let binding = test_binding(SessionOwnerType::Project);
        let mut meta = test_meta();
        meta.companion_context = Some(CompanionSessionContext {
            dispatch_id: "dispatch-1".to_string(),
            parent_session_id: "parent-session".to_string(),
            parent_turn_id: "turn-1".to_string(),
            companion_label: "researcher".to_string(),
            slice_mode: "focused".to_string(),
            adoption_mode: "manual".to_string(),
            request_type: None,
            inherited_fragment_labels: Vec::new(),
            inherited_constraint_keys: Vec::new(),
            agent_name: None,
        });

        let (agent_key, agent_display_name) = resolve_agent_info(&binding, &meta, &HashMap::new());

        assert_eq!(agent_key.as_deref(), Some("researcher"));
        assert_eq!(agent_display_name.as_deref(), Some("researcher"));
    }
}
