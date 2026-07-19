use agentdash_domain::common::{Mount, MountCapability};
use uuid::Uuid;

use super::vfs_catalog::lifecycle_directory_hint;
use agentdash_application_vfs::PROVIDER_LIFECYCLE_VFS;

pub fn build_agent_run_lifecycle_mount(
    run_id: Uuid,
    agent_id: Uuid,
    runtime_thread_id: &str,
    launch_frame_id: Uuid,
    orchestration_id: Option<Uuid>,
    node_path: Option<&str>,
    attempt: Option<u32>,
) -> Mount {
    let mut metadata = serde_json::json!({
        "run_id": run_id.to_string(),
        "agent_id": agent_id.to_string(),
        "runtime_thread_id": runtime_thread_id,
        "launch_frame_id": launch_frame_id.to_string(),
        "scope": "agent_run_history",
        "directory_hint": lifecycle_directory_hint()
    });
    if let Some(orchestration_id) = orchestration_id {
        metadata["orchestration_id"] = serde_json::json!(orchestration_id.to_string());
    }
    if let Some(node_path) = node_path {
        metadata["node_path"] = serde_json::json!(node_path);
    }
    if let Some(attempt) = attempt {
        metadata["attempt"] = serde_json::json!(attempt);
    }

    Mount {
        id: "lifecycle".to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        // AppliedResourceSurface requires a stable backing identity for every mount. The
        // canonical history is backed by the Managed Runtime thread rather than a filesystem.
        backend_id: format!("managed-runtime:{runtime_thread_id}"),
        root_ref: format!(
            "lifecycle://run/{run_id}/agent/{agent_id}/thread/{}",
            encode_lifecycle_uri_segment(runtime_thread_id)
        ),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata,
    }
}

pub fn build_lifecycle_mount_with_node_scope(
    run_id: Uuid,
    agent_id: Option<Uuid>,
    orchestration_id: Uuid,
    node_path: &str,
    lifecycle_key: &str,
    writable_port_keys: &[String],
    attempt: Option<u32>,
) -> Mount {
    let capabilities = vec![
        MountCapability::Read,
        MountCapability::Write,
        MountCapability::List,
        MountCapability::Search,
    ];

    let mut metadata = serde_json::json!({
        "run_id": run_id.to_string(),
        "orchestration_id": orchestration_id.to_string(),
        "node_path": node_path,
        "lifecycle_key": lifecycle_key,
        "scope": "node_runtime",
        "writable_port_keys": writable_port_keys,
        "directory_hint": lifecycle_directory_hint()
    });
    if let Some(agent_id) = agent_id {
        metadata["agent_id"] = serde_json::json!(agent_id.to_string());
    }
    if let Some(attempt) = attempt {
        metadata["attempt"] = serde_json::json!(attempt);
    }

    Mount {
        id: "lifecycle".to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        backend_id: format!(
            "lifecycle-node:{run_id}:{orchestration_id}:{}",
            encode_lifecycle_uri_segment(node_path)
        ),
        root_ref: format!(
            "lifecycle://run/{run_id}/orchestration/{orchestration_id}/node/{}",
            encode_lifecycle_uri_segment(node_path)
        ),
        capabilities,
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata,
    }
}

fn encode_lifecycle_uri_segment(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        let is_safe = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
        if is_safe {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0F) as usize]));
        }
    }
    encoded
}
