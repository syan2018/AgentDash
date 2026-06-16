use crate::runtime::{Mount, MountCapability};
use uuid::Uuid;

use super::lifecycle_catalog::lifecycle_directory_hint;
use super::mount::PROVIDER_LIFECYCLE_VFS;

pub fn build_lifecycle_mount(
    run_id: Uuid,
    orchestration_id: Uuid,
    node_path: &str,
    lifecycle_key: &str,
) -> Mount {
    build_lifecycle_mount_with_ports(run_id, orchestration_id, node_path, lifecycle_key, &[])
}

pub fn build_agent_run_session_lifecycle_mount(
    run_id: Uuid,
    agent_id: Uuid,
    runtime_session_id: &str,
    launch_frame_id: Uuid,
    orchestration_id: Option<Uuid>,
    node_path: Option<&str>,
    attempt: Option<u32>,
) -> Mount {
    let mut metadata = serde_json::json!({
        "run_id": run_id.to_string(),
        "agent_id": agent_id.to_string(),
        "runtime_session_id": runtime_session_id,
        "launch_frame_id": launch_frame_id.to_string(),
        "scope": "agent_run_session",
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
        backend_id: String::new(),
        root_ref: format!(
            "lifecycle://run/{run_id}/agent/{agent_id}/session/{}",
            crate::lifecycle::execution_log::encode_node_path_segment(runtime_session_id)
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

/// 构建 attempt=1 且带 output port 写入权限的 lifecycle mount。
/// mount 始终启用 Write capability 以支持 `records/{name}` overlay；
/// `artifacts/{port_key}` 仍由 `writable_port_keys` 做路径级白名单控制。
pub fn build_lifecycle_mount_with_ports(
    run_id: Uuid,
    orchestration_id: Uuid,
    node_path: &str,
    lifecycle_key: &str,
    writable_port_keys: &[String],
) -> Mount {
    build_lifecycle_mount_with_node_scope(
        run_id,
        orchestration_id,
        node_path,
        lifecycle_key,
        writable_port_keys,
        Some(1),
    )
}

pub fn build_lifecycle_mount_with_node_scope(
    run_id: Uuid,
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
    if let Some(attempt) = attempt {
        metadata["attempt"] = serde_json::json!(attempt);
    }

    Mount {
        id: "lifecycle".to_string(),
        provider: PROVIDER_LIFECYCLE_VFS.to_string(),
        backend_id: String::new(),
        root_ref: format!(
            "lifecycle://run/{run_id}/orchestration/{orchestration_id}/node/{}",
            crate::lifecycle::execution_log::encode_node_path_segment(node_path)
        ),
        capabilities,
        default_write: false,
        display_name: "Lifecycle 执行记录".to_string(),
        metadata,
    }
}
