use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeChangeSequence;
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBindingRepository, AgentRunRuntimeTarget,
};
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationChangePage, WorkspaceModulePresentationChangeSequence,
    WorkspaceModulePresentationRepository, WorkspaceModulePresentationSnapshot,
};
use thiserror::Error;

use super::product_protocol::AgentRunRuntimeProjectionPort;
use super::terminal_projection_protocol::{
    AgentRunTerminalChangePage, AgentRunTerminalChangeSequence,
    AgentRunTerminalProjectionRepository, AgentRunTerminalSnapshot,
};

pub struct AgentRunProductProjectionGateway {
    runtime_bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    runtime_projection: Arc<dyn AgentRunRuntimeProjectionPort>,
    workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
    terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
}

impl AgentRunProductProjectionGateway {
    pub fn new(
        runtime_bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        runtime_projection: Arc<dyn AgentRunRuntimeProjectionPort>,
        workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
        terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
    ) -> Self {
        Self {
            runtime_bindings,
            runtime_projection,
            workspace_presentations,
            terminals,
        }
    }

    pub async fn runtime_snapshot(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<
        agentdash_agent_runtime_contract::ManagedRuntimeSnapshot,
        AgentRunProductProjectionError,
    > {
        let binding = self
            .runtime_bindings
            .load(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Binding(error.to_string()))?
            .ok_or(AgentRunProductProjectionError::TargetNotBound)?;
        let snapshot = self
            .runtime_projection
            .load_snapshot(&binding.thread_id)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        if snapshot.thread_id != binding.thread_id {
            return Err(AgentRunProductProjectionError::RuntimeThreadMismatch);
        }
        Ok(snapshot)
    }

    pub async fn runtime_changes(
        &self,
        target: &AgentRunRuntimeTarget,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<
        agentdash_agent_runtime_contract::ManagedRuntimeChangePage,
        AgentRunProductProjectionError,
    > {
        let binding = self
            .runtime_bindings
            .load(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Binding(error.to_string()))?
            .ok_or(AgentRunProductProjectionError::TargetNotBound)?;
        let page = self
            .runtime_projection
            .load_changes(&binding.thread_id, after)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        if page.thread_id != binding.thread_id
            || page
                .changes
                .iter()
                .any(|change| change.thread_id != binding.thread_id)
        {
            return Err(AgentRunProductProjectionError::RuntimeThreadMismatch);
        }
        Ok(page)
    }

    pub async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
        let snapshot = self
            .workspace_presentations
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if snapshot.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn workspace_presentation_changes(
        &self,
        target: &AgentRunRuntimeTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
        let page = self
            .workspace_presentations
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if page.target != *target || page.changes.iter().any(|change| change.target != *target) {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }

    pub async fn terminal_snapshot(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
        let snapshot = self
            .terminals
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if snapshot.target != *target
            || snapshot
                .terminals
                .iter()
                .any(|terminal| terminal.owner.target != *target)
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn terminal_changes(
        &self,
        target: &AgentRunRuntimeTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
        let page = self
            .terminals
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if page.target != *target
            || page
                .changes
                .iter()
                .any(|change| change.target != *target || change.delta.owner().target != *target)
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunProductProjectionError {
    #[error("AgentRun Runtime binding load failed: {0}")]
    Binding(String),
    #[error("AgentRun target has no committed Runtime binding")]
    TargetNotBound,
    #[error("Managed Runtime projection load failed: {0}")]
    Runtime(String),
    #[error("Managed Runtime projection returned a different Runtime thread")]
    RuntimeThreadMismatch,
    #[error("Product projection returned a different AgentRun target")]
    TargetMismatch,
    #[error("Workspace Module presentation projection load failed: {0}")]
    Workspace(String),
    #[error("AgentRun terminal projection load failed: {0}")]
    Terminal(String),
}
