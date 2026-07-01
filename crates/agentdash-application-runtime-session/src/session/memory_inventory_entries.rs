use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{RuntimeMemoryDiagnosticEntry, RuntimeMemorySourceEntry};
use agentdash_spi::{DiscoveredMemorySource, MemoryDiscoveryDiagnostic, MemoryDiscoveryOutput};

pub(crate) fn flatten_memory_sources(
    inventory: &MemoryDiscoveryOutput,
) -> Vec<DiscoveredMemorySource> {
    inventory
        .clusters
        .iter()
        .flat_map(|cluster| cluster.sources.iter().cloned())
        .collect()
}

pub(crate) fn runtime_memory_source_entry(
    source: &DiscoveredMemorySource,
) -> RuntimeMemorySourceEntry {
    RuntimeMemorySourceEntry {
        provider_key: source.provider_key.clone(),
        source_key: source.source_key.clone(),
        display_name: source.display_name.clone(),
        source_uri: source.source_uri.clone(),
        index_uri: source.index_uri.clone(),
        mount_id: source.mount_id.clone(),
        scope: enum_name(source.scope),
        index_status: enum_name(source.index_status),
        trust_level: enum_name(source.trust_level),
        revision: memory_source_revision(source),
        summary: source.summary.clone(),
        context_usage_kind: Some(context_usage_kind::MEMORY.to_string()),
    }
}

pub(crate) fn runtime_memory_diagnostic_entry(
    diagnostic: &MemoryDiscoveryDiagnostic,
) -> RuntimeMemoryDiagnosticEntry {
    RuntimeMemoryDiagnosticEntry {
        provider_key: diagnostic.provider_key.clone(),
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        source_key: diagnostic.source_key.clone(),
        uri: diagnostic.uri.clone(),
        context_usage_kind: Some(context_usage_kind::MEMORY.to_string()),
    }
}

pub(crate) fn memory_source_key(source: &DiscoveredMemorySource) -> String {
    agentdash_spi::connector::capability_delta::memory_source_key(source)
}

fn memory_source_revision(source: &DiscoveredMemorySource) -> String {
    let payload = serde_json::to_string(source).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}",
            source.provider_key, source.source_key, source.index_uri, source.index_status as u8
        )
    });
    format!("{:016x}", fnv1a64(payload.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn enum_name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}
