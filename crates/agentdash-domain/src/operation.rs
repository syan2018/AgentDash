use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationProviderRef {
    pub namespace: String,
    pub provider_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationRef {
    pub provider: OperationProviderRef,
    pub operation_key: String,
    pub contract_version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEffect {
    Read,
    LocalMutation,
    ExternalSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationReplayPolicy {
    NonReplayable,
    Idempotent,
    ReplaySafe,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationPrincipalRef {
    User { user_id: String },
    AgentRunAgent { run_id: Uuid, agent_id: Uuid },
    WorkflowNode { run_id: Uuid, node_key: String },
    ExtensionInstallation { installation_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationScopeRef {
    EnvironmentSetup {
        project_id: Option<Uuid>,
        workspace_id: Option<Uuid>,
        backend_id: Option<String>,
    },
    Project {
        project_id: Uuid,
    },
    InteractionInstance {
        instance_id: Uuid,
    },
    WorkspaceBinding {
        project_id: Uuid,
        workspace_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationOriginRef {
    EnvironmentSetup,
    AgentTool,
    UserWorkshop,
    Canvas {
        definition_id: Uuid,
    },
    Interaction {
        instance_id: Uuid,
    },
    ComponentEvent {
        instance_id: Uuid,
        component_key: String,
    },
    ExtensionPanel {
        installation_id: Uuid,
    },
    Workflow,
    OperationScriptNested {
        script_invocation_id: String,
    },
    EffectReplay {
        effect_id: Uuid,
    },
    ExtensionService,
}

impl OperationPrincipalRef {
    pub fn validate(&self) -> Result<(), OperationContextRefError> {
        match self {
            Self::User { user_id } => validate_non_empty("principal.user_id", user_id),
            Self::AgentRunAgent { run_id, agent_id } => {
                validate_uuid("principal.run_id", *run_id)?;
                validate_uuid("principal.agent_id", *agent_id)
            }
            Self::WorkflowNode { run_id, node_key } => {
                validate_uuid("principal.run_id", *run_id)?;
                validate_non_empty("principal.node_key", node_key)
            }
            Self::ExtensionInstallation { installation_id } => {
                validate_uuid("principal.installation_id", *installation_id)
            }
        }
    }
}

impl OperationScopeRef {
    pub fn validate(&self) -> Result<(), OperationContextRefError> {
        match self {
            Self::EnvironmentSetup {
                project_id,
                workspace_id,
                backend_id,
            } => {
                if let Some(project_id) = project_id {
                    validate_uuid("scope.project_id", *project_id)?;
                }
                if let Some(workspace_id) = workspace_id {
                    validate_uuid("scope.workspace_id", *workspace_id)?;
                    if project_id.is_none() {
                        return Err(OperationContextRefError::InvalidCombination {
                            reason: "EnvironmentSetup workspace scope 必须同时携带 project_id",
                        });
                    }
                }
                if let Some(backend_id) = backend_id {
                    validate_non_empty("scope.backend_id", backend_id)?;
                }
                Ok(())
            }
            Self::Project { project_id } => validate_uuid("scope.project_id", *project_id),
            Self::InteractionInstance { instance_id } => {
                validate_uuid("scope.instance_id", *instance_id)
            }
            Self::WorkspaceBinding {
                project_id,
                workspace_id,
            } => {
                validate_uuid("scope.project_id", *project_id)?;
                validate_uuid("scope.workspace_id", *workspace_id)
            }
        }
    }
}

impl OperationOriginRef {
    pub fn validate(&self) -> Result<(), OperationContextRefError> {
        match self {
            Self::EnvironmentSetup
            | Self::AgentTool
            | Self::UserWorkshop
            | Self::Workflow
            | Self::ExtensionService => Ok(()),
            Self::Canvas { definition_id } => validate_uuid("origin.definition_id", *definition_id),
            Self::Interaction { instance_id }
            | Self::EffectReplay {
                effect_id: instance_id,
            } => validate_uuid("origin.object_id", *instance_id),
            Self::ComponentEvent {
                instance_id,
                component_key,
            } => {
                validate_uuid("origin.instance_id", *instance_id)?;
                validate_non_empty("origin.component_key", component_key)
            }
            Self::ExtensionPanel { installation_id } => {
                validate_uuid("origin.installation_id", *installation_id)
            }
            Self::OperationScriptNested {
                script_invocation_id,
            } => validate_non_empty("origin.script_invocation_id", script_invocation_id),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationContextRefError {
    #[error("Operation context ref 字段无效: {field}")]
    InvalidField { field: &'static str },
    #[error("Operation context ref 组合无效: {reason}")]
    InvalidCombination { reason: &'static str },
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), OperationContextRefError> {
    if value.trim().is_empty() || value.trim() != value {
        Err(OperationContextRefError::InvalidField { field })
    } else {
        Ok(())
    }
}

fn validate_uuid(field: &'static str, value: Uuid) -> Result<(), OperationContextRefError> {
    if value.is_nil() {
        Err(OperationContextRefError::InvalidField { field })
    } else {
        Ok(())
    }
}

impl OperationRef {
    pub fn new(
        namespace: impl Into<String>,
        provider_key: impl Into<String>,
        operation_key: impl Into<String>,
        contract_version: u16,
    ) -> Result<Self, OperationRefError> {
        let operation_ref = Self {
            provider: OperationProviderRef {
                namespace: namespace.into(),
                provider_key: provider_key.into(),
            },
            operation_key: operation_key.into(),
            contract_version,
        };
        operation_ref.validate()?;
        Ok(operation_ref)
    }

    pub fn validate(&self) -> Result<(), OperationRefError> {
        validate_segment("provider.namespace", &self.provider.namespace)?;
        validate_segment("provider.provider_key", &self.provider.provider_key)?;
        validate_segment("operation_key", &self.operation_key)?;
        if self.contract_version == 0 {
            return Err(OperationRefError::InvalidVersion);
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationRefError {
    #[error("OperationRef 字段无效: {field}")]
    InvalidSegment { field: &'static str },
    #[error("OperationRef contract_version 必须大于 0")]
    InvalidVersion,
}

fn validate_segment(field: &'static str, value: &str) -> Result<(), OperationRefError> {
    let valid = !value.is_empty()
        && value.trim() == value
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        });
    if valid {
        Ok(())
    } else {
        Err(OperationRefError::InvalidSegment { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ref_is_provider_qualified_and_versioned() {
        let operation_ref = OperationRef::new("extension", "acme.weather", "forecast", 1)
            .expect("valid operation ref");
        assert_eq!(operation_ref.provider.namespace, "extension");
        assert_eq!(operation_ref.contract_version, 1);
    }

    #[test]
    fn operation_ref_rejects_unstable_segments() {
        assert!(matches!(
            OperationRef::new("extension", "bad provider", "forecast", 1),
            Err(OperationRefError::InvalidSegment { .. })
        ));
    }

    #[test]
    fn context_refs_reject_empty_and_ambiguous_authority() {
        assert!(
            OperationPrincipalRef::User {
                user_id: " ".to_string()
            }
            .validate()
            .is_err()
        );
        assert!(
            OperationScopeRef::EnvironmentSetup {
                project_id: None,
                workspace_id: Some(Uuid::new_v4()),
                backend_id: None,
            }
            .validate()
            .is_err()
        );
        assert!(
            OperationOriginRef::OperationScriptNested {
                script_invocation_id: String::new()
            }
            .validate()
            .is_err()
        );
    }
}
