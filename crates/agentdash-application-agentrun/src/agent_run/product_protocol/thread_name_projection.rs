use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangeDelta, ManagedRuntimePlatformChange, ManagedRuntimeProjectionAuthority,
    ManagedRuntimeProjectionFidelity, RuntimeChangeSequence, RuntimeProjectionRevision,
};
use agentdash_application_ports::project_projection_notification::{
    ProjectProjectionInvalidation, ProjectProjectionNotificationPort,
};
use agentdash_domain::{agent_run_target::AgentRunTarget, workflow::LifecycleRunRepository};
use async_trait::async_trait;
use thiserror::Error;

use crate::agent_run::{
    AgentRunProductRuntimeBindingRepository, AgentRunProductRuntimeChange,
    AgentRunProductRuntimeChangeObserver, AgentRunProductRuntimeChangeOutcome,
    AgentRunRuntimeProjectionPort,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunThreadNameProjectionOutcome {
    Ignored,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunThreadNameProjectionError {
    #[error("AgentRun Runtime binding load failed: {0}")]
    Binding(String),
    #[error("AgentRun target has no current Runtime binding")]
    TargetNotBound,
    #[error("AgentRun Runtime binding returned a different Product target")]
    TargetMismatch,
    #[error("thread-name change belongs to a different Runtime thread")]
    RuntimeThreadMismatch,
    #[error("Managed Runtime snapshot load failed: {0}")]
    Runtime(String),
    #[error("Managed Runtime snapshot belongs to a different Runtime thread")]
    SnapshotThreadMismatch,
    #[error("Managed Runtime source binding is not current for this AgentRun")]
    RuntimeSourceBindingMismatch,
    #[error(
        "Managed Runtime snapshot is behind thread-name change revision {change_revision:?} / sequence {change_sequence:?}"
    )]
    SnapshotBehindChange {
        change_revision: RuntimeProjectionRevision,
        change_sequence: RuntimeChangeSequence,
    },
    #[error("Managed Runtime snapshot has no source evidence for its current thread name")]
    ThreadNameSourceMissing,
    #[error("thread-name change source evidence is stale for the current Runtime snapshot")]
    ThreadNameSourceMismatch,
    #[error("thread-name change is not source-authoritative with exact fidelity")]
    ThreadNameSourceNotAuthoritative,
    #[error("thread-name change carries invalid committed source coordinates")]
    ThreadNameChangeCoordinateMismatch,
    #[error("LifecycleRun query failed: {0}")]
    Run(String),
    #[error("AgentRun thread-name projection is missing LifecycleRun {0}")]
    RunMissing(uuid::Uuid),
    #[error("Project projection notification failed: {0}")]
    Notification(String),
}

/// Product observer for source-authoritative names already committed by Managed Runtime.
///
/// The change only invalidates the Project AgentRun list. Consumers query the canonical Runtime
/// snapshot for the current title; the invalidation payload never becomes a second title owner.
pub struct AgentRunThreadNameProjectionObserver {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    runtime: Arc<dyn AgentRunRuntimeProjectionPort>,
    runs: Arc<dyn LifecycleRunRepository>,
    notifications: Arc<dyn ProjectProjectionNotificationPort>,
}

impl AgentRunThreadNameProjectionObserver {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        runtime: Arc<dyn AgentRunRuntimeProjectionPort>,
        runs: Arc<dyn LifecycleRunRepository>,
        notifications: Arc<dyn ProjectProjectionNotificationPort>,
    ) -> Self {
        Self {
            bindings,
            runtime,
            runs,
            notifications,
        }
    }

    pub async fn observe(
        &self,
        target: &AgentRunTarget,
        change: &ManagedRuntimePlatformChange,
    ) -> Result<AgentRunThreadNameProjectionOutcome, AgentRunThreadNameProjectionError> {
        let (source_change_sequence, source_projection_revision, change_source) =
            match &change.delta {
                ManagedRuntimeChangeDelta::ThreadNameChanged {
                    source_change_sequence,
                    source_projection_revision,
                    source,
                    ..
                } => (*source_change_sequence, *source_projection_revision, source),
                _ => return Ok(AgentRunThreadNameProjectionOutcome::Ignored),
            };
        if source_change_sequence == 0 || source_projection_revision != change.revision {
            return Err(AgentRunThreadNameProjectionError::ThreadNameChangeCoordinateMismatch);
        }
        if change_source.authority != ManagedRuntimeProjectionAuthority::SourceAuthoritative
            || change_source.fidelity != ManagedRuntimeProjectionFidelity::Exact
        {
            return Err(AgentRunThreadNameProjectionError::ThreadNameSourceNotAuthoritative);
        }

        let binding = self
            .bindings
            .load_product_binding(target)
            .await
            .map_err(AgentRunThreadNameProjectionError::Binding)?
            .ok_or(AgentRunThreadNameProjectionError::TargetNotBound)?;
        if binding.target != *target {
            return Err(AgentRunThreadNameProjectionError::TargetMismatch);
        }
        if change.thread_id != binding.runtime_thread_id {
            return Err(AgentRunThreadNameProjectionError::RuntimeThreadMismatch);
        }

        let snapshot = self
            .runtime
            .load_snapshot(&binding.runtime_thread_id)
            .await
            .map_err(AgentRunThreadNameProjectionError::Runtime)?;
        if snapshot.thread_id != binding.runtime_thread_id {
            return Err(AgentRunThreadNameProjectionError::SnapshotThreadMismatch);
        }
        if snapshot.source_binding.as_ref() != Some(&binding.source_binding) {
            return Err(AgentRunThreadNameProjectionError::RuntimeSourceBindingMismatch);
        }
        if snapshot.revision < change.revision || snapshot.latest_change_sequence < change.sequence
        {
            return Err(AgentRunThreadNameProjectionError::SnapshotBehindChange {
                change_revision: change.revision,
                change_sequence: change.sequence,
            });
        }
        let snapshot_source = snapshot
            .thread_name_source
            .as_ref()
            .ok_or(AgentRunThreadNameProjectionError::ThreadNameSourceMissing)?;
        if snapshot_source.authority != ManagedRuntimeProjectionAuthority::SourceAuthoritative
            || snapshot_source.fidelity != ManagedRuntimeProjectionFidelity::Exact
        {
            return Err(AgentRunThreadNameProjectionError::ThreadNameSourceNotAuthoritative);
        }
        if snapshot_source.source_identity_digest != change_source.source_identity_digest {
            return Err(AgentRunThreadNameProjectionError::ThreadNameSourceMismatch);
        }

        let run = self
            .runs
            .get_by_id(target.run_id)
            .await
            .map_err(|error| AgentRunThreadNameProjectionError::Run(error.to_string()))?
            .ok_or(AgentRunThreadNameProjectionError::RunMissing(target.run_id))?;
        self.notifications
            .publish_project_projection_invalidated(ProjectProjectionInvalidation::agent_run_list(
                run.project_id,
                target.run_id,
                target.agent_id,
                None,
                agentdash_application_ports::project_projection_notification::ControlPlaneProjectionChangeReason::TitleChanged,
                Some(binding.runtime_thread_id),
            ))
            .await
            .map_err(AgentRunThreadNameProjectionError::Notification)?;
        Ok(AgentRunThreadNameProjectionOutcome::Published)
    }
}

#[async_trait]
impl AgentRunProductRuntimeChangeObserver for AgentRunThreadNameProjectionObserver {
    fn consumer_name(&self) -> &'static str {
        "agent_run_thread_name_projection"
    }

    async fn observe_product_runtime_change(
        &self,
        input: &AgentRunProductRuntimeChange,
    ) -> Result<AgentRunProductRuntimeChangeOutcome, String> {
        match self
            .observe(&input.binding.target, &input.change)
            .await
            .map_err(|error| error.to_string())?
        {
            AgentRunThreadNameProjectionOutcome::Ignored => {
                Ok(AgentRunProductRuntimeChangeOutcome::Ignored)
            }
            AgentRunThreadNameProjectionOutcome::Published => {
                Ok(AgentRunProductRuntimeChangeOutcome::Applied)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangePage, ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
        ManagedRuntimeSourceBindingEvidence, ManagedRuntimeThreadNameSource, RuntimePayloadDigest,
        RuntimeSourceRef, RuntimeThreadId, SurfaceRevision,
    };
    use agentdash_application_ports::project_projection_notification::ProjectProjectionInvalidation;
    use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
    use agentdash_test_support::workflow::MemoryLifecycleRunRepository;
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::AgentRunProductRuntimeBinding;

    fn fixture_execution_profile() -> crate::agent_run::ProductExecutionProfileRef {
        let mut profile = crate::agent_run::ProductExecutionProfileRef {
            profile_key: "codex".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({"executor": "codex"}),
            credential_scope: None,
        };
        profile.refresh_digest();
        profile
    }

    struct StaticBindingRepository {
        binding: Option<AgentRunProductRuntimeBinding>,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for StaticBindingRepository {
        async fn load_product_binding(
            &self,
            _: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(self.binding.clone())
        }
    }

    struct StaticRuntimeProjection {
        snapshot: ManagedRuntimeSnapshot,
    }

    #[async_trait]
    impl AgentRunRuntimeProjectionPort for StaticRuntimeProjection {
        async fn load_snapshot(
            &self,
            _: &RuntimeThreadId,
        ) -> Result<ManagedRuntimeSnapshot, String> {
            Ok(self.snapshot.clone())
        }

        async fn load_changes(
            &self,
            thread_id: &RuntimeThreadId,
            _: Option<RuntimeChangeSequence>,
        ) -> Result<ManagedRuntimeChangePage, String> {
            Ok(ManagedRuntimeChangePage {
                thread_id: thread_id.clone(),
                changes: Vec::new(),
                next: self.snapshot.latest_change_sequence,
                gap: None,
            })
        }
    }

    #[derive(Default)]
    struct RecordingNotifications {
        invalidations: Mutex<Vec<ProjectProjectionInvalidation>>,
    }

    #[async_trait]
    impl ProjectProjectionNotificationPort for RecordingNotifications {
        async fn publish_project_projection_invalidated(
            &self,
            invalidation: ProjectProjectionInvalidation,
        ) -> Result<(), String> {
            self.invalidations.lock().await.push(invalidation);
            Ok(())
        }
    }

    fn source_binding(value: &str) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new(value).expect("source ref"),
            committed_at_revision: RuntimeProjectionRevision(1),
            applied_surface_revision: SurfaceRevision(1),
            activated_at_revision: Some(RuntimeProjectionRevision(2)),
        }
    }

    fn thread_name_source() -> ManagedRuntimeThreadNameSource {
        ManagedRuntimeThreadNameSource {
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            source_identity_digest: RuntimePayloadDigest::new("sha256:source").expect("digest"),
            source_revision_digest: Some(
                RuntimePayloadDigest::new("sha256:revision").expect("digest"),
            ),
            observed_at_ms: 10,
        }
    }

    fn snapshot(
        thread_id: RuntimeThreadId,
        binding: ManagedRuntimeSourceBindingEvidence,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id,
            revision: RuntimeProjectionRevision(4),
            latest_change_sequence: RuntimeChangeSequence(8),
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            conversation_history: Vec::new(),
            thread_name: Some("当前标题".to_owned()),
            thread_name_source: Some(thread_name_source()),
            operations: Vec::new(),
            source_binding: Some(binding),
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::new(),
        }
    }

    fn name_change(
        thread_id: RuntimeThreadId,
        sequence: u64,
        thread_name: Option<&str>,
    ) -> ManagedRuntimePlatformChange {
        ManagedRuntimePlatformChange {
            thread_id,
            sequence: RuntimeChangeSequence(sequence),
            revision: RuntimeProjectionRevision(4),
            delta: ManagedRuntimeChangeDelta::ThreadNameChanged {
                source_change_sequence: sequence,
                source_projection_revision: RuntimeProjectionRevision(4),
                thread_name: thread_name.map(str::to_owned),
                source: thread_name_source(),
            },
        }
    }

    async fn fixture(
        runtime_binding: ManagedRuntimeSourceBindingEvidence,
        snapshot_binding: ManagedRuntimeSourceBindingEvidence,
    ) -> (
        AgentRunTarget,
        RuntimeThreadId,
        AgentRunThreadNameProjectionObserver,
        Arc<RecordingNotifications>,
    ) {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let target = AgentRunTarget {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        let thread_id = RuntimeThreadId::new("runtime-thread-name").expect("thread");
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        runs.create(&run).await.expect("seed run");
        let notifications = Arc::new(RecordingNotifications::default());
        let observer = AgentRunThreadNameProjectionObserver::new(
            Arc::new(StaticBindingRepository {
                binding: Some(AgentRunProductRuntimeBinding {
                    target: target.clone(),
                    runtime_thread_id: thread_id.clone(),
                    launch_frame: crate::agent_run::ProductAgentFrameRef {
                        frame_id: Uuid::new_v4(),
                        agent_id: target.agent_id,
                        revision: 1,
                    },
                    execution_profile_digest: fixture_execution_profile().profile_digest,
                    execution_profile: fixture_execution_profile(),
                    source_binding: runtime_binding,
                }),
            }),
            Arc::new(StaticRuntimeProjection {
                snapshot: snapshot(thread_id.clone(), snapshot_binding),
            }),
            runs,
            notifications.clone(),
        );
        (target, thread_id, observer, notifications)
    }

    #[tokio::test]
    async fn set_and_clear_publish_exact_project_run_agent_invalidation() {
        let binding = source_binding("source-ref-current");
        let (target, thread_id, observer, notifications) = fixture(binding.clone(), binding).await;

        assert_eq!(
            observer
                .observe(&target, &name_change(thread_id.clone(), 7, Some("新标题")))
                .await
                .expect("set invalidation"),
            AgentRunThreadNameProjectionOutcome::Published
        );
        assert_eq!(
            observer
                .observe(&target, &name_change(thread_id.clone(), 8, None))
                .await
                .expect("clear invalidation"),
            AgentRunThreadNameProjectionOutcome::Published
        );

        let invalidations = notifications.invalidations.lock().await;
        assert_eq!(invalidations.len(), 2);
        for invalidation in invalidations.iter() {
            assert_eq!(invalidation.run_id, target.run_id);
            assert_eq!(invalidation.agent_id, target.agent_id);
            assert_eq!(invalidation.runtime_thread_id.as_ref(), Some(&thread_id));
            assert_eq!(
                invalidation.reason,
                agentdash_application_ports::project_projection_notification::ControlPlaneProjectionChangeReason::TitleChanged
            );
        }
    }

    #[tokio::test]
    async fn wrong_thread_stale_snapshot_and_rebound_source_are_rejected() {
        let current = source_binding("source-ref-current");
        let rebound = source_binding("source-ref-rebound");
        let (target, thread_id, observer, _) = fixture(current.clone(), current.clone()).await;
        let wrong_thread = RuntimeThreadId::new("runtime-thread-other").expect("thread");
        assert!(matches!(
            observer
                .observe(&target, &name_change(wrong_thread, 7, Some("标题")))
                .await,
            Err(AgentRunThreadNameProjectionError::RuntimeThreadMismatch)
        ));
        assert!(matches!(
            observer
                .observe(&target, &name_change(thread_id.clone(), 9, Some("标题")))
                .await,
            Err(AgentRunThreadNameProjectionError::SnapshotBehindChange { .. })
        ));
        let mut stale_source = name_change(thread_id.clone(), 7, Some("标题"));
        let ManagedRuntimeChangeDelta::ThreadNameChanged { source, .. } = &mut stale_source.delta
        else {
            unreachable!()
        };
        source.source_identity_digest =
            RuntimePayloadDigest::new("sha256:stale-source").expect("digest");
        assert!(matches!(
            observer.observe(&target, &stale_source).await,
            Err(AgentRunThreadNameProjectionError::ThreadNameSourceMismatch)
        ));
        let mut wrong_coordinate = name_change(thread_id.clone(), 7, Some("标题"));
        let ManagedRuntimeChangeDelta::ThreadNameChanged {
            source_projection_revision,
            ..
        } = &mut wrong_coordinate.delta
        else {
            unreachable!()
        };
        *source_projection_revision = RuntimeProjectionRevision(3);
        assert!(matches!(
            observer.observe(&target, &wrong_coordinate).await,
            Err(AgentRunThreadNameProjectionError::ThreadNameChangeCoordinateMismatch)
        ));
        let mut observed_source = name_change(thread_id.clone(), 7, Some("标题"));
        let ManagedRuntimeChangeDelta::ThreadNameChanged { source, .. } =
            &mut observed_source.delta
        else {
            unreachable!()
        };
        source.authority = ManagedRuntimeProjectionAuthority::SourceObserved;
        assert!(matches!(
            observer.observe(&target, &observed_source).await,
            Err(AgentRunThreadNameProjectionError::ThreadNameSourceNotAuthoritative)
        ));

        let (target, thread_id, rebound_observer, _) = fixture(rebound, current).await;
        assert!(matches!(
            rebound_observer
                .observe(&target, &name_change(thread_id, 7, Some("标题")))
                .await,
            Err(AgentRunThreadNameProjectionError::RuntimeSourceBindingMismatch)
        ));
    }

    #[tokio::test]
    async fn unrelated_runtime_change_does_not_query_or_publish_product_state() {
        let binding = source_binding("source-ref-current");
        let (target, thread_id, observer, notifications) = fixture(binding.clone(), binding).await;
        let change = ManagedRuntimePlatformChange {
            thread_id,
            sequence: RuntimeChangeSequence(7),
            revision: RuntimeProjectionRevision(4),
            delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                lifecycle: ManagedRuntimeLifecycleStatus::Active,
            },
        };
        assert_eq!(
            observer.observe(&target, &change).await.expect("ignore"),
            AgentRunThreadNameProjectionOutcome::Ignored
        );
        assert!(notifications.invalidations.lock().await.is_empty());
    }
}
