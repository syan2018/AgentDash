use std::sync::Arc;

use agentdash_agent_service_api::{
    AgentServiceError, AgentServiceErrorCode, AgentServiceInstanceId, CompleteAgentService,
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
