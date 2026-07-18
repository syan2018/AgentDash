use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimeChangePage, ManagedRuntimeSnapshot,
    ManagedRuntimeSourceBindingEvidence, RuntimeChangeSequence, RuntimeThreadId,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAcknowledgePort, WorkspaceModulePresentationAcknowledgeRequest,
    WorkspaceModulePresentationChange, WorkspaceModulePresentationChangePage,
    WorkspaceModulePresentationChangeSequence, WorkspaceModulePresentationRepository,
    WorkspaceModulePresentationSnapshot,
};
use async_trait::async_trait;
use thiserror::Error;

use super::product_protocol::AgentRunRuntimeProjectionPort;
use super::terminal_projection_protocol::{
    AgentRunTerminalChangePage, AgentRunTerminalChangeSequence,
    AgentRunTerminalProjectionRepository, AgentRunTerminalSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductRuntimeBinding {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub source_binding: ManagedRuntimeSourceBindingEvidence,
}

#[async_trait]
pub trait AgentRunProductRuntimeBindingRepository: Send + Sync {
    async fn load_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String>;
}

pub struct AgentRunProductProjectionGateway {
    runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    runtime_projection: Arc<dyn AgentRunRuntimeProjectionPort>,
    workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
    workspace_presentation_acknowledgements: Arc<dyn WorkspaceModulePresentationAcknowledgePort>,
    terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
}

impl AgentRunProductProjectionGateway {
    pub fn new(
        runtime_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        runtime_projection: Arc<dyn AgentRunRuntimeProjectionPort>,
        workspace_presentations: Arc<dyn WorkspaceModulePresentationRepository>,
        workspace_presentation_acknowledgements: Arc<
            dyn WorkspaceModulePresentationAcknowledgePort,
        >,
        terminals: Arc<dyn AgentRunTerminalProjectionRepository>,
    ) -> Self {
        Self {
            runtime_bindings,
            runtime_projection,
            workspace_presentations,
            workspace_presentation_acknowledgements,
            terminals,
        }
    }

    async fn binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunProductRuntimeBinding, AgentRunProductProjectionError> {
        let binding = self
            .runtime_bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunProductProjectionError::Binding)?
            .ok_or(AgentRunProductProjectionError::TargetNotBound)?;
        if binding.target != *target {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(binding)
    }

    pub async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let snapshot = self
            .runtime_projection
            .load_snapshot(&binding.runtime_thread_id)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        if snapshot.thread_id != binding.runtime_thread_id {
            return Err(AgentRunProductProjectionError::RuntimeThreadMismatch);
        }
        if snapshot.source_binding.as_ref() != Some(&binding.source_binding) {
            return Err(AgentRunProductProjectionError::RuntimeSourceBindingMismatch);
        }
        Ok(snapshot)
    }

    pub async fn runtime_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<ManagedRuntimeChangePage, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let snapshot = self
            .runtime_projection
            .load_snapshot(&binding.runtime_thread_id)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        if snapshot.thread_id != binding.runtime_thread_id {
            return Err(AgentRunProductProjectionError::RuntimeThreadMismatch);
        }
        if snapshot.source_binding.as_ref() != Some(&binding.source_binding) {
            return Err(AgentRunProductProjectionError::RuntimeSourceBindingMismatch);
        }
        let page = self
            .runtime_projection
            .load_changes(&binding.runtime_thread_id, after)
            .await
            .map_err(AgentRunProductProjectionError::Runtime)?;
        if page.thread_id != binding.runtime_thread_id
            || page
                .changes
                .iter()
                .any(|change| change.thread_id != binding.runtime_thread_id)
        {
            return Err(AgentRunProductProjectionError::RuntimeThreadMismatch);
        }
        if page.changes.iter().any(|change| {
            matches!(
                &change.delta,
                ManagedRuntimeChangeDelta::SourceBindingChanged { binding: changed }
                    if changed.as_ref() != Some(&binding.source_binding)
            )
        }) {
            return Err(AgentRunProductProjectionError::RuntimeSourceBindingMismatch);
        }
        Ok(page)
    }

    pub async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let snapshot = self
            .workspace_presentations
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if snapshot.target != *target
            || snapshot.pending_intents.iter().any(|pending| {
                pending.intent.target != *target
                    || pending.intent.currentness_fence.runtime_thread_id
                        != binding.runtime_thread_id
                    || pending.intent.currentness_fence.source_binding != binding.source_binding
            })
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let page = self
            .workspace_presentations
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if page.target != *target
            || page.changes.iter().any(|change| {
                change.target != *target
                    || change.intent.currentness_fence.runtime_thread_id
                        != binding.runtime_thread_id
                    || change.intent.currentness_fence.source_binding != binding.source_binding
            })
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }

    pub async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError> {
        let binding = self.binding(&request.target).await?;
        let target = request.target.clone();
        let change = self
            .workspace_presentation_acknowledgements
            .acknowledge(request)
            .await
            .map_err(|error| AgentRunProductProjectionError::Workspace(error.to_string()))?;
        if change.target != target
            || change.intent.target != target
            || change.intent.currentness_fence.runtime_thread_id != binding.runtime_thread_id
            || change.intent.currentness_fence.source_binding != binding.source_binding
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(change)
    }

    pub async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let snapshot = self
            .terminals
            .load_snapshot(target)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if snapshot.target != *target
            || snapshot.terminals.iter().any(|terminal| {
                terminal.owner.target != *target
                    || terminal.owner.runtime_thread_id != binding.runtime_thread_id
                    || terminal.owner.source_binding != binding.source_binding
            })
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(snapshot)
    }

    pub async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
        let binding = self.binding(target).await?;
        let page = self
            .terminals
            .load_changes(target, after, limit)
            .await
            .map_err(|error| AgentRunProductProjectionError::Terminal(error.to_string()))?;
        if page.target != *target
            || page.changes.iter().any(|change| {
                change.target != *target
                    || change.delta.owner().target != *target
                    || change.delta.owner().runtime_thread_id != binding.runtime_thread_id
                    || change.delta.owner().source_binding != binding.source_binding
            })
        {
            return Err(AgentRunProductProjectionError::TargetMismatch);
        }
        Ok(page)
    }
}

#[async_trait]
pub trait AgentRunProductProjectionQueryPort: Send + Sync {
    async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError>;
    async fn runtime_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<ManagedRuntimeChangePage, AgentRunProductProjectionError>;
    async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError>;
    async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError>;
    async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError>;
    async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError>;
    async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError>;
}

#[async_trait]
impl AgentRunProductProjectionQueryPort for AgentRunProductProjectionGateway {
    async fn runtime_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ManagedRuntimeSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_snapshot(self, target).await
    }

    async fn runtime_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<ManagedRuntimeChangePage, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::runtime_changes(self, target, after).await
    }

    async fn workspace_presentation_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::workspace_presentation_snapshot(self, target).await
    }

    async fn workspace_presentation_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::workspace_presentation_changes(self, target, after, limit)
            .await
    }

    async fn acknowledge_workspace_presentation(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::acknowledge_workspace_presentation(self, request).await
    }

    async fn terminal_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::terminal_snapshot(self, target).await
    }

    async fn terminal_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<AgentRunTerminalChangePage, AgentRunProductProjectionError> {
        AgentRunProductProjectionGateway::terminal_changes(self, target, after, limit).await
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
    #[error("Managed Runtime projection returned different source binding evidence")]
    RuntimeSourceBindingMismatch,
    #[error("Product projection returned a different AgentRun target")]
    TargetMismatch,
    #[error("Workspace Module presentation projection load failed: {0}")]
    Workspace(String),
    #[error("AgentRun terminal projection load failed: {0}")]
    Terminal(String),
}
