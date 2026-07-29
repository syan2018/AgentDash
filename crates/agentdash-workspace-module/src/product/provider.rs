use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application_agentrun::agent_run::AppliedVfsMount;
use agentdash_application_operation_gateway::OperationDescriptor;
use agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModulePresentation,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_platform_spi::WorkspaceModuleDimension;
use async_trait::async_trait;
use serde_json::Value;

/// Stable host context shared by every Workspace Module provider.
///
/// The core owns authority resolution. Providers receive only the resolved Project coordinates,
/// actor identity, visibility policy and current canonical Operation descriptors.
#[derive(Clone)]
pub struct WorkspaceModuleProviderContext {
    pub project_id: uuid::Uuid,
    pub actor: WorkspaceModuleActor,
    pub invocation_id: String,
    pub visibility: WorkspaceModuleDimension,
    pub vfs_mounts: Vec<AppliedVfsMount>,
    pub operations: Vec<OperationDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceModuleActor {
    User {
        user_id: String,
    },
    AgentRunAgent {
        user_id: String,
        target: AgentRunTarget,
    },
}

impl WorkspaceModuleProviderContext {
    pub fn user_id(&self) -> &str {
        match &self.actor {
            WorkspaceModuleActor::User { user_id }
            | WorkspaceModuleActor::AgentRunAgent { user_id, .. } => user_id,
        }
    }

    pub fn agent_target(&self) -> Option<&AgentRunTarget> {
        match &self.actor {
            WorkspaceModuleActor::User { .. } => None,
            WorkspaceModuleActor::AgentRunAgent { target, .. } => Some(target),
        }
    }
}

pub struct WorkspaceModuleOperateRequest<'a> {
    pub context: &'a WorkspaceModuleProviderContext,
    pub operation: &'a str,
    pub input: Value,
}

pub struct WorkspaceModulePresentationRequest<'a> {
    pub context: &'a WorkspaceModuleProviderContext,
    pub module: &'a WorkspaceModuleDescriptor,
    pub view_key: &'a str,
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum WorkspaceModulePresentationPreparation {
    #[default]
    Requested,
    Redirected {
        module_id: String,
        view_key: String,
        diagnostics: Option<Value>,
    },
}

#[async_trait]
pub trait WorkspaceModuleProvider: Send + Sync {
    fn provider_key(&self) -> &str;

    async fn operation_capabilities(
        &self,
        _context: &WorkspaceModuleProviderContext,
    ) -> Result<BTreeSet<String>, ProductRuntimeToolOutcome> {
        Ok(BTreeSet::new())
    }

    async fn modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome>;

    fn handles_operate(&self, _operation: &str) -> bool {
        false
    }

    async fn operate(
        &self,
        _request: WorkspaceModuleOperateRequest<'_>,
    ) -> Result<Value, ProductRuntimeToolOutcome> {
        Err(ProductRuntimeToolOutcome::Rejected {
            code: "workspace_module_operation_not_routable".to_owned(),
            message: format!(
                "Workspace Module provider `{}` does not handle operate",
                self.provider_key()
            ),
        })
    }

    fn owns_module(&self, _module: &WorkspaceModuleDescriptor) -> bool {
        false
    }

    async fn prepare_presentation(
        &self,
        _request: WorkspaceModulePresentationRequest<'_>,
    ) -> Result<WorkspaceModulePresentationPreparation, ProductRuntimeToolOutcome> {
        Ok(WorkspaceModulePresentationPreparation::default())
    }
}

#[derive(Clone, Default)]
pub struct WorkspaceModuleProviderRegistry {
    providers: Arc<Vec<Arc<dyn WorkspaceModuleProvider>>>,
}

impl WorkspaceModuleProviderRegistry {
    pub fn new(providers: Vec<Arc<dyn WorkspaceModuleProvider>>) -> Self {
        let mut keys = BTreeSet::new();
        for provider in &providers {
            assert!(
                keys.insert(provider.provider_key().to_owned()),
                "duplicate Workspace Module provider key: {}",
                provider.provider_key()
            );
        }
        Self {
            providers: Arc::new(providers),
        }
    }

    pub async fn operation_capabilities(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<BTreeSet<String>, ProductRuntimeToolOutcome> {
        let mut capabilities = BTreeSet::new();
        for provider in self.providers.iter() {
            capabilities.extend(provider.operation_capabilities(context).await?);
        }
        Ok(capabilities)
    }

    pub async fn modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        let mut modules = Vec::new();
        for provider in self.providers.iter() {
            modules.extend(provider.modules(context).await?);
        }
        modules.sort_by(|left, right| left.summary.module_id.cmp(&right.summary.module_id));
        for pair in modules.windows(2) {
            if pair[0].summary.module_id == pair[1].summary.module_id {
                return Err(ProductRuntimeToolOutcome::Failed {
                    code: "workspace_module_duplicate_module_id".to_owned(),
                    message: format!(
                        "multiple providers projected Workspace Module `{}`",
                        pair[0].summary.module_id
                    ),
                });
            }
        }
        Ok(modules)
    }

    pub async fn operate(
        &self,
        request: WorkspaceModuleOperateRequest<'_>,
    ) -> Result<Value, ProductRuntimeToolOutcome> {
        let matching = self
            .providers
            .iter()
            .filter(|provider| provider.handles_operate(request.operation))
            .collect::<Vec<_>>();
        let [provider] = matching.as_slice() else {
            return Err(ProductRuntimeToolOutcome::Rejected {
                code: if matching.is_empty() {
                    "workspace_module_operation_not_routable"
                } else {
                    "workspace_module_operation_route_ambiguous"
                }
                .to_owned(),
                message: format!(
                    "Workspace Module operation `{}` must resolve to exactly one provider",
                    request.operation
                ),
            });
        };
        provider.operate(request).await
    }

    pub async fn prepare_presentation(
        &self,
        request: WorkspaceModulePresentationRequest<'_>,
    ) -> Result<WorkspaceModulePresentationPreparation, ProductRuntimeToolOutcome> {
        let matching = self
            .providers
            .iter()
            .filter(|provider| provider.owns_module(request.module))
            .collect::<Vec<_>>();
        let [provider] = matching.as_slice() else {
            return Err(ProductRuntimeToolOutcome::Rejected {
                code: if matching.is_empty() {
                    "workspace_module_provider_not_found"
                } else {
                    "workspace_module_provider_ambiguous"
                }
                .to_owned(),
                message: format!(
                    "Workspace Module `{}` must resolve to exactly one provider",
                    request.module.summary.module_id
                ),
            });
        };
        provider.prepare_presentation(request).await
    }

    pub async fn present(
        &self,
        context: &WorkspaceModuleProviderContext,
        module: &WorkspaceModuleDescriptor,
        view_key: &str,
        payload: Option<Value>,
    ) -> Result<WorkspaceModulePresentation, ProductRuntimeToolOutcome> {
        let preparation = self
            .prepare_presentation(WorkspaceModulePresentationRequest {
                context,
                module,
                view_key,
                payload: payload.clone(),
            })
            .await?;
        let (target_module, target_view_key, diagnostics) = match preparation {
            WorkspaceModulePresentationPreparation::Requested => {
                (module.clone(), view_key.to_owned(), None)
            }
            WorkspaceModulePresentationPreparation::Redirected {
                module_id,
                view_key,
                diagnostics,
            } => {
                let target = self
                    .modules(context)
                    .await?
                    .into_iter()
                    .find(|candidate| candidate.summary.module_id == module_id)
                    .ok_or_else(|| ProductRuntimeToolOutcome::Failed {
                        code: "workspace_module_presentation_target_missing".to_owned(),
                        message: format!(
                            "prepared Workspace Module presentation target is missing: {module_id}"
                        ),
                    })?;
                (target, view_key, diagnostics)
            }
        };
        super::build_workspace_module_presentation(
            &target_module,
            &target_view_key,
            payload,
            diagnostics,
        )
        .map_err(|error| ProductRuntimeToolOutcome::Rejected {
            code: "workspace_module_presentation_invalid".to_owned(),
            message: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::workspace_module::{
        WorkspaceModuleKind, WorkspaceModuleStatus, WorkspaceModuleSummary,
    };

    struct FixtureProvider {
        key: &'static str,
        module_id: &'static str,
        operate_route: bool,
    }

    #[async_trait]
    impl WorkspaceModuleProvider for FixtureProvider {
        fn provider_key(&self) -> &str {
            self.key
        }

        async fn modules(
            &self,
            _context: &WorkspaceModuleProviderContext,
        ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
            Ok(vec![fixture_module(self.module_id)])
        }

        fn handles_operate(&self, _operation: &str) -> bool {
            self.operate_route
        }
    }

    fn context() -> WorkspaceModuleProviderContext {
        WorkspaceModuleProviderContext {
            project_id: uuid::Uuid::new_v4(),
            actor: WorkspaceModuleActor::User {
                user_id: "user-1".to_owned(),
            },
            invocation_id: "invocation-1".to_owned(),
            visibility: WorkspaceModuleDimension::all(),
            vfs_mounts: Vec::new(),
            operations: Vec::new(),
        }
    }

    fn fixture_module(module_id: &str) -> WorkspaceModuleDescriptor {
        WorkspaceModuleDescriptor {
            summary: WorkspaceModuleSummary {
                module_id: module_id.to_owned(),
                kind: WorkspaceModuleKind::new("fixture"),
                title: module_id.to_owned(),
                description: String::new(),
                source: module_id.to_owned(),
                ui_summary: None,
                operation_summary: Vec::new(),
                permission_summary: Vec::new(),
                status: WorkspaceModuleStatus::ready(),
            },
            ui_entries: Vec::new(),
            operations: Vec::new(),
            runtime_backing: None,
            agent_state_projection: None,
        }
    }

    #[test]
    #[should_panic(expected = "duplicate Workspace Module provider key")]
    fn duplicate_provider_keys_are_rejected_at_registration() {
        WorkspaceModuleProviderRegistry::new(vec![
            Arc::new(FixtureProvider {
                key: "fixture",
                module_id: "fixture:one",
                operate_route: false,
            }),
            Arc::new(FixtureProvider {
                key: "fixture",
                module_id: "fixture:two",
                operate_route: false,
            }),
        ]);
    }

    #[tokio::test]
    async fn duplicate_module_ids_are_rejected_across_providers() {
        let registry = WorkspaceModuleProviderRegistry::new(vec![
            Arc::new(FixtureProvider {
                key: "fixture.one",
                module_id: "fixture:same",
                operate_route: false,
            }),
            Arc::new(FixtureProvider {
                key: "fixture.two",
                module_id: "fixture:same",
                operate_route: false,
            }),
        ]);

        let error = registry
            .modules(&context())
            .await
            .expect_err("duplicate id");

        assert!(matches!(
            error,
            ProductRuntimeToolOutcome::Failed { code, .. }
                if code == "workspace_module_duplicate_module_id"
        ));
    }

    #[tokio::test]
    async fn operate_route_must_have_exactly_one_owner() {
        let registry = WorkspaceModuleProviderRegistry::new(vec![
            Arc::new(FixtureProvider {
                key: "fixture.one",
                module_id: "fixture:one",
                operate_route: true,
            }),
            Arc::new(FixtureProvider {
                key: "fixture.two",
                module_id: "fixture:two",
                operate_route: true,
            }),
        ]);
        let context = context();

        let error = registry
            .operate(WorkspaceModuleOperateRequest {
                context: &context,
                operation: "fixture.create",
                input: Value::Object(Default::default()),
            })
            .await
            .expect_err("ambiguous route");

        assert!(matches!(
            error,
            ProductRuntimeToolOutcome::Rejected { code, .. }
                if code == "workspace_module_operation_route_ambiguous"
        ));
    }
}
