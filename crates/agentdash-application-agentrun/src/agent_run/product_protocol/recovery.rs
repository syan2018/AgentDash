use thiserror::Error;

use super::{
    AgentRunForkSagaRepositoryError, AgentRunForkSagaWorker, AgentRunProductProtocolPorts,
    CompanionFreshRepositoryError, CompanionFreshSagaWorker,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRunProductProtocolRecoveryReport {
    pub fork_sagas_advanced: usize,
    pub fresh_sagas_advanced: usize,
    pub failures: Vec<AgentRunProductProtocolRecoveryFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductProtocolRecoveryFailure {
    pub protocol: &'static str,
    pub request_id: String,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum AgentRunProductProtocolRecoveryError {
    #[error("failed to scan recoverable AgentRun fork sagas: {0}")]
    ForkScan(#[from] AgentRunForkSagaRepositoryError),
    #[error("failed to scan recoverable fresh Companion sagas: {0}")]
    FreshScan(#[from] CompanionFreshRepositoryError),
}

/// Restart-safe Product protocol recovery over the same durable repositories and Runtime ports
/// used by foreground dispatch.
pub struct AgentRunProductProtocolRecoveryWorker<'a> {
    ports: &'a AgentRunProductProtocolPorts,
}

impl<'a> AgentRunProductProtocolRecoveryWorker<'a> {
    pub fn new(ports: &'a AgentRunProductProtocolPorts) -> Self {
        Self { ports }
    }

    /// Advances every selected saga by one durable state-machine step. Repeated ticks converge
    /// without allocating a second child because each Runtime effect identity is persisted before
    /// dispatch.
    pub async fn advance_batch(
        &self,
        limit: usize,
    ) -> Result<AgentRunProductProtocolRecoveryReport, AgentRunProductProtocolRecoveryError> {
        let fork_request_ids = self.ports.fork_sagas.list_recoverable(limit).await?;
        let fork_worker = AgentRunForkSagaWorker::new(
            self.ports.fork_sagas.as_ref(),
            self.ports.fork_runtime.as_ref(),
            self.ports.fork_product_graph.as_ref(),
        );
        let mut fork_sagas_advanced = 0;
        let mut failures = Vec::new();
        for request_id in &fork_request_ids {
            match fork_worker.advance(request_id).await {
                Ok(_) => fork_sagas_advanced += 1,
                Err(error) => failures.push(AgentRunProductProtocolRecoveryFailure {
                    protocol: "agent_run_fork",
                    request_id: request_id.0.to_string(),
                    reason: error.to_string(),
                }),
            }
        }

        let fresh_request_ids = self
            .ports
            .companion_fresh_sagas
            .list_recoverable(limit)
            .await?;
        let fresh_worker = CompanionFreshSagaWorker::new(
            self.ports.companion_fresh_sagas.as_ref(),
            self.ports.companion_fresh_runtime.as_ref(),
        );
        let mut fresh_sagas_advanced = 0;
        for request_id in &fresh_request_ids {
            match fresh_worker.advance(request_id).await {
                Ok(_) => fresh_sagas_advanced += 1,
                Err(error) => failures.push(AgentRunProductProtocolRecoveryFailure {
                    protocol: "companion_fresh",
                    request_id: request_id.0.to_string(),
                    reason: error.to_string(),
                }),
            }
        }

        Ok(AgentRunProductProtocolRecoveryReport {
            fork_sagas_advanced,
            fresh_sagas_advanced,
            failures,
        })
    }
}
