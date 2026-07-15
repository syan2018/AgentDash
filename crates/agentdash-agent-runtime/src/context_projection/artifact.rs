use agentdash_agent_protocol::ContextFrame;
use agentdash_agent_runtime_contract::{
    ImmutablePresentationEvent, PresentationDurability, PresentationThreadId, PresentationTurnId,
    RuntimePresentationCoordinate, RuntimePresentationInput,
};
use serde::{Deserialize, Serialize};

/// Immutable presentation half of a compiled Agent Surface artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSurfacePresentationPlan {
    pub digest: String,
    pub source_frame_id: String,
    pub source_frame_revision: u64,
    pub transition_phase_node: Option<String>,
    pub bootstrap_frames: Vec<ContextFrame>,
    pub adoption_frames: Vec<ContextFrame>,
}

impl RuntimeSurfacePresentationPlan {
    pub fn for_adoption(
        previous: &crate::AgentSurfaceSnapshot,
        target: &crate::CompiledBusinessAgentSurface,
    ) -> Result<Self, RuntimeSurfacePresentationPlanError> {
        let delta = super::NormalizedContextSurfaceDelta::between(
            &previous.normalized_context_surface,
            &target.snapshot.normalized_context_surface,
        );
        let adoption_frames = if delta.is_empty() {
            Vec::new()
        } else {
            let phase_node = target
                .presentation
                .transition_phase_node
                .as_deref()
                .ok_or(RuntimeSurfacePresentationPlanError::MissingTransitionPhase)?;
            let recorded_at_ms = target
                .presentation
                .bootstrap_frames
                .first()
                .map_or(0, |frame| frame.created_at_ms);
            super::project_live_surface_transition(
                &previous.normalized_context_surface,
                &target.snapshot.normalized_context_surface,
                &super::ContextProjectionIdentity {
                    operation_id: format!(
                        "surface-adopt-{}-{}",
                        target.presentation.source_frame_id,
                        target.presentation.source_frame_revision
                    ),
                    source_frame_id: target.presentation.source_frame_id.clone(),
                    source_frame_revision: target.presentation.source_frame_revision,
                    recorded_at_ms,
                },
                phase_node,
                "live",
            )
        };
        let digest = presentation_digest(&adoption_frames);
        Ok(Self {
            digest,
            source_frame_id: target.presentation.source_frame_id.clone(),
            source_frame_revision: target.presentation.source_frame_revision,
            transition_phase_node: target.presentation.transition_phase_node.clone(),
            bootstrap_frames: Vec::new(),
            adoption_frames,
        })
    }

    /// Materializes the adoption half of this compiled plan into canonical Runtime presentation.
    ///
    /// Keeping this mapping beside the immutable plan ensures production adoption and replay tests
    /// cannot bypass the same frame ordering and coordinate assignment used by the Runtime UoW.
    #[must_use]
    pub fn adoption_presentation(
        &self,
        thread_id: &PresentationThreadId,
        turn_id: Option<&PresentationTurnId>,
        operation_id: &str,
    ) -> Vec<RuntimePresentationInput> {
        self.adoption_frames
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, frame)| RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some(thread_id.to_string()),
                    source_turn_id: turn_id.map(ToString::to_string),
                    source_item_id: None,
                    source_request_id: Some(operation_id.to_string()),
                    source_entry_index: Some(
                        u32::try_from(index).expect("surface presentation plan is bounded"),
                    ),
                },
                event: ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(Box::new(
                            agentdash_agent_protocol::ContextFrameChanged { frame },
                        )),
                    ),
                ),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSurfacePresentationPlanError {
    #[error("changed surface adoption requires workflow transition phase provenance")]
    MissingTransitionPhase,
}

pub(super) fn render_tool_schema_delta(
    phase_node: &str,
    tools: &[agentdash_agent_protocol::RuntimeToolSchemaEntry],
) -> String {
    let mut sections = vec![
        format!("## Tool Schema Delta — Step Transition: {phase_node}"),
        "以下只列出本次 capability state delta 真正新增给 Agent 的工具 schema；provider 的完整工具集合以实际 tool list 为准。"
            .to_string(),
        "### Added / Restored Tool Schemas".to_string(),
    ];
    sections.extend(tools.iter().map(render_tool_schema_entry));
    sections.join("\n\n")
}

fn presentation_digest(frames: &[ContextFrame]) -> String {
    use sha2::{Digest, Sha256};

    let encoded = serde_json::to_vec(frames).expect("ContextFrame is serializable");
    format!("sha256:{:x}", Sha256::digest(encoded))
}

fn render_tool_schema_entry(tool: &agentdash_agent_protocol::RuntimeToolSchemaEntry) -> String {
    let mut sections = vec![format!("### `{}`", tool.name)];
    let mut metadata = Vec::new();
    if let Some(value) = tool.capability_key.as_deref() {
        metadata.push(format!("capability: `{value}`"));
    }
    if let Some(value) = tool.source.as_deref() {
        metadata.push(format!("source: `{value}`"));
    }
    if let Some(value) = tool.tool_path.as_deref() {
        metadata.push(format!("path: `{value}`"));
    }
    if !metadata.is_empty() {
        sections.push(metadata.join("；"));
    }
    if !tool.description.trim().is_empty() {
        sections.push(tool.description.trim().to_string());
    }
    sections.push("参数说明：".to_string());
    sections.extend(render_parameter_summary(&tool.parameters_schema));
    sections.join("\n\n")
}

fn render_parameter_summary(schema: &serde_json::Value) -> Vec<String> {
    const MAX_FIELDS: usize = 48;
    const MAX_DEPTH: usize = 2;

    let mut lines = Vec::new();
    collect_schema_fields(schema, "", 0, MAX_DEPTH, MAX_FIELDS, &mut lines, &mut false);
    if lines.is_empty() {
        lines.push("- 无参数。".to_string());
    }
    lines
}

fn collect_schema_fields(
    schema: &serde_json::Value,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    max_fields: usize,
    lines: &mut Vec<String>,
    truncated: &mut bool,
) {
    if lines.len() >= max_fields {
        append_schema_truncation(lines, truncated);
        return;
    }
    let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    else {
        if prefix.is_empty() {
            lines.push(format!("- 参数整体类型：{}", schema_type_summary(schema)));
        }
        return;
    };
    let required = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut names = properties.keys().collect::<Vec<_>>();
    names.sort();
    for name in names {
        if lines.len() >= max_fields {
            append_schema_truncation(lines, truncated);
            return;
        }
        let field_schema = &properties[name];
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        let requirement = if required.contains(name.as_str()) {
            "required"
        } else {
            "optional"
        };
        let description = schema_description(field_schema);
        let suffix = if description.is_empty() {
            String::new()
        } else {
            format!(": {description}")
        };
        lines.push(format!(
            "- `{path}` ({requirement}, {}){suffix}",
            schema_type_summary(field_schema)
        ));
        if depth >= max_depth {
            continue;
        }
        if field_schema.get("properties").is_some() {
            collect_schema_fields(
                field_schema,
                &path,
                depth + 1,
                max_depth,
                max_fields,
                lines,
                truncated,
            );
        } else if let Some(items) = field_schema.get("items")
            && items.get("properties").is_some()
        {
            collect_schema_fields(
                items,
                &format!("{path}[]"),
                depth + 1,
                max_depth,
                max_fields,
                lines,
                truncated,
            );
        }
    }
}

fn append_schema_truncation(lines: &mut Vec<String>, truncated: &mut bool) {
    if !*truncated {
        lines.push(
            "- 其余嵌套字段已省略；完整机器 schema 已通过 provider tools 字段提供。".to_string(),
        );
        *truncated = true;
    }
}

fn schema_description(schema: &serde_json::Value) -> String {
    schema
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            const MAX_DESCRIPTION_CHARS: usize = 140;
            let mut output = value
                .split('\n')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if output.chars().count() > MAX_DESCRIPTION_CHARS {
                output = output.chars().take(MAX_DESCRIPTION_CHARS).collect();
                output.push_str("...");
            }
            output
        })
        .unwrap_or_default()
}

fn schema_type_summary(schema: &serde_json::Value) -> String {
    if let Some(any_of) = schema.get("anyOf").and_then(serde_json::Value::as_array) {
        let mut variants = any_of.iter().map(schema_type_summary).collect::<Vec<_>>();
        variants.sort();
        variants.dedup();
        return variants.join(" | ");
    }
    let Some(schema_type) = schema.get("type") else {
        if schema.get("properties").is_some() {
            return "object".to_string();
        }
        if schema.get("items").is_some() {
            return "array".to_string();
        }
        if let Some(values) = schema.get("enum").and_then(serde_json::Value::as_array) {
            return format!("enum{}", enum_values_summary(values));
        }
        return "any".to_string();
    };
    match schema_type {
        serde_json::Value::String(value) if value == "array" => {
            let item = schema
                .get("items")
                .map(schema_type_summary)
                .unwrap_or_else(|| "any".to_string());
            format!("array<{item}>")
        }
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ => "any".to_string(),
    }
}

fn enum_values_summary(values: &[serde_json::Value]) -> String {
    let items = values
        .iter()
        .map(|value| match value {
            serde_json::Value::String(text) => text.clone(),
            _ => value.to_string(),
        })
        .take(6)
        .collect::<Vec<_>>();
    format!("({})", items.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::render_parameter_summary;

    #[test]
    fn tool_schema_parameter_summary_matches_main_golden_matrix() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/wi03_tool_schema_formatter_main_957fa9d.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let actual = render_parameter_summary(&case["schema"]);
            let expected = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "case {}", case["name"]);
        }

        let mut properties = serde_json::Map::new();
        for index in 0..49 {
            properties.insert(
                format!("field_{index:02}"),
                serde_json::json!({"type":"string"}),
            );
        }
        let actual = render_parameter_summary(&serde_json::json!({
            "type": "object",
            "properties": properties
        }));
        assert_eq!(actual.len(), 49);
        assert_eq!(
            actual.last().unwrap(),
            fixture["truncation_expected"].as_str().unwrap()
        );
    }
}
