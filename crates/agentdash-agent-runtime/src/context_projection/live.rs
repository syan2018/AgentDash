use agentdash_agent_protocol::{
    ContextDeliveryChannel, ContextDeliveryMetadata, ContextDeliveryStatus, ContextFrame,
    ContextFrameKind, ContextFrameSection, ContextFrameSource, ContextMessageRole,
};

use super::{
    ContextFrameFacts, ContextProjectionIdentity, dimension,
    surface_state::{NormalizedContextSurfaceDelta, NormalizedContextSurfaceState},
};

/// Projects the complete initial capability surface from an empty Runtime state.
///
/// Assignment and stable bootstrap families remain separate facts so the compiler can retain the
/// main insertion order: capability, assignment, then stable bootstrap context.
#[must_use]
pub fn project_initial_surface(
    target: &NormalizedContextSurfaceState,
) -> Option<ContextFrameFacts> {
    let previous = NormalizedContextSurfaceState::default();
    let delta = NormalizedContextSurfaceDelta::between(&previous, target);
    Some(
        project_capability_surface_facts(&delta, &previous, target, Some("bootstrap"), "initial")
            .unwrap_or_else(|| empty_capability_surface_facts("bootstrap", "initial")),
    )
}

/// Projects one accepted previous/target surface pair into the main-equivalent live frame order.
///
/// The result contains at most one capability frame followed by an independent assignment frame.
/// The caller attaches canonical Runtime coordinates and commits the batch with `SurfaceAdopt`.
#[must_use]
pub fn project_live_surface_transition<'a>(
    previous: &NormalizedContextSurfaceState,
    target: &NormalizedContextSurfaceState,
    identity: &ContextProjectionIdentity,
    phase_node: impl Into<Option<&'a str>>,
    apply_mode: &str,
) -> Vec<ContextFrame> {
    let phase_node = phase_node.into();
    let delta = NormalizedContextSurfaceDelta::between(previous, target);
    if delta.is_empty() {
        return Vec::new();
    }
    let mut frames = Vec::with_capacity(2);
    if let Some(facts) =
        project_capability_surface_facts(&delta, previous, target, phase_node, apply_mode)
    {
        frames.push(ContextFrame {
            id: format!("runtime-context-{}-capability", identity.operation_id),
            kind: facts.kind,
            source: facts.source,
            phase_node: facts.phase_node,
            apply_mode: facts.apply_mode,
            delivery_status: facts.delivery_status,
            delivery_channel: facts.delivery_channel,
            message_role: facts.message_role,
            delivery_metadata: ContextDeliveryMetadata::for_frame(
                facts.kind,
                facts.delivery_channel,
                facts.message_role,
            ),
            rendered_text: facts.rendered_text,
            sections: facts.sections,
            created_at_ms: identity.recorded_at_ms,
        });
    }
    if delta.assignment_changed
        && let Some(assignment) = target.assignment.as_ref()
        && !assignment.fragments.is_empty()
    {
        let kind = ContextFrameKind::AssignmentContext;
        let delivery_channel = ContextDeliveryChannel::TurnStart;
        let message_role = ContextMessageRole::User;
        frames.push(ContextFrame {
            id: format!("runtime-context-{}-assignment", identity.operation_id),
            kind,
            source: ContextFrameSource::RuntimeContextUpdate,
            phase_node: phase_node.map(str::to_string),
            apply_mode: Some(apply_mode.to_string()),
            delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
            delivery_channel,
            message_role,
            delivery_metadata: ContextDeliveryMetadata::for_frame(
                kind,
                delivery_channel,
                message_role,
            ),
            rendered_text: render_assignment_context_text(&assignment.fragments),
            sections: vec![ContextFrameSection::AssignmentContext {
                title: "Assignment Context".to_string(),
                summary: format!(
                    "当前任务上下文已注入，本 frame 汇聚 {} 个上下文片段。",
                    assignment.fragments.len()
                ),
                fragments: assignment.fragments.clone(),
            }],
            created_at_ms: identity.recorded_at_ms,
        });
    }
    frames
}

fn project_capability_surface_facts(
    delta: &NormalizedContextSurfaceDelta,
    previous: &NormalizedContextSurfaceState,
    target: &NormalizedContextSurfaceState,
    phase_node: Option<&str>,
    apply_mode: &str,
) -> Option<ContextFrameFacts> {
    if delta.capability_dimensions_are_empty() {
        return None;
    }
    let dimensions = dimension::project_all(delta, previous, target, phase_node);
    debug_assert!(
        !dimensions.is_empty(),
        "every presentation-relevant surface delta must project at least one dimension"
    );
    let sections = dimensions
        .iter()
        .map(|dimension| dimension.section.clone())
        .collect();
    let rendered_text = dimensions
        .into_iter()
        .map(|dimension| dimension.rendered_text)
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(ContextFrameFacts {
        kind: ContextFrameKind::CapabilityStateDelta,
        source: ContextFrameSource::RuntimeContextUpdate,
        phase_node: phase_node.map(str::to_string),
        apply_mode: Some(apply_mode.to_string()),
        delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
        delivery_channel: ContextDeliveryChannel::TurnStart,
        message_role: ContextMessageRole::User,
        rendered_text,
        sections,
    })
}

fn empty_capability_surface_facts(phase_node: &str, apply_mode: &str) -> ContextFrameFacts {
    ContextFrameFacts {
        kind: ContextFrameKind::CapabilityStateDelta,
        source: ContextFrameSource::RuntimeContextUpdate,
        phase_node: Some(phase_node.to_string()),
        apply_mode: Some(apply_mode.to_string()),
        delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
        delivery_channel: ContextDeliveryChannel::TurnStart,
        message_role: ContextMessageRole::User,
        rendered_text: String::new(),
        sections: Vec::new(),
    }
}

fn render_assignment_context_text(
    fragments: &[agentdash_agent_protocol::RuntimeContextFragmentEntry],
) -> String {
    let mut lines = vec![
        "# Assignment Context".to_string(),
        "以下上下文片段已在本轮对话开始前注入，用于约束任务目标、工作流要求与项目规则。"
            .to_string(),
    ];
    for fragment in fragments {
        let label = if fragment.label.trim().is_empty() {
            fragment.slot.as_str()
        } else {
            fragment.label.as_str()
        };
        lines.push(format!(
            "## {} (`{}`)\nsource: `{}`\n\n{}",
            label,
            fragment.slot,
            fragment.source,
            demote_markdown_headings(fragment.content.trim())
        ));
    }
    lines.join("\n\n")
}

fn demote_markdown_headings(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            let heading_marks = line
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if (1..6).contains(&heading_marks) && line.chars().nth(heading_marks) == Some(' ') {
                format!("#{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_protocol::{
        ContextFrameKind, ContextFrameSection, RuntimeCompanionAgentEntry,
        RuntimeContextFragmentEntry, RuntimeMemorySourceEntry, RuntimeSkillEntry,
        RuntimeToolSchemaEntry, SkillContextExposure,
    };

    use super::*;
    use crate::context_projection::surface_state::{
        NormalizedAssignmentContext, NormalizedMcpServerReadiness, NormalizedSkillCluster,
        NormalizedSurfaceEntity,
    };

    fn identity() -> ContextProjectionIdentity {
        ContextProjectionIdentity {
            operation_id: "surface-adopt".to_string(),
            source_frame_id: "frame-2".to_string(),
            source_frame_revision: 2,
            recorded_at_ms: 1_726_000_000_000,
        }
    }

    fn entity(fingerprint: &str) -> NormalizedSurfaceEntity {
        NormalizedSurfaceEntity {
            fingerprint: fingerprint.to_string(),
        }
    }

    fn tool(name: &str, capability: &str, path: &str) -> RuntimeToolSchemaEntry {
        RuntimeToolSchemaEntry {
            name: name.to_string(),
            description: format!("{name} description"),
            parameters_schema: serde_json::json!({"type": "object"}),
            capability_key: Some(capability.to_string()),
            source: Some("agent_frame".to_string()),
            tool_path: Some(path.to_string()),
            context_usage_kind: Some("system_tools".to_string()),
        }
    }

    fn memory(key: &str, revision: &str) -> RuntimeMemorySourceEntry {
        RuntimeMemorySourceEntry {
            provider_key: "project".to_string(),
            source_key: key.to_string(),
            display_name: key.to_string(),
            source_uri: format!("agentdash://memory/{key}"),
            index_uri: format!("agentdash://memory/{key}/index"),
            mount_id: "workspace".to_string(),
            scope: "project".to_string(),
            index_status: "ready".to_string(),
            trust_level: "trusted".to_string(),
            revision: revision.to_string(),
            summary: None,
            context_usage_kind: Some("memory".to_string()),
        }
    }

    fn skill(name: &str, description: &str) -> RuntimeSkillEntry {
        RuntimeSkillEntry {
            name: name.to_string(),
            capability_key: format!("skill:{name}"),
            provider_key: "builtin".to_string(),
            local_name: name.to_string(),
            display_name: None,
            description: description.to_string(),
            file_path: format!("skills/{name}/SKILL.md"),
            base_dir: None,
            exposure: SkillContextExposure::DefaultExposed,
            disable_model_invocation: false,
            context_usage_kind: Some("skills".to_string()),
        }
    }

    #[test]
    fn all_live_dimensions_precede_independent_assignment() {
        let previous = NormalizedContextSurfaceState {
            capability_keys: ["file_read".to_string()].into(),
            excluded_tool_paths: ["file_read::fs_grep".to_string()].into(),
            mcp_servers: BTreeMap::from([("server-a".to_string(), entity("old"))]),
            companion_agents: BTreeMap::from([(
                "reviewer".to_string(),
                RuntimeCompanionAgentEntry {
                    agent_key: "reviewer".to_string(),
                    executor: "native".to_string(),
                    display_name: "Reviewer".to_string(),
                    context_usage_kind: Some("agents".to_string()),
                },
            )]),
            vfs_mounts: BTreeMap::from([("workspace".to_string(), entity("old"))]),
            default_vfs_mount: Some("workspace".to_string()),
            memory_sources: BTreeMap::from([("project:old".to_string(), memory("old", "1"))]),
            skills: BTreeMap::from([("skill:review".to_string(), skill("review", "old"))]),
            ..Default::default()
        };
        let mut target = previous.clone();
        target
            .capability_keys
            .extend(["collaboration".to_string(), "file_write".to_string()]);
        target.excluded_tool_paths.clear();
        target
            .included_tool_paths
            .insert("file_write::fs_apply_patch".to_string());
        target
            .mcp_servers
            .insert("server-a".to_string(), entity("new"));
        target
            .mcp_servers
            .insert("server-b".to_string(), entity("new"));
        target.unavailable_mcp_servers = vec![NormalizedMcpServerReadiness {
            name: "server-b".to_string(),
            reason_code: "offline".to_string(),
            message: "connection refused".to_string(),
        }];
        target.companion_agents.insert(
            "builder".to_string(),
            RuntimeCompanionAgentEntry {
                agent_key: "builder".to_string(),
                executor: "native".to_string(),
                display_name: "Builder".to_string(),
                context_usage_kind: Some("agents".to_string()),
            },
        );
        target.companion_agent_order = vec!["builder".to_string(), "reviewer".to_string()];
        target.vfs_mounts.remove("workspace");
        target
            .vfs_mounts
            .insert("project".to_string(), entity("new"));
        target.default_vfs_mount = Some("project".to_string());
        target.memory_sources.remove("project:old");
        target
            .memory_sources
            .insert("project:new".to_string(), memory("new", "2"));
        target.memory_source_order = vec!["project:new".to_string()];
        target
            .skills
            .insert("skill:review".to_string(), skill("review", "new"));
        target
            .skills
            .insert("skill:test".to_string(), skill("test", "tests"));
        target.skill_clusters = vec![NormalizedSkillCluster {
            provider_key: "builtin".to_string(),
            display_name: "Builtin".to_string(),
            model_summary: Some("Project skills".to_string()),
        }];
        target.tool_schemas = BTreeMap::from([(
            "fs_apply_patch".to_string(),
            tool("fs_apply_patch", "file_write", "file_write::fs_apply_patch"),
        )]);
        target.assignment = Some(NormalizedAssignmentContext {
            fragments: vec![RuntimeContextFragmentEntry {
                slot: "task".to_string(),
                label: "Task".to_string(),
                source: "workflow".to_string(),
                content: "# Implement\nShip it".to_string(),
                context_usage_kind: Some("system_developer".to_string()),
            }],
        });

        let frames = project_live_surface_transition(
            &previous,
            &target,
            &identity(),
            "implementation",
            "live",
        );
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].kind, ContextFrameKind::CapabilityStateDelta);
        assert_eq!(frames[1].kind, ContextFrameKind::AssignmentContext);
        assert!(matches!(
            frames[0].sections.as_slice(),
            [
                ContextFrameSection::CapabilityKeyDelta { .. },
                ContextFrameSection::ToolPathDelta { .. },
                ContextFrameSection::McpServerDelta { .. },
                ContextFrameSection::CompanionAgentRosterDelta { .. },
                ContextFrameSection::VfsDelta { .. },
                ContextFrameSection::MemoryInventory { .. },
                ContextFrameSection::SkillDelta { .. },
                ContextFrameSection::ToolSchemaDelta { .. }
            ]
        ));
        assert!(frames[1].rendered_text.contains("## Task (`task`)"));
        assert!(frames[1].rendered_text.contains("## Implement"));
    }

    #[test]
    fn assignment_only_and_empty_transitions_have_distinct_results() {
        let previous = NormalizedContextSurfaceState::default();
        let mut target = previous.clone();
        target.assignment = Some(NormalizedAssignmentContext {
            fragments: vec![RuntimeContextFragmentEntry {
                slot: "task".to_string(),
                label: String::new(),
                source: "workflow".to_string(),
                content: "Continue".to_string(),
                context_usage_kind: None,
            }],
        });
        let frames =
            project_live_surface_transition(&previous, &target, &identity(), "assignment", "live");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, ContextFrameKind::AssignmentContext);
        assert!(
            project_live_surface_transition(&target, &target, &identity(), "assignment", "live",)
                .is_empty()
        );
    }

    #[test]
    fn isolated_dimensions_preserve_main_add_remove_change_semantics() {
        let sections = |previous: &NormalizedContextSurfaceState,
                        target: &NormalizedContextSurfaceState| {
            project_live_surface_transition(previous, target, &identity(), "apply", "live")
                .into_iter()
                .find(|frame| frame.kind == ContextFrameKind::CapabilityStateDelta)
                .expect("presentation-relevant delta must emit a capability frame")
                .sections
        };

        let previous = NormalizedContextSurfaceState {
            capability_keys: ["removed".to_string()].into(),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            capability_keys: ["added".to_string()].into(),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::CapabilityKeyDelta {
                added_capabilities,
                removed_capabilities,
                ..
            }] if added_capabilities == &["added"] && removed_capabilities == &["removed"]
        ));

        let previous = NormalizedContextSurfaceState {
            excluded_tool_paths: ["blocked-old".to_string()].into(),
            included_tool_paths: ["allowed-old".to_string()].into(),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            excluded_tool_paths: ["blocked-new".to_string()].into(),
            included_tool_paths: ["allowed-new".to_string()].into(),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::ToolPathDelta {
                blocked_tool_paths,
                unblocked_tool_paths,
                whitelisted_tool_paths,
                removed_whitelist_paths,
            }] if blocked_tool_paths == &["blocked-new"]
                && unblocked_tool_paths == &["blocked-old"]
                && whitelisted_tool_paths == &["allowed-new"]
                && removed_whitelist_paths == &["allowed-old"]
        ));

        let previous = NormalizedContextSurfaceState {
            mcp_servers: BTreeMap::from([
                ("changed".to_string(), entity("old")),
                ("removed".to_string(), entity("old")),
            ]),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            mcp_servers: BTreeMap::from([
                ("added".to_string(), entity("new")),
                ("changed".to_string(), entity("new")),
            ]),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::McpServerDelta {
                added_mcp_servers,
                removed_mcp_servers,
                changed_mcp_servers,
            }] if added_mcp_servers == &["added"]
                && removed_mcp_servers == &["removed"]
                && changed_mcp_servers == &["changed"]
        ));

        let companion = |key: &str, display_name: &str| RuntimeCompanionAgentEntry {
            agent_key: key.to_string(),
            executor: "native".to_string(),
            display_name: display_name.to_string(),
            context_usage_kind: Some("agents".to_string()),
        };
        let previous = NormalizedContextSurfaceState {
            companion_agents: BTreeMap::from([
                ("changed".to_string(), companion("changed", "Old")),
                ("removed".to_string(), companion("removed", "Removed")),
            ]),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            companion_agents: BTreeMap::from([
                ("added".to_string(), companion("added", "Added")),
                ("changed".to_string(), companion("changed", "New")),
            ]),
            companion_agent_order: vec!["changed".to_string(), "added".to_string()],
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::CompanionAgentRosterDelta {
                added_agents,
                removed_agent_keys,
                changed_agents,
                effective_agents,
            }] if added_agents[0].agent_key == "added"
                && removed_agent_keys == &["removed"]
                && changed_agents[0].display_name == "New"
                && effective_agents.iter().map(|agent| agent.agent_key.as_str()).collect::<Vec<_>>() == ["changed", "added"]
        ));

        let previous = NormalizedContextSurfaceState {
            vfs_mounts: BTreeMap::from([("removed".to_string(), entity("old"))]),
            default_vfs_mount: Some("removed".to_string()),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            vfs_mounts: BTreeMap::from([("added".to_string(), entity("new"))]),
            default_vfs_mount: Some("added".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::VfsDelta {
                vfs_mounts_added,
                vfs_mounts_removed,
                default_mount_before,
                default_mount_after,
            }] if vfs_mounts_added == &["added"]
                && vfs_mounts_removed == &["removed"]
                && default_mount_before.as_deref() == Some("removed")
                && default_mount_after.as_deref() == Some("added")
        ));

        let previous = NormalizedContextSurfaceState {
            memory_sources: BTreeMap::from([
                ("project:changed".to_string(), memory("changed", "1")),
                ("project:removed".to_string(), memory("removed", "1")),
            ]),
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            memory_sources: BTreeMap::from([
                ("project:added".to_string(), memory("added", "1")),
                ("project:changed".to_string(), memory("changed", "2")),
            ]),
            memory_source_order: vec!["project:changed".to_string(), "project:added".to_string()],
            ..Default::default()
        };
        let memory_section = sections(&previous, &target)
            .into_iter()
            .find(|section| matches!(section, ContextFrameSection::MemoryInventory { .. }))
            .expect("memory delta");
        assert!(matches!(
            memory_section,
            ContextFrameSection::MemoryInventory {
                added_sources,
                removed_sources,
                changed_sources,
                sources,
                ..
            } if added_sources[0].source_key == "added"
                && removed_sources[0].source_key == "removed"
                && changed_sources[0].revision == "2"
                && sources.iter().map(|source| source.source_key.as_str()).collect::<Vec<_>>() == ["changed", "added"]
        ));

        let previous = NormalizedContextSurfaceState {
            skills: BTreeMap::from([
                ("skill:changed".to_string(), skill("changed", "old")),
                ("skill:removed".to_string(), skill("removed", "old")),
            ]),
            skill_clusters: vec![NormalizedSkillCluster {
                provider_key: "builtin".to_string(),
                display_name: "Builtin".to_string(),
                model_summary: None,
            }],
            ..Default::default()
        };
        let target = NormalizedContextSurfaceState {
            skills: BTreeMap::from([
                ("skill:added".to_string(), skill("added", "new")),
                ("skill:changed".to_string(), skill("changed", "new")),
            ]),
            skill_clusters: previous.skill_clusters.clone(),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [ContextFrameSection::SkillDelta {
                added_skills,
                removed_skills,
                changed_skills,
                ..
            }] if added_skills[0].name == "added"
                && removed_skills[0].capability_key == "skill:removed"
                && changed_skills[0].description == "new"
        ));

        let previous = NormalizedContextSurfaceState::default();
        let target = NormalizedContextSurfaceState {
            capability_keys: ["file_write".to_string()].into(),
            tool_schemas: BTreeMap::from([(
                "apply_patch".to_string(),
                tool("apply_patch", "file_write", "file_write::apply_patch"),
            )]),
            ..Default::default()
        };
        assert!(matches!(
            sections(&previous, &target).as_slice(),
            [
                ContextFrameSection::CapabilityKeyDelta { .. },
                ContextFrameSection::ToolSchemaDelta { added_tools }
            ] if added_tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>() == ["apply_patch"]
        ));
    }

    #[test]
    fn persistent_mcp_readiness_does_not_turn_noop_or_assignment_only_into_capability_delta() {
        let readiness = NormalizedMcpServerReadiness {
            name: "offline".to_string(),
            reason_code: "connection_failed".to_string(),
            message: "connection refused".to_string(),
        };
        let previous = NormalizedContextSurfaceState {
            unavailable_mcp_servers: vec![readiness.clone()],
            ..Default::default()
        };
        assert!(
            project_live_surface_transition(&previous, &previous, &identity(), "apply", "live")
                .is_empty()
        );
        let mut removed = previous.clone();
        removed.tool_schemas.clear();
        assert!(
            project_live_surface_transition(&previous, &removed, &identity(), "apply", "live")
                .is_empty()
        );

        let mut target = previous.clone();
        target.assignment = Some(NormalizedAssignmentContext {
            fragments: vec![RuntimeContextFragmentEntry {
                slot: "task".to_string(),
                label: "Task".to_string(),
                source: "workflow".to_string(),
                content: "Continue".to_string(),
                context_usage_kind: Some("system_developer".to_string()),
            }],
        });
        let frames =
            project_live_surface_transition(&previous, &target, &identity(), "apply", "live");
        assert!(
            matches!(frames.as_slice(), [frame] if frame.kind == ContextFrameKind::AssignmentContext)
        );

        let mut readiness_changed = previous.clone();
        readiness_changed.unavailable_mcp_servers[0].message = "timeout".to_string();
        assert!(matches!(
            project_live_surface_transition(
                &previous,
                &readiness_changed,
                &identity(),
                "apply",
                "live"
            )[0]
            .sections
            .as_slice(),
            [ContextFrameSection::McpServerDelta { .. }]
        ));
    }

    #[test]
    fn pure_tool_schema_add_or_change_projects_live_but_noop_does_not() {
        let previous = NormalizedContextSurfaceState {
            capability_keys: ["file_read".to_string()].into(),
            tool_schemas: BTreeMap::from([(
                "read".to_string(),
                tool("read", "file_read", "file_read::read"),
            )]),
            ..Default::default()
        };
        let mut changed = previous.clone();
        changed.tool_schemas.get_mut("read").unwrap().description = "changed".to_string();
        let frames =
            project_live_surface_transition(&previous, &changed, &identity(), "apply", "live");
        assert!(matches!(
            frames[0].sections.as_slice(),
            [ContextFrameSection::ToolSchemaDelta { added_tools }]
                if added_tools[0].description == "changed"
        ));

        let mut added = previous.clone();
        added.tool_schemas.insert(
            "search".to_string(),
            tool("search", "file_read", "file_read::search"),
        );
        assert!(matches!(
            project_live_surface_transition(
                &previous,
                &added,
                &identity(),
                "apply",
                "live"
            )[0]
            .sections
            .as_slice(),
            [ContextFrameSection::ToolSchemaDelta { added_tools }]
                if added_tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>() == ["search"]
        ));
        assert!(
            project_live_surface_transition(&previous, &previous, &identity(), "apply", "live")
                .is_empty()
        );
    }

    #[test]
    fn initial_surface_projects_all_dimensions_with_exact_bootstrap_metadata() {
        let target = NormalizedContextSurfaceState {
            capability_keys: ["collaboration".to_string(), "file_read".to_string()].into(),
            excluded_tool_paths: ["file_read::blocked".to_string()].into(),
            included_tool_paths: ["file_read::read".to_string()].into(),
            mcp_servers: BTreeMap::from([("server".to_string(), entity("ready"))]),
            companion_agents: BTreeMap::from([(
                "reviewer".to_string(),
                RuntimeCompanionAgentEntry {
                    agent_key: "reviewer".to_string(),
                    executor: "native".to_string(),
                    display_name: "Reviewer".to_string(),
                    context_usage_kind: Some("agents".to_string()),
                },
            )]),
            companion_agent_order: vec!["reviewer".to_string()],
            vfs_mounts: BTreeMap::from([("workspace".to_string(), entity("mounted"))]),
            default_vfs_mount: Some("workspace".to_string()),
            memory_sources: BTreeMap::from([("project:memory".to_string(), memory("memory", "1"))]),
            memory_source_order: vec!["project:memory".to_string()],
            skills: BTreeMap::from([(
                "skill:review".to_string(),
                skill("review", "Review changes"),
            )]),
            skill_clusters: vec![NormalizedSkillCluster {
                provider_key: "builtin".to_string(),
                display_name: "Builtin".to_string(),
                model_summary: None,
            }],
            tool_schemas: BTreeMap::from([(
                "read".to_string(),
                tool("read", "file_read", "file_read::read"),
            )]),
            ..Default::default()
        };
        let facts = project_initial_surface(&target).expect("full initial capability snapshot");
        assert_eq!(facts.phase_node.as_deref(), Some("bootstrap"));
        assert_eq!(facts.apply_mode.as_deref(), Some("initial"));
        assert_eq!(
            facts.delivery_status,
            ContextDeliveryStatus::QueuedForTransformContext
        );
        assert_eq!(facts.delivery_channel, ContextDeliveryChannel::TurnStart);
        assert_eq!(facts.message_role, ContextMessageRole::User);
        assert!(matches!(
            facts.sections.as_slice(),
            [
                ContextFrameSection::CapabilityKeyDelta { .. },
                ContextFrameSection::ToolPathDelta { .. },
                ContextFrameSection::McpServerDelta { .. },
                ContextFrameSection::CompanionAgentRosterDelta { .. },
                ContextFrameSection::VfsDelta { .. },
                ContextFrameSection::MemoryInventory { .. },
                ContextFrameSection::SkillDelta { .. },
                ContextFrameSection::ToolSchemaDelta { .. }
            ]
        ));
    }

    #[test]
    fn initial_collaboration_snapshot_includes_empty_companion_roster() {
        let target = NormalizedContextSurfaceState {
            capability_keys: ["collaboration".to_string()].into(),
            ..Default::default()
        };
        let facts = project_initial_surface(&target).expect("initial collaboration snapshot");
        let effective = facts
            .sections
            .iter()
            .find_map(|section| match section {
                ContextFrameSection::CompanionAgentRosterDelta {
                    effective_agents, ..
                } => Some(effective_agents),
                _ => None,
            })
            .expect("collaboration snapshot must expose the effective roster");
        assert!(effective.is_empty());
    }

    #[test]
    fn initial_empty_surface_still_emits_exact_empty_capability_frame() {
        let facts = project_initial_surface(&NormalizedContextSurfaceState::default())
            .expect("initial capability frame is unconditional");
        assert_eq!(facts.kind, ContextFrameKind::CapabilityStateDelta);
        assert_eq!(facts.phase_node.as_deref(), Some("bootstrap"));
        assert_eq!(facts.apply_mode.as_deref(), Some("initial"));
        assert_eq!(
            facts.delivery_status,
            ContextDeliveryStatus::QueuedForTransformContext
        );
        assert_eq!(facts.delivery_channel, ContextDeliveryChannel::TurnStart);
        assert_eq!(facts.message_role, ContextMessageRole::User);
        assert!(facts.rendered_text.is_empty());
        assert!(facts.sections.is_empty());
    }
}
