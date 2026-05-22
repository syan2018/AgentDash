/// Relay ↔ Runtime 类型转换适配器
///
/// relay-specific 的 FileEntry 转换逻辑放在 API adapter 层，
/// 避免 Application 层直接依赖 `agentdash-relay` 类型。
pub use agentdash_application::runtime_bridge::{
    session_mcp_server_to_runtime, session_mcp_servers_to_runtime,
};

use agentdash_application::runtime::RuntimeFileEntry;
use agentdash_relay::FileEntryRelay;

pub fn relay_file_entry_to_runtime(entry: &FileEntryRelay) -> RuntimeFileEntry {
    let attributes = relay_file_entry_attributes(entry);
    RuntimeFileEntry {
        path: entry.path.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
        is_virtual: false,
        attributes,
    }
}

pub fn relay_file_entries_to_runtime(entries: &[FileEntryRelay]) -> Vec<RuntimeFileEntry> {
    entries.iter().map(relay_file_entry_to_runtime).collect()
}

pub fn runtime_file_entry_to_relay(entry: &RuntimeFileEntry) -> FileEntryRelay {
    FileEntryRelay {
        path: entry.path.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
        content_kind: entry_attribute(entry, "content_kind"),
        mime_type: entry_attribute(entry, "mime_type"),
    }
}

pub fn runtime_file_entries_to_relay(entries: &[RuntimeFileEntry]) -> Vec<FileEntryRelay> {
    entries.iter().map(runtime_file_entry_to_relay).collect()
}

fn relay_file_entry_attributes(
    entry: &FileEntryRelay,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut attrs = serde_json::Map::new();
    if let Some(content_kind) = &entry.content_kind {
        attrs.insert(
            "content_kind".to_string(),
            serde_json::Value::String(content_kind.clone()),
        );
    }
    if let Some(mime_type) = &entry.mime_type {
        attrs.insert(
            "mime_type".to_string(),
            serde_json::Value::String(mime_type.clone()),
        );
    }
    (!attrs.is_empty()).then_some(attrs)
}

fn entry_attribute(entry: &RuntimeFileEntry, key: &str) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get(key))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}
