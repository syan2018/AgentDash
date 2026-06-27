use crate::project::{ProjectAuthorization, ProjectAuthorizationContext};

use super::{Canvas, CanvasAccessProjection, CanvasScope};

pub fn canvas_access_projection(
    canvas: &Canvas,
    current_user: &ProjectAuthorizationContext,
    project_access: &ProjectAuthorization,
) -> CanvasAccessProjection {
    let is_owner = canvas.owner_user_id.as_deref() == Some(current_user.user_id.as_str());
    let is_publisher =
        canvas.published_by_user_id.as_deref() == Some(current_user.user_id.as_str());
    let can_view_project = project_access.can_view_project();
    let can_edit_project = project_access.can_edit_project();
    let can_manage_project = project_access.can_manage_project_sharing();
    let is_admin_bypass = project_access.can_admin_bypass();

    match canvas.scope {
        CanvasScope::Personal => {
            let can_view = is_owner || is_admin_bypass;
            let can_edit_source = is_owner;
            CanvasAccessProjection {
                can_view,
                can_edit_source,
                can_publish: can_edit_source && can_edit_project,
                can_manage_shared: is_admin_bypass,
                can_copy: can_view,
                runtime_write_allowed: can_edit_source,
            }
        }
        CanvasScope::Project => {
            let can_view = can_view_project;
            let can_manage_shared = can_view && (can_manage_project || is_publisher);
            CanvasAccessProjection {
                can_view,
                can_edit_source: false,
                can_publish: can_manage_shared,
                can_manage_shared,
                can_copy: can_view,
                runtime_write_allowed: false,
            }
        }
    }
}
