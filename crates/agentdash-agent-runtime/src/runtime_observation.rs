use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    AgentContextAuthority, AgentContextFidelity, AgentContextQuery, AgentLiveEvent, AgentReadQuery,
    AgentRuntimeContextProjection, AgentRuntimeContextRequirement, AgentRuntimeUpdate,
    AgentRuntimeView, AgentServiceError, AgentSnapshotRevision, AgentSourceCoordinate,
    CompleteAgentService, RuntimeThreadId,
};
use thiserror::Error;

use crate::project_authoritative_agent_view;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRuntimeObservationError {
    #[error("Complete Agent observation failed: {0}")]
    Agent(String),
    #[error("Complete Agent returned a different source identity")]
    SourceIdentityMismatch,
    #[error(
        "Complete Agent context is behind the required observation: required {required:?}, actual {actual:?}"
    )]
    ContextBehind {
        required: AgentSnapshotRevision,
        actual: AgentSnapshotRevision,
    },
    #[error("Complete Agent context disagrees with the Runtime observation at the same revision")]
    SameRevisionCoordinateMismatch,
    #[error("Complete Agent context recipe is not authoritative and exact")]
    UnsupportedContextFidelity,
    #[error("Complete Agent live event belongs to a different source")]
    LiveSourceMismatch,
    #[error("Complete Agent snapshot cannot enter the Runtime projection: {0}")]
    InvalidProjection(String),
}

impl From<AgentServiceError> for AgentRuntimeObservationError {
    fn from(error: AgentServiceError) -> Self {
        Self::Agent(error.to_string())
    }
}

/// Deep observation seam for one resolved Product Runtime target.
///
/// Product code resolves the durable binding once, then delegates all source reads, coordinate
/// checks and wrapper projection to this module.
pub struct AgentRuntimeObservation {
    thread_id: RuntimeThreadId,
    source: AgentSourceCoordinate,
    service: Arc<dyn CompleteAgentService>,
}

impl AgentRuntimeObservation {
    pub fn new(
        thread_id: RuntimeThreadId,
        source: AgentSourceCoordinate,
        service: Arc<dyn CompleteAgentService>,
    ) -> Self {
        Self {
            thread_id,
            source,
            service,
        }
    }

    pub async fn read_view(&self) -> Result<AgentRuntimeView, AgentRuntimeObservationError> {
        let snapshot = self
            .service
            .read(AgentReadQuery {
                source: self.source.clone(),
                at_revision: None,
            })
            .await?;
        if snapshot.source != self.source {
            return Err(AgentRuntimeObservationError::SourceIdentityMismatch);
        }
        project_authoritative_agent_view(self.thread_id.clone(), snapshot)
            .map_err(|error| AgentRuntimeObservationError::InvalidProjection(error.to_string()))
    }

    pub async fn read_context(
        &self,
        requirement: AgentRuntimeContextRequirement,
    ) -> Result<AgentRuntimeContextProjection, AgentRuntimeObservationError> {
        let snapshot = self
            .service
            .context(AgentContextQuery {
                source: self.source.clone(),
                required_revision: Some(requirement.at_least.snapshot_revision),
            })
            .await?;
        if snapshot.source != self.source {
            return Err(AgentRuntimeObservationError::SourceIdentityMismatch);
        }
        let actual = snapshot.coordinate();
        if actual.snapshot_revision < requirement.at_least.snapshot_revision {
            return Err(AgentRuntimeObservationError::ContextBehind {
                required: requirement.at_least.snapshot_revision,
                actual: actual.snapshot_revision,
            });
        }
        if actual.snapshot_revision == requirement.at_least.snapshot_revision
            && actual != &requirement.at_least
        {
            return Err(AgentRuntimeObservationError::SameRevisionCoordinateMismatch);
        }
        if actual.authority != AgentContextAuthority::AgentOwned
            || actual.fidelity != AgentContextFidelity::Exact
        {
            return Err(AgentRuntimeObservationError::UnsupportedContextFidelity);
        }
        Ok(AgentRuntimeContextProjection {
            thread_id: self.thread_id.clone(),
            recipe: snapshot.recipe,
        })
    }

    pub async fn reconcile_live(
        &self,
        event: AgentLiveEvent,
    ) -> Result<AgentRuntimeUpdate, AgentRuntimeObservationError> {
        if event.source != self.source {
            return Err(AgentRuntimeObservationError::LiveSourceMismatch);
        }
        let view = self.read_view().await?;
        Ok(AgentRuntimeUpdate {
            lane_sequence: event.sequence.0,
            observation: view.observation,
            presentations: vec![event.record],
        })
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_contract::{
        AgentChangePage, AgentChangesQuery, AgentCommandEnvelope, AgentCommandReceipt,
        AgentContextCoordinate, AgentContextRecipe, AgentContextSnapshot, AgentEffectIdentity,
        AgentEffectInspection, AgentPayloadDigest, AgentServiceDescriptor, AgentSnapshot,
        AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface, CreateAgentCommand, ForkAgentCommand,
        ForkAgentReceipt, ResumeAgentCommand, RevokeBoundAgentSurface,
    };
    use async_trait::async_trait;

    use super::*;

    struct ContextService {
        snapshot: AgentContextSnapshot,
    }

    #[async_trait]
    impl CompleteAgentService for ContextService {
        async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
            unreachable!()
        }

        async fn create(
            &self,
            _command: CreateAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn resume(
            &self,
            _command: ResumeAgentCommand,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn fork(
            &self,
            _command: ForkAgentCommand,
        ) -> Result<ForkAgentReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn execute(
            &self,
            _command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn read(&self, _query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
            unreachable!()
        }

        async fn context(
            &self,
            _query: AgentContextQuery,
        ) -> Result<AgentContextSnapshot, AgentServiceError> {
            Ok(self.snapshot.clone())
        }

        async fn changes(
            &self,
            _query: AgentChangesQuery,
        ) -> Result<AgentChangePage, AgentServiceError> {
            unreachable!()
        }

        async fn inspect(
            &self,
            _identity: AgentEffectIdentity,
        ) -> Result<AgentEffectInspection, AgentServiceError> {
            unreachable!()
        }

        async fn apply_surface(
            &self,
            _command: ApplyBoundAgentSurface,
        ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
            unreachable!()
        }

        async fn revoke_surface(
            &self,
            _command: RevokeBoundAgentSurface,
        ) -> Result<AgentCommandReceipt, AgentServiceError> {
            unreachable!()
        }
    }

    fn coordinate(revision: u64, digest: &str) -> AgentContextCoordinate {
        AgentContextCoordinate {
            snapshot_revision: AgentSnapshotRevision(revision),
            context_revision: Some(format!("context-{revision}")),
            recipe_digest: AgentPayloadDigest::new(digest).expect("digest"),
            authority: AgentContextAuthority::AgentOwned,
            fidelity: AgentContextFidelity::Exact,
        }
    }

    fn observation_with(snapshot: AgentContextSnapshot) -> AgentRuntimeObservation {
        let source = snapshot.source.clone();
        AgentRuntimeObservation::new(
            RuntimeThreadId::new("thread-1").expect("thread"),
            source,
            Arc::new(ContextService { snapshot }),
        )
    }

    #[tokio::test]
    async fn context_projection_rejects_snapshot_behind_required_observation() {
        let source = AgentSourceCoordinate::new("source-1").expect("source");
        let observation = observation_with(AgentContextSnapshot {
            source,
            recipe: AgentContextRecipe {
                coordinate: coordinate(6, "sha256:context-6"),
                contributions: Vec::new(),
            },
        });

        let error = observation
            .read_context(AgentRuntimeContextRequirement {
                at_least: coordinate(7, "sha256:context-7"),
            })
            .await
            .expect_err("stale context must not commit");

        assert!(matches!(
            error,
            AgentRuntimeObservationError::ContextBehind {
                required: AgentSnapshotRevision(7),
                actual: AgentSnapshotRevision(6),
            }
        ));
    }

    #[tokio::test]
    async fn context_projection_rejects_same_revision_with_different_recipe() {
        let source = AgentSourceCoordinate::new("source-1").expect("source");
        let observation = observation_with(AgentContextSnapshot {
            source,
            recipe: AgentContextRecipe {
                coordinate: coordinate(7, "sha256:stale"),
                contributions: Vec::new(),
            },
        });

        let error = observation
            .read_context(AgentRuntimeContextRequirement {
                at_least: coordinate(7, "sha256:required"),
            })
            .await
            .expect_err("coordinate mismatch must not commit");

        assert_eq!(
            error,
            AgentRuntimeObservationError::SameRevisionCoordinateMismatch
        );
    }

    #[tokio::test]
    async fn context_projection_commits_newer_authoritative_recipe() {
        let source = AgentSourceCoordinate::new("source-1").expect("source");
        let actual = coordinate(8, "sha256:context-8");
        let observation = observation_with(AgentContextSnapshot {
            source,
            recipe: AgentContextRecipe {
                coordinate: actual.clone(),
                contributions: Vec::new(),
            },
        });

        let projection = observation
            .read_context(AgentRuntimeContextRequirement {
                at_least: coordinate(7, "sha256:context-7"),
            })
            .await
            .expect("newer authoritative recipe");

        assert_eq!(projection.recipe.coordinate, actual);
    }
}
