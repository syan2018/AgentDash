use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentRuntimeOffer, AgentServiceDescriptor, AgentServiceInstanceId,
    AgentSourceCoordinate, CompleteAgentService,
};
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
    CompleteAgentEffectRecord,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompleteAgentHostRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentHostFacts {
    pub service_instances: BTreeMap<AgentServiceInstanceId, AgentServiceDescriptor>,
    pub offers: BTreeMap<AgentServiceInstanceId, AgentRuntimeOffer>,
    pub bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    pub source_coordinates: BTreeMap<CompleteAgentBindingId, AgentSourceCoordinate>,
    pub effects: BTreeMap<AgentEffectIdentity, CompleteAgentEffectRecord>,
    pub leases: BTreeMap<CompleteAgentBindingId, CompleteAgentBindingLease>,
    pub lease_epochs: BTreeMap<CompleteAgentBindingId, u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentHostSnapshot {
    pub revision: CompleteAgentHostRevision,
    pub facts: CompleteAgentHostFacts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentHostCommit {
    pub expected_revision: CompleteAgentHostRevision,
    pub facts: CompleteAgentHostFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentHostStoreError {
    #[error("Complete Agent Host revision conflict: expected {expected:?}, actual {actual:?}")]
    Conflict {
        expected: CompleteAgentHostRevision,
        actual: CompleteAgentHostRevision,
    },
    #[error("Complete Agent Host persistence invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("Complete Agent Host persistence failed: {reason}")]
    Persistence { reason: String },
}

/// Durable authority for Complete Agent service, offer, binding, source, effect, lease, and
/// generation facts.
///
/// A commit is one Host transaction. Implementations must compare `expected_revision`, validate
/// the complete fact graph, and atomically persist every changed fact before advancing revision.
/// Replaying the exact already-committed fact graph is idempotent even when the expected revision
/// is stale; a different graph returns `Conflict`.
#[async_trait]
pub trait CompleteAgentHostRepository: Send + Sync {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;
}

/// Process-local resolver for live Complete Agent service handles.
///
/// Service handles are deliberately outside durable Host facts. Production composition attaches
/// handles from the final Complete Agent registrations; a reconstructed Host resolves the same
/// durable service instance through this port.
#[async_trait]
pub trait CompleteAgentServiceRegistry: Send + Sync {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    );

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>>;
}

#[derive(Default)]
pub struct RecordingCompleteAgentServiceRegistry {
    handles: RwLock<BTreeMap<AgentServiceInstanceId, Arc<dyn CompleteAgentService>>>,
}

impl RecordingCompleteAgentServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompleteAgentServiceRegistry for RecordingCompleteAgentServiceRegistry {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    ) {
        self.handles.write().await.insert(instance_id, service);
    }

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>> {
        self.handles.read().await.get(instance_id).cloned()
    }
}

/// Target-lane recording adapter. Production activation supplies the PostgreSQL implementation.
#[derive(Default)]
pub struct RecordingCompleteAgentHostRepository {
    snapshot: Mutex<CompleteAgentHostSnapshot>,
}

impl RecordingCompleteAgentHostRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompleteAgentHostRepository for RecordingCompleteAgentHostRepository {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        Ok(self.snapshot.lock().await.clone())
    }

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        validate_facts(&commit.facts)?;
        let mut current = self.snapshot.lock().await;
        if current.revision != commit.expected_revision {
            if current.facts == commit.facts {
                return Ok(current.clone());
            }
            return Err(CompleteAgentHostStoreError::Conflict {
                expected: commit.expected_revision,
                actual: current.revision,
            });
        }
        current.revision =
            CompleteAgentHostRevision(current.revision.0.checked_add(1).ok_or_else(|| {
                CompleteAgentHostStoreError::Invariant {
                    reason: "Host repository revision is exhausted".to_owned(),
                }
            })?);
        current.facts = commit.facts;
        Ok(current.clone())
    }
}

pub type SharedCompleteAgentHostRepository = Arc<dyn CompleteAgentHostRepository>;
pub type SharedCompleteAgentServiceRegistry = Arc<dyn CompleteAgentServiceRegistry>;

fn validate_facts(facts: &CompleteAgentHostFacts) -> Result<(), CompleteAgentHostStoreError> {
    for (instance_id, offer) in &facts.offers {
        let descriptor = facts.service_instances.get(instance_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "Runtime offer has no owning service instance".to_owned(),
            }
        })?;
        if offer.profile_digest != descriptor.profile_digest {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "Runtime offer profile does not match its service instance".to_owned(),
            });
        }
    }
    for (binding_id, binding) in &facts.bindings {
        if binding_id != &binding.id {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "binding map key does not match binding identity".to_owned(),
            });
        }
        if !facts
            .service_instances
            .contains_key(&binding.service_instance_id)
        {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "binding has no owning service instance".to_owned(),
            });
        }
        if facts.source_coordinates.get(binding_id) != Some(&binding.source) {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "binding source coordinate is missing or inconsistent".to_owned(),
            });
        }
    }
    for (binding_id, source) in &facts.source_coordinates {
        if !facts.bindings.contains_key(binding_id) {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "source coordinate has no owning binding".to_owned(),
            });
        }
        if facts
            .source_coordinates
            .iter()
            .any(|(other_id, other_source)| other_id != binding_id && other_source == source)
        {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "source coordinate is assigned to multiple bindings".to_owned(),
            });
        }
    }
    for effect in facts.effects.values() {
        let binding = facts.bindings.get(&effect.binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "effect has no owning binding".to_owned(),
            }
        })?;
        if effect.generation != binding.generation
            || effect.service_instance_id != binding.service_instance_id
            || effect.source != binding.source
        {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "effect coordinates do not match its binding generation".to_owned(),
            });
        }
    }
    for (binding_id, lease) in &facts.leases {
        let binding = facts.bindings.get(binding_id).ok_or_else(|| {
            CompleteAgentHostStoreError::Invariant {
                reason: "lease has no owning binding".to_owned(),
            }
        })?;
        if lease.binding_id != *binding_id || lease.generation != binding.generation {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "lease coordinates do not match its binding generation".to_owned(),
            });
        }
        if facts.lease_epochs.get(binding_id).copied() != Some(lease.epoch) {
            return Err(CompleteAgentHostStoreError::Invariant {
                reason: "lease epoch does not match the generation fence".to_owned(),
            });
        }
    }
    if facts
        .lease_epochs
        .keys()
        .any(|binding_id| !facts.bindings.contains_key(binding_id))
    {
        return Err(CompleteAgentHostStoreError::Invariant {
            reason: "lease epoch has no owning binding".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exact_stale_commit_replay_is_idempotent() {
        let repository = RecordingCompleteAgentHostRepository::new();
        let facts = CompleteAgentHostFacts::default();
        let first = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(0),
                facts: facts.clone(),
            })
            .await
            .expect("first commit");
        let replay = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: CompleteAgentHostRevision(0),
                facts,
            })
            .await
            .expect("exact replay");

        assert_eq!(first, replay);
        assert_eq!(replay.revision, CompleteAgentHostRevision(1));
    }

    #[tokio::test]
    async fn invalid_fact_graph_is_rejected_atomically() {
        let repository = RecordingCompleteAgentHostRepository::new();
        let mut facts = CompleteAgentHostFacts::default();
        facts.lease_epochs.insert(
            CompleteAgentBindingId::new("missing-binding").expect("binding"),
            1,
        );

        assert!(matches!(
            repository
                .commit(CompleteAgentHostCommit {
                    expected_revision: CompleteAgentHostRevision(0),
                    facts,
                })
                .await,
            Err(CompleteAgentHostStoreError::Invariant { .. })
        ));
        assert_eq!(
            repository.load().await.expect("snapshot"),
            CompleteAgentHostSnapshot::default()
        );
    }
}
