use std::collections::{BTreeMap, BTreeSet};

use super::{
    CompactionId, ContextRevision, DashSurface, DashSurfaceInstruction, DashToolDefinition,
    InitialContextInstallation,
};
use agentdash_agent_protocol::{
    AgentCapabilityManifest, AgentSurfaceInstructionPresentation, ContextDeliveryStatus,
    ContextFrame, ContextFrameKind, ContextFrameSection, RuntimeCompanionAgentEntry,
    RuntimeContextFragmentEntry, RuntimeMemoryDiagnosticEntry, RuntimeMemoryInventoryMode,
    RuntimeMemorySourceEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry, SkillContextExposure,
};
use serde_json::Value;

pub(crate) fn compile_initial_context(
    installation: &InitialContextInstallation,
) -> Vec<ContextFrame> {
    let mut frames = compile_stable_frames(Some(installation), None, "initial-context");
    for (index, contribution) in installation
        .contributions
        .iter()
        .filter(|contribution| contribution.kind == "compact_summary")
        .enumerate()
    {
        let rendered_text = format!(
            "## AgentDash Initial Context: Compaction Summary\n{}",
            contribution.payload
        );
        frames.push(ContextFrame {
            id: format!(
                "initial-context:{}:summary:{index}",
                installation.package_id
            ),
            kind: ContextFrameKind::CompactionSummary,
            delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
            rendered_text: rendered_text.clone(),
            sections: vec![ContextFrameSection::ContextFragments {
                fragments: vec![RuntimeContextFragmentEntry {
                    slot: "initial_context".to_owned(),
                    label: "Compaction Summary".to_owned(),
                    source: contribution.authority.clone(),
                    content: rendered_text,
                    context_usage_kind: Some(contribution.kind.clone()),
                }],
            }],
            created_at_ms: 0,
        });
    }
    frames.sort_by(frame_order);
    frames
}

pub(crate) fn compile_surface_update(
    installation: Option<&InitialContextInstallation>,
    surface: &DashSurface,
    previous: Option<&DashSurface>,
) -> Result<Vec<ContextFrame>, serde_json::Error> {
    let mut frames = compile_stable_frames(
        installation,
        Some(surface),
        &format!("surface:{}:{}", surface.revision, surface.digest),
    );
    if let Some(frame) = materialize_capability_state_frame(surface, previous)? {
        frames.push(frame);
    }
    frames.sort_by(frame_order);
    Ok(frames)
}

pub(crate) fn compile_surface_revoke(
    installation: Option<&InitialContextInstallation>,
    previous: &DashSurface,
) -> Result<Vec<ContextFrame>, serde_json::Error> {
    let mut frames = compile_stable_frames(
        installation,
        None,
        &format!("surface-revoke:{}:{}", previous.revision, previous.digest),
    );
    let empty = DashSurface {
        revision: previous.revision.saturating_add(1),
        digest: format!("revoked:{}", previous.digest),
        instructions: Vec::new(),
        tools: Vec::new(),
    };
    if let Some(frame) = materialize_capability_state_frame(&empty, Some(previous))? {
        frames.push(frame);
    }
    frames.sort_by(frame_order);
    Ok(frames)
}

pub(crate) fn compile_compaction_rebuild(
    installation: Option<&InitialContextInstallation>,
    surface: Option<&DashSurface>,
    compaction_id: &CompactionId,
    revision: &ContextRevision,
    summary_frame: ContextFrame,
) -> Result<Vec<ContextFrame>, serde_json::Error> {
    let mut frames = compile_stable_frames(
        installation,
        surface,
        &format!("compaction:{}:{}", compaction_id.0, revision.0),
    );
    if let Some(surface) = surface
        && let Some(frame) = materialize_capability_state_frame(surface, None)?
    {
        frames.push(frame);
    }
    frames.push(summary_frame);
    frames.sort_by(frame_order);
    let created_at_ms = frames
        .iter()
        .find(|frame| frame.kind == ContextFrameKind::CompactionSummary)
        .map_or(0, |frame| frame.created_at_ms);
    for (index, frame) in frames.iter_mut().enumerate() {
        frame.id = format!(
            "compaction:{}:{}:{index}:{}",
            compaction_id.0,
            revision.0,
            frame.kind.as_key()
        );
        frame.delivery_status = ContextDeliveryStatus::AppliedToCompactedContext;
        frame.created_at_ms = created_at_ms;
    }
    Ok(frames)
}

pub(crate) fn validate_compaction_rebuild(
    frames: &[ContextFrame],
    compaction_id: &CompactionId,
    revision: &ContextRevision,
) -> bool {
    if frames.is_empty()
        || frames
            .windows(2)
            .any(|pair| frame_order(&pair[0], &pair[1]).is_gt())
    {
        return false;
    }

    let id_prefix = format!("compaction:{}:{}:", compaction_id.0, revision.0);
    let mut frame_ids = BTreeSet::new();
    let mut stable_kinds = BTreeSet::new();
    let mut capability_count = 0;
    let mut summary = None;
    let accepted_at = frames[0].created_at_ms;

    for (index, frame) in frames.iter().enumerate() {
        if frame.delivery_status != ContextDeliveryStatus::AppliedToCompactedContext
            || frame.created_at_ms != accepted_at
            || !frame_ids.insert(frame.id.as_str())
            || frame.id != format!("{id_prefix}{index}:{}", frame.kind.as_key())
        {
            return false;
        }

        match frame.kind {
            ContextFrameKind::Identity
            | ContextFrameKind::UserContext
            | ContextFrameKind::Environment
            | ContextFrameKind::SystemGuidelines
            | ContextFrameKind::AssignmentContext
            | ContextFrameKind::MemoryContext => {
                if !stable_kinds.insert(frame.kind.as_key()) {
                    return false;
                }
            }
            ContextFrameKind::CapabilityStateDelta => {
                capability_count += 1;
                if capability_count > 1 {
                    return false;
                }
            }
            ContextFrameKind::CompactionSummary => {
                if summary.replace(frame).is_some() {
                    return false;
                }
            }
        }
    }

    let Some(summary_frame) = summary else {
        return false;
    };
    let [
        ContextFrameSection::CompactionSummary {
            summary,
            compaction_id: Some(summary_compaction_id),
            timestamp_ms: Some(timestamp_ms),
            ..
        },
    ] = summary_frame.sections.as_slice()
    else {
        return false;
    };
    summary_compaction_id == &compaction_id.0
        && summary_frame.rendered_text
            == format!("<compacted_context>\n{summary}\n</compacted_context>")
        && summary_frame.created_at_ms == i64::try_from(*timestamp_ms).unwrap_or(i64::MAX)
}

fn compile_stable_frames(
    installation: Option<&InitialContextInstallation>,
    surface: Option<&DashSurface>,
    occurrence: &str,
) -> Vec<ContextFrame> {
    let mut grouped = stable_kinds()
        .into_iter()
        .map(|kind| (kind, Vec::new()))
        .collect::<Vec<_>>();
    if let Some(installation) = installation {
        for contribution in &installation.contributions {
            let Some(kind) = initial_contribution_kind(&contribution.kind) else {
                continue;
            };
            let title = frame_title(kind);
            push_fragment(
                &mut grouped,
                kind,
                RuntimeContextFragmentEntry {
                    slot: "initial_context".to_owned(),
                    label: title.to_owned(),
                    source: contribution.authority.clone(),
                    content: format!(
                        "## AgentDash Initial Context: {title}\n{}",
                        contribution.payload
                    ),
                    context_usage_kind: Some(contribution.kind.clone()),
                },
            );
        }
    }
    if let Some(surface) = surface {
        for instruction in &surface.instructions {
            if instruction.text.trim().is_empty()
                || matches!(
                    instruction.presentation,
                    AgentSurfaceInstructionPresentation::CapabilityManifest { .. }
                )
            {
                continue;
            }
            let (kind, _) = instruction_presentation(instruction);
            push_fragment(
                &mut grouped,
                kind,
                RuntimeContextFragmentEntry {
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
                },
            );
        }
    }
    grouped
        .into_iter()
        .filter_map(|(kind, fragments)| {
            if fragments.is_empty() {
                return None;
            }
            let rendered_text = fragments
                .iter()
                .map(|fragment| fragment.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            Some(ContextFrame {
                id: format!("{occurrence}:{}", kind.as_key()),
                kind,
                delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
                rendered_text,
                sections: vec![ContextFrameSection::ContextFragments { fragments }],
                created_at_ms: 0,
            })
        })
        .collect()
}

fn stable_kinds() -> [ContextFrameKind; 6] {
    [
        ContextFrameKind::Identity,
        ContextFrameKind::UserContext,
        ContextFrameKind::Environment,
        ContextFrameKind::SystemGuidelines,
        ContextFrameKind::AssignmentContext,
        ContextFrameKind::MemoryContext,
    ]
}

fn push_fragment(
    grouped: &mut [(ContextFrameKind, Vec<RuntimeContextFragmentEntry>)],
    kind: ContextFrameKind,
    fragment: RuntimeContextFragmentEntry,
) {
    if let Some((_, fragments)) = grouped.iter_mut().find(|(candidate, _)| *candidate == kind) {
        fragments.push(fragment);
    }
}

fn initial_contribution_kind(kind: &str) -> Option<ContextFrameKind> {
    match kind {
        "constraint_set" => Some(ContextFrameKind::SystemGuidelines),
        "workflow_context" => Some(ContextFrameKind::AssignmentContext),
        _ => None,
    }
}

fn frame_title(kind: ContextFrameKind) -> &'static str {
    match kind {
        ContextFrameKind::Identity => "Agent Identity",
        ContextFrameKind::UserContext => "User Context",
        ContextFrameKind::Environment => "Runtime Environment",
        ContextFrameKind::SystemGuidelines => "System Guidelines",
        ContextFrameKind::AssignmentContext => "Assignment Context",
        ContextFrameKind::MemoryContext => "Memory Context",
        _ => "Context",
    }
}

fn materialize_capability_state_frame(
    surface: &DashSurface,
    previous: Option<&DashSurface>,
) -> Result<Option<ContextFrame>, serde_json::Error> {
    let current_manifest =
        surface
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.presentation {
                AgentSurfaceInstructionPresentation::CapabilityManifest { manifest } => {
                    Some(manifest)
                }
                _ => None,
            });
    let previous_manifest = previous
        .into_iter()
        .flat_map(|surface| surface.instructions.iter())
        .find_map(|instruction| match &instruction.presentation {
            AgentSurfaceInstructionPresentation::CapabilityManifest { manifest } => Some(manifest),
            _ => None,
        });
    let mut sections = match (previous_manifest, current_manifest) {
        (None, None) => Vec::new(),
        (previous, Some(current)) => capability_manifest_sections(previous, current)?,
        (Some(previous), None) => {
            capability_manifest_sections(Some(previous), &AgentCapabilityManifest::default())?
        }
    };
    if let Some(tool_schema_delta) = tool_schema_delta(surface, previous) {
        sections.push(tool_schema_delta);
    }
    if sections.is_empty() {
        return Ok(None);
    }

    let kind = ContextFrameKind::CapabilityStateDelta;
    Ok(Some(ContextFrame {
        id: format!(
            "surface:{}:{}:capability-state-delta",
            surface.revision, surface.digest
        ),
        kind,
        delivery_status: ContextDeliveryStatus::AppliedBeforePrompt,
        rendered_text: render_capability_state_delta(&sections),
        sections,
        created_at_ms: 0,
    }))
}

fn tool_schema_delta(
    surface: &DashSurface,
    previous: Option<&DashSurface>,
) -> Option<ContextFrameSection> {
    let previous_tools = previous
        .into_iter()
        .flat_map(|surface| surface.tools.iter())
        .map(|tool| (tool_identity(tool), tool))
        .collect::<std::collections::BTreeMap<_, _>>();
    let current_tools = surface
        .tools
        .iter()
        .map(|tool| (tool_identity(tool), tool))
        .collect::<std::collections::BTreeMap<_, _>>();
    let added_tools = current_tools
        .iter()
        .filter(|(key, _)| !previous_tools.contains_key(*key))
        .map(|(_, tool)| runtime_tool_schema_entry(tool))
        .collect::<Vec<_>>();
    let changed_tools = current_tools
        .iter()
        .filter(|(key, tool)| {
            previous_tools
                .get(*key)
                .is_some_and(|previous| *previous != **tool)
        })
        .map(|(_, tool)| runtime_tool_schema_entry(tool))
        .collect::<Vec<_>>();
    let removed_tools = previous_tools
        .keys()
        .filter(|key| !current_tools.contains_key(*key))
        .map(|(_, _, _, name)| name.clone())
        .collect::<Vec<_>>();
    if added_tools.is_empty() && removed_tools.is_empty() && changed_tools.is_empty() {
        None
    } else {
        Some(ContextFrameSection::ToolSchemaDelta {
            added_tools,
            removed_tools,
            changed_tools,
        })
    }
}

fn tool_identity(tool: &DashToolDefinition) -> (String, String, String, String) {
    (
        tool.capability_key.clone(),
        tool.source.clone(),
        tool.tool_path.clone(),
        tool.name.clone(),
    )
}

fn instruction_presentation(
    instruction: &DashSurfaceInstruction,
) -> (ContextFrameKind, &'static str) {
    match &instruction.presentation {
        AgentSurfaceInstructionPresentation::SystemGuidelines => {
            (ContextFrameKind::SystemGuidelines, "System Guidelines")
        }
        AgentSurfaceInstructionPresentation::Identity => {
            (ContextFrameKind::Identity, "Agent Identity")
        }
        AgentSurfaceInstructionPresentation::Environment => {
            (ContextFrameKind::Environment, "Runtime Environment")
        }
        AgentSurfaceInstructionPresentation::MemoryContext => {
            (ContextFrameKind::MemoryContext, "Memory Context")
        }
        AgentSurfaceInstructionPresentation::CapabilityManifest { .. } => {
            (ContextFrameKind::CapabilityStateDelta, "Capability Surface")
        }
        AgentSurfaceInstructionPresentation::UserContext => {
            (ContextFrameKind::UserContext, "User Context")
        }
        AgentSurfaceInstructionPresentation::AssignmentContext => {
            (ContextFrameKind::AssignmentContext, "Assignment Context")
        }
    }
}

pub(crate) fn capability_manifest_sections(
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
    let is_initial = previous.is_none();
    let skill_diagnostics_changed =
        previous.is_none_or(|manifest| manifest.skill_diagnostics != current.skill_diagnostics);
    let memory_diagnostics_changed =
        previous.is_none_or(|manifest| manifest.memory_diagnostics != current.memory_diagnostics);
    let channels_changed = previous.is_none_or(|manifest| manifest.channels != current.channels);
    let workspace_module_changed =
        previous.is_none_or(|manifest| manifest.workspace_module != current.workspace_module);
    let tool_clusters_changed =
        previous.is_none_or(|manifest| manifest.tool_clusters != current.tool_clusters);
    let mut vfs_mounts_added = map_added(&before_mounts, &after_mounts);
    let mut vfs_mounts_removed = map_added(&after_mounts, &before_mounts);
    let changed_vfs_mounts = map_changed(&before_mounts, &after_mounts);
    vfs_mounts_added.extend(changed_vfs_mounts.iter().cloned());
    vfs_mounts_removed.extend(changed_vfs_mounts);
    vfs_mounts_added.sort();
    vfs_mounts_added.dedup();
    vfs_mounts_removed.sort();
    vfs_mounts_removed.dedup();

    let mut sections = vec![
        ContextFrameSection::CapabilityKeyDelta {
            added_capabilities: set_added(&before_capabilities, &after_capabilities),
            removed_capabilities: set_added(&after_capabilities, &before_capabilities),
            effective_capabilities: current.tool_capabilities.clone(),
        },
        ContextFrameSection::SystemNotice {
            title: "Tool Clusters".to_owned(),
            summary: format!("{} enabled tool clusters", current.tool_clusters.len()),
            body: Some(
                current
                    .tool_clusters
                    .iter()
                    .map(|cluster| format!("- `{cluster}`"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
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
            vfs_mounts_added,
            vfs_mounts_removed,
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
            sources: if is_initial {
                current.memory_sources.iter().map(memory_entry).collect()
            } else {
                Vec::new()
            },
            diagnostics: if is_initial || memory_diagnostics_changed {
                current
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
                    .collect()
            } else {
                Vec::new()
            },
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
    if !is_initial {
        sections.retain(|section| match section {
            ContextFrameSection::CapabilityKeyDelta {
                added_capabilities,
                removed_capabilities,
                ..
            } => !added_capabilities.is_empty() || !removed_capabilities.is_empty(),
            ContextFrameSection::ToolPathDelta {
                blocked_tool_paths,
                unblocked_tool_paths,
                whitelisted_tool_paths,
                removed_whitelist_paths,
            } => {
                !blocked_tool_paths.is_empty()
                    || !unblocked_tool_paths.is_empty()
                    || !whitelisted_tool_paths.is_empty()
                    || !removed_whitelist_paths.is_empty()
            }
            ContextFrameSection::McpServerDelta {
                added_mcp_servers,
                removed_mcp_servers,
                changed_mcp_servers,
            } => {
                !added_mcp_servers.is_empty()
                    || !removed_mcp_servers.is_empty()
                    || !changed_mcp_servers.is_empty()
            }
            ContextFrameSection::VfsDelta {
                vfs_mounts_added,
                vfs_mounts_removed,
                default_mount_before,
                default_mount_after,
            } => {
                !vfs_mounts_added.is_empty()
                    || !vfs_mounts_removed.is_empty()
                    || default_mount_before != default_mount_after
            }
            ContextFrameSection::SkillDelta {
                added_skills,
                removed_skills,
                changed_skills,
            } => {
                !added_skills.is_empty() || !removed_skills.is_empty() || !changed_skills.is_empty()
            }
            ContextFrameSection::MemoryInventory {
                added_sources,
                removed_sources,
                changed_sources,
                ..
            } => {
                !added_sources.is_empty()
                    || !removed_sources.is_empty()
                    || !changed_sources.is_empty()
                    || memory_diagnostics_changed
            }
            ContextFrameSection::CompanionAgentRosterDelta {
                added_agents,
                removed_agent_keys,
                changed_agents,
                ..
            } => {
                !added_agents.is_empty()
                    || !removed_agent_keys.is_empty()
                    || !changed_agents.is_empty()
            }
            ContextFrameSection::SystemNotice { title, .. } => match title.as_str() {
                "Tool Clusters" => tool_clusters_changed,
                "Skill Discovery" => skill_diagnostics_changed,
                "Channels" => channels_changed,
                "Workspace Modules" => workspace_module_changed,
                _ => true,
            },
            _ => true,
        });
    }
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

fn runtime_tool_schema_entry(tool: &DashToolDefinition) -> RuntimeToolSchemaEntry {
    RuntimeToolSchemaEntry {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters_schema: tool.input_schema.clone(),
        capability_key: Some(tool.capability_key.clone()),
        source: Some(tool.source.clone()),
        tool_path: Some(tool.tool_path.clone()),
        context_usage_kind: Some(tool.context_usage_kind.clone()),
    }
}

fn render_capability_state_delta(sections: &[ContextFrameSection]) -> String {
    let mut rendered = vec![
        "## Capability State Delta".to_owned(),
        "Only capability facts changed by this accepted surface revision are appended below."
            .to_owned(),
    ];
    for section in sections {
        match section {
            ContextFrameSection::CapabilityKeyDelta {
                added_capabilities,
                removed_capabilities,
                ..
            } => {
                push_string_delta(&mut rendered, "Added Capabilities", added_capabilities);
                push_string_delta(&mut rendered, "Removed Capabilities", removed_capabilities);
            }
            ContextFrameSection::ToolPathDelta {
                blocked_tool_paths,
                unblocked_tool_paths,
                whitelisted_tool_paths,
                removed_whitelist_paths,
            } => {
                push_string_delta(&mut rendered, "Blocked Tool Paths", blocked_tool_paths);
                push_string_delta(&mut rendered, "Unblocked Tool Paths", unblocked_tool_paths);
                push_string_delta(
                    &mut rendered,
                    "Whitelisted Tool Paths",
                    whitelisted_tool_paths,
                );
                push_string_delta(
                    &mut rendered,
                    "Removed Tool Path Whitelist",
                    removed_whitelist_paths,
                );
            }
            ContextFrameSection::McpServerDelta {
                added_mcp_servers,
                removed_mcp_servers,
                changed_mcp_servers,
            } => {
                push_string_delta(&mut rendered, "Added MCP Servers", added_mcp_servers);
                push_string_delta(&mut rendered, "Removed MCP Servers", removed_mcp_servers);
                push_string_delta(&mut rendered, "Changed MCP Servers", changed_mcp_servers);
            }
            ContextFrameSection::VfsDelta {
                vfs_mounts_added,
                vfs_mounts_removed,
                default_mount_before,
                default_mount_after,
            } => {
                push_string_delta(
                    &mut rendered,
                    "Added / Changed VFS Mounts",
                    vfs_mounts_added,
                );
                push_string_delta(
                    &mut rendered,
                    "Removed / Replaced VFS Mounts",
                    vfs_mounts_removed,
                );
                if default_mount_before != default_mount_after {
                    rendered.push(format!(
                        "### Default VFS Mount\n- before: `{}`\n- after: `{}`",
                        default_mount_before.as_deref().unwrap_or("none"),
                        default_mount_after.as_deref().unwrap_or("none")
                    ));
                }
            }
            ContextFrameSection::ToolSchemaDelta {
                added_tools,
                removed_tools,
                changed_tools,
            } => render_tool_schema_delta(&mut rendered, added_tools, removed_tools, changed_tools),
            ContextFrameSection::SkillDelta {
                added_skills,
                removed_skills,
                changed_skills,
            } => {
                push_skill_delta(&mut rendered, "Added Skills", added_skills);
                push_skill_delta(&mut rendered, "Removed Skills", removed_skills);
                push_skill_delta(&mut rendered, "Changed Skills", changed_skills);
            }
            ContextFrameSection::MemoryInventory {
                title,
                summary,
                added_sources,
                removed_sources,
                changed_sources,
                diagnostics,
                ..
            } => {
                push_memory_delta(&mut rendered, "Added Memory Sources", added_sources);
                push_memory_delta(&mut rendered, "Removed Memory Sources", removed_sources);
                push_memory_delta(&mut rendered, "Changed Memory Sources", changed_sources);
                if !diagnostics.is_empty() {
                    rendered.push(format!(
                        "### Memory Diagnostics\n{}",
                        diagnostics
                            .iter()
                            .map(|diagnostic| format!(
                                "- `{}` / `{}`: {}",
                                diagnostic.provider_key, diagnostic.code, diagnostic.message
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
                if added_sources.is_empty()
                    && removed_sources.is_empty()
                    && changed_sources.is_empty()
                    && diagnostics.is_empty()
                {
                    rendered.push(format!("### {title}\n{summary}\n- diagnostics: none"));
                }
            }
            ContextFrameSection::CompanionAgentRosterDelta {
                added_agents,
                removed_agent_keys,
                changed_agents,
                ..
            } => {
                if !added_agents.is_empty() {
                    rendered.push(format!(
                        "### Added Companion Agents\n{}",
                        added_agents
                            .iter()
                            .map(|agent| format!(
                                "- `{}` (`{}` via `{}`)",
                                agent.agent_key, agent.display_name, agent.executor
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
                push_string_delta(
                    &mut rendered,
                    "Removed Companion Agents",
                    removed_agent_keys,
                );
                if !changed_agents.is_empty() {
                    rendered.push(format!(
                        "### Changed Companion Agents\n{}",
                        changed_agents
                            .iter()
                            .map(|agent| format!(
                                "- `{}` (`{}` via `{}`)",
                                agent.agent_key, agent.display_name, agent.executor
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
            }
            ContextFrameSection::SystemNotice {
                title,
                summary,
                body,
            } => {
                let mut notice = format!("### {title}\n{summary}");
                if let Some(body) = body.as_deref().filter(|body| !body.trim().is_empty()) {
                    notice.push('\n');
                    notice.push_str(body);
                }
                rendered.push(notice);
            }
            _ => {}
        }
    }
    rendered.join("\n\n")
}

fn push_string_delta(rendered: &mut Vec<String>, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    rendered.push(format!(
        "### {title}\n{}",
        values
            .iter()
            .map(|value| format!("- `{value}`"))
            .collect::<Vec<_>>()
            .join("\n")
    ));
}

fn push_skill_delta(rendered: &mut Vec<String>, title: &str, skills: &[RuntimeSkillEntry]) {
    if skills.is_empty() {
        return;
    }
    rendered.push(format!(
        "### {title}\n{}",
        skills
            .iter()
            .map(|skill| format!(
                "- `{}`: {} (path: `{}`)",
                skill.display_name.as_deref().unwrap_or(&skill.name),
                skill.description,
                skill.file_path
            ))
            .collect::<Vec<_>>()
            .join("\n")
    ));
}

fn push_memory_delta(
    rendered: &mut Vec<String>,
    title: &str,
    sources: &[RuntimeMemorySourceEntry],
) {
    if sources.is_empty() {
        return;
    }
    rendered.push(format!(
        "### {title}\n{}",
        sources
            .iter()
            .map(|source| format!(
                "- `{}` / `{}`: `{}`",
                source.provider_key, source.source_key, source.source_uri
            ))
            .collect::<Vec<_>>()
            .join("\n")
    ));
}

fn render_tool_schema_delta(
    rendered: &mut Vec<String>,
    added_tools: &[RuntimeToolSchemaEntry],
    removed_tools: &[String],
    changed_tools: &[RuntimeToolSchemaEntry],
) {
    if !added_tools.is_empty() {
        rendered.push(format!(
            "### Added Tool Schemas\n{}",
            added_tools
                .iter()
                .map(|tool| render_tool_schema_entry(tool, ToolSchemaRenderMode::Summary))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
    push_string_delta(rendered, "Removed Tool Schemas", removed_tools);
    if !changed_tools.is_empty() {
        rendered.push(format!(
            "### Changed Tool Schemas\n{}",
            changed_tools
                .iter()
                .map(|tool| render_tool_schema_entry(tool, ToolSchemaRenderMode::Summary))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolSchemaRenderMode {
    Summary,
    Complete,
}

pub(crate) fn render_tool_schema_entry(
    tool: &RuntimeToolSchemaEntry,
    mode: ToolSchemaRenderMode,
) -> String {
    let mut lines = vec![format!("#### `{}`", tool.name)];
    let mut provenance = Vec::new();
    if let Some(capability_key) = tool.capability_key.as_deref() {
        provenance.push(format!("capability: `{capability_key}`"));
    }
    if let Some(source) = tool.source.as_deref() {
        provenance.push(format!("source: `{source}`"));
    }
    if let Some(tool_path) = tool.tool_path.as_deref() {
        provenance.push(format!("path: `{tool_path}`"));
    }
    if !provenance.is_empty() {
        lines.push(provenance.join("; "));
    }
    let description = tool.description.trim();
    if !description.is_empty() {
        lines.push(description.to_owned());
    }
    lines.push("Parameters:".to_owned());
    let mut parameters = Vec::new();
    render_schema_node(&tool.parameters_schema, "$", None, 0, &mut parameters);
    lines.extend(parameters);
    if mode == ToolSchemaRenderMode::Complete {
        lines.push("Complete JSON Schema:".to_owned());
        lines.push(format!(
            "```json\n{}\n```",
            serde_json::to_string_pretty(&tool.parameters_schema)
                .unwrap_or_else(|_| tool.parameters_schema.to_string())
        ));
    }
    lines.join("\n")
}

fn render_schema_node(
    schema: &Value,
    path: &str,
    required: Option<bool>,
    depth: usize,
    lines: &mut Vec<String>,
) {
    let indent = "  ".repeat(depth);
    let requirement = match required {
        Some(true) => ", required",
        Some(false) => ", optional",
        None => "",
    };
    let description = schema
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(": {value}"))
        .unwrap_or_default();
    let constraints = schema_constraints(schema);
    let constraints = if constraints.is_empty() {
        String::new()
    } else {
        format!(", {}", constraints.join(", "))
    };
    lines.push(format!(
        "{indent}- `{path}` ({}{requirement}{constraints}){description}",
        schema_type(schema)
    ));

    let required_fields = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let mut names = properties.keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let child_path = if path == "$" {
                name.to_owned()
            } else {
                format!("{path}.{name}")
            };
            render_schema_node(
                &properties[name],
                &child_path,
                Some(required_fields.contains(name.as_str())),
                depth + 1,
                lines,
            );
        }
    }
    if let Some(items) = schema.get("items") {
        render_schema_node(items, &format!("{path}[]"), None, depth + 1, lines);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(variants) = schema.get(keyword).and_then(Value::as_array) {
            for (index, variant) in variants.iter().enumerate() {
                render_schema_node(
                    variant,
                    &format!("{path}.{keyword}[{index}]"),
                    None,
                    depth + 1,
                    lines,
                );
            }
        }
    }
}

fn schema_type(schema: &Value) -> String {
    if let Some(value) = schema.get("const") {
        return format!("const {}", compact_json(value));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return format!(
            "enum {}",
            values
                .iter()
                .map(compact_json)
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    match schema.get("type") {
        Some(Value::String(kind)) if kind == "array" => {
            let item = schema
                .get("items")
                .map(schema_type)
                .unwrap_or_else(|| "any".to_owned());
            format!("array<{item}>")
        }
        Some(Value::String(kind)) => kind.clone(),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ if schema.get("properties").is_some() => "object".to_owned(),
        _ if schema.get("items").is_some() => "array".to_owned(),
        _ if schema.get("oneOf").is_some() => "oneOf".to_owned(),
        _ if schema.get("anyOf").is_some() => "anyOf".to_owned(),
        _ if schema.get("allOf").is_some() => "allOf".to_owned(),
        _ => "any".to_owned(),
    }
}

fn schema_constraints(schema: &Value) -> Vec<String> {
    const RENDERED_KEYS: &[&str] = &[
        "type",
        "description",
        "required",
        "properties",
        "items",
        "enum",
        "const",
        "allOf",
        "anyOf",
        "oneOf",
    ];

    schema
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(key, _)| !RENDERED_KEYS.contains(&key.as_str()))
        .map(|(key, value)| format!("{key}={}", compact_json(value)))
        .collect()
}

fn compact_json(value: &Value) -> String {
    match value {
        Value::String(value) => format!("\"{value}\""),
        other => other.to_string(),
    }
}

fn frame_order(left: &ContextFrame, right: &ContextFrame) -> std::cmp::Ordering {
    fn rank(kind: ContextFrameKind) -> u8 {
        match kind {
            ContextFrameKind::Identity => 10,
            ContextFrameKind::UserContext => 20,
            ContextFrameKind::Environment => 30,
            ContextFrameKind::SystemGuidelines => 40,
            ContextFrameKind::AssignmentContext => 50,
            ContextFrameKind::MemoryContext => 60,
            ContextFrameKind::CapabilityStateDelta => 70,
            ContextFrameKind::CompactionSummary => 80,
        }
    }
    (rank(left.kind), left.created_at_ms, left.id.as_str()).cmp(&(
        rank(right.kind),
        right.created_at_ms,
        right.id.as_str(),
    ))
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::ToolProtocolProjector;

    use super::*;

    fn capability_instruction(manifest: AgentCapabilityManifest) -> DashSurfaceInstruction {
        DashSurfaceInstruction {
            key: "instruction:capability:manifest".to_owned(),
            channel: "capabilities".to_owned(),
            text: "UPSTREAM FULL CAPABILITY MANIFEST MUST NOT BE INJECTED".to_owned(),
            presentation: AgentSurfaceInstructionPresentation::CapabilityManifest { manifest },
        }
    }

    fn test_tool(name: &str, description: &str) -> DashToolDefinition {
        DashToolDefinition {
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "string",
                        "description": format!("{name} value")
                    }
                },
                "required": ["value"]
            }),
            capability_key: "workspace/write".to_owned(),
            source: "platform:workspace".to_owned(),
            tool_path: format!("workspace/write::{name}"),
            context_usage_kind: "system_tools".to_owned(),
            protocol_projector: ToolProtocolProjector::Dynamic,
        }
    }

    #[test]
    fn capability_and_tool_schema_are_one_platform_owned_transition_frame() {
        let surface = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: vec![capability_instruction(AgentCapabilityManifest {
                tool_capabilities: vec!["workspace/write".to_owned()],
                ..AgentCapabilityManifest::default()
            })],
            tools: vec![test_tool("workspace_write", "Write a workspace document.")],
        };

        let frames = compile_surface_update(None, &surface, None).expect("frames");
        let capability_frames = frames
            .iter()
            .filter(|frame| frame.kind == ContextFrameKind::CapabilityStateDelta)
            .collect::<Vec<_>>();

        assert_eq!(
            capability_frames.len(),
            1,
            "capability facts and readable tool schemas must share one CAP frame"
        );
        let frame = capability_frames[0];
        assert_eq!(
            frame.delivery_status,
            ContextDeliveryStatus::AppliedBeforePrompt
        );
        assert!(
            frame
                .sections
                .iter()
                .any(|section| matches!(section, ContextFrameSection::CapabilityKeyDelta { .. }))
        );
        assert!(
            frame
                .sections
                .iter()
                .any(|section| matches!(section, ContextFrameSection::ToolSchemaDelta { .. }))
        );
        assert!(frame.rendered_text.contains("#### `workspace_write`"));
        assert!(frame.rendered_text.contains("`value`"));
        assert!(
            !frame
                .rendered_text
                .contains("UPSTREAM FULL CAPABILITY MANIFEST"),
            "accepted text must be rendered from typed delta facts"
        );
    }

    #[test]
    fn capability_state_update_renders_only_changed_tool_schemas() {
        let manifest = AgentCapabilityManifest {
            tool_capabilities: vec!["workspace/write".to_owned()],
            ..AgentCapabilityManifest::default()
        };
        let previous = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: vec![capability_instruction(manifest.clone())],
            tools: vec![test_tool("existing_tool", "Existing tool description.")],
        };
        let current = DashSurface {
            revision: 2,
            digest: "surface-2".to_owned(),
            instructions: vec![capability_instruction(manifest)],
            tools: vec![
                test_tool("existing_tool", "Existing tool description."),
                test_tool("new_tool", "New tool description."),
            ],
        };

        let frames = compile_surface_update(None, &current, Some(&previous)).expect("frames");
        let frame = frames
            .iter()
            .find(|frame| frame.kind == ContextFrameKind::CapabilityStateDelta)
            .expect("capability delta");

        assert_eq!(
            frame.sections.len(),
            1,
            "an update frame must contain only dimensions that actually changed"
        );
        assert!(frame.rendered_text.contains("#### `new_tool`"));
        assert!(frame.rendered_text.contains("New tool description."));
        assert!(
            !frame.rendered_text.contains("existing_tool"),
            "an unchanged schema must not be replayed in a delta"
        );
        assert!(
            !frame.rendered_text.contains("Existing tool description."),
            "an unchanged schema description must not be replayed in a delta"
        );
    }

    #[test]
    fn unchanged_capability_state_does_not_emit_an_empty_delta_frame() {
        let manifest = AgentCapabilityManifest {
            tool_capabilities: vec!["workspace/write".to_owned()],
            ..AgentCapabilityManifest::default()
        };
        let previous = DashSurface {
            revision: 1,
            digest: "surface-1".to_owned(),
            instructions: vec![capability_instruction(manifest.clone())],
            tools: vec![test_tool("workspace_write", "Write a workspace document.")],
        };
        let current = DashSurface {
            revision: 2,
            digest: "surface-2".to_owned(),
            instructions: vec![capability_instruction(manifest)],
            tools: previous.tools.clone(),
        };

        let frames = compile_surface_update(None, &current, Some(&previous)).expect("frames");

        assert!(
            frames
                .iter()
                .all(|frame| frame.kind != ContextFrameKind::CapabilityStateDelta),
            "a surface revision without capability changes must not publish an empty CAP delta"
        );
    }

    #[test]
    fn tool_schema_frame_renders_complete_readable_summary_and_retains_structured_schema() {
        let surface = DashSurface {
            revision: 7,
            digest: "surface-7".to_owned(),
            instructions: Vec::new(),
            tools: vec![DashToolDefinition {
                name: "workspace_write".to_owned(),
                description: "Write a workspace document.".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "document": {
                            "type": "object",
                            "properties": {
                                "format": {
                                    "type": "string",
                                    "enum": ["markdown", "json"]
                                },
                                "blocks": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "text": {"type": "string", "minLength": 1}
                                        },
                                        "required": ["text"]
                                    }
                                }
                            },
                            "required": ["format", "blocks"]
                        }
                    },
                    "required": ["document"]
                }),
                capability_key: "workspace/write".to_owned(),
                source: "platform:workspace".to_owned(),
                tool_path: "workspace/write::workspace_write".to_owned(),
                context_usage_kind: "system_tools".to_owned(),
                protocol_projector: ToolProtocolProjector::Dynamic,
            }],
        };
        let frames = compile_surface_update(None, &surface, None).expect("frames");

        let frame = frames.last().expect("tool frame");
        assert!(
            frame
                .rendered_text
                .contains("capability: `workspace/write`")
        );
        assert!(frame.rendered_text.contains("`document.format`"));
        assert!(frame.rendered_text.contains("enum \"markdown\" | \"json\""));
        assert!(frame.rendered_text.contains("`document.blocks[].text`"));
        assert!(!frame.rendered_text.contains("Complete JSON Schema"));
        assert!(!frame.rendered_text.contains("\"properties\""));
        let ContextFrameSection::ToolSchemaDelta { added_tools, .. } = &frame.sections[0] else {
            panic!("tool frame should retain its structured ToolSchemaDelta");
        };
        assert_eq!(
            added_tools[0].parameters_schema["properties"]["document"]["properties"]["blocks"]["items"]
                ["properties"]["text"]["minLength"],
            1
        );
        let complete = render_tool_schema_entry(&added_tools[0], ToolSchemaRenderMode::Complete);
        assert!(complete.contains("Complete JSON Schema"));
        assert!(complete.contains("\"properties\""));
        assert!(complete.contains("\"minLength\": 1"));
    }

    #[test]
    fn compaction_rebuild_validation_rejects_noncanonical_occurrence_frames() {
        let compaction_id = CompactionId::new("compact-validation");
        let revision = ContextRevision::new("sha256:validation");
        let summary = super::super::accepted_compaction_summary_frame(
            &compaction_id,
            &revision,
            "accepted summary",
            super::super::CompactionMode::Manual,
            3,
            2,
            None,
            None,
            None,
            42,
        );
        let surface = DashSurface {
            revision: 1,
            digest: "surface-validation".to_owned(),
            instructions: vec![DashSurfaceInstruction {
                key: "identity:validation".to_owned(),
                channel: "identity".to_owned(),
                text: "Validation identity".to_owned(),
                presentation: AgentSurfaceInstructionPresentation::Identity,
            }],
            tools: vec![test_tool("validation_tool", "Validate a rebuild.")],
        };
        let frames =
            compile_compaction_rebuild(None, Some(&surface), &compaction_id, &revision, summary)
                .expect("compile rebuild");
        assert!(validate_compaction_rebuild(
            &frames,
            &compaction_id,
            &revision
        ));

        let mut wrong_occurrence = frames.clone();
        wrong_occurrence[0].id = "compaction:other:revision:0:identity".to_owned();
        assert!(!validate_compaction_rebuild(
            &wrong_occurrence,
            &compaction_id,
            &revision
        ));

        let mut duplicate_id = frames.clone();
        let first_id = duplicate_id[0].id.clone();
        duplicate_id[1].id = first_id;
        assert!(!validate_compaction_rebuild(
            &duplicate_id,
            &compaction_id,
            &revision
        ));

        let mut wrong_order = frames.clone();
        wrong_order.swap(0, 1);
        assert!(!validate_compaction_rebuild(
            &wrong_order,
            &compaction_id,
            &revision
        ));
    }
}
