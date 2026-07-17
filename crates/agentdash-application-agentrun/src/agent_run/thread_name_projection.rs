use std::sync::Arc;

use agentdash_agent_runtime::{CommittedDurablePresentation, RuntimeCommittedPresentationObserver};
use agentdash_application_ports::{
    agent_run_runtime::AgentRunRuntimeBindingRepository,
    project_projection_notification::{
        ProjectProjectionInvalidation, ProjectProjectionNotificationPort,
    },
};
use agentdash_domain::workflow::LifecycleRunRepository;
use async_trait::async_trait;

pub struct AgentRunThreadNameProjectionNotifier {
    bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    runs: Arc<dyn LifecycleRunRepository>,
    notifications: Arc<dyn ProjectProjectionNotificationPort>,
}

impl AgentRunThreadNameProjectionNotifier {
    pub fn new(
        bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        runs: Arc<dyn LifecycleRunRepository>,
        notifications: Arc<dyn ProjectProjectionNotificationPort>,
    ) -> Self {
        Self {
            bindings,
            runs,
            notifications,
        }
    }
}

#[async_trait]
impl RuntimeCommittedPresentationObserver for AgentRunThreadNameProjectionNotifier {
    async fn observe(&self, presentation: CommittedDurablePresentation) -> Result<(), String> {
        if !presentation.projection_changed
            || !matches!(
                presentation
                    .record
                    .as_presentation()
                    .map(|event| &event.event),
                Some(agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(
                    _
                ))
            )
        {
            return Ok(());
        }

        let thread_id = presentation.record.carrier().thread_id.clone();
        let Some(binding) = self
            .bindings
            .load_by_thread_id(&thread_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let run = self
            .runs
            .get_by_id(binding.target.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "AgentRun thread name projection is missing LifecycleRun {}",
                    binding.target.run_id
                )
            })?;

        self.notifications
            .publish_project_projection_invalidated(ProjectProjectionInvalidation::agent_run_list(
                run.project_id,
                binding.target.run_id,
                binding.target.agent_id,
                None,
                agentdash_agent_protocol::ControlPlaneProjectionChangeReason::TitleChanged,
                Some(thread_id),
            ))
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use agentdash_agent_runtime_contract::{
        BindingEpoch, BoundRuntimeHookPlan, ConfigurationBoundary, ContextFidelity, ContextProfile,
        ContextRecipeRevision, DeliveryMechanism, DriverThreadId, EventSequence, HookPlanDigest,
        HookPlanRevision, HookProfile, ImmutablePresentationEvent, InputProfile,
        InstructionProfile, InteractionProfile, PresentationDurability, PresentationThreadId,
        ProfileDigest, ProfileProvenance, ReferenceRuntimeClass, RuntimeBindingId,
        RuntimeCarrierMetadata, RuntimeDriverGeneration, RuntimeJournalFact, RuntimeJournalRecord,
        RuntimePresentationCoordinate, RuntimeProfile, RuntimeRevision, RuntimeSurfaceDescriptor,
        RuntimeThreadId, SurfaceDigest, SurfaceRevision, ThreadSettingsRevision, ToolProfile,
        ToolSetRevision, WorkspaceProfile,
    };
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunContextDeliveryTarget, AgentRunRuntimeBinding, AgentRunRuntimeBindingError,
        AgentRunRuntimeTarget,
    };
    use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
    use agentdash_test_support::workflow::MemoryLifecycleRunRepository;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid fixture id")
    }

    struct StaticBindingRepository {
        binding: AgentRunRuntimeBinding,
    }

    #[async_trait]
    impl AgentRunRuntimeBindingRepository for StaticBindingRepository {
        async fn load(
            &self,
            target: &AgentRunRuntimeTarget,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((&self.binding.target == target).then(|| self.binding.clone()))
        }

        async fn load_by_thread_id(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((&self.binding.thread_id == thread_id).then(|| self.binding.clone()))
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((self.binding.target.run_id == run_id)
                .then(|| self.binding.clone())
                .into_iter()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok((self.binding.target.agent_id == agent_id)
                .then(|| self.binding.clone())
                .into_iter()
                .collect())
        }

        async fn insert(
            &self,
            binding: AgentRunRuntimeBinding,
        ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
            Ok(binding)
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

    fn binding(run_id: Uuid, agent_id: Uuid) -> AgentRunRuntimeBinding {
        AgentRunRuntimeBinding {
            target: AgentRunRuntimeTarget { run_id, agent_id },
            presentation_thread_id: id::<PresentationThreadId>("presentation-title"),
            thread_id: id("runtime-title"),
            binding_id: id::<RuntimeBindingId>("binding-title"),
            binding_epoch: BindingEpoch(1),
            driver_generation: RuntimeDriverGeneration(1),
            source_thread_id: id::<DriverThreadId>("source-title"),
            profile_digest: id::<ProfileDigest>("profile-title"),
            profile_provenance: ProfileProvenance {
                service_digest: id("service-title"),
                transport_digest: id("transport-title"),
                host_policy_digest: id("policy-title"),
            },
            bound_profile: RuntimeProfile {
                reference_class: ReferenceRuntimeClass::ManagedThread,
                input: InputProfile {
                    modalities: BTreeSet::new(),
                },
                instruction: InstructionProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                tools: ToolProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                    cancellation: true,
                },
                workspace: WorkspaceProfile {
                    capabilities: BTreeSet::new(),
                    mechanism: DeliveryMechanism::Native,
                },
                interactions: InteractionProfile {
                    kinds: BTreeSet::new(),
                    durable_correlation: true,
                },
                lifecycle: BTreeSet::new(),
                hooks: HookProfile {
                    points: Vec::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                context: ContextProfile {
                    capabilities: BTreeSet::new(),
                    fidelity: ContextFidelity::Opaque,
                    activation_idempotent: false,
                },
                telemetry_config: BTreeSet::new(),
            },
            surface: RuntimeSurfaceDescriptor {
                source_frame_id: "frame-title".to_string(),
                surface_revision: SurfaceRevision(1),
                surface_digest: id::<SurfaceDigest>("surface-title"),
                vfs_digest: "vfs-title".to_string(),
                context_recipe_revision: ContextRecipeRevision(1),
                context_digest: id("context-title"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                tool_set_digest: "tools-title".to_string(),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id::<HookPlanDigest>("hook-title"),
                    entries: Vec::new(),
                },
                terminal_hook_effect_binding: None,
            },
            settings_revision: ThreadSettingsRevision(0),
            context_delivery_target: AgentRunContextDeliveryTarget {
                connector_id: "native".to_string(),
                executor: "PI_AGENT".to_string(),
            },
        }
    }

    fn committed_name(
        thread_id: RuntimeThreadId,
        projection_changed: bool,
    ) -> CommittedDurablePresentation {
        let event = ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            agentdash_agent_protocol::BackboneEvent::ThreadNameUpdated(
                agentdash_agent_protocol::codex_app_server_protocol::ThreadNameUpdatedNotification {
                    thread_id: "source-title".to_string(),
                    thread_name: Some("会话标题".to_string()),
                },
            ),
        );
        CommittedDurablePresentation {
            record: RuntimeJournalRecord::new(
                RuntimeCarrierMetadata {
                    thread_id,
                    recorded_at_ms: 1,
                    sequence: Some(EventSequence(1)),
                    transient: None,
                    revision: RuntimeRevision(1),
                    operation_id: None,
                    append_idempotency_key: None,
                    binding_id: Some(id("binding-title")),
                    coordinate: RuntimePresentationCoordinate {
                        runtime_turn_id: None,
                        presentation_turn_id: None,
                        runtime_item_id: None,
                        interaction_id: None,
                        source_thread_id: Some("source-title".to_string()),
                        source_turn_id: None,
                        source_item_id: None,
                        source_request_id: None,
                        source_entry_index: None,
                    },
                },
                RuntimeJournalFact::Presentation(event),
            )
            .expect("durable name record"),
            projection_changed,
        }
    }

    #[tokio::test]
    async fn publishes_existing_title_changed_invalidation_only_for_semantic_name_changes() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let agent_id = Uuid::new_v4();
        let binding = binding(run.id, agent_id);
        let thread_id = binding.thread_id.clone();
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        runs.create(&run).await.expect("seed LifecycleRun");
        let notifications = Arc::new(RecordingNotifications::default());
        let notifier = AgentRunThreadNameProjectionNotifier::new(
            Arc::new(StaticBindingRepository { binding }),
            runs,
            notifications.clone(),
        );

        notifier
            .observe(committed_name(thread_id.clone(), false))
            .await
            .expect("unchanged name is ignored");
        notifier
            .observe(committed_name(thread_id.clone(), true))
            .await
            .expect("changed name publishes invalidation");

        let invalidations = notifications.invalidations.lock().await;
        assert_eq!(invalidations.len(), 1);
        let invalidation = &invalidations[0];
        assert_eq!(invalidation.project_id, project_id);
        assert_eq!(invalidation.run_id, run.id);
        assert_eq!(invalidation.agent_id, agent_id);
        assert_eq!(
            invalidation.projection,
            agentdash_agent_protocol::ControlPlaneProjection::AgentRunList
        );
        assert_eq!(
            invalidation.reason,
            agentdash_agent_protocol::ControlPlaneProjectionChangeReason::TitleChanged
        );
        assert_eq!(invalidation.runtime_thread_id.as_ref(), Some(&thread_id));
    }
}
