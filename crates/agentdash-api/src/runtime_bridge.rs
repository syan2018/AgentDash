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
    RuntimeFileEntry {
        path: entry.path.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
        is_virtual: false,
        attributes: None,
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
    }
}

pub fn runtime_file_entries_to_relay(entries: &[RuntimeFileEntry]) -> Vec<FileEntryRelay> {
    entries.iter().map(runtime_file_entry_to_relay).collect()
}
