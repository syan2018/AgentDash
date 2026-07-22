use std::collections::BTreeMap;

use agentdash_agent::dash::{
    ActivityStatus, AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentItemId,
    HistoryPayload, ItemDetails,
};
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentCapabilityManifest, AgentDashThreadItem, AgentSurfaceInstructionPresentation,
    BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
    CanonicalConversationRecord, ContextAgentConsumption, ContextAgentConsumptionMode,
    ContextConnectorProfile, ContextDeliveryChannel, ContextDeliveryMetadata,
    ContextDeliveryStatus, ContextFrame, ContextFrameChanged, ContextFrameKind,
    ContextFrameSection, ContextFrameSource, ContextMessageRole, ItemCompletedNotification,
    ItemStartedNotification, ItemUpdatedNotification, PlatformEvent, PresentationDurability,
    RuntimeCompanionAgentEntry, RuntimeContextFragmentEntry, RuntimeMemoryDiagnosticEntry,
    RuntimeMemoryInventoryMode, RuntimeMemorySourceEntry, RuntimeSkillEntry,
    RuntimeToolSchemaEntry, SkillContextExposure, SourceInfo, TraceInfo, Turn,
    TurnCompletedNotification, TurnStartedNotification, UserInputSource, UserInputSubmissionKind,
    UserInputSubmittedNotification, WorkspaceModulePresentation,
};

use crate::tool_presentation::{ToolPresentationResult, project_tool_item};

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
            )?);
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
) -> Result<Vec<BackboneEvent>, serde_json::Error> {
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
        let sections = match &instruction.presentation {
            AgentSurfaceInstructionPresentation::CapabilityManifest { manifest } => {
                let previous_manifest = previous_instructions
                    .get(instruction.key.as_str())
                    .and_then(|instruction| match &instruction.presentation {
                        AgentSurfaceInstructionPresentation::CapabilityManifest { manifest } => {
                            Some(manifest)
                        }
                        _ => None,
                    });
                capability_manifest_sections(previous_manifest, manifest)?
            }
            AgentSurfaceInstructionPresentation::Identity => {
                vec![ContextFrameSection::Identity {
                    title: title.to_owned(),
                    summary: instruction.key.clone(),
                    fragments: vec![fragment],
                }]
            }
            _ => vec![ContextFrameSection::AssignmentContext {
                title: title.to_owned(),
                summary: instruction.key.clone(),
                fragments: vec![fragment],
            }],
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
    Ok(events)
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

fn capability_manifest_sections(
    previous: Option<&AgentCapabilityManifest>,
    current: &AgentCapabilityManifest,
) -> Result<Vec<ContextFrameSection>, serde_json::Error> {
    let before_capabilities = previous
        .map(|manifest| manifest.tool_capabilities.iter().cloned().collect())
        .unwrap_or_default();
    let after_capabilities = current.tool_capabilities.iter().cloned().collect();
    let before_excluded = previous
        .map(|manifest| manifest.excluded_tool_paths.iter().cloned().collect())
        .unwrap_or_default();
    let after_excluded = current.excluded_tool_paths.iter().cloned().collect();
    let before_included = previous
        .map(|manifest| manifest.included_tool_paths.iter().cloned().collect())
        .unwrap_or_default();
    let after_included = current.included_tool_paths.iter().cloned().collect();
    let before_mcp = previous
        .map(|manifest| {
            manifest
                .mcp_servers
                .iter()
                .map(|server| (server.name.clone(), server))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let after_mcp = current
        .mcp_servers
        .iter()
        .map(|server| (server.name.clone(), server))
        .collect::<BTreeMap<_, _>>();
    let before_mounts = previous
        .and_then(|manifest| manifest.vfs.as_ref())
        .map(|vfs| {
            vfs.mounts
                .iter()
                .map(|mount| (mount.id.clone(), mount))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let after_mounts = current
        .vfs
        .as_ref()
        .map(|vfs| {
            vfs.mounts
                .iter()
                .map(|mount| (mount.id.clone(), mount))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let before_skills = previous
        .map(|manifest| {
            manifest
                .skills
                .iter()
                .map(|skill| (capability_skill_key(skill), skill))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let after_skills = current
        .skills
        .iter()
        .map(|skill| (capability_skill_key(skill), skill))
        .collect::<BTreeMap<_, _>>();
    let before_memory = previous
        .map(|manifest| {
            manifest
                .memory_sources
                .iter()
                .map(|source| (capability_memory_key(source), source))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let after_memory = current
        .memory_sources
        .iter()
        .map(|source| (capability_memory_key(source), source))
        .collect::<BTreeMap<_, _>>();
    let before_companions = previous
        .map(|manifest| {
            manifest
                .companion_agents
                .iter()
                .map(|agent| (agent.agent_key.clone(), agent))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let after_companions = current
        .companion_agents
        .iter()
        .map(|agent| (agent.agent_key.clone(), agent))
        .collect::<BTreeMap<_, _>>();
    let skill_entry = |skill: &agentdash_agent_protocol::AgentCapabilitySkill| {
        Ok(RuntimeSkillEntry {
            name: skill.name.clone(),
            capability_key: skill.capability_key.clone(),
            provider_key: skill.provider_key.clone(),
            local_name: skill.local_name.clone(),
            display_name: skill.display_name.clone(),
            description: skill.description.clone(),
            file_path: skill.file_path.clone(),
            base_dir: skill.base_dir.clone(),
            exposure: match skill.exposure.as_str() {
                "default_exposed" => SkillContextExposure::DefaultExposed,
                "explicit_only" => SkillContextExposure::ExplicitOnly,
                other => {
                    return Err(serde_json::Error::io(std::io::Error::other(format!(
                        "accepted capability manifest contains invalid skill exposure `{other}`"
                    ))));
                }
            },
            disable_model_invocation: skill.disable_model_invocation,
            context_usage_kind: Some("agent_surface".to_owned()),
        })
    };
    let memory_entry =
        |source: &agentdash_agent_protocol::AgentCapabilityMemorySource| RuntimeMemorySourceEntry {
            provider_key: source.provider_key.clone(),
            source_key: source.source_key.clone(),
            display_name: source.display_name.clone(),
            source_uri: source.source_uri.clone(),
            index_uri: source.index_uri.clone(),
            mount_id: source.mount_id.clone(),
            scope: source.scope.clone(),
            index_status: source.index_status.clone(),
            trust_level: source.trust_level.clone(),
            revision: String::new(),
            summary: source.summary.clone(),
            context_usage_kind: Some("agent_surface".to_owned()),
        };
    let companion_entry = |agent: &agentdash_agent_protocol::AgentCapabilityCompanionAgent| {
        RuntimeCompanionAgentEntry {
            agent_key: agent.agent_key.clone(),
            executor: agent.executor.clone(),
            display_name: agent.display_name.clone(),
            context_usage_kind: Some("agent_surface".to_owned()),
        }
    };
    let sections = vec![
        ContextFrameSection::CapabilityKeyDelta {
            added_capabilities: set_added(&before_capabilities, &after_capabilities),
            removed_capabilities: set_added(&after_capabilities, &before_capabilities),
            effective_capabilities: current.tool_capabilities.clone(),
        },
        ContextFrameSection::ToolPathDelta {
            blocked_tool_paths: set_added(&before_excluded, &after_excluded),
            unblocked_tool_paths: set_added(&after_excluded, &before_excluded),
            whitelisted_tool_paths: set_added(&before_included, &after_included),
            removed_whitelist_paths: set_added(&after_included, &before_included),
        },
        ContextFrameSection::McpServerDelta {
            added_mcp_servers: map_added(&before_mcp, &after_mcp),
            removed_mcp_servers: map_added(&after_mcp, &before_mcp),
            changed_mcp_servers: map_changed(&before_mcp, &after_mcp),
        },
        ContextFrameSection::VfsDelta {
            vfs_mounts_added: map_added(&before_mounts, &after_mounts),
            vfs_mounts_removed: map_added(&after_mounts, &before_mounts),
            default_mount_before: previous
                .and_then(|manifest| manifest.vfs.as_ref())
                .and_then(|vfs| vfs.default_mount.clone()),
            default_mount_after: current
                .vfs
                .as_ref()
                .and_then(|vfs| vfs.default_mount.clone()),
        },
        ContextFrameSection::SkillDelta {
            added_skills: map_added_values(&before_skills, &after_skills)
                .into_iter()
                .map(skill_entry)
                .collect::<Result<Vec<_>, _>>()?,
            removed_skills: map_added_values(&after_skills, &before_skills)
                .into_iter()
                .map(skill_entry)
                .collect::<Result<Vec<_>, _>>()?,
            changed_skills: map_changed_values(&before_skills, &after_skills)
                .into_iter()
                .map(skill_entry)
                .collect::<Result<Vec<_>, _>>()?,
        },
        ContextFrameSection::SystemNotice {
            title: "Skill Discovery".to_owned(),
            summary: if current.skill_diagnostics.is_empty() {
                "Discovery completed without diagnostics".to_owned()
            } else {
                format!("{} discovery diagnostics", current.skill_diagnostics.len())
            },
            body: (!current.skill_diagnostics.is_empty()).then(|| {
                current
                    .skill_diagnostics
                    .iter()
                    .map(|diagnostic| {
                        format!(
                            "- `{}` / `{}`: {}",
                            diagnostic.provider_key, diagnostic.code, diagnostic.message
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
        },
        ContextFrameSection::MemoryInventory {
            title: "Memory Inventory".to_owned(),
            summary: format!("{} memory sources", current.memory_sources.len()),
            mode: if previous.is_some() {
                RuntimeMemoryInventoryMode::Delta
            } else {
                RuntimeMemoryInventoryMode::Snapshot
            },
            sources: current.memory_sources.iter().map(memory_entry).collect(),
            diagnostics: current
                .memory_diagnostics
                .iter()
                .map(|diagnostic| RuntimeMemoryDiagnosticEntry {
                    provider_key: diagnostic.provider_key.clone(),
                    code: diagnostic.code.clone(),
                    message: diagnostic.message.clone(),
                    source_key: diagnostic.source_key.clone(),
                    uri: diagnostic.uri.clone(),
                    context_usage_kind: Some("agent_surface".to_owned()),
                })
                .collect(),
            added_sources: map_added_values(&before_memory, &after_memory)
                .into_iter()
                .map(memory_entry)
                .collect(),
            removed_sources: map_added_values(&after_memory, &before_memory)
                .into_iter()
                .map(memory_entry)
                .collect(),
            changed_sources: map_changed_values(&before_memory, &after_memory)
                .into_iter()
                .map(memory_entry)
                .collect(),
        },
        ContextFrameSection::CompanionAgentRosterDelta {
            added_agents: map_added_values(&before_companions, &after_companions)
                .into_iter()
                .map(companion_entry)
                .collect(),
            removed_agent_keys: map_added(&after_companions, &before_companions),
            changed_agents: map_changed_values(&before_companions, &after_companions)
                .into_iter()
                .map(companion_entry)
                .collect(),
            effective_agents: current
                .companion_agents
                .iter()
                .map(companion_entry)
                .collect(),
        },
        ContextFrameSection::SystemNotice {
            title: "Channels".to_owned(),
            summary: format!("{} visible channels", current.channels.len()),
            body: Some(
                current
                    .channels
                    .iter()
                    .map(|channel| {
                        format!(
                            "- `{}` [{}] ({})",
                            channel.channel_ref,
                            channel.operations.join(", "),
                            channel.readiness
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        },
        ContextFrameSection::SystemNotice {
            title: "Workspace Modules".to_owned(),
            summary: format!("visibility: {}", current.workspace_module.mode),
            body: Some(if current.workspace_module.allowed_module_ids.is_empty() {
                "No module-id allowlist is applied.".to_owned()
            } else {
                current.workspace_module.allowed_module_ids.join("\n")
            }),
        },
    ];
    Ok(sections)
}

fn capability_skill_key(skill: &agentdash_agent_protocol::AgentCapabilitySkill) -> String {
    if skill.capability_key.is_empty() {
        skill.name.clone()
    } else {
        skill.capability_key.clone()
    }
}

fn capability_memory_key(source: &agentdash_agent_protocol::AgentCapabilityMemorySource) -> String {
    format!("{}:{}", source.provider_key, source.source_key)
}

fn map_added<K: Ord + Clone, V>(before: &BTreeMap<K, V>, after: &BTreeMap<K, V>) -> Vec<K> {
    after
        .keys()
        .filter(|key| !before.contains_key(*key))
        .cloned()
        .collect()
}

fn set_added<K: Ord + Clone>(
    before: &std::collections::BTreeSet<K>,
    after: &std::collections::BTreeSet<K>,
) -> Vec<K> {
    after.difference(before).cloned().collect()
}

fn map_changed<K: Ord + Clone, V: PartialEq>(
    before: &BTreeMap<K, V>,
    after: &BTreeMap<K, V>,
) -> Vec<K> {
    after
        .iter()
        .filter(|(key, value)| before.get(*key).is_some_and(|before| before != *value))
        .map(|(key, _)| key.clone())
        .collect()
}

fn map_added_values<'a, K: Ord, V>(
    before: &BTreeMap<K, &'a V>,
    after: &BTreeMap<K, &'a V>,
) -> Vec<&'a V> {
    after
        .iter()
        .filter(|(key, _)| !before.contains_key(*key))
        .map(|(_, value)| *value)
        .collect()
}

fn map_changed_values<'a, K: Ord, V: PartialEq>(
    before: &BTreeMap<K, &'a V>,
    after: &BTreeMap<K, &'a V>,
) -> Vec<&'a V> {
    after
        .iter()
        .filter(|(key, value)| before.get(*key).is_some_and(|before| before != *value))
        .map(|(_, value)| *value)
        .collect()
}

fn surface_instruction_presentation(
    instruction: &agentdash_agent::dash::DashSurfaceInstruction,
) -> (ContextFrameKind, ContextMessageRole, &'static str) {
    match &instruction.presentation {
        AgentSurfaceInstructionPresentation::SystemGuidelines => (
            ContextFrameKind::SystemGuidelines,
            ContextMessageRole::System,
            "System Guidelines",
        ),
        AgentSurfaceInstructionPresentation::Identity => (
            ContextFrameKind::Identity,
            ContextMessageRole::System,
            "Agent Identity",
        ),
        AgentSurfaceInstructionPresentation::Environment => (
            ContextFrameKind::Environment,
            ContextMessageRole::Context,
            "Runtime Environment",
        ),
        AgentSurfaceInstructionPresentation::MemoryContext => (
            ContextFrameKind::MemoryContext,
            ContextMessageRole::Context,
            "Memory Context",
        ),
        AgentSurfaceInstructionPresentation::CapabilityManifest { .. } => (
            ContextFrameKind::CapabilityStateDelta,
            ContextMessageRole::Context,
            "Capability Surface",
        ),
        AgentSurfaceInstructionPresentation::UserContext => (
            ContextFrameKind::UserContext,
            ContextMessageRole::User,
            "User Context",
        ),
        AgentSurfaceInstructionPresentation::AssignmentContext => (
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
                protocol_projector: agentdash_agent_protocol::ToolProtocolProjector::FsRead,
            }],
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
                protocol_projector: projector.clone(),
            }],
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
