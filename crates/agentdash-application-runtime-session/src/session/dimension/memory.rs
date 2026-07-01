//! Memory 维度 — 追踪 memory source inventory 的增删与变更。

use std::collections::BTreeMap;

use agentdash_spi::hooks::{
    ContextFrameSection, RuntimeMemoryDiagnosticEntry, RuntimeMemoryInventoryMode,
    RuntimeMemorySourceEntry,
};
use agentdash_spi::{CapabilityStateDelta, DiscoveredMemorySource, MemoryDiscoveryOutput};

use super::DimensionDelta;
use crate::session::memory_inventory_entries::{
    flatten_memory_sources, memory_source_key, runtime_memory_diagnostic_entry,
    runtime_memory_source_entry,
};

#[derive(Debug, Clone)]
pub(crate) struct MemoryDimensionDelta {
    pub sources: Vec<RuntimeMemorySourceEntry>,
    pub diagnostics: Vec<RuntimeMemoryDiagnosticEntry>,
    pub added: Vec<RuntimeMemorySourceEntry>,
    pub removed: Vec<RuntimeMemorySourceEntry>,
    pub changed: Vec<RuntimeMemorySourceEntry>,
}

impl MemoryDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
        before_inventory: Option<&MemoryDiscoveryOutput>,
        after_inventory: &MemoryDiscoveryOutput,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        if state_delta.memory_sources.is_empty() {
            return None;
        }

        let before_sources = before_inventory
            .map(flatten_memory_sources)
            .unwrap_or_default();
        let after_sources = flatten_memory_sources(after_inventory);
        let before_lookup = source_lookup(&before_sources);
        let after_lookup = source_lookup(&after_sources);
        let lookup_after = |key: &str| {
            after_lookup
                .get(key)
                .or_else(|| before_lookup.get(key))
                .map(|source| runtime_memory_source_entry(source))
                .unwrap_or_else(|| fallback_entry(key))
        };
        let lookup_before = |key: &str| {
            before_lookup
                .get(key)
                .or_else(|| after_lookup.get(key))
                .map(|source| runtime_memory_source_entry(source))
                .unwrap_or_else(|| fallback_entry(key))
        };

        Some(Box::new(Self {
            sources: after_sources
                .iter()
                .map(runtime_memory_source_entry)
                .collect(),
            diagnostics: after_inventory
                .diagnostics
                .iter()
                .map(runtime_memory_diagnostic_entry)
                .collect(),
            added: state_delta
                .memory_sources
                .added
                .iter()
                .map(|key| lookup_after(key))
                .collect(),
            removed: state_delta
                .memory_sources
                .removed
                .iter()
                .map(|key| lookup_before(key))
                .collect(),
            changed: state_delta
                .memory_sources
                .changed
                .iter()
                .map(|key| lookup_after(key))
                .collect(),
        }))
    }
}

impl DimensionDelta for MemoryDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::MemoryInventory {
            title: "Memory Inventory Delta".to_string(),
            summary: "Runtime-discovered memory sources changed.".to_string(),
            mode: RuntimeMemoryInventoryMode::Delta,
            sources: self.sources.clone(),
            diagnostics: self.diagnostics.clone(),
            added_sources: self.added.clone(),
            removed_sources: self.removed.clone(),
            changed_sources: self.changed.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## Memory Inventory Delta — Step Transition: {node}"),
            None => "## Memory Inventory Delta".to_string(),
        }];
        append_memory_lines(&mut lines, "Added Memory Sources", &self.added, "已加入");
        append_memory_lines(
            &mut lines,
            "Removed Memory Sources",
            &self.removed,
            "已移除",
        );
        append_memory_lines(
            &mut lines,
            "Changed Memory Sources",
            &self.changed,
            "索引或元信息已变更",
        );
        lines.join("\n")
    }
}

fn source_lookup(sources: &[DiscoveredMemorySource]) -> BTreeMap<String, &DiscoveredMemorySource> {
    sources
        .iter()
        .map(|source| (memory_source_key(source), source))
        .collect()
}

fn fallback_entry(key: &str) -> RuntimeMemorySourceEntry {
    RuntimeMemorySourceEntry {
        provider_key: key
            .split_once(':')
            .map(|(provider, _)| provider)
            .unwrap_or("")
            .to_string(),
        source_key: key
            .split_once(':')
            .map(|(_, source)| source)
            .unwrap_or(key)
            .to_string(),
        display_name: key.to_string(),
        source_uri: String::new(),
        index_uri: String::new(),
        mount_id: String::new(),
        scope: "unknown".to_string(),
        index_status: "unknown".to_string(),
        trust_level: "unknown".to_string(),
        revision: String::new(),
        summary: None,
        context_usage_kind: Some(agentdash_spi::context_usage_kind::MEMORY.to_string()),
    }
}

fn append_memory_lines(
    lines: &mut Vec<String>,
    title: &str,
    values: &[RuntimeMemorySourceEntry],
    suffix: &str,
) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("### {title}"));
    for source in values {
        lines.push(format!(
            "- `{}`: source `{}`, index `{}`, status `{}` — {suffix}",
            source.display_name, source.source_uri, source.index_uri, source.index_status
        ));
    }
}
