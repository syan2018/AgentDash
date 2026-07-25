use std::sync::Arc;

use agentdash_agent_service_api::{
    AgentPayloadDigest, AgentServiceDefinitionId, AgentServiceError, AgentServiceErrorCode,
    AgentServiceInstanceId, CompleteAgentService,
};
use agentdash_integration_api::{
    AgentDashIntegration, CompleteAgentPlacementRequirement, CompleteAgentRegistrationClaim,
    CompleteAgentRegistrationContribution, CompleteAgentServiceFactory,
    CompleteAgentServiceFactoryError,
};
use codex_app_server_protocol::{
    ClientInfo, InitializeCapabilities, InitializeParams, InitializeResponse,
};

use crate::{
    complete_agent::{
        CodexAppServerTransport, CodexCompleteAgentConfig, CodexCompleteAgentService,
    },
    process_transport::CodexProcessTransport,
};

pub const CODEX_COMPLETE_AGENT_DEFINITION_ID: &str = "builtin.codex-app-server";
pub const CODEX_COMPLETE_AGENT_INSTANCE_ID: &str = "builtin.codex-app-server.default";
pub const CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE: &str = "codex-complete-agent-v1";

struct CodexProcessCompleteAgentFactory;

#[async_trait::async_trait]
impl CompleteAgentServiceFactory for CodexProcessCompleteAgentFactory {
    async fn materialize(
        &self,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
        let cwd = std::env::current_dir().map_err(|error| {
            CompleteAgentServiceFactoryError::InvalidConfiguration {
                reason: format!("failed to resolve Codex working directory: {error}"),
            }
        })?;
        let registration = CodexCompleteAgentRegistration::spawn(
            AgentServiceInstanceId::new(CODEX_COMPLETE_AGENT_INSTANCE_ID)
                .expect("static Codex Complete Agent instance id"),
            CodexCompleteAgentConfig {
                definition_id: AgentServiceDefinitionId::new(CODEX_COMPLETE_AGENT_DEFINITION_ID)
                    .expect("static Codex Complete Agent definition id"),
                title: "Codex App Server".to_owned(),
                cwd,
                model: None,
                model_provider: None,
                base_instructions: None,
                developer_instructions: None,
                runtime_workspace_roots: Vec::new(),
            },
        )
        .await
        .map_err(map_factory_error)?;
        Ok(registration.service())
    }
}

pub struct CodexCompleteAgentIntegration;

impl AgentDashIntegration for CodexCompleteAgentIntegration {
    fn name(&self) -> &str {
        "builtin.codex_runtime"
    }

    fn complete_agent_registrations(&self) -> Vec<CompleteAgentRegistrationContribution> {
        vec![codex_complete_agent_contribution()]
    }
}

pub fn codex_complete_agent_contribution() -> CompleteAgentRegistrationContribution {
    let declared_descriptor = codex_complete_agent_descriptor();
    CompleteAgentRegistrationContribution::new(
        declared_descriptor,
        AgentServiceInstanceId::new(CODEX_COMPLETE_AGENT_INSTANCE_ID)
            .expect("static Codex Complete Agent instance id"),
        CompleteAgentPlacementRequirement::InProcess,
        None,
        CompleteAgentRegistrationClaim {
            publisher_integration: "builtin.codex_runtime".to_owned(),
            service_version: crate::CODEX_APP_SERVER_PROTOCOL_REVISION.to_string(),
            claimed_service_build_digest: AgentPayloadDigest::new(format!(
                "codex-app-server:{}",
                crate::CODEX_APP_SERVER_PROTOCOL_REVISION
            ))
            .expect("static Codex Complete Agent build digest"),
            claimed_conformance_suite_revision: CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE.to_owned(),
        },
        Arc::new(CodexProcessCompleteAgentFactory),
    )
    .expect("static Codex Complete Agent contribution")
}

pub fn codex_complete_agent_descriptor() -> agentdash_agent_service_api::AgentServiceDescriptor {
    CodexCompleteAgentService::descriptor_for(
        AgentServiceDefinitionId::new(CODEX_COMPLETE_AGENT_DEFINITION_ID)
            .expect("static Codex Complete Agent definition id"),
        "Codex App Server",
    )
}

/// Host-ready registration for one Codex App Server Complete Agent instance.
///
/// The component contains only the stable service instance identity and Complete Agent service.
/// Process placement and transport construction stay at the composition root.
pub struct CodexCompleteAgentRegistration {
    instance_id: AgentServiceInstanceId,
    service: Arc<CodexCompleteAgentService>,
}

impl CodexCompleteAgentRegistration {
    pub async fn spawn(
        instance_id: AgentServiceInstanceId,
        config: CodexCompleteAgentConfig,
    ) -> Result<Self, AgentServiceError> {
        let transport = CodexProcessTransport::spawn(&config.cwd)
            .map_err(map_initialization_transport_error)?;
        Self::new(instance_id, config, transport).await
    }

    pub async fn new(
        instance_id: AgentServiceInstanceId,
        config: CodexCompleteAgentConfig,
        transport: Arc<dyn CodexAppServerTransport>,
    ) -> Result<Self, AgentServiceError> {
        initialize_transport(transport.as_ref()).await?;
        Ok(Self {
            instance_id,
            service: Arc::new(CodexCompleteAgentService::new(config, transport)?),
        })
    }

    pub fn instance_id(&self) -> &AgentServiceInstanceId {
        &self.instance_id
    }

    pub fn service(&self) -> Arc<dyn CompleteAgentService> {
        self.service.clone()
    }

    pub fn into_parts(self) -> (AgentServiceInstanceId, Arc<dyn CompleteAgentService>) {
        (self.instance_id, self.service)
    }
}

async fn initialize_transport(
    transport: &dyn CodexAppServerTransport,
) -> Result<InitializeResponse, AgentServiceError> {
    let params = InitializeParams {
        client_info: ClientInfo {
            name: "agentdash".to_owned(),
            title: Some("AgentDash".to_owned()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        capabilities: Some(InitializeCapabilities {
            experimental_api: true,
            request_attestation: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: None,
        }),
    };
    let params = serde_json::to_value(params).map_err(|error| {
        AgentServiceError::new(
            AgentServiceErrorCode::Internal,
            format!("failed to encode Codex initialize params: {error}"),
            false,
        )
    })?;
    let response = transport
        .request("initialize", params)
        .await
        .map_err(map_initialization_transport_error)?;
    let response = serde_json::from_value::<InitializeResponse>(response).map_err(|error| {
        AgentServiceError::new(
            AgentServiceErrorCode::ProtocolViolation,
            format!("invalid Codex initialize response: {error}"),
            false,
        )
    })?;
    if response.user_agent.trim().is_empty()
        || response.platform_family.trim().is_empty()
        || response.platform_os.trim().is_empty()
    {
        return Err(AgentServiceError::new(
            AgentServiceErrorCode::ProtocolViolation,
            "Codex initialize response contains empty server identity fields",
            false,
        ));
    }
    transport
        .notify("initialized", None)
        .await
        .map_err(map_initialization_transport_error)?;
    Ok(response)
}

fn map_initialization_transport_error(
    error: crate::CodexCompleteAgentTransportError,
) -> AgentServiceError {
    AgentServiceError::new(
        if error.retryable {
            AgentServiceErrorCode::Unavailable
        } else {
            AgentServiceErrorCode::ProtocolViolation
        },
        error.message,
        error.retryable,
    )
}

fn map_factory_error(error: AgentServiceError) -> CompleteAgentServiceFactoryError {
    match error.code {
        AgentServiceErrorCode::InvalidArgument => {
            CompleteAgentServiceFactoryError::InvalidConfiguration {
                reason: error.message,
            }
        }
        AgentServiceErrorCode::Unavailable => CompleteAgentServiceFactoryError::Unavailable {
            reason: error.message,
            retryable: error.retryable,
        },
        _ => CompleteAgentServiceFactoryError::Unhealthy {
            reason: error.message,
            retryable: error.retryable,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_contribution_exposes_complete_agent_definition_and_no_runtime_driver() {
        let contribution = codex_complete_agent_contribution();

        assert_eq!(
            contribution
                .facts()
                .declared_descriptor()
                .definition_id
                .as_str(),
            CODEX_COMPLETE_AGENT_DEFINITION_ID
        );
        assert_eq!(
            contribution.facts().instance_id().as_str(),
            CODEX_COMPLETE_AGENT_INSTANCE_ID
        );
        assert_eq!(
            contribution
                .facts()
                .registration_claim()
                .claimed_conformance_suite_revision,
            CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE
        );
        assert!(matches!(
            contribution.facts().placement(),
            CompleteAgentPlacementRequirement::InProcess
        ));
    }
}
