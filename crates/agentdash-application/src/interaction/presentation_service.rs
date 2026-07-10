use std::sync::Arc;

use agentdash_domain::interaction::{
    InteractionDefinitionRepository, InteractionError, InteractionEvent,
    InteractionEventRepository, InteractionInstance, InteractionInstanceRepository,
    InteractionPresentationRepository, InteractionPresentationState, InteractionRendererLease,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::{
    InteractionApplicationError, InteractionApplicationResult, InteractionInstanceAccessResolver,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceInteractionPresentationInput {
    pub instance_id: Uuid,
    pub presentation_key: String,
    pub value: Value,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertInteractionRendererLeaseInput {
    pub instance_id: Uuid,
    pub renderer_key: String,
    pub ttl_seconds: i64,
    pub expected_revision: Option<u64>,
}

#[derive(Clone)]
pub struct InteractionPresentationService {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn InteractionInstanceRepository>,
    events: Arc<dyn InteractionEventRepository>,
    presentations: Arc<dyn InteractionPresentationRepository>,
    access: Arc<dyn InteractionInstanceAccessResolver>,
}

impl InteractionPresentationService {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn InteractionInstanceRepository>,
        events: Arc<dyn InteractionEventRepository>,
        presentations: Arc<dyn InteractionPresentationRepository>,
        access: Arc<dyn InteractionInstanceAccessResolver>,
    ) -> Self {
        Self {
            definitions,
            instances,
            events,
            presentations,
            access,
        }
    }

    pub async fn list_events(
        &self,
        instance_id: Uuid,
        user_id: &str,
        after_sequence: u64,
    ) -> InteractionApplicationResult<Vec<InteractionEvent>> {
        self.authorize(instance_id, user_id).await?;
        self.events
            .list_events(instance_id, after_sequence)
            .await
            .map_err(Into::into)
    }

    pub async fn get_presentation(
        &self,
        instance_id: Uuid,
        user_id: &str,
        presentation_key: &str,
    ) -> InteractionApplicationResult<Option<InteractionPresentationState>> {
        self.authorize(instance_id, user_id).await?;
        self.presentations
            .get_presentation_state(instance_id, user_id, presentation_key)
            .await
            .map_err(Into::into)
    }

    pub async fn replace_presentation(
        &self,
        input: ReplaceInteractionPresentationInput,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<InteractionPresentationState> {
        self.authorize(input.instance_id, user_id).await?;
        let current = self
            .presentations
            .get_presentation_state(input.instance_id, user_id, &input.presentation_key)
            .await?;
        let state = match (current, input.expected_revision) {
            (None, None) => InteractionPresentationState::new(
                input.instance_id,
                user_id,
                input.presentation_key,
                input.value,
                now,
            )?,
            (Some(mut state), Some(expected_revision)) => {
                state.replace(expected_revision, input.value, now)?;
                state
            }
            (None, Some(expected)) => {
                return Err(InteractionError::StateRevisionConflict {
                    instance_id: input.instance_id,
                    expected,
                    actual: 0,
                }
                .into());
            }
            (Some(state), None) => {
                return Err(InteractionError::StateRevisionConflict {
                    instance_id: input.instance_id,
                    expected: 0,
                    actual: state.revision,
                }
                .into());
            }
        };
        self.presentations
            .upsert_presentation_state(&state, input.expected_revision)
            .await?;
        Ok(state)
    }

    pub async fn list_renderer_leases(
        &self,
        instance_id: Uuid,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<Vec<InteractionRendererLease>> {
        self.authorize(instance_id, user_id).await?;
        self.presentations
            .list_active_renderer_leases(instance_id, now)
            .await
            .map_err(Into::into)
    }

    pub async fn upsert_renderer_lease(
        &self,
        input: UpsertInteractionRendererLeaseInput,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<InteractionRendererLease> {
        self.authorize(input.instance_id, user_id).await?;
        let active = self
            .presentations
            .list_active_renderer_leases(input.instance_id, now)
            .await?;
        let current = active
            .into_iter()
            .find(|lease| lease.renderer_key == input.renderer_key);
        let ttl = Duration::seconds(input.ttl_seconds);
        let lease = match (current, input.expected_revision) {
            (None, None) => InteractionRendererLease::acquire(
                input.instance_id,
                input.renderer_key,
                user_id,
                now,
                ttl,
            )?,
            (Some(mut lease), Some(expected_revision)) => {
                if lease.user_id != user_id || lease.revision != expected_revision {
                    return Err(InteractionApplicationError::AccessDenied {
                        reason: "renderer lease 不属于当前用户或 fencing revision 已变化".into(),
                    });
                }
                lease.renew(now, ttl)?;
                lease
            }
            (None, Some(expected)) => {
                return Err(InteractionError::StateRevisionConflict {
                    instance_id: input.instance_id,
                    expected,
                    actual: 0,
                }
                .into());
            }
            (Some(lease), None) => {
                return Err(InteractionError::StateRevisionConflict {
                    instance_id: input.instance_id,
                    expected: 0,
                    actual: lease.revision,
                }
                .into());
            }
        };
        self.presentations
            .upsert_renderer_lease(&lease, input.expected_revision)
            .await?;
        Ok(lease)
    }

    pub async fn release_renderer_lease(
        &self,
        instance_id: Uuid,
        lease_id: Uuid,
        expected_revision: u64,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<()> {
        self.authorize(instance_id, user_id).await?;
        let lease = self
            .presentations
            .list_active_renderer_leases(instance_id, now)
            .await?
            .into_iter()
            .find(|lease| lease.id == lease_id)
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_renderer_lease",
                id: lease_id.to_string(),
            })?;
        if lease.user_id != user_id {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "renderer lease 不属于当前用户".into(),
            });
        }
        if lease.revision != expected_revision {
            return Err(InteractionError::StateRevisionConflict {
                instance_id,
                expected: expected_revision,
                actual: lease.revision,
            }
            .into());
        }
        self.presentations
            .release_renderer_lease(lease_id, expected_revision)
            .await?;
        Ok(())
    }

    async fn authorize(
        &self,
        instance_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionInstance> {
        let instance =
            self.instances
                .get(instance_id)
                .await?
                .ok_or_else(|| InteractionError::NotFound {
                    entity: "interaction_instance",
                    id: instance_id.to_string(),
                })?;
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: instance.definition_revision_id.to_string(),
            })?;
        let access = self
            .access
            .resolve(&instance.owner, revision.project_id, user_id)
            .await?;
        if !access.can_view {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "当前用户不可访问 Interaction presentation".into(),
            });
        }
        Ok(instance)
    }
}
