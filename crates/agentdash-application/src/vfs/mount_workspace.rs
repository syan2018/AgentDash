use agentdash_domain::workspace::{Workspace, WorkspaceBinding};

use crate::runtime::{Mount, Vfs};

use super::mount::PROVIDER_RELAY_FS;
use super::path::validate_vfs;

/// 为 Workspace 创建简易单 mount VFS
pub fn build_workspace_vfs(workspace: &Workspace) -> Result<Vfs, String> {
    let vfs = Vfs {
        mounts: vec![workspace_mount(workspace)?],
        default_mount_id: Some("main".to_string()),
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };
    validate_vfs(&vfs)?;
    Ok(vfs)
}

pub fn workspace_mount(workspace: &Workspace) -> Result<Mount, String> {
    let binding = selected_workspace_binding(workspace)
        .ok_or_else(|| "Workspace 当前没有可用 binding".to_string())?;
    let backend_id = binding.backend_id.trim();
    if backend_id.is_empty() {
        return Err("Workspace binding.backend_id 不能为空".to_string());
    }
    if binding.root_ref.trim().is_empty() {
        return Err("Workspace binding.root_ref 不能为空".to_string());
    }

    let capabilities = workspace.mount_capabilities.to_vec();

    Ok(Mount {
        id: "main".to_string(),
        provider: PROVIDER_RELAY_FS.to_string(),
        backend_id: backend_id.to_string(),
        root_ref: binding.root_ref.clone(),
        capabilities,
        default_write: true,
        display_name: if workspace.name.trim().is_empty() {
            "主工作空间".to_string()
        } else {
            workspace.name.clone()
        },
        metadata: serde_json::json!({
            "workspace_id": workspace.id,
            "workspace_identity_kind": workspace.identity_kind,
            "workspace_identity_payload": workspace.identity_payload,
            "workspace_binding_id": binding.id,
            "workspace_detected_facts": binding.detected_facts.clone(),
        }),
    })
}

pub fn selected_workspace_binding(workspace: &Workspace) -> Option<&WorkspaceBinding> {
    if let Some(default_binding_id) = workspace.default_binding_id {
        return workspace
            .bindings
            .iter()
            .find(|binding| binding.id == default_binding_id);
    }
    if workspace.bindings.len() == 1 {
        return workspace.bindings.first();
    }
    None
}
