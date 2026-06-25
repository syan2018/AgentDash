//! Lifecycle VFS mount projection helpers.
//!
//! 这里负责把 AgentRun / active workflow 的领域身份投影成可挂载的 VFS mount；
//! `vfs` 模块只保留 provider / mount 的通用访问能力，不反向理解 lifecycle 领域对象。

use agentdash_spi::Vfs;

use uuid::Uuid;

use agentdash_domain::workflow::RuntimeSessionExecutionAnchor;

use crate::lifecycle::projection::ActiveWorkflowProjection;
use crate::lifecycle::{
    build_agent_run_session_lifecycle_mount, build_lifecycle_mount_with_node_scope,
};
use crate::vfs::append_lifecycle_skill_asset_projection;
use crate::vfs::mount::{SKILL_ASSET_KEYS_METADATA_KEY, SKILL_ASSET_PROJECT_ID_METADATA_KEY};

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
    anchor: &RuntimeSessionExecutionAnchor,
) {
    let existing_skill_projection = lifecycle_skill_projection(vfs);
    vfs.mounts.retain(|candidate| candidate.id != "lifecycle");
    vfs.mounts.push(build_agent_run_session_lifecycle_mount(
        anchor.run_id,
        anchor.agent_id,
        &anchor.runtime_session_id,
        anchor.launch_frame_id,
        anchor.orchestration_id,
        anchor.node_path.as_deref(),
        anchor.node_attempt,
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

pub(crate) fn project_active_workflow_lifecycle_vfs(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::append_lifecycle_skill_asset_projection;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, BashExecExecutorSpec, DefinitionSource,
        FunctionActivityExecutorSpec, LifecycleRun, OutputPortDefinition, PlanNodeKind,
        RuntimeNodeState, RuntimeNodeStatus, WorkflowGraph, WorkflowGraphDraft,
    };
    use uuid::Uuid;

    fn active_workflow_projection() -> ActiveWorkflowProjection {
        let project_id = Uuid::new_v4();
        let activity = ActivityDefinition {
            key: "plan".to_string(),
            description: "规划".to_string(),
            // 无 agent workflow 绑定 → manual node;用 function executor 表达"无 workflow"。
            executor: ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
                BashExecExecutorSpec {
                    command: "true".to_string(),
                    args: vec![],
                    working_directory: None,
                },
            )),
            input_ports: Vec::new(),
            output_ports: vec![OutputPortDefinition {
                key: "brief".to_string(),
                description: "规划记录".to_string(),
                gate_strategy: Default::default(),
                gate_params: None,
            }],
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        };
        let lifecycle = WorkflowGraph::new(WorkflowGraphDraft {
            project_id,
            key: "workflow_admin".to_string(),
            name: "Workflow Admin".to_string(),
            description: "Workflow admin lifecycle".to_string(),
            source: DefinitionSource::BuiltinSeed,
            entry_activity_key: "plan".to_string(),
            activities: vec![activity.clone()],
            transitions: vec![],
        })
        .expect("lifecycle");
        let active_attempt = RuntimeNodeState {
            node_id: "plan".to_string(),
            node_path: "plan".to_string(),
            kind: PlanNodeKind::LocalEffect,
            status: RuntimeNodeStatus::Running,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: None,
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        };
        let run = LifecycleRun::new_control(project_id);

        ActiveWorkflowProjection {
            run,
            orchestration_id: uuid::Uuid::new_v4(),
            node_path: "plan".to_string(),
            lifecycle_graph_id: Some(lifecycle.id),
            lifecycle_key: lifecycle.key.clone(),
            lifecycle_name: lifecycle.name.clone(),
            active_activity: activity,
            active_attempt,
            active_node_type: agentdash_domain::workflow::LifecycleNodeType::AgentNode,
            active_procedure_key: None,
            snapshot_contract: None,
            primary_workflow: None,
        }
    }

    fn workspace_mount() -> Mount {
        Mount {
            id: "main".to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend-test".to_string(),
            root_ref: "workspace://test".to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: true,
            display_name: "Workspace".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn active_workflow_projection_creates_vfs_when_base_is_absent() {
        let workflow = active_workflow_projection();
        let vfs = project_active_workflow_lifecycle_vfs(None, Some(&workflow)).expect("vfs");

        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount");
        assert_eq!(lifecycle.provider, "lifecycle_vfs");
        assert_eq!(
            lifecycle.root_ref,
            format!(
                "lifecycle://run/{}/orchestration/{}/node/{}",
                workflow.run.id, workflow.orchestration_id, workflow.node_path
            )
        );
        assert!(lifecycle.capabilities.contains(&MountCapability::Write));
        assert_eq!(
            lifecycle
                .metadata
                .pointer("/writable_port_keys/0")
                .and_then(serde_json::Value::as_str),
            Some("brief")
        );
        assert_eq!(vfs.default_mount_id.as_deref(), Some("lifecycle"));
    }

    #[test]
    fn active_workflow_projection_preserves_existing_mounts_and_replaces_stale_lifecycle() {
        let workflow = active_workflow_projection();
        let stale_run_id = Uuid::new_v4();
        let base = Vfs {
            mounts: vec![
                workspace_mount(),
                lifecycle_mount_overlay_for_surface(&LifecycleMountSurface {
                    run_id: stale_run_id,
                    orchestration_id: Uuid::new_v4(),
                    node_path: "stale-node",
                    lifecycle_key: "stale",
                    attempt: 1,
                    writable_port_keys: Vec::new(),
                })
                .mounts
                .into_iter()
                .next()
                .expect("lifecycle mount"),
            ],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let vfs = project_active_workflow_lifecycle_vfs(Some(base), Some(&workflow)).expect("vfs");

        assert!(vfs.mounts.iter().any(|mount| mount.id == "main"));
        let lifecycle_mounts = vfs
            .mounts
            .iter()
            .filter(|mount| mount.id == "lifecycle")
            .collect::<Vec<_>>();
        assert_eq!(lifecycle_mounts.len(), 1);
        assert_eq!(
            lifecycle_mounts[0].root_ref,
            format!(
                "lifecycle://run/{}/orchestration/{}/node/{}",
                workflow.run.id, workflow.orchestration_id, workflow.node_path
            )
        );
        assert_eq!(vfs.default_mount_id.as_deref(), Some("main"));
    }

    fn agent_run_anchor() -> RuntimeSessionExecutionAnchor {
        RuntimeSessionExecutionAnchor::new_dispatch(
            "session-1",
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn agent_run_lifecycle_vfs_installs_session_scoped_mount() {
        let anchor = agent_run_anchor();
        let mut vfs = workflow_vfs();
        install_agent_run_lifecycle_mount(&mut vfs, &anchor);

        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount");

        assert_eq!(lifecycle.provider, "lifecycle_vfs");
        assert_eq!(
            lifecycle.root_ref,
            format!(
                "lifecycle://run/{}/agent/{}/session/session-1",
                anchor.run_id, anchor.agent_id
            )
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("agent_run_session")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("runtime_session_id")
                .and_then(serde_json::Value::as_str),
            Some("session-1")
        );
    }

    #[test]
    fn agent_run_lifecycle_vfs_replaces_stale_node_scoped_mount() {
        let workflow = active_workflow_projection();
        let vfs = project_active_workflow_lifecycle_vfs(Some(workflow_vfs()), Some(&workflow))
            .expect("vfs");

        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "session-2",
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "phase/plan",
            3,
        );
        let mut vfs = vfs;
        install_agent_run_lifecycle_mount(&mut vfs, &anchor);
        let lifecycle_mounts = vfs
            .mounts
            .iter()
            .filter(|mount| mount.id == "lifecycle")
            .collect::<Vec<_>>();

        assert_eq!(lifecycle_mounts.len(), 1);
        assert_eq!(
            lifecycle_mounts[0]
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("agent_run_session")
        );
        assert_eq!(
            lifecycle_mounts[0]
                .metadata
                .get("node_path")
                .and_then(serde_json::Value::as_str),
            Some("phase/plan")
        );
        assert_eq!(
            lifecycle_mounts[0]
                .metadata
                .get("attempt")
                .and_then(serde_json::Value::as_u64),
            Some(3)
        );
    }

    #[test]
    fn agent_run_lifecycle_vfs_preserves_existing_projection_metadata() {
        let project_id = Uuid::new_v4();
        let workflow = active_workflow_projection();
        let mut vfs = project_active_workflow_lifecycle_vfs(Some(workflow_vfs()), Some(&workflow))
            .expect("vfs");
        assert!(append_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &["companion-system".to_string()],
        ));

        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "session-3",
            workflow.run.id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            workflow.orchestration_id,
            "phase/plan",
            3,
        );
        let mut vfs = vfs;
        install_agent_run_lifecycle_mount(&mut vfs, &anchor);
        assert!(append_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &["workspace-module-system".to_string()],
        ));
        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount");

        assert_eq!(
            lifecycle
                .metadata
                .get("scope")
                .and_then(serde_json::Value::as_str),
            Some("agent_run_session")
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("skill_asset_project_id")
                .and_then(serde_json::Value::as_str),
            Some(project_id.to_string().as_str())
        );
        let keys = lifecycle
            .metadata
            .get("skill_asset_keys")
            .and_then(serde_json::Value::as_array)
            .expect("skill keys")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["companion-system", "workspace-module-system"]);
    }

    #[test]
    fn agent_run_lifecycle_vfs_uses_lifecycle_default_when_base_is_absent() {
        let anchor = agent_run_anchor();
        let mut vfs = empty_vfs();
        install_agent_run_lifecycle_mount(&mut vfs, &anchor);

        assert_eq!(vfs.mounts.len(), 1);
        assert_eq!(vfs.mounts[0].id, "lifecycle");
        assert_eq!(vfs.default_mount_id.as_deref(), Some("lifecycle"));
    }

    fn workflow_vfs() -> Vfs {
        Vfs {
            mounts: vec![workspace_mount()],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }
}
