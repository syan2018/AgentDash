use agentdash_agent_runtime_contract::RuntimeThreadId;
use uuid::Uuid;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_domain::workflow::LifecycleRun;

use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::dto::{ContextAuditEventDto, ContextAuditQuery};

/// Runtime trace 权限检查通过 canonical AgentRun Runtime binding 进入 LifecycleRun project。
pub async fn ensure_runtime_trace_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    session_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let thread_id = RuntimeThreadId::new(session_id.to_string())
        .map_err(|error| ApiError::BadRequest(format!("无效的 Runtime Thread ID: {error}")))?;
    let binding = match state
        .repos
        .agent_run_runtime_binding_repo
        .load_by_thread_id(&thread_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        Some(binding) => binding,
        None => {
            return Err(ApiError::BadRequest(format!(
                "runtime trace 缺少 AgentRun Runtime binding: {session_id}"
            )));
        }
    };
    let run = load_lifecycle_run_for_session(state, binding.target.run_id).await?;
    load_project_with_permission(state, user, run.project_id, permission).await?;
    Ok(())
}
async fn load_lifecycle_run_for_session(
    state: &AppState,
    run_id: Uuid,
) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

// ═══════════════════════════════════════════════════════════════════
// Context Audit —— Bundle / Fragment 产出与消费的可观测轨迹（Step 10d）
// ═══════════════════════════════════════════════════════════════════

/// Content preview 的最大字节数（超过时截断）。
const CONTEXT_AUDIT_CONTENT_PREVIEW_MAX: usize = 2048;

fn parse_scope_tag(tag: &str) -> Option<agentdash_platform_spi::FragmentScope> {
    match tag {
        "runtime_agent" => Some(agentdash_platform_spi::FragmentScope::RuntimeAgent),
        "title_gen" => Some(agentdash_platform_spi::FragmentScope::TitleGen),
        "summarizer" => Some(agentdash_platform_spi::FragmentScope::Summarizer),
        "bridge_replay" => Some(agentdash_platform_spi::FragmentScope::BridgeReplay),
        "audit" => Some(agentdash_platform_spi::FragmentScope::Audit),
        _ => None,
    }
}

fn scope_set_to_tags(scope: agentdash_platform_spi::FragmentScopeSet) -> Vec<String> {
    let mut tags = Vec::new();
    for (label, s) in [
        (
            "runtime_agent",
            agentdash_platform_spi::FragmentScope::RuntimeAgent,
        ),
        ("title_gen", agentdash_platform_spi::FragmentScope::TitleGen),
        (
            "summarizer",
            agentdash_platform_spi::FragmentScope::Summarizer,
        ),
        (
            "bridge_replay",
            agentdash_platform_spi::FragmentScope::BridgeReplay,
        ),
        ("audit", agentdash_platform_spi::FragmentScope::Audit),
    ] {
        if scope.contains(s) {
            tags.push(label.to_string());
        }
    }
    tags
}

pub(crate) async fn load_runtime_trace_context_audit(
    state: &AppState,
    run_id: &str,
    agent_id: &str,
    query: ContextAuditQuery,
) -> Result<Vec<ContextAuditEventDto>, ApiError> {
    let scope = match query.scope.as_deref() {
        Some(raw) => match parse_scope_tag(raw) {
            Some(s) => Some(s),
            None => return Err(ApiError::BadRequest(format!("无效的 scope: {raw}"))),
        },
        None => None,
    };

    let filter = agentdash_application::context::AuditFilter {
        since_ms: query.since_ms,
        scope,
        slot: query.slot.clone(),
        source_prefix: query.source_prefix.clone(),
    };

    let events = state.services.audit_bus.query(run_id, agent_id, &filter);
    let dtos: Vec<ContextAuditEventDto> = events
        .into_iter()
        .map(|event| {
            let full_len = event.fragment.content.len();
            let truncated = full_len > CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
            let preview = if truncated {
                // 按字符边界截断，避免切断 UTF-8 多字节
                let mut end = CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
                while end > 0 && !event.fragment.content.is_char_boundary(end) {
                    end -= 1;
                }
                event.fragment.content[..end].to_string()
            } else {
                event.fragment.content.clone()
            };
            ContextAuditEventDto {
                event_id: event.event_id,
                bundle_id: event.bundle_id,
                run_id: event.run_id,
                agent_id: event.agent_id,
                bundle_session_uuid: event.bundle_session_uuid,
                at_ms: event.at_ms,
                trigger: event.trigger.as_tag(),
                slot: event.fragment.slot,
                label: event.fragment.label,
                source: event.fragment.source,
                order: event.fragment.order,
                scope: scope_set_to_tags(event.fragment.scope),
                content_preview: preview,
                content_hash: event.content_hash,
                full_content_available: truncated,
            }
        })
        .collect();

    Ok(dtos)
}
