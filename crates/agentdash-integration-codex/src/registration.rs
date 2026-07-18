use std::sync::Arc;

use agentdash_agent_service_api::{
    AgentServiceError, AgentServiceErrorCode, AgentServiceInstanceId, CompleteAgentService,
};

use crate::{
    CodexAppServerTransport, CodexCompleteAgentConfig, CodexCompleteAgentService,
    CodexProcessTransport,
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
    pub fn spawn(
        instance_id: AgentServiceInstanceId,
        config: CodexCompleteAgentConfig,
    ) -> Result<Self, AgentServiceError> {
        let transport = CodexProcessTransport::spawn(&config.cwd).map_err(|error| {
            AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                error.message,
                error.retryable,
            )
        })?;
        Self::new(instance_id, config, transport)
    }

    pub fn new(
        instance_id: AgentServiceInstanceId,
        config: CodexCompleteAgentConfig,
        transport: Arc<dyn CodexAppServerTransport>,
    ) -> Result<Self, AgentServiceError> {
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
