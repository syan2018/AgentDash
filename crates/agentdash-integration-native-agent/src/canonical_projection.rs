use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryReplayer, AgentHistoryState,
    AgentItemId, HistoryPayload, ItemDetails,
};
#[cfg(test)]
use agentdash_agent_protocol::AgentCapabilityManifest;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
    CanonicalConversationRecord, ContextAgentConsumption, ContextAgentConsumptionMode,
    ContextConnectorProfile, ContextDeliveryChannel, ContextDeliveryMetadata,
    ContextDeliveryStatus, ContextFrame, ContextFrameChanged, ContextFrameKind,
    ContextFrameSection, ContextFrameSource, ContextMessageRole, ItemCompletedNotification,
    ItemStartedNotification, ItemUpdatedNotification, PlatformEvent, PresentationDurability,
    SourceInfo, TraceInfo, Turn, TurnCompletedNotification, TurnStartedNotification,
    UserInputSource, UserInputSubmissionKind, UserInputSubmittedNotification,
    WorkspaceModulePresentation,
};

#[cfg(test)]
use crate::accepted_context::capability_manifest_sections;
use crate::tool_presentation::{ToolPresentationResult, project_tool_item};

pub(crate) fn history_records(
    history: &AgentHistory,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut records = Vec::new();
    let mut replay = AgentHistoryReplayer::new(history);
    for entry in history.entries() {
        let previous_surface = replay.state().surface.clone();
        let state = replay
            .apply(entry)
            .expect("validated Dash history prefix must fold");
        records.extend(entry_records(
            &history.session_id.0,
            entry,
            previous_surface.as_ref(),
            state,
        )?);
    }
    Ok(records)
}

pub(crate) fn entry_records(
    session_id: &str,
    entry: &AgentHistoryEntry,
    previous_surface: Option<&agentdash_agent::dash::DashSurface>,
    state: &AgentHistoryState,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut events = Vec::new();
    match &entry.payload {
        HistoryPayload::InitialContextInstalled { installation } => {
            events.extend(accepted_context_events(&installation.context_frames));
        }
        HistoryPayload::SurfaceApplied { surface } => {
            events.extend(accepted_surface_context_events(previous_surface, surface));
        }
        HistoryPayload::SurfaceRevoked { surface } => {
            events.push(surface_revoked_event(entry, surface));
        }
        HistoryPayload::ThreadNameChanged { thread_name } => {
            events.push(BackboneEvent::ThreadNameUpdated(
                codex::ThreadNameUpdatedNotification {
                    thread_id: session_id.to_owned(),
                    thread_name: Some(thread_name.clone()),
                },
            ));
        }
        HistoryPayload::InputAccepted { input_id, content } => {
            let turn_id = state
                .active_turn
                .as_ref()
                .map_or_else(|| format!("input:{input_id}"), |turn| turn.0.clone());
            events.push(BackboneEvent::UserInputSubmitted(
                UserInputSubmittedNotification::new(
                    session_id,
                    turn_id,
                    input_id,
                    UserInputSubmissionKind::Prompt,
                    UserInputSource::core_composer(),
                    agentdash_agent_protocol::text_user_input_blocks(content),
                ),
            ));
        }
        HistoryPayload::TurnStarted { turn_id } => {
            events.push(BackboneEvent::TurnStarted(TurnStartedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, None)?,
            }));
        }
        HistoryPayload::ItemStarted {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemStarted(ItemStartedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                started_at_ms: 0,
            }));
        }
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: None,
            content,
        } => {
            events.push(BackboneEvent::AgentMessageDelta(
                codex::AgentMessageDeltaNotification {
                    delta: content.clone(),
                    item_id: entry.entry_id.0.clone(),
                    thread_id: session_id.to_owned(),
                    turn_id: turn_id.0.clone(),
                },
            ));
        }
        HistoryPayload::AgentOutput {
            turn_id,
            item_id: Some(item_id),
            ..
        }
        | HistoryPayload::ToolCall {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
        }
        HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
            if let Some(presentation) = workspace_module_presentation(state, item_id)? {
                events.push(BackboneEvent::Platform(
                    PlatformEvent::WorkspaceModulePresentationRequested(Box::new(presentation)),
                ));
            }
        }
        HistoryPayload::ItemCompleted { turn_id, item_id } => {
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                completed_at_ms: 0,
            }));
        }
        HistoryPayload::CompactionStarted { compaction_id, .. } => {
            events.push(BackboneEvent::ItemStarted(ItemStartedNotification {
                item: codex::ThreadItem::ContextCompaction {
                    id: compaction_id.0.clone(),
                }
                .into(),
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                started_at_ms: 0,
            }));
        }
        HistoryPayload::CompactionApplied {
            compaction_id,
            context_frame,
            ..
        } => {
            events.push(BackboneEvent::ExecutorContextCompacted(
                codex::ContextCompactedNotification {
                    thread_id: session_id.to_owned(),
                    turn_id: compaction_id.0.clone(),
                },
            ));
            events.extend(accepted_context_events(std::slice::from_ref(context_frame)));
        }
        HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => {
            events.push(BackboneEvent::ItemCompleted(ItemCompletedNotification {
                item: codex::ThreadItem::ContextCompaction {
                    id: compaction_id.0.clone(),
                }
                .into(),
                thread_id: session_id.to_owned(),
                turn_id: compaction_id.0.clone(),
                completed_at_ms: 0,
            }));
        }
        HistoryPayload::TurnCompleted { turn_id } | HistoryPayload::TurnInterrupted { turn_id } => {
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, None)?,
            }));
        }
        HistoryPayload::TurnFailed { turn_id, error, .. } => {
            events.push(BackboneEvent::TurnCompleted(TurnCompletedNotification {
                thread_id: session_id.to_owned(),
                turn: turn(state, turn_id, Some(error))?,
            }));
        }
        HistoryPayload::InteractionRequested { .. }
        | HistoryPayload::InteractionResolved { .. }
        | HistoryPayload::Closed => {}
    }

    Ok(events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let turn_id = turn_id(&entry.payload).map(ToOwned::to_owned);
            let envelope = BackboneEnvelope::new(
                event,
                session_id,
                SourceInfo {
                    connector_id: "dash-agent".to_owned(),
                    connector_type: "native".to_owned(),
                    executor_id: None,
                },
            )
            .with_trace(TraceInfo {
                turn_id,
                entry_index: u32::try_from(entry.sequence).ok(),
            })
            .with_observed_at_ms(0);
            CanonicalConversationRecord::new(
                format!("native:{session_id}:{}:{index}", entry.entry_id.0),
                CanonicalConversationPresentation::new(PresentationDurability::Durable, envelope),
            )
        })
        .collect())
}

fn accepted_context_events(frames: &[ContextFrame]) -> Vec<BackboneEvent> {
    frames
        .iter()
        .cloned()
        .map(|frame| {
            BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
                ContextFrameChanged { frame },
            )))
        })
        .collect()
}

fn accepted_surface_context_events(
    previous: Option<&agentdash_agent::dash::DashSurface>,
    current: &agentdash_agent::dash::DashSurface,
) -> Vec<BackboneEvent> {
    let changed_frames = current.context_frames.iter().filter(|frame| {
        if frame.delivery_metadata.agent_consumption.mode
            == ContextAgentConsumptionMode::SystemAppend
        {
            return true;
        }
        let Some(identity) = surface_instruction_identity(frame) else {
            return true;
        };
        !previous
            .into_iter()
            .flat_map(|surface| surface.context_frames.iter())
            .find(|candidate| surface_instruction_identity(candidate) == Some(identity))
            .is_some_and(|candidate| same_surface_frame_semantics(candidate, frame))
    });
    accepted_context_events(&changed_frames.cloned().collect::<Vec<_>>())
}

fn surface_instruction_identity(frame: &ContextFrame) -> Option<&str> {
    frame.sections.iter().find_map(|section| match section {
        ContextFrameSection::Identity { fragments, .. }
        | ContextFrameSection::AssignmentContext { fragments, .. } => {
            fragments.first().map(|fragment| fragment.source.as_str())
        }
        _ => None,
    })
}

fn same_surface_frame_semantics(left: &ContextFrame, right: &ContextFrame) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.id.clear();
    right.id.clear();
    left.delivery_metadata.cache_key = None;
    right.delivery_metadata.cache_key = None;
    left.delivery_metadata.cache_revision = None;
    right.delivery_metadata.cache_revision = None;
    left == right
}

fn workspace_module_presentation(
    state: &AgentHistoryState,
    item_id: &AgentItemId,
) -> Result<Option<WorkspaceModulePresentation>, serde_json::Error> {
    let item = state.items.get(item_id).expect("folded item must exist");
    let ItemDetails::ToolActivity {
        result: Some(result),
        ..
    } = &item.details
    else {
        return Ok(None);
    };
    if result.is_error {
        return Ok(None);
    }
    let Some(value) = result
        .details
        .as_ref()
        .and_then(|details| details.get("workspace_module_presentation"))
    else {
        return Ok(None);
    };
    serde_json::from_value(value.clone()).map(Some)
}

fn surface_revoked_event(
    entry: &AgentHistoryEntry,
    surface: &agentdash_agent::dash::DashSurface,
) -> BackboneEvent {
    let kind = ContextFrameKind::SystemNotice;
    let role = ContextMessageRole::System;
    let mut metadata =
        ContextDeliveryMetadata::for_frame(kind, ContextDeliveryChannel::ConnectorContext, role);
    metadata.cache_key = Some(surface.digest.clone());
    metadata.cache_revision = Some(surface.revision.to_string());
    metadata.agent_consumption = ContextAgentConsumption {
        target: "dash-agent".to_owned(),
        mode: ContextAgentConsumptionMode::Ignore,
        reason: "dash_materialized_surface_revoked".to_owned(),
    };
    metadata.connector_profile = ContextConnectorProfile {
        profile_id: "dash-agent".to_owned(),
        declared_consumption_modes: vec![ContextAgentConsumptionMode::Ignore],
    };
    BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
        ContextFrameChanged {
            frame: ContextFrame {
                id: format!("{}:revoked", entry.entry_id.0),
                kind,
                source: ContextFrameSource::RuntimeContextUpdate,
                phase_node: None,
                apply_mode: Some("surface_revoke".to_owned()),
                delivery_status: ContextDeliveryStatus::Accepted,
                delivery_channel: ContextDeliveryChannel::ConnectorContext,
                message_role: role,
                delivery_metadata: metadata,
                rendered_text: format!("Agent surface revision {} revoked", surface.revision),
                sections: vec![ContextFrameSection::SystemNotice {
                    title: "Agent Surface Revoked".to_owned(),
                    summary: format!("Surface revision {} is no longer active", surface.revision),
                    body: None,
                }],
                created_at_ms: 0,
            },
        },
    )))
}

fn item(
    state: &AgentHistoryState,
    item_id: &AgentItemId,
) -> Result<AgentDashThreadItem, serde_json::Error> {
    let item = state.items.get(item_id).expect("folded item must exist");
    let value = match &item.details {
        ItemDetails::AssistantMessage { content } => serde_json::json!({
            "type": "agentMessage",
            "id": item_id.0,
            "text": content,
        }),
        ItemDetails::ToolActivity {
            call_id: _,
            name,
            arguments,
            protocol_projector,
            result,
        } => {
            let arguments = serde_json::from_str::<serde_json::Value>(arguments)?;
            return project_tool_item(
                &item_id.0,
                name,
                arguments,
                protocol_projector,
                item.status == ActivityStatus::Active,
                matches!(
                    item.status,
                    ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted
                ),
                result.as_ref().map(|result| ToolPresentationResult {
                    content: result.content.as_slice(),
                    details: result.details.as_ref(),
                    is_error: result.is_error,
                }),
            )
            .map_err(|error| serde_json::Error::io(std::io::Error::other(error)));
        }
        ItemDetails::Interaction { prompt } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": "user_input",
            "arguments": {"prompt": prompt},
            "status": status(item.status),
        }),
        ItemDetails::ContextCompaction => serde_json::json!({
            "type": "contextCompaction",
            "id": item_id.0,
        }),
        ItemDetails::Pending => match item.kind {
            agentdash_agent::dash::ItemKind::AssistantMessage => serde_json::json!({
                "type": "agentMessage",
                "id": item_id.0,
                "text": "",
            }),
            _ => serde_json::json!({
                "type": "dynamicToolCall",
                "id": item_id.0,
                "tool": format!("{:?}", item.kind).to_ascii_lowercase(),
                "arguments": {},
                "status": status(item.status),
            }),
        },
    };
    serde_json::from_value::<codex::ThreadItem>(value).map(Into::into)
}

fn turn(
    state: &AgentHistoryState,
    turn_id: &agentdash_agent::dash::AgentTurnId,
    failure: Option<&agentdash_agent::dash::DashExecutionFailure>,
) -> Result<Turn, serde_json::Error> {
    let turn = state.turns.get(turn_id).expect("folded turn must exist");
    let items = state
        .items
        .iter()
        .filter(|(_, item_state)| item_state.turn_id == *turn_id)
        .map(|(item_id, _)| item(state, item_id))
        .collect::<Result<Vec<_>, _>>()?;
    let error = failure.map(|failure| codex::TurnError {
        message: failure.message.clone(),
        codex_error_info: None,
        additional_details: Some(Some(format!(
            "code={}; retryable={}",
            failure.code, failure.retryable
        ))),
    });
    Ok(Turn {
        id: turn_id.0.clone(),
        items,
        items_view: codex::TurnItemsView::Full,
        status: turn_status(turn.status),
        started_at: None,
        completed_at: None,
        duration_ms: None,
        error,
    })
}

fn status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted => "failed",
    }
}

fn turn_status(status: ActivityStatus) -> codex::TurnStatus {
    match status {
        ActivityStatus::Active => codex::TurnStatus::InProgress,
        ActivityStatus::Completed => codex::TurnStatus::Completed,
        ActivityStatus::Failed | ActivityStatus::Lost => codex::TurnStatus::Failed,
        ActivityStatus::Interrupted => codex::TurnStatus::Interrupted,
    }
}

fn turn_id(payload: &HistoryPayload) -> Option<&str> {
    match payload {
        HistoryPayload::TurnStarted { turn_id }
        | HistoryPayload::ItemStarted { turn_id, .. }
        | HistoryPayload::ItemCompleted { turn_id, .. }
        | HistoryPayload::AgentOutput { turn_id, .. }
        | HistoryPayload::ToolCall { turn_id, .. }
        | HistoryPayload::ToolResult { turn_id, .. }
        | HistoryPayload::InteractionRequested { turn_id, .. }
        | HistoryPayload::TurnCompleted { turn_id }
        | HistoryPayload::TurnFailed { turn_id, .. }
        | HistoryPayload::TurnInterrupted { turn_id } => Some(&turn_id.0),
        HistoryPayload::CompactionStarted { compaction_id, .. }
        | HistoryPayload::CompactionApplied { compaction_id, .. }
        | HistoryPayload::CompactionCompleted { compaction_id }
        | HistoryPayload::CompactionFailed { compaction_id, .. } => Some(&compaction_id.0),
        HistoryPayload::InitialContextInstalled { .. }
        | HistoryPayload::SurfaceApplied { .. }
        | HistoryPayload::SurfaceRevoked { .. }
        | HistoryPayload::ThreadNameChanged { .. }
        | HistoryPayload::InputAccepted { .. }
        | HistoryPayload::InteractionResolved { .. }
        | HistoryPayload::Closed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent::dash::{
        AgentSessionId, AgentTurnId, BranchId, DashSurface, DashToolDefinition,
        HistoryContribution, HistoryEntryId, ItemKind,
    };
    use agentdash_agent_protocol::{
        AgentCapabilityChannel, AgentCapabilityCompanionAgent, AgentCapabilityDiagnostic,
        AgentCapabilityMcpServer, AgentCapabilityMemorySource, AgentCapabilityMount,
        AgentCapabilitySkill, AgentCapabilityVfs, AgentCapabilityWorkspaceModule,
    };

    #[test]
    fn accepted_capability_manifest_projects_the_complete_platform_basket() {
        let mut manifest = AgentCapabilityManifest {
            tool_capabilities: vec!["task::write".to_owned()],
            tool_clusters: vec!["task".to_owned()],
            included_tool_paths: vec!["task::write::task_write".to_owned()],
            excluded_tool_paths: vec![],
            mcp_servers: vec![AgentCapabilityMcpServer {
                name: "docs".to_owned(),
                uses_relay: false,
                status: "ready".to_owned(),
                tool_count: Some(2),
                reason_code: None,
                message: None,
            }],
            companion_agents: vec![AgentCapabilityCompanionAgent {
                agent_key: "reviewer".to_owned(),
                executor: "dash".to_owned(),
                display_name: "Reviewer".to_owned(),
            }],
            channels: vec![AgentCapabilityChannel {
                channel_ref: "project:1:2".to_owned(),
                aliases: vec!["review".to_owned()],
                operations: vec!["read".to_owned(), "reply".to_owned()],
                readiness: "ready".to_owned(),
            }],
            vfs: Some(AgentCapabilityVfs {
                default_mount: Some("main".to_owned()),
                mounts: vec![AgentCapabilityMount {
                    id: "main".to_owned(),
                    display_name: "Workspace".to_owned(),
                    root_ref: "relay://workspace".to_owned(),
                    capabilities: vec!["read".to_owned(), "write".to_owned()],
                }],
            }),
            skills: vec![AgentCapabilitySkill {
                name: "review".to_owned(),
                capability_key: "workspace/review".to_owned(),
                provider_key: "workspace".to_owned(),
                local_name: "review".to_owned(),
                display_name: Some("Review".to_owned()),
                description: "Review changes".to_owned(),
                file_path: "main://.agents/skills/review/SKILL.md".to_owned(),
                base_dir: Some("main://.agents/skills/review".to_owned()),
                exposure: "default_exposed".to_owned(),
                disable_model_invocation: false,
            }],
            skill_diagnostics: vec![AgentCapabilityDiagnostic {
                provider_key: "workspace".to_owned(),
                code: "fixture".to_owned(),
                message: "diagnostic".to_owned(),
                source_key: None,
                uri: None,
            }],
            memory_sources: vec![AgentCapabilityMemorySource {
                provider_key: "project".to_owned(),
                source_key: "memory".to_owned(),
                display_name: "Project memory".to_owned(),
                source_uri: "main://MEMORY.md".to_owned(),
                index_uri: "main://memory/index.json".to_owned(),
                mount_id: "main".to_owned(),
                scope: "project".to_owned(),
                index_status: "ready".to_owned(),
                trust_level: "first_party".to_owned(),
                summary: None,
            }],
            memory_diagnostics: vec![],
            workspace_module: AgentCapabilityWorkspaceModule {
                mode: "all".to_owned(),
                allowed_module_ids: vec![],
            },
        };

        let sections = capability_manifest_sections(None, &manifest).expect("project manifest");

        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::CapabilityKeyDelta { effective_capabilities, .. }
                if effective_capabilities == &["task::write"]
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::McpServerDelta { added_mcp_servers, .. }
                if added_mcp_servers == &["docs"]
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::VfsDelta { vfs_mounts_added, .. }
                if vfs_mounts_added == &["main"]
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::SkillDelta { added_skills, .. }
                if added_skills.iter().any(|skill| skill.capability_key == "workspace/review")
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::MemoryInventory { sources, .. } if sources.len() == 1
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::CompanionAgentRosterDelta { effective_agents, .. }
                if effective_agents.len() == 1
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::SystemNotice { title, .. } if title == "Channels"
        )));
        assert!(sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::SystemNotice { title, .. } if title == "Workspace Modules"
        )));

        manifest.memory_sources.clear();
        manifest.memory_diagnostics.clear();
        let empty_memory_sections =
            capability_manifest_sections(None, &manifest).expect("empty memory inventory");
        assert!(empty_memory_sections.iter().any(|section| matches!(
            section,
            ContextFrameSection::MemoryInventory { sources, diagnostics, .. }
                if sources.is_empty() && diagnostics.is_empty()
        )));
    }

    #[test]
    fn historical_tool_item_retains_its_accepted_projector_after_surface_revoke() {
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "read_document".to_owned(),
                description: "Read a document".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                capability_key: "test/read".to_owned(),
                source: "test".to_owned(),
                tool_path: "test/read::read_document".to_owned(),
                context_usage_kind: "test_tools".to_owned(),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FsRead,
            }],
            context_frames: Vec::new(),
        };
        let turn_id = AgentTurnId::new("turn-1");
        let item_id = AgentItemId::new("item-1");
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let contributions = vec![
            HistoryPayload::SurfaceApplied {
                surface: surface.clone(),
            },
            HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
            },
            HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::ToolCall,
            },
            HistoryPayload::ToolCall {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                call_id: "call-1".to_owned(),
                name: "read_document".to_owned(),
                arguments: r#"{"path":"README.md"}"#.to_owned(),
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FsRead,
            },
            HistoryPayload::ToolResult {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                content: vec![agentdash_agent::ContentPart::text(
                    "file: README.md\n1 | first\n2 | second",
                )],
                is_error: false,
                details: None,
            },
            HistoryPayload::ItemCompleted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
            },
            HistoryPayload::TurnCompleted { turn_id },
            HistoryPayload::SurfaceRevoked { surface },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, payload)| HistoryContribution {
            entry_id: HistoryEntryId::new(format!("entry-{}", index + 1)),
            payload,
        })
        .collect();
        history.append_batch(contributions).expect("valid history");

        let state = history.state().expect("folded history");
        assert!(state.surface.is_none());
        let projected = item(&state, &item_id).expect("historical tool projection");
        let projected = serde_json::to_value(projected).unwrap();
        assert_eq!(projected["type"], "fsRead");
        assert_eq!(
            projected["contentItems"][0]["text"],
            "file: README.md\n1 | first\n2 | second"
        );

        let records = history_records(&history)
            .expect("canonical turn container must retain AgentDash-native items");
        let completed_turn = records
            .iter()
            .find_map(|record| match &record.presentation.envelope.event {
                BackboneEvent::TurnCompleted(notification) => Some(&notification.turn),
                _ => None,
            })
            .expect("completed turn");
        assert!(completed_turn.items.iter().any(|item| {
            serde_json::to_value(item).is_ok_and(|value| value["type"] == "fsRead")
        }));
    }

    #[test]
    fn workspace_module_presentation_is_projected_from_committed_tool_result() {
        let projector = agentdash_agent_protocol::ToolProtocolProjector::Dynamic;
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "workspace_module_present".to_owned(),
                description: "Present a Workspace Module".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                capability_key: "test/workspace".to_owned(),
                source: "test".to_owned(),
                tool_path: "test/workspace::workspace_module_present".to_owned(),
                context_usage_kind: "test_tools".to_owned(),
                protocol_projector: projector.clone(),
            }],
            context_frames: Vec::new(),
        };
        let turn_id = AgentTurnId::new("turn-1");
        let item_id = AgentItemId::new("item-1");
        let presentation = serde_json::json!({
            "module_id": "canvas:cvs-live",
            "view_key": "default",
            "renderer_kind": "canvas",
            "presentation_uri": "canvas://cvs-live",
            "title": "Live Canvas",
        });
        let mut history =
            AgentHistory::empty(AgentSessionId::new("session-1"), BranchId::new("branch-1"));
        let contributions = vec![
            HistoryPayload::SurfaceApplied { surface },
            HistoryPayload::TurnStarted {
                turn_id: turn_id.clone(),
            },
            HistoryPayload::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                kind: ItemKind::ToolCall,
            },
            HistoryPayload::ToolCall {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                call_id: "call-1".to_owned(),
                name: "workspace_module_present".to_owned(),
                arguments: presentation.to_string(),
                protocol_projector: projector,
            },
            HistoryPayload::ToolResult {
                turn_id,
                item_id,
                content: vec![agentdash_agent::ContentPart::text("presentation requested")],
                is_error: false,
                details: Some(serde_json::json!({
                    "workspace_module_presentation": presentation,
                })),
            },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, payload)| HistoryContribution {
            entry_id: HistoryEntryId::new(format!("entry-{}", index + 1)),
            payload,
        })
        .collect();
        history.append_batch(contributions).expect("valid history");

        let records = history_records(&history).expect("canonical presentation records");
        let event = records.iter().find_map(|record| {
            let BackboneEvent::Platform(PlatformEvent::WorkspaceModulePresentationRequested(
                presentation,
            )) = &record.presentation.envelope.event
            else {
                return None;
            };
            Some(presentation)
        });
        let event = event.expect("typed presentation event");
        assert_eq!(event.module_id, "canvas:cvs-live");
        assert_eq!(event.presentation_uri, "canvas://cvs-live");
    }
}
