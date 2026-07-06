use async_trait::async_trait;
use uuid::Uuid;

use agentdash_spi::{AgentConfig, AuthIdentity};

#[derive(Debug, Clone)]
pub struct CompanionModelPreflightRequest {
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub selected_project_agent_id: Uuid,
    pub selected_agent_key: String,
    pub companion_label: String,
    pub executor_config: AgentConfig,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionModelPreflightError {
    pub message: String,
}

impl CompanionModelPreflightError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait CompanionModelPreflightPort: Send + Sync {
    async fn preflight_companion_model(
        &self,
        request: CompanionModelPreflightRequest,
    ) -> Result<(), CompanionModelPreflightError>;
}
