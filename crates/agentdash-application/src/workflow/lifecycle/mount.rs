//! Lifecycle VFS mount projection helpers.
//!
//! 这里负责把活跃 workflow run 投影成 session 可挂载的 VFS mount；`vfs` 模块只保留
//! provider / mount 的通用访问能力，不反向理解 lifecycle 领域对象。

use agentdash_spi::Vfs;

use crate::vfs::build_lifecycle_mount_with_ports;
use crate::workflow::projection::ActiveWorkflowProjection;

fn empty_vfs() -> Vfs {
    Vfs {
        mounts: Vec::new(),
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    }
}

pub fn writable_port_keys_for_active_workflow(workflow: &ActiveWorkflowProjection) -> Vec<String> {
    workflow
        .active_step
        .output_ports
        .iter()
        .map(|port| port.key.clone())
        .collect()
}

pub fn append_active_workflow_lifecycle_mount(vfs: &mut Vfs, workflow: &ActiveWorkflowProjection) {
    let mount = build_lifecycle_mount_with_ports(
        workflow.run.id,
        &workflow.lifecycle.key,
        &writable_port_keys_for_active_workflow(workflow),
    );

    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|candidate| candidate.id == "lifecycle")
    {
        *existing = mount;
    } else {
        vfs.mounts.push(mount);
    }
}

pub fn ensure_active_workflow_lifecycle_mount(
    vfs: Option<Vfs>,
    workflow: Option<&ActiveWorkflowProjection>,
) -> Option<Vfs> {
    let Some(workflow) = workflow else {
        return vfs;
    };

    let mut vfs = vfs.unwrap_or_else(empty_vfs);
    append_active_workflow_lifecycle_mount(&mut vfs, workflow);
    Some(vfs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, OutputPortDefinition,
        WorkflowBindingKind, WorkflowDefinitionSource,
    };
    use uuid::Uuid;

    fn active_workflow_projection() -> ActiveWorkflowProjection {
        let project_id = Uuid::new_v4();
        let step = LifecycleStepDefinition {
            key: "plan".to_string(),
            description: "规划".to_string(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![OutputPortDefinition {
                key: "brief".to_string(),
                description: "规划记录".to_string(),
                gate_strategy: Default::default(),
                gate_params: None,
            }],
            input_ports: Vec::new(),
            capability_config: Default::default(),
        };
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "workflow_admin",
            "Workflow Admin",
            "Workflow admin lifecycle",
            vec![WorkflowBindingKind::Project],
            WorkflowDefinitionSource::BuiltinSeed,
            "plan",
            vec![step.clone()],
            vec![],
        )
        .expect("lifecycle");
        let mut run = LifecycleRun::new(
            project_id,
            lifecycle.id,
            "sess-owner",
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .expect("run");
        run.activate_step("plan").expect("activate step");

        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step: step,
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
    fn ensure_lifecycle_mount_creates_vfs_when_base_is_absent() {
        let workflow = active_workflow_projection();
        let vfs = ensure_active_workflow_lifecycle_mount(None, Some(&workflow)).expect("vfs");

        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount");
        assert_eq!(lifecycle.provider, "lifecycle_vfs");
        assert_eq!(
            lifecycle.root_ref,
            format!("lifecycle://run/{}", workflow.run.id)
        );
        assert!(lifecycle.capabilities.contains(&MountCapability::Write));
        assert_eq!(
            lifecycle
                .metadata
                .pointer("/writable_port_keys/0")
                .and_then(serde_json::Value::as_str),
            Some("brief")
        );
    }

    #[test]
    fn ensure_lifecycle_mount_preserves_existing_mounts_and_replaces_stale_lifecycle() {
        let workflow = active_workflow_projection();
        let stale_run_id = Uuid::new_v4();
        let base = Vfs {
            mounts: vec![
                workspace_mount(),
                build_lifecycle_mount_with_ports(stale_run_id, "stale", &[]),
            ],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let vfs = ensure_active_workflow_lifecycle_mount(Some(base), Some(&workflow)).expect("vfs");

        assert!(vfs.mounts.iter().any(|mount| mount.id == "main"));
        let lifecycle_mounts = vfs
            .mounts
            .iter()
            .filter(|mount| mount.id == "lifecycle")
            .collect::<Vec<_>>();
        assert_eq!(lifecycle_mounts.len(), 1);
        assert_eq!(
            lifecycle_mounts[0].root_ref,
            format!("lifecycle://run/{}", workflow.run.id)
        );
        assert_eq!(vfs.default_mount_id.as_deref(), Some("main"));
    }
}
