//! Lifecycle VFS mount projection helpers.
//!
//! 这里负责把 AgentRun / active workflow 的领域身份投影成可挂载的 VFS mount；
//! `vfs` 模块只保留 provider / mount 的通用访问能力，不反向理解 lifecycle 领域对象。

use agentdash_application_vfs::append_lifecycle_skill_asset_projection;
use agentdash_platform_spi::Vfs;

use uuid::Uuid;

use crate::lifecycle::projection::ActiveWorkflowProjection;
use crate::lifecycle::{
    build_agent_run_session_lifecycle_mount, build_lifecycle_mount_with_node_scope,
};
const SKILL_ASSET_PROJECT_ID_METADATA_KEY: &str = "skill_asset_project_id";
const SKILL_ASSET_KEYS_METADATA_KEY: &str = "skill_asset_keys";

fn empty_vfs() -> Vfs {
    Vfs {
        mounts: Vec::new(),
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

pub(crate) fn writable_port_keys_for_active_workflow(
    workflow: &ActiveWorkflowProjection,
) -> Vec<String> {
    workflow
        .active_activity
        .output_ports
        .iter()
        .map(|port| port.key.clone())
        .collect()
}

pub(crate) struct LifecycleMountSurface<'a> {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub lifecycle_key: &'a str,
    pub attempt: u32,
    pub writable_port_keys: Vec<String>,
}

pub(crate) fn lifecycle_mount_surface_for_active_workflow(
    workflow: &ActiveWorkflowProjection,
) -> LifecycleMountSurface<'_> {
    LifecycleMountSurface {
        run_id: workflow.run.id,
        orchestration_id: workflow.orchestration_id,
        node_path: &workflow.node_path,
        lifecycle_key: &workflow.lifecycle_key,
        attempt: workflow.active_attempt.attempt,
        writable_port_keys: writable_port_keys_for_active_workflow(workflow),
    }
}

pub(crate) fn lifecycle_mount_overlay_for_surface(surface: &LifecycleMountSurface<'_>) -> Vfs {
    Vfs {
        mounts: vec![build_lifecycle_mount_with_node_scope(
            surface.run_id,
            None,
            surface.orchestration_id,
            surface.node_path,
            surface.lifecycle_key,
            &surface.writable_port_keys,
            Some(surface.attempt),
        )],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

fn append_active_workflow_lifecycle_mount(vfs: &mut Vfs, workflow: &ActiveWorkflowProjection) {
    let existing_skill_projection = lifecycle_skill_projection(vfs);
    let surface = lifecycle_mount_surface_for_active_workflow(workflow);
    let mut overlay = lifecycle_mount_overlay_for_surface(&surface);
    let mount = overlay
        .mounts
        .pop()
        .expect("lifecycle surface overlay must contain one mount");

    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|candidate| candidate.id == "lifecycle")
    {
        *existing = mount;
    } else {
        vfs.mounts.push(mount);
    }
    if let Some((project_id, keys)) = existing_skill_projection {
        append_lifecycle_skill_asset_projection(vfs, project_id, &keys);
    }
}

fn normalize_default_mount(vfs: &mut Vfs) {
    if vfs
        .default_mount_id
        .as_ref()
        .is_some_and(|id| vfs.mounts.iter().any(|mount| &mount.id == id))
    {
        return;
    }
    vfs.default_mount_id = vfs
        .mounts
        .iter()
        .find(|mount| mount.default_write)
        .or_else(|| vfs.mounts.first())
        .map(|mount| mount.id.clone());
}

pub(super) fn install_agent_run_lifecycle_mount(
    vfs: &mut Vfs,
    run_id: Uuid,
    agent_id: Uuid,
    runtime_thread_id: &str,
    frame_id: Uuid,
    node_evidence: Option<(Uuid, &str, u32)>,
) {
    let existing_skill_projection = lifecycle_skill_projection(vfs);
    vfs.mounts.retain(|candidate| candidate.id != "lifecycle");
    let (orchestration_id, node_path, node_attempt) = node_evidence
        .map(|(orchestration_id, node_path, node_attempt)| {
            (Some(orchestration_id), Some(node_path), Some(node_attempt))
        })
        .unwrap_or((None, None, None));
    vfs.mounts.push(build_agent_run_session_lifecycle_mount(
        run_id,
        agent_id,
        runtime_thread_id,
        frame_id,
        orchestration_id,
        node_path,
        node_attempt,
    ));
    if let Some((project_id, keys)) = existing_skill_projection {
        append_lifecycle_skill_asset_projection(vfs, project_id, &keys);
    }
    normalize_default_mount(vfs);
}

fn lifecycle_skill_projection(vfs: &Vfs) -> Option<(Uuid, Vec<String>)> {
    vfs.mounts
        .iter()
        .find(|mount| mount.id == "lifecycle")
        .and_then(|mount| {
            let project_id = mount
                .metadata
                .get(SKILL_ASSET_PROJECT_ID_METADATA_KEY)
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())?;
            let keys = mount
                .metadata
                .get(SKILL_ASSET_KEYS_METADATA_KEY)
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some((project_id, keys))
        })
        .filter(|(_, keys)| !keys.is_empty())
}

pub fn project_active_workflow_lifecycle_vfs(
    vfs: Option<Vfs>,
    workflow: Option<&ActiveWorkflowProjection>,
) -> Option<Vfs> {
    let Some(workflow) = workflow else {
        return vfs;
    };

    let mut vfs = vfs.unwrap_or_else(empty_vfs);
    append_active_workflow_lifecycle_mount(&mut vfs, workflow);
    normalize_default_mount(&mut vfs);
    Some(vfs)
}
