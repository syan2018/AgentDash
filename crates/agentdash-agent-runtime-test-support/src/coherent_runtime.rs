use std::collections::BTreeMap;

use agentdash_agent_protocol::CanonicalConversationRecord;
use agentdash_agent_runtime_contract::{
    AgentContextAuthority, AgentContextCoordinate, AgentContextFidelity, AgentControlAvailability,
    AgentControlKind, AgentExecutionSnapshot, AgentLifecycleStatus, AgentObservation,
    AgentPayloadDigest, AgentRuntimeView, AgentSnapshot, AgentSnapshotAuthority,
    AgentSnapshotRevision, AgentSnapshotSource, AgentSourceCoordinate, RuntimeThreadId,
    SemanticFidelity,
};

/// 构造内部坐标一致的 Agent observation，测试只覆盖自己关心的差异字段。
pub struct CoherentAgentObservationBuilder {
    revision: AgentSnapshotRevision,
    lifecycle: AgentLifecycleStatus,
    execution: AgentExecutionSnapshot,
    command_availability: Option<BTreeMap<AgentControlKind, AgentControlAvailability>>,
    conversation: Vec<CanonicalConversationRecord>,
    observed_at_ms: u64,
}

impl CoherentAgentObservationBuilder {
    pub fn new(revision: u64) -> Self {
        Self {
            revision: AgentSnapshotRevision(revision),
            lifecycle: AgentLifecycleStatus::Active,
            execution: AgentExecutionSnapshot {
                active_turn: None,
                queued_compaction: None,
                last_compaction_outcome: None,
            },
            command_availability: None,
            conversation: Vec::new(),
            observed_at_ms: revision,
        }
    }

    pub fn execution(mut self, execution: AgentExecutionSnapshot) -> Self {
        self.execution = execution;
        self
    }

    pub fn command_availability(
        mut self,
        command_availability: BTreeMap<AgentControlKind, AgentControlAvailability>,
    ) -> Self {
        self.command_availability = Some(command_availability);
        self
    }

    pub fn conversation(mut self, conversation: Vec<CanonicalConversationRecord>) -> Self {
        self.conversation = conversation;
        self
    }

    pub fn observed_at_ms(mut self, observed_at_ms: u64) -> Self {
        self.observed_at_ms = observed_at_ms;
        self
    }

    pub fn observation(self) -> AgentObservation {
        let command_availability = self.command_availability.unwrap_or_else(|| {
            self.execution
                .command_availability(self.lifecycle, self.revision, false)
        });
        AgentObservation {
            revision: self.revision,
            context: AgentContextCoordinate {
                snapshot_revision: self.revision,
                context_revision: Some(format!("context-{}", self.revision.0)),
                recipe_digest: AgentPayloadDigest::new(format!(
                    "sha256:context-{}",
                    self.revision.0
                ))
                .expect("fixture digest is valid"),
                authority: AgentContextAuthority::AgentOwned,
                fidelity: AgentContextFidelity::Exact,
            },
            lifecycle: self.lifecycle,
            execution: self.execution,
            command_availability,
            interactions: Vec::new(),
            thread_name: None,
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: None,
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: self.observed_at_ms,
            },
            conversation: self.conversation,
        }
    }

    pub fn snapshot(self, source: &str) -> AgentSnapshot {
        AgentSnapshot {
            source: AgentSourceCoordinate::new(source).expect("fixture source is valid"),
            observation: self.observation(),
            applied_surface: None,
            initial_context: None,
        }
    }

    pub fn runtime_view(self, thread_id: &str) -> AgentRuntimeView {
        AgentRuntimeView {
            thread_id: RuntimeThreadId::new(thread_id).expect("fixture thread is valid"),
            observation: self.observation(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_keeps_observation_and_context_revision_coherent() {
        let snapshot = CoherentAgentObservationBuilder::new(7).snapshot("source-1");
        assert_eq!(snapshot.observation.revision, AgentSnapshotRevision(7));
        assert_eq!(
            snapshot.observation.context.snapshot_revision,
            snapshot.observation.revision
        );
        assert_eq!(
            snapshot.observation.context.context_revision.as_deref(),
            Some("context-7")
        );
    }
}
