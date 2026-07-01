//! 动态发现的 memory context 帧。

use agentdash_spi::hooks::{ContextFrame, ContextFrameSection, RuntimeEventSource};
use agentdash_spi::{
    DiscoveredMemorySource, MemoryDiscoveryDiagnostic, MemoryDiscoveryOutput, MemoryIndexStatus,
};

use super::context_frame::{self, ContextFramePayload};

pub(crate) const MEMORY_CONTEXT_FRAME_KIND: &str = "memory_context";

pub(crate) struct MemoryContextFrameInput<'a> {
    pub inventory: &'a MemoryDiscoveryOutput,
}

pub(crate) fn build_memory_context_frame(
    input: &MemoryContextFrameInput<'_>,
) -> Option<ContextFrame> {
    let sources = input
        .inventory
        .clusters
        .iter()
        .flat_map(|cluster| cluster.sources.iter().cloned())
        .collect::<Vec<_>>();
    let diagnostics = input.inventory.diagnostics.clone();
    if sources.is_empty() && diagnostics.is_empty() {
        return None;
    }

    Some(context_frame::build_context_frame(&MemoryContextFrame {
        sources,
        diagnostics,
    }))
}

#[derive(Debug, Clone)]
struct MemoryContextFrame {
    sources: Vec<DiscoveredMemorySource>,
    diagnostics: Vec<MemoryDiscoveryDiagnostic>,
}

impl ContextFramePayload for MemoryContextFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("memory-context-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        MEMORY_CONTEXT_FRAME_KIND
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "prepared_for_connector".to_string()
    }

    fn delivery_channel(&self) -> &'static str {
        "turn_start"
    }

    fn message_role(&self) -> &'static str {
        "user"
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::SystemNotice {
            title: "Memory Context".to_string(),
            summary: "Runtime-discovered memory source inventory and index pointers.".to_string(),
            body: Some(self.rendered_text()),
        }]
    }

    fn rendered_text(&self) -> String {
        render_memory_context(self)
    }
}

fn render_memory_context(frame: &MemoryContextFrame) -> String {
    let mut parts = vec![
        "## Memory Context".to_string(),
        render_policy_section(),
        render_inventory_section(&frame.sources, &frame.diagnostics),
    ];

    if let Some(default_source) = default_memory_source(&frame.sources) {
        parts.push(format!(
            "Default source: `{}`\nDefault index: `{}`",
            default_source.source_uri, default_source.index_uri
        ));
    }

    let index_sections = frame
        .sources
        .iter()
        .filter(|source| source.index_status == MemoryIndexStatus::Present)
        .filter_map(|source| {
            let content = source.bounded_index_content.as_deref()?.trim();
            (!content.is_empty()).then(|| {
                format!(
                    "### Bounded Index: `{}`\n\n```markdown\n{}\n```",
                    source.index_uri, content
                )
            })
        })
        .collect::<Vec<_>>();
    if !index_sections.is_empty() {
        parts.push(index_sections.join("\n\n"));
    }

    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_policy_section() -> String {
    [
        "Policy:",
        "- Treat memory as historical claims; verify code, configuration, paths, and external facts before acting on them.",
        "- Use the listed index to select relevant topic files, then read topic bodies through VFS only when needed.",
        "- If the user says to ignore memory, do not use, cite, compare, or mention memory.",
    ]
    .join("\n")
}

fn render_inventory_section(
    sources: &[DiscoveredMemorySource],
    diagnostics: &[MemoryDiscoveryDiagnostic],
) -> String {
    let mut lines = Vec::new();
    lines.push("Sources:".to_string());
    for source in sources {
        lines.push(format!(
            "- {}: source `{}`, index `{}`, scope {}, status {}, capabilities [{}]",
            source.display_name,
            source.source_uri,
            source.index_uri,
            enum_name(source.scope),
            enum_name(source.index_status),
            source
                .capabilities
                .iter()
                .map(|capability| format!("{capability:?}").to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !diagnostics.is_empty() {
        lines.push("Diagnostics:".to_string());
        for diagnostic in diagnostics {
            let uri = diagnostic
                .uri
                .as_deref()
                .map(|uri| format!(" `{uri}`"))
                .unwrap_or_default();
            lines.push(format!(
                "- {}{}: {}",
                diagnostic.code, uri, diagnostic.message
            ));
        }
    }
    lines.join("\n")
}

fn default_memory_source(sources: &[DiscoveredMemorySource]) -> Option<&DiscoveredMemorySource> {
    sources
        .iter()
        .find(|source| source.source_uri == "agent://")
        .or_else(|| sources.first())
}

fn enum_name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::{
        MemoryDiscoveryCluster, MemorySourceFormat, MemorySourceScope, MemorySourceTrustLevel,
        MountCapability,
    };

    fn source(
        status: MemoryIndexStatus,
        bounded_index_content: Option<&str>,
    ) -> DiscoveredMemorySource {
        DiscoveredMemorySource {
            provider_key: "builtin.project_agent_memory".to_string(),
            source_key: "agent".to_string(),
            display_name: "Agent Memory".to_string(),
            source_uri: "agent://".to_string(),
            index_uri: "agent://MEMORY.md".to_string(),
            mount_id: "agent".to_string(),
            scope: MemorySourceScope::Agent,
            capabilities: vec![MountCapability::Read, MountCapability::Write],
            format: MemorySourceFormat::AgentDash,
            index_status: status,
            trust_level: MemorySourceTrustLevel::FirstParty,
            summary: None,
            bounded_index_content: bounded_index_content.map(ToString::to_string),
        }
    }

    fn output(source: DiscoveredMemorySource) -> MemoryDiscoveryOutput {
        MemoryDiscoveryOutput {
            clusters: vec![MemoryDiscoveryCluster {
                provider_key: "builtin.project_agent_memory".to_string(),
                display_name: "ProjectAgent Memory".to_string(),
                sources: vec![source],
                ..Default::default()
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn bounded_memory_index_appears_in_context_frame() {
        let frame = build_memory_context_frame(&MemoryContextFrameInput {
            inventory: &output(source(
                MemoryIndexStatus::Present,
                Some("- [Project decisions](topics/project.md)"),
            )),
        })
        .expect("memory frame");

        assert_eq!(frame.kind, MEMORY_CONTEXT_FRAME_KIND);
        assert_eq!(frame.delivery_channel, "turn_start");
        assert_eq!(frame.message_role, "user");
        assert_eq!(
            frame.delivery_metadata.delivery_phase,
            agentdash_spi::ContextDeliveryPhase::DiscoveredInventory
        );
        assert_eq!(
            frame.delivery_metadata.cache_policy,
            agentdash_spi::ContextCachePolicy::DiscoveryDigest
        );
        assert_eq!(
            frame.delivery_metadata.model_channel,
            agentdash_spi::ContextModelChannel::Context
        );
        assert!(frame.rendered_text.contains("Default source: `agent://`"));
        assert!(
            frame
                .rendered_text
                .contains("Default index: `agent://MEMORY.md`")
        );
        assert!(frame.rendered_text.contains("## Memory Context"));
        assert!(
            frame
                .rendered_text
                .contains("- [Project decisions](topics/project.md)")
        );
    }

    #[test]
    fn oversized_memory_index_reports_status_without_body() {
        let inventory = MemoryDiscoveryOutput {
            clusters: vec![MemoryDiscoveryCluster {
                provider_key: "builtin.project_agent_memory".to_string(),
                display_name: "ProjectAgent Memory".to_string(),
                sources: vec![source(
                    MemoryIndexStatus::TooLarge,
                    Some("this body must be ignored"),
                )],
                ..Default::default()
            }],
            diagnostics: vec![MemoryDiscoveryDiagnostic {
                provider_key: "builtin.project_agent_memory".to_string(),
                code: "memory_index_too_large".to_string(),
                message: "文件过大".to_string(),
                source_key: Some("agent".to_string()),
                uri: Some("agent://MEMORY.md".to_string()),
            }],
        };

        let frame = build_memory_context_frame(&MemoryContextFrameInput {
            inventory: &inventory,
        })
        .expect("memory frame");

        assert!(frame.rendered_text.contains("status too_large"));
        assert!(frame.rendered_text.contains("memory_index_too_large"));
        assert!(!frame.rendered_text.contains("this body must be ignored"));
    }
}
