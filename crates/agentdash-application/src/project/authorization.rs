use agentdash_spi::platform::auth::AuthIdentity;

pub use agentdash_domain::project::{
    ProjectAuthorization, ProjectAuthorizationContext, ProjectAuthorizationService,
    ProjectPermission,
};

pub fn project_authorization_context_from_identity(
    identity: &AuthIdentity,
) -> ProjectAuthorizationContext {
    ProjectAuthorizationContext::new(
        identity.user_id.clone(),
        identity
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
        identity.is_admin,
    )
}
