use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{
    OperationAuthorityGrant, OperationAuthorityResolver, OperationAuthorizationScope,
    OperationExecutionError, OperationOriginRef, OperationPrincipal, OperationPrincipalRef,
    OperationScopeRef,
};

/// Explicit principal/surface routing for canonical Operation admission.
///
/// Every branch is required at composition time. This keeps an unavailable authority domain from
/// silently falling through to a broader resolver.
pub struct CompositeOperationAuthorityResolver {
    setup_user: Arc<dyn OperationAuthorityResolver>,
    user_surface: Arc<dyn OperationAuthorityResolver>,
    agent_run: Arc<dyn OperationAuthorityResolver>,
    workflow: Arc<dyn OperationAuthorityResolver>,
    extension: Arc<dyn OperationAuthorityResolver>,
}

impl CompositeOperationAuthorityResolver {
    pub fn new(
        setup_user: Arc<dyn OperationAuthorityResolver>,
        user_surface: Arc<dyn OperationAuthorityResolver>,
        agent_run: Arc<dyn OperationAuthorityResolver>,
        workflow: Arc<dyn OperationAuthorityResolver>,
        extension: Arc<dyn OperationAuthorityResolver>,
    ) -> Self {
        Self {
            setup_user,
            user_surface,
            agent_run,
            workflow,
            extension,
        }
    }

    fn resolver_for(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationScopeRef,
    ) -> Result<&Arc<dyn OperationAuthorityResolver>, OperationExecutionError> {
        match (principal.principal_ref(), scope) {
            (OperationPrincipalRef::User { .. }, OperationScopeRef::EnvironmentSetup { .. }) => {
                Ok(&self.setup_user)
            }
            (OperationPrincipalRef::User { .. }, _) => Ok(&self.user_surface),
            (OperationPrincipalRef::AgentRunAgent { .. }, _) => Ok(&self.agent_run),
            (OperationPrincipalRef::WorkflowNode { .. }, _) => Ok(&self.workflow),
            (OperationPrincipalRef::ExtensionInstallation { .. }, _) => Ok(&self.extension),
        }
    }
}

#[async_trait]
impl OperationAuthorityResolver for CompositeOperationAuthorityResolver {
    async fn resolve(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
        self.resolver_for(principal, &scope.scope_ref)?
            .resolve(principal, scope, origin, cancel)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_spi::{AuthIdentity, AuthMode};
    use serde_json::Value;
    use uuid::Uuid;

    use super::*;

    struct TaggedResolver(&'static str);

    #[async_trait]
    impl OperationAuthorityResolver for TaggedResolver {
        async fn resolve(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            _: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
            Ok(OperationAuthorityGrant {
                authority_revision: self.0.to_string(),
                capabilities: BTreeSet::new(),
            })
        }
    }

    fn tagged(tag: &'static str) -> Arc<dyn OperationAuthorityResolver> {
        Arc::new(TaggedResolver(tag))
    }

    fn composite() -> CompositeOperationAuthorityResolver {
        CompositeOperationAuthorityResolver::new(
            tagged("setup"),
            tagged("user"),
            tagged("agent"),
            tagged("workflow"),
            tagged("extension"),
        )
    }

    fn user() -> OperationPrincipal {
        OperationPrincipal::authenticated_user(AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "user-1".to_string(),
            subject: "user-1".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: None,
            extra: Value::Null,
        })
    }

    async fn revision(principal: OperationPrincipal, scope_ref: OperationScopeRef) -> String {
        composite()
            .resolve(
                &principal,
                &OperationAuthorizationScope {
                    scope_ref,
                    authority_revision: "caller-value-is-ignored-by-router".to_string(),
                },
                &OperationOriginRef::UserWorkshop,
                CancellationToken::new(),
            )
            .await
            .expect("grant")
            .authority_revision
    }

    #[tokio::test]
    async fn routes_setup_and_standalone_user_surfaces_separately() {
        assert_eq!(
            revision(
                user(),
                OperationScopeRef::EnvironmentSetup {
                    project_id: None,
                    workspace_id: None,
                    backend_id: None,
                }
            )
            .await,
            "setup"
        );
        assert_eq!(
            revision(
                user(),
                OperationScopeRef::Project {
                    project_id: Uuid::new_v4()
                }
            )
            .await,
            "user"
        );
    }

    #[tokio::test]
    async fn routes_server_principals_to_exact_authority_domains() {
        let scope = OperationScopeRef::Project {
            project_id: Uuid::new_v4(),
        };
        assert_eq!(
            revision(
                OperationPrincipal::server_resolved(OperationPrincipalRef::AgentRunAgent {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                }),
                scope.clone(),
            )
            .await,
            "agent"
        );
        assert_eq!(
            revision(
                OperationPrincipal::server_resolved(OperationPrincipalRef::ExtensionInstallation {
                    installation_id: Uuid::new_v4(),
                }),
                scope,
            )
            .await,
            "extension"
        );
    }
}
