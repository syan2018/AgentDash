use agentdash_agent_protocol::{
    ContextFrameSection, RuntimeMemoryInventoryMode, RuntimeMemorySourceEntry,
};

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::{
    NormalizedContextSurfaceDelta, NormalizedContextSurfaceState,
};

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    previous: &NormalizedContextSurfaceState,
    target: &NormalizedContextSurfaceState,
    phase_node: Option<&str>,
) -> Option<ProjectedSurfaceDimension> {
    if delta.memory_sources.is_empty() {
        return None;
    }
    let target_or_fallback = |key: &str| {
        target
            .memory_sources
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback_entry(key))
    };
    let sources = ordered_sources(target);
    let added = delta
        .memory_sources
        .added
        .iter()
        .map(|key| target_or_fallback(key))
        .collect::<Vec<_>>();
    let removed = delta
        .memory_sources
        .removed
        .iter()
        .map(|key| {
            previous
                .memory_sources
                .get(key)
                .or_else(|| target.memory_sources.get(key))
                .cloned()
                .unwrap_or_else(|| fallback_entry(key))
        })
        .collect::<Vec<_>>();
    let changed = delta
        .memory_sources
        .changed
        .iter()
        .map(|key| target_or_fallback(key))
        .collect::<Vec<_>>();
    let mut lines = vec![super::surface_update_heading(
        "Memory Inventory Delta",
        phase_node,
    )];
    append_memory_lines(&mut lines, "Added Memory Sources", &added, "已加入");
    append_memory_lines(&mut lines, "Removed Memory Sources", &removed, "已移除");
    append_memory_lines(
        &mut lines,
        "Changed Memory Sources",
        &changed,
        "索引或元信息已变更",
    );
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::MemoryInventory {
            title: "Memory Inventory Delta".to_string(),
            summary: "Runtime-discovered memory sources changed.".to_string(),
            mode: RuntimeMemoryInventoryMode::Delta,
            sources,
            diagnostics: target.memory_diagnostics.clone(),
            added_sources: added,
            removed_sources: removed,
            changed_sources: changed,
        },
        rendered_text: lines.join("\n"),
    })
}

fn ordered_sources(target: &NormalizedContextSurfaceState) -> Vec<RuntimeMemorySourceEntry> {
    let mut rendered = target
        .memory_source_order
        .iter()
        .filter_map(|key| target.memory_sources.get(key))
        .cloned()
        .collect::<Vec<_>>();
    for (key, source) in &target.memory_sources {
        if !target.memory_source_order.contains(key) {
            rendered.push(source.clone());
        }
    }
    rendered
}

fn fallback_entry(key: &str) -> RuntimeMemorySourceEntry {
    let (provider_key, source_key) = key.split_once(':').unwrap_or(("", key));
    RuntimeMemorySourceEntry {
        provider_key: provider_key.to_string(),
        source_key: source_key.to_string(),
        display_name: key.to_string(),
        source_uri: String::new(),
        index_uri: String::new(),
        mount_id: String::new(),
        scope: "unknown".to_string(),
        index_status: "unknown".to_string(),
        trust_level: "unknown".to_string(),
        revision: String::new(),
        summary: None,
        context_usage_kind: Some("memory".to_string()),
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
    lines.extend(values.iter().map(|source| {
        format!(
            "- `{}`: source `{}`, index `{}`, status `{}` — {suffix}",
            source.display_name, source.source_uri, source.index_uri, source.index_status
        )
    }));
}
