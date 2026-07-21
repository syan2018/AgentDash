use std::collections::BTreeMap;

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentItemId,
    HistoryPayload, ItemDetails,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
    CanonicalConversationRecord, ContextAgentConsumption, ContextAgentConsumptionMode,
    ContextConnectorProfile, ContextDeliveryChannel, ContextDeliveryMetadata,
    ContextDeliveryStatus, ContextFrame, ContextFrameChanged, ContextFrameKind,
    ContextFrameSection, ContextFrameSource, ContextMessageRole, ItemCompletedNotification,
    ItemStartedNotification, ItemUpdatedNotification, PlatformEvent, PresentationDurability,
    RuntimeContextFragmentEntry, RuntimeToolSchemaEntry, SourceInfo, TraceInfo, UserInputSource,
    UserInputSubmissionKind, UserInputSubmittedNotification,
};

pub(crate) fn history_records(
    history: &AgentHistory,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut records = Vec::new();
    for entry in history.entries() {
        let previous_state = if entry.sequence > 1 {
            Some(
                history
                    .state_at(entry.sequence - 1)
                    .expect("validated Dash history prefix must fold"),
            )
        } else {
            None
        };
        let state = history
            .state_at(entry.sequence)
            .expect("validated Dash history prefix must fold");
        records.extend(entry_records(
            &history.session_id.0,
            entry,
            previous_state.as_ref(),
            &state,
        )?);
    }
    Ok(records)
}

pub(crate) fn entry_records(
    session_id: &str,
    entry: &AgentHistoryEntry,
    previous_state: Option<&AgentHistoryState>,
    state: &AgentHistoryState,
) -> Result<Vec<CanonicalConversationRecord>, serde_json::Error> {
    let mut events = Vec::new();
    match &entry.payload {
        HistoryPayload::InitialContextInstalled { installation } => {
            events.extend(initial_context_events(installation));
        }
        HistoryPayload::SurfaceApplied { surface } => {
            events.extend(surface_events(
                entry,
                previous_state.and_then(|state| state.surface.as_ref()),
                surface,
            ));
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
            events.push(BackboneEvent::TurnStarted(serde_json::from_value(
                serde_json::json!({
                    "threadId": session_id,
                    "turn": turn_json(state, turn_id, None)?,
                }),
            )?));
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
        }
        | HistoryPayload::ToolResult {
            turn_id, item_id, ..
        } => {
            events.push(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                item: item(state, item_id)?,
                thread_id: session_id.to_owned(),
                turn_id: turn_id.0.clone(),
                updated_at_ms: 0,
            }));
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
        HistoryPayload::CompactionApplied { compaction_id, .. } => {
            events.push(BackboneEvent::ExecutorContextCompacted(
                codex::ContextCompactedNotification {
                    thread_id: session_id.to_owned(),
                    turn_id: compaction_id.0.clone(),
                },
            ));
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
            events.push(BackboneEvent::TurnCompleted(serde_json::from_value(
                serde_json::json!({
                    "threadId": session_id,
                    "turn": turn_json(state, turn_id, None)?,
                }),
            )?));
        }
        HistoryPayload::TurnFailed { turn_id, error, .. } => {
            events.push(BackboneEvent::TurnCompleted(serde_json::from_value(
                serde_json::json!({
                    "threadId": session_id,
                    "turn": turn_json(state, turn_id, Some(error))?,
                }),
            )?));
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

fn initial_context_events(
    installation: &agentdash_agent::dash::InitialContextInstallation,
) -> Vec<BackboneEvent> {
    installation
        .contributions
        .iter()
        .enumerate()
        .map(|(index, contribution)| {
            let (kind, title, role) = match contribution.kind.as_str() {
                "compact_summary" => (
                    ContextFrameKind::CompactionSummary,
                    "Compaction Summary",
                    ContextMessageRole::Context,
                ),
                "constraint_set" => (
                    ContextFrameKind::SystemGuidelines,
                    "System Guidelines",
                    ContextMessageRole::System,
                ),
                _ => (
                    ContextFrameKind::AssignmentContext,
                    "Workflow Context",
                    ContextMessageRole::Context,
                ),
            };
            let mut metadata = ContextDeliveryMetadata::for_frame(
                kind,
                ContextDeliveryChannel::ConnectorContext,
                role,
            );
            metadata.cache_key = Some(installation.package_digest.clone());
            metadata.cache_revision = Some(contribution.source_revision.clone());
            metadata.agent_consumption = ContextAgentConsumption {
                target: "dash-agent".to_owned(),
                mode: ContextAgentConsumptionMode::Consume,
                reason: "dash_initial_context_installed".to_owned(),
            };
            metadata.connector_profile = ContextConnectorProfile {
                profile_id: "dash-agent".to_owned(),
                declared_consumption_modes: vec![ContextAgentConsumptionMode::Consume],
            };
            BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(Box::new(
                ContextFrameChanged {
                    frame: ContextFrame {
                        id: format!("initial-context:{}:{index}", installation.package_id),
                        kind,
                        source: ContextFrameSource::RuntimeContextUpdate,
                        phase_node: None,
                        apply_mode: Some("initial_context_install".to_owned()),
                        delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                        delivery_channel: ContextDeliveryChannel::ConnectorContext,
                        message_role: role,
                        delivery_metadata: metadata,
                        rendered_text: contribution.payload.clone(),
                        sections: vec![ContextFrameSection::SystemNotice {
                            title: title.to_owned(),
                            summary: contribution.kind.clone(),
                            body: Some(contribution.payload.clone()),
                        }],
                        created_at_ms: 0,
                    },
                },
            )))
        })
        .collect()
}

fn surface_events(
    entry: &AgentHistoryEntry,
    previous: Option<&agentdash_agent::dash::DashSurface>,
    surface: &agentdash_agent::dash::DashSurface,
) -> Vec<BackboneEvent> {
    let mut events = Vec::new();
    let previous_instructions = previous
        .into_iter()
        .flat_map(|surface| surface.instructions.iter())
        .map(|instruction| (instruction.key.as_str(), instruction))
        .collect::<BTreeMap<_, _>>();
    for (index, instruction) in surface
        .instructions
        .iter()
        .filter(|instruction| {
            previous_instructions
                .get(instruction.key.as_str())
                .is_none_or(|previous| *previous != *instruction)
        })
        .enumerate()
    {
        if instruction.text.trim().is_empty() {
            continue;
        }
        let (kind, role, title) = surface_instruction_presentation(instruction);
        let mut metadata = ContextDeliveryMetadata::for_frame(
            kind,
            ContextDeliveryChannel::ConnectorContext,
            role,
        );
        metadata.cache_key = Some(surface.digest.clone());
        metadata.cache_revision = Some(surface.revision.to_string());
        metadata.delivery_order = metadata
            .delivery_order
            .saturating_add(u32::try_from(index).unwrap_or(u32::MAX));
        metadata.agent_consumption = ContextAgentConsumption {
            target: "dash-agent".to_owned(),
            mode: ContextAgentConsumptionMode::SystemAppend,
            reason: "dash_materialized_instruction".to_owned(),
        };
        metadata.connector_profile = ContextConnectorProfile {
            profile_id: "dash-agent".to_owned(),
            declared_consumption_modes: vec![ContextAgentConsumptionMode::SystemAppend],
        };
        let fragment = RuntimeContextFragmentEntry {
            slot: instruction.channel.clone(),
            label: instruction
                .key
                .rsplit(':')
                .next()
                .unwrap_or(instruction.key.as_str())
                .to_owned(),
            source: instruction.key.clone(),
            content: instruction.text.clone(),
            context_usage_kind: Some("agent_surface".to_owned()),
        };
        let sections = if kind == ContextFrameKind::Identity {
            vec![ContextFrameSection::Identity {
                title: title.to_owned(),
                summary: instruction.key.clone(),
                fragments: vec![fragment],
            }]
        } else if kind == ContextFrameKind::CapabilityStateDelta {
            vec![ContextFrameSection::SystemNotice {
                title: title.to_owned(),
                summary: instruction.key.clone(),
                body: Some(instruction.text.clone()),
            }]
        } else {
            vec![ContextFrameSection::AssignmentContext {
                title: title.to_owned(),
                summary: instruction.key.clone(),
                fragments: vec![fragment],
            }]
        };
        events.push(BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(
            Box::new(ContextFrameChanged {
                frame: ContextFrame {
                    id: format!("{}:instruction:{index}", entry.entry_id.0),
                    kind,
                    source: ContextFrameSource::RuntimeContextUpdate,
                    phase_node: None,
                    apply_mode: Some(
                        if previous_instructions.contains_key(instruction.key.as_str()) {
                            "surface_update"
                        } else {
                            "surface_apply"
                        }
                        .to_owned(),
                    ),
                    delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                    delivery_channel: ContextDeliveryChannel::ConnectorContext,
                    message_role: role,
                    delivery_metadata: metadata,
                    rendered_text: instruction.text.clone(),
                    sections,
                    created_at_ms: 0,
                },
            }),
        )));
    }
    let removed_instructions = previous
        .into_iter()
        .flat_map(|surface| surface.instructions.iter())
        .filter(|instruction| {
            !surface
                .instructions
                .iter()
                .any(|current| current.key == instruction.key)
        })
        .map(|instruction| instruction.key.clone())
        .collect::<Vec<_>>();
    if !removed_instructions.is_empty() {
        let kind = ContextFrameKind::SystemNotice;
        let role = ContextMessageRole::Context;
        let mut metadata = ContextDeliveryMetadata::for_frame(
            kind,
            ContextDeliveryChannel::ConnectorContext,
            role,
        );
        metadata.cache_key = Some(surface.digest.clone());
        metadata.cache_revision = Some(surface.revision.to_string());
        events.push(BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(
            Box::new(ContextFrameChanged {
                frame: ContextFrame {
                    id: format!("{}:instructions-removed", entry.entry_id.0),
                    kind,
                    source: ContextFrameSource::RuntimeContextUpdate,
                    phase_node: None,
                    apply_mode: Some("surface_update".to_owned()),
                    delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                    delivery_channel: ContextDeliveryChannel::ConnectorContext,
                    message_role: role,
                    delivery_metadata: metadata,
                    rendered_text: format!(
                        "## Removed Surface Instructions\n{}",
                        removed_instructions
                            .iter()
                            .map(|key| format!("- `{key}`"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                    sections: vec![ContextFrameSection::SystemNotice {
                        title: "Surface Instructions Removed".to_owned(),
                        summary: format!("{} instructions removed", removed_instructions.len()),
                        body: Some(removed_instructions.join("\n")),
                    }],
                    created_at_ms: 0,
                },
            }),
        )));
    }

    let previous_tools = previous
        .into_iter()
        .flat_map(|surface| surface.tools.iter())
        .map(|tool| (tool.name.as_str(), tool))
        .collect::<BTreeMap<_, _>>();
    let current_tools = surface
        .tools
        .iter()
        .map(|tool| (tool.name.as_str(), tool))
        .collect::<BTreeMap<_, _>>();
    let added_tools = surface
        .tools
        .iter()
        .filter(|tool| !previous_tools.contains_key(tool.name.as_str()))
        .map(runtime_tool_schema_entry)
        .collect::<Vec<_>>();
    let changed_tools = surface
        .tools
        .iter()
        .filter(|tool| {
            previous_tools
                .get(tool.name.as_str())
                .is_some_and(|previous| *previous != *tool)
        })
        .map(runtime_tool_schema_entry)
        .collect::<Vec<_>>();
    let removed_tools = previous_tools
        .keys()
        .filter(|name| !current_tools.contains_key(**name))
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    if !added_tools.is_empty() || !removed_tools.is_empty() || !changed_tools.is_empty() {
        let kind = ContextFrameKind::CapabilityStateDelta;
        let role = ContextMessageRole::Context;
        let mut metadata = ContextDeliveryMetadata::for_frame(
            kind,
            ContextDeliveryChannel::ConnectorContext,
            role,
        );
        metadata.cache_key = Some(surface.digest.clone());
        metadata.cache_revision = Some(surface.revision.to_string());
        metadata.agent_consumption = ContextAgentConsumption {
            target: "dash-agent".to_owned(),
            mode: ContextAgentConsumptionMode::ConnectorNative,
            reason: "dash_materialized_tool_registry".to_owned(),
        };
        metadata.connector_profile = ContextConnectorProfile {
            profile_id: "dash-agent".to_owned(),
            declared_consumption_modes: vec![ContextAgentConsumptionMode::ConnectorNative],
        };
        events.push(BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(
            Box::new(ContextFrameChanged {
                frame: ContextFrame {
                    id: format!("{}:tools", entry.entry_id.0),
                    kind,
                    source: ContextFrameSource::RuntimeContextUpdate,
                    phase_node: None,
                    apply_mode: Some(
                        if previous.is_some() {
                            "surface_update"
                        } else {
                            "surface_apply"
                        }
                        .to_owned(),
                    ),
                    delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                    delivery_channel: ContextDeliveryChannel::ConnectorContext,
                    message_role: role,
                    delivery_metadata: metadata,
                    rendered_text: render_tool_surface_delta(
                        &added_tools,
                        &removed_tools,
                        &changed_tools,
                    ),
                    sections: vec![ContextFrameSection::ToolSchemaDelta {
                        added_tools,
                        removed_tools,
                        changed_tools,
                    }],
                    created_at_ms: 0,
                },
            }),
        )));
    }
    events
}

fn runtime_tool_schema_entry(
    tool: &agentdash_agent::dash::DashToolDefinition,
) -> RuntimeToolSchemaEntry {
    let mcp = tool.name.starts_with("mcp_");
    RuntimeToolSchemaEntry {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters_schema: tool.input_schema.clone(),
        capability_key: None,
        source: Some(if mcp { "mcp" } else { "agentdash" }.to_owned()),
        tool_path: Some(tool.name.clone()),
        context_usage_kind: Some("agent_surface".to_owned()),
    }
}

fn render_tool_surface_delta(
    added: &[RuntimeToolSchemaEntry],
    removed: &[String],
    changed: &[RuntimeToolSchemaEntry],
) -> String {
    let mut sections = vec!["## Tool Surface Delta".to_owned()];
    if !added.is_empty() {
        sections.push(format!(
            "### Added Tools\n{}",
            added
                .iter()
                .map(|tool| format!("- `{}`: {}", tool.name, tool.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !removed.is_empty() {
        sections.push(format!(
            "### Removed Tools\n{}",
            removed
                .iter()
                .map(|name| format!("- `{name}`"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !changed.is_empty() {
        sections.push(format!(
            "### Changed Tools\n{}",
            changed
                .iter()
                .map(|tool| format!("- `{}`: {}", tool.name, tool.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    sections.join("\n\n")
}

fn surface_instruction_presentation(
    instruction: &agentdash_agent::dash::DashSurfaceInstruction,
) -> (ContextFrameKind, ContextMessageRole, &'static str) {
    if instruction
        .key
        .starts_with("instruction:execution-profile:")
    {
        return (
            ContextFrameKind::SystemGuidelines,
            ContextMessageRole::System,
            "System Guidelines",
        );
    }
    match instruction.channel.as_str() {
        "persona" | "agent_identity" => (
            ContextFrameKind::Identity,
            ContextMessageRole::System,
            "Agent Identity",
        ),
        "workspace" | "vfs" | "runtime_policy" => (
            ContextFrameKind::Environment,
            ContextMessageRole::Context,
            "Runtime Environment",
        ),
        "constraint" | "constraints" | "instruction" | "instruction_append" => (
            ContextFrameKind::SystemGuidelines,
            ContextMessageRole::Developer,
            "System Guidelines",
        ),
        "memory" | "codebase" | "references" => (
            ContextFrameKind::MemoryContext,
            ContextMessageRole::Context,
            "Memory Context",
        ),
        "skills" => (
            ContextFrameKind::CapabilityStateDelta,
            ContextMessageRole::Context,
            "Available Skills",
        ),
        "mcp" => (
            ContextFrameKind::CapabilityStateDelta,
            ContextMessageRole::Context,
            "MCP Servers",
        ),
        "user_context" => (
            ContextFrameKind::UserContext,
            ContextMessageRole::User,
            "User Context",
        ),
        _ => (
            ContextFrameKind::AssignmentContext,
            ContextMessageRole::Context,
            "Assignment Context",
        ),
    }
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
            result,
        } => serde_json::json!({
            "type": "dynamicToolCall",
            "id": item_id.0,
            "tool": name,
            "arguments": serde_json::from_str::<serde_json::Value>(arguments)
                .unwrap_or_else(|_| serde_json::Value::String(arguments.clone())),
            "status": result.as_ref().map_or_else(
                || status(item.status),
                |result| if result.is_error { "failed" } else { status(item.status) },
            ),
            "contentItems": result.as_ref().map(|result| vec![serde_json::json!({
                "type": "inputText",
                "text": result.content,
            })]),
            "success": result.as_ref().map(|result| !result.is_error),
        }),
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

fn turn_json(
    state: &AgentHistoryState,
    turn_id: &agentdash_agent::dash::AgentTurnId,
    failure: Option<&agentdash_agent::dash::DashExecutionFailure>,
) -> Result<serde_json::Value, serde_json::Error> {
    let turn = state.turns.get(turn_id).expect("folded turn must exist");
    let items = state
        .items
        .iter()
        .filter(|(_, item_state)| item_state.turn_id == *turn_id)
        .map(|(item_id, _)| item(state, item_id))
        .collect::<Result<Vec<_>, _>>()?;
    let error = failure.map(|failure| {
        serde_json::json!({
            "message": failure.message,
            "additionalDetails": format!(
                "code={}; retryable={}",
                failure.code, failure.retryable
            ),
        })
    });
    Ok(serde_json::json!({
        "id": turn_id.0,
        "items": items,
        "status": turn_status(turn.status),
        "startedAt": null,
        "completedAt": null,
        "durationMs": null,
        "error": error,
    }))
}

fn status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost | ActivityStatus::Interrupted => "failed",
    }
}

fn turn_status(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Active => "inProgress",
        ActivityStatus::Completed => "completed",
        ActivityStatus::Failed | ActivityStatus::Lost => "failed",
        ActivityStatus::Interrupted => "interrupted",
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
