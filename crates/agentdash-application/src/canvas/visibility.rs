use std::collections::BTreeSet;

use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::project::ProjectAuthorization;
use agentdash_spi::{AuthIdentity, Vfs};

use crate::canvas::canvas_access_projection;
use crate::project::project_authorization_context_from_identity;
use crate::vfs::{CanvasMountAccess, append_canvas_mount};

pub fn canvas_runtime_mount_access(
    canvas: &Canvas,
    identity: Option<&AuthIdentity>,
) -> CanvasMountAccess {
    let Some(identity) = identity else {
        return CanvasMountAccess::read_only();
    };
    let current_user = project_authorization_context_from_identity(identity);
    let project_access = ProjectAuthorization {
        role: None,
        via_admin_bypass: identity.is_admin,
        via_template_visibility: false,
    };
    CanvasMountAccess::from(canvas_access_projection(
        canvas,
        &current_user,
        &project_access,
    ))
}

/// 根据会话显式声明的 canvas mount_id 列表，向 VFS 追加可见 canvas。
///
/// 注意：默认不注入任何 canvas。只有会话里记录过的 mount_id 才会被追加。
pub async fn append_visible_canvas_mounts(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    vfs: &mut Vfs,
    visible_mount_ids: &[String],
    identity: Option<&AuthIdentity>,
) -> Result<(), DomainError> {
    let selected = visible_mount_ids
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Ok(());
    }

    let canvases = canvas_repo.list_by_project(project_id).await?;
    let visible = canvases
        .into_iter()
        .filter(|canvas| selected.contains(canvas.mount_id.as_str()))
        .collect::<Vec<_>>();
    for canvas in visible {
        let access = canvas_runtime_mount_access(&canvas, identity);
        append_canvas_mount(vfs, &canvas, access);
    }
    Ok(())
}
