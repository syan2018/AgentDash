use agentdash_domain::canvas::{Canvas, CanvasScope, canvas_access_projection};
use agentdash_domain::project::{ProjectAuthorization, ProjectAuthorizationContext};
use agentdash_platform_spi::AuthIdentity;

use crate::canvas::CanvasMountAccess;

pub fn canvas_runtime_mount_access(
    canvas: &Canvas,
    identity: Option<&AuthIdentity>,
) -> Option<CanvasMountAccess> {
    canvas_runtime_mount_access_for_user(canvas, identity.map(|identity| identity.user_id.as_str()))
}

pub fn canvas_runtime_mount_access_for_user(
    canvas: &Canvas,
    user_id: Option<&str>,
) -> Option<CanvasMountAccess> {
    if canvas.scope == CanvasScope::Project {
        return Some(CanvasMountAccess::read_only());
    }

    let user_id = user_id?;
    let current_user = ProjectAuthorizationContext::new(user_id.to_owned(), Vec::new(), false);
    let project_access = ProjectAuthorization {
        role: None,
        via_admin_bypass: false,
        via_template_visibility: false,
    };
    let access = canvas_access_projection(canvas, &current_user, &project_access);
    if !access.can_view {
        return None;
    }
    Some(CanvasMountAccess::from(access))
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentdash_platform_spi::AuthMode;
    use uuid::Uuid;

    fn identity(user_id: &str) -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: user_id.to_string(),
            subject: user_id.to_string(),
            display_name: Some(user_id.to_string()),
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: Some("test".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    #[test]
    fn personal_owner_runtime_mount_access_is_writable() {
        let canvas = Canvas::new_personal(
            Uuid::new_v4(),
            "alice".to_string(),
            "cvs-dashboard".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );

        let access = canvas_runtime_mount_access(&canvas, Some(&identity("alice")))
            .expect("owner personal canvas should be exposed");

        assert!(access.runtime_write_allowed);
        assert!(
            canvas_runtime_mount_access_for_user(&canvas, Some("alice"))
                .expect("owner access from canonical user id")
                .runtime_write_allowed
        );
    }

    #[test]
    fn personal_canvas_without_view_access_is_omitted() {
        let canvas = Canvas::new_personal(
            Uuid::new_v4(),
            "alice".to_string(),
            "cvs-dashboard".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );

        assert!(canvas_runtime_mount_access(&canvas, Some(&identity("bob"))).is_none());
        assert!(canvas_runtime_mount_access(&canvas, None).is_none());
    }

    #[test]
    fn project_shared_canvas_reprojection_is_read_only() {
        let canvas = Canvas::new_project_shared(
            Uuid::new_v4(),
            "cvs-dashboard".to_string(),
            "Dashboard".to_string(),
            String::new(),
            None,
            None,
        );

        let access = canvas_runtime_mount_access(&canvas, None)
            .expect("project shared canvas should remain readable during reprojection");

        assert!(!access.runtime_write_allowed);
    }
}
