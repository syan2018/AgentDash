use uuid::Uuid;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_application_runtime_session::session::SessionContextProjectionReadModel;
use agentdash_contracts::session::{
    SessionAttachmentContextContributionResponse, SessionContextUsageAnalysisResponse,
    SessionContextUsageCategoryResponse, SessionContextUsageItemResponse,
    SessionMessageContextBreakdownResponse, SessionProjectionMessageRefResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
    SessionToolContextContributionResponse,
};
use agentdash_domain::workflow::LifecycleRun;

use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::dto::{ContextAuditEventDto, ContextAuditQuery};

/// Runtime trace 权限检查通过 RuntimeSessionExecutionAnchor 进入 LifecycleRun project。
pub async fn ensure_runtime_trace_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    session_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("runtime trace {session_id} 不存在")))?;
    let anchor = match state
        .repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await?
    {
        Some(anchor) => anchor,
        None => {
            return Err(ApiError::BadRequest(format!(
                "runtime trace 缺少 RuntimeSessionExecutionAnchor: {session_id}"
            )));
        }
    };
    let run = load_lifecycle_run_for_session(state, anchor.run_id).await?;
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

pub(crate) async fn ensure_runtime_trace_exists(
    state: &AppState,
    session_id: &str,
) -> Result<(), ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("runtime trace {session_id} 不存在")))?;
    Ok(())
}

pub(crate) async fn load_runtime_trace_context_projection(
    state: &AppState,
    session_id: &str,
) -> Result<SessionProjectionViewResponse, ApiError> {
    ensure_runtime_trace_exists(state, session_id).await?;
    let projection = state
        .services
        .session_eventing
        .build_context_projection_read_model(session_id)
        .await
        .map_err(ApiError::from)?;

    Ok(runtime_trace_context_projection_to_response(projection))
}

fn runtime_trace_context_projection_to_response(
    projection: SessionContextProjectionReadModel,
) -> SessionProjectionViewResponse {
    SessionProjectionViewResponse {
        session_id: projection.session_id,
        projection_kind: projection.projection_kind,
        projection_version: projection.projection_version,
        head_event_seq: projection.head_event_seq,
        active_compaction_id: projection.active_compaction_id,
        token_estimate: projection.token_estimate,
        message_count: projection.message_count,
        segments: projection
            .segments
            .into_iter()
            .map(|segment| SessionProjectionSegmentViewResponse {
                id: segment.id,
                sort_order: segment.sort_order,
                segment_type: segment.segment_type,
                role: segment.role,
                origin: segment.origin,
                synthetic: segment.synthetic,
                projection_kind: segment.projection_kind,
                message_ref: SessionProjectionMessageRefResponse {
                    turn_id: segment.message_ref.turn_id,
                    entry_index: segment.message_ref.entry_index,
                },
                source_event_seq: segment.source_event_seq,
                source_range: segment.source_range.map(|range| {
                    SessionProjectionSourceRangeResponse {
                        start_event_seq: range.start_event_seq,
                        end_event_seq: range.end_event_seq,
                    }
                }),
                projection_segment_id: segment.projection_segment_id,
                preview: segment.preview,
                token_estimate: segment.token_estimate,
                attachment_tokens: segment.attachment_tokens,
                attachment_names: segment.attachment_names,
                tool_names: segment.tool_names,
                provenance: SessionProjectionSegmentProvenanceResponse {
                    compaction_id: segment.provenance.compaction_id,
                    projection_version: segment.provenance.projection_version,
                    segment_type: segment.provenance.segment_type,
                    strategy: segment.provenance.strategy,
                    trigger: segment.provenance.trigger,
                    phase: segment.provenance.phase,
                },
            })
            .collect(),
        context_usage: SessionContextUsageAnalysisResponse {
            categories: projection
                .context_usage
                .categories
                .into_iter()
                .map(|category| SessionContextUsageCategoryResponse {
                    kind: category.kind,
                    label: category.label,
                    token_estimate: category.token_estimate,
                    source: category.source,
                    deferred: category.deferred,
                })
                .collect(),
            items: projection
                .context_usage
                .items
                .into_iter()
                .map(|item| SessionContextUsageItemResponse {
                    kind: item.kind,
                    label: item.label,
                    name: item.name,
                    token_estimate: item.token_estimate,
                    source: item.source,
                    deferred: item.deferred,
                    source_event_seq: item.source_event_seq,
                    turn_id: item.turn_id,
                })
                .collect(),
            messages: SessionMessageContextBreakdownResponse {
                user_message_tokens: projection.context_usage.messages.user_message_tokens,
                assistant_message_tokens: projection
                    .context_usage
                    .messages
                    .assistant_message_tokens,
                tool_call_tokens: projection.context_usage.messages.tool_call_tokens,
                tool_result_tokens: projection.context_usage.messages.tool_result_tokens,
                attachment_tokens: projection.context_usage.messages.attachment_tokens,
            },
            top_tools: projection
                .context_usage
                .top_tools
                .into_iter()
                .map(|tool| SessionToolContextContributionResponse {
                    name: tool.name,
                    call_tokens: tool.call_tokens,
                    result_tokens: tool.result_tokens,
                })
                .collect(),
            top_attachments: projection
                .context_usage
                .top_attachments
                .into_iter()
                .map(|attachment| SessionAttachmentContextContributionResponse {
                    name: attachment.name,
                    tokens: attachment.tokens,
                })
                .collect(),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════
// Context Audit —— Bundle / Fragment 产出与消费的可观测轨迹（Step 10d）
// ═══════════════════════════════════════════════════════════════════

/// Content preview 的最大字节数（超过时截断）。
const CONTEXT_AUDIT_CONTENT_PREVIEW_MAX: usize = 2048;

fn parse_scope_tag(tag: &str) -> Option<agentdash_spi::FragmentScope> {
    match tag {
        "runtime_agent" => Some(agentdash_spi::FragmentScope::RuntimeAgent),
        "title_gen" => Some(agentdash_spi::FragmentScope::TitleGen),
        "summarizer" => Some(agentdash_spi::FragmentScope::Summarizer),
        "bridge_replay" => Some(agentdash_spi::FragmentScope::BridgeReplay),
        "audit" => Some(agentdash_spi::FragmentScope::Audit),
        _ => None,
    }
}

fn scope_set_to_tags(scope: agentdash_spi::FragmentScopeSet) -> Vec<String> {
    let mut tags = Vec::new();
    for (label, s) in [
        ("runtime_agent", agentdash_spi::FragmentScope::RuntimeAgent),
        ("title_gen", agentdash_spi::FragmentScope::TitleGen),
        ("summarizer", agentdash_spi::FragmentScope::Summarizer),
        ("bridge_replay", agentdash_spi::FragmentScope::BridgeReplay),
        ("audit", agentdash_spi::FragmentScope::Audit),
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_runtime_session::session::{
        ExecutionStatus, PromptLaunchPath, RuntimeTraceLaunchState,
        SessionAttachmentContextContribution, SessionContextUsageCategory, SessionContextUsageItem,
        SessionContextUsageReadModel, SessionMessageContextBreakdown, SessionMeta,
        SessionProjectionMessageRefReadModel, SessionProjectionSegmentProvenanceReadModel,
        SessionProjectionSegmentReadModel, SessionProjectionSourceRangeReadModel,
        SessionRepositoryRehydrateMode, SessionToolContextContribution,
        resolve_prompt_launch_path,
    };

    fn test_meta(id: &str, event_seq: u64, executor_session_id: Option<&str>) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            created_at: 1,
            updated_at: 1,
            last_event_seq: event_seq,
            last_delivery_status: if event_seq > 0 {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Idle
            },
            last_turn_id: if event_seq > 0 {
                Some("t-last".to_string())
            } else {
                None
            },
            last_terminal_message: None,
            executor_session_id: executor_session_id.map(String::from),
        }
    }

    fn trace_state(meta: &SessionMeta) -> RuntimeTraceLaunchState {
        RuntimeTraceLaunchState::from(meta)
    }

    #[test]
    fn prompt_launch_path_marks_pending_as_owner_bootstrap() {
        let meta = test_meta("sess-1", 0, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, true),
            PromptLaunchPath::OwnerBootstrap
        );
    }

    #[test]
    fn prompt_launch_path_requires_repository_rehydrate_after_cold_restart() {
        let meta = test_meta("sess-2", 12, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::SystemContext,)
        );
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), true, false, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_prefers_executor_follow_up_when_available() {
        let meta = test_meta("sess-3", 5, Some("exec-1"));
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_uses_executor_state_restore_when_supported() {
        let meta = test_meta("sess-4", 7, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::ExecutorState,)
        );
    }

    #[test]
    fn context_projection_mapper_preserves_usage_read_facts() {
        let response =
            runtime_trace_context_projection_to_response(SessionContextProjectionReadModel {
                session_id: "sess-1".to_string(),
                projection_kind: "model_context".to_string(),
                projection_version: 2,
                head_event_seq: 42,
                active_compaction_id: Some("compaction-1".to_string()),
                token_estimate: Some(128),
                message_count: 1,
                segments: vec![SessionProjectionSegmentReadModel {
                    id: "segment-1".to_string(),
                    sort_order: 0,
                    segment_type: "summary_chunk".to_string(),
                    role: "compaction_summary".to_string(),
                    origin: "projection".to_string(),
                    synthetic: true,
                    projection_kind: "compaction_summary".to_string(),
                    message_ref: SessionProjectionMessageRefReadModel {
                        turn_id: "_projection:summary".to_string(),
                        entry_index: 0,
                    },
                    source_event_seq: None,
                    source_range: Some(SessionProjectionSourceRangeReadModel {
                        start_event_seq: 1,
                        end_event_seq: 30,
                    }),
                    projection_segment_id: Some("segment-1".to_string()),
                    preview: "summary".to_string(),
                    token_estimate: Some(20),
                    attachment_tokens: 0,
                    attachment_names: Vec::new(),
                    tool_names: vec!["read_file".to_string()],
                    provenance: SessionProjectionSegmentProvenanceReadModel {
                        compaction_id: Some("compaction-1".to_string()),
                        projection_version: Some(2),
                        segment_type: Some("summary_chunk".to_string()),
                        strategy: Some("summary_prefix".to_string()),
                        trigger: Some("auto".to_string()),
                        phase: Some("pre_provider".to_string()),
                    },
                }],
                context_usage: SessionContextUsageReadModel {
                    categories: vec![SessionContextUsageCategory {
                        kind: "system_developer".to_string(),
                        label: "System / Developer".to_string(),
                        token_estimate: 12,
                        source: "context_frame".to_string(),
                        deferred: false,
                    }],
                    items: vec![SessionContextUsageItem {
                        kind: "system_developer".to_string(),
                        label: "System / Developer".to_string(),
                        name: "Identity".to_string(),
                        token_estimate: 12,
                        source: "context_frame".to_string(),
                        deferred: false,
                        source_event_seq: Some(8),
                        turn_id: Some("turn-1".to_string()),
                    }],
                    messages: SessionMessageContextBreakdown {
                        user_message_tokens: 1,
                        assistant_message_tokens: 2,
                        tool_call_tokens: 3,
                        tool_result_tokens: 4,
                        attachment_tokens: 5,
                    },
                    top_tools: vec![SessionToolContextContribution {
                        name: "read_file".to_string(),
                        call_tokens: 3,
                        result_tokens: 0,
                    }],
                    top_attachments: vec![SessionAttachmentContextContribution {
                        name: "image/png image #0".to_string(),
                        tokens: 5,
                    }],
                },
            });

        assert_eq!(response.session_id, "sess-1");
        assert_eq!(
            response.segments[0]
                .source_range
                .as_ref()
                .unwrap()
                .end_event_seq,
            30
        );
        assert_eq!(
            response.segments[0].provenance.compaction_id.as_deref(),
            Some("compaction-1")
        );
        assert_eq!(
            response.context_usage.categories[0].kind,
            "system_developer"
        );
        assert_eq!(response.context_usage.items[0].source_event_seq, Some(8));
        assert_eq!(response.context_usage.messages.attachment_tokens, 5);
        assert_eq!(response.context_usage.top_tools[0].name, "read_file");
        assert_eq!(
            response.context_usage.top_attachments[0].name,
            "image/png image #0"
        );
    }
}
