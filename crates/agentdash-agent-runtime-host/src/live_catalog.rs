use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_service_api::{
    AgentRuntimeOffer, AgentServiceDescriptor, AgentServiceError, AgentServiceInstanceId,
    CompleteAgentLiveAttachmentId, CompleteAgentService,
};
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    CompleteAgentBindingTarget, CompleteAgentVerifiedServiceRegistration,
    complete_agent::{runtime_offer_from_descriptor, validate_service_descriptor},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteAgentAvailability {
    Available {
        attachment_id: CompleteAgentLiveAttachmentId,
    },
    Unavailable {
        reason: String,
    },
}

impl CompleteAgentAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Available { .. } => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

#[derive(Clone)]
pub struct CompleteAgentLiveSelection {
    pub target: CompleteAgentBindingTarget,
    pub descriptor: AgentServiceDescriptor,
    pub verification: crate::CompleteAgentServiceVerification,
    pub offer: AgentRuntimeOffer,
    service: Arc<dyn CompleteAgentService>,
}

impl CompleteAgentLiveSelection {
    pub fn service(&self) -> Arc<dyn CompleteAgentService> {
        self.service.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentLiveCatalogError {
    #[error("Complete Agent live attachment is retired: {attachment_id}")]
    RetiredAttachment {
        attachment_id: CompleteAgentLiveAttachmentId,
    },
    #[error("Complete Agent live attachment identity was reused with different verified facts")]
    AttachmentConflict,
    #[error("Complete Agent live attachment is invalid: {reason}")]
    Invariant { reason: String },
    #[error(transparent)]
    Service(#[from] AgentServiceError),
}

#[async_trait]
pub trait CompleteAgentLiveCatalog: Send + Sync {
    async fn attach(
        &self,
        registration: CompleteAgentVerifiedServiceRegistration,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentLiveCatalogError>;

    async fn resolve(
        &self,
        attachment_id: &CompleteAgentLiveAttachmentId,
    ) -> Option<CompleteAgentLiveSelection>;

    async fn availability(
        &self,
        logical_instance_id: &AgentServiceInstanceId,
    ) -> CompleteAgentAvailability;

    async fn mark_unavailable(&self, logical_instance_id: AgentServiceInstanceId, reason: String);

    async fn retire(&self, attachment_id: &CompleteAgentLiveAttachmentId, reason: String) -> bool;
}

pub type SharedCompleteAgentLiveCatalog = Arc<dyn CompleteAgentLiveCatalog>;

#[derive(Default)]
pub struct ProcessCompleteAgentLiveCatalog {
    state: RwLock<ProcessCompleteAgentLiveCatalogState>,
}

#[derive(Default)]
struct ProcessCompleteAgentLiveCatalogState {
    entries: BTreeMap<CompleteAgentLiveAttachmentId, CompleteAgentLiveSelection>,
    current_by_logical: BTreeMap<AgentServiceInstanceId, CompleteAgentLiveAttachmentId>,
    diagnostics: BTreeMap<AgentServiceInstanceId, String>,
    retired: BTreeSet<CompleteAgentLiveAttachmentId>,
}

impl ProcessCompleteAgentLiveCatalog {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompleteAgentLiveCatalog for ProcessCompleteAgentLiveCatalog {
    async fn attach(
        &self,
        registration: CompleteAgentVerifiedServiceRegistration,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<CompleteAgentLiveSelection, CompleteAgentLiveCatalogError> {
        validate_registration(&registration)?;
        let descriptor = registration.descriptor.clone();
        validate_service_descriptor(&descriptor).map_err(|error| {
            CompleteAgentLiveCatalogError::Invariant {
                reason: error.to_string(),
            }
        })?;
        let offer = runtime_offer_from_descriptor(&descriptor).map_err(|error| {
            CompleteAgentLiveCatalogError::Invariant {
                reason: error.to_string(),
            }
        })?;
        if descriptor.profile_digest != registration.verification.verified_profile_digest {
            return Err(CompleteAgentLiveCatalogError::Invariant {
                reason: "verified profile does not match the materialized service descriptor"
                    .to_owned(),
            });
        }
        let attachment_id = live_attachment_id(&registration)?;
        let target = CompleteAgentBindingTarget {
            logical_instance_id: registration.instance_id.clone(),
            live_attachment_id: attachment_id.clone(),
            definition_id: descriptor.definition_id.clone(),
            verified_build_digest: registration
                .verification
                .verified_build
                .claimed_build_digest
                .clone(),
            verified_profile_digest: registration.verification.verified_profile_digest.clone(),
            offer_profile_digest: offer.profile_digest.clone(),
            placement: registration.placement.clone(),
            remote_binding: registration.remote_binding.clone(),
        };
        let selection = CompleteAgentLiveSelection {
            target,
            descriptor,
            verification: registration.verification,
            offer,
            service,
        };

        let mut state = self.state.write().await;
        if state.retired.contains(&attachment_id) {
            return Err(CompleteAgentLiveCatalogError::RetiredAttachment { attachment_id });
        }
        if let Some(existing) = state.entries.get(&attachment_id) {
            if !same_verified_facts(existing, &selection) {
                return Err(CompleteAgentLiveCatalogError::AttachmentConflict);
            }
            return Ok(existing.clone());
        }
        state
            .entries
            .insert(attachment_id.clone(), selection.clone());
        state
            .current_by_logical
            .insert(registration.instance_id.clone(), attachment_id);
        state.diagnostics.remove(&registration.instance_id);
        Ok(selection)
    }

    async fn resolve(
        &self,
        attachment_id: &CompleteAgentLiveAttachmentId,
    ) -> Option<CompleteAgentLiveSelection> {
        self.state.read().await.entries.get(attachment_id).cloned()
    }

    async fn availability(
        &self,
        logical_instance_id: &AgentServiceInstanceId,
    ) -> CompleteAgentAvailability {
        let state = self.state.read().await;
        if let Some(attachment_id) = state.current_by_logical.get(logical_instance_id)
            && state.entries.contains_key(attachment_id)
        {
            return CompleteAgentAvailability::Available {
                attachment_id: attachment_id.clone(),
            };
        }
        CompleteAgentAvailability::Unavailable {
            reason: state
                .diagnostics
                .get(logical_instance_id)
                .cloned()
                .unwrap_or_else(|| {
                    "Complete Agent 当前 Host incarnation 未 materialize".to_owned()
                }),
        }
    }

    async fn mark_unavailable(&self, logical_instance_id: AgentServiceInstanceId, reason: String) {
        let mut state = self.state.write().await;
        state.current_by_logical.remove(&logical_instance_id);
        state.diagnostics.insert(logical_instance_id, reason);
    }

    async fn retire(&self, attachment_id: &CompleteAgentLiveAttachmentId, reason: String) -> bool {
        let mut state = self.state.write().await;
        let Some(selection) = state.entries.remove(attachment_id) else {
            return false;
        };
        state.retired.insert(attachment_id.clone());
        if state
            .current_by_logical
            .get(&selection.target.logical_instance_id)
            == Some(attachment_id)
        {
            state
                .current_by_logical
                .remove(&selection.target.logical_instance_id);
            state
                .diagnostics
                .insert(selection.target.logical_instance_id.clone(), reason);
        }
        true
    }
}

fn validate_registration(
    registration: &CompleteAgentVerifiedServiceRegistration,
) -> Result<(), CompleteAgentLiveCatalogError> {
    if !registration.placement.is_valid() {
        return Err(CompleteAgentLiveCatalogError::Invariant {
            reason: "placement coordinates must not be empty".to_owned(),
        });
    }
    if registration.verification.service_instance_id != registration.instance_id {
        return Err(CompleteAgentLiveCatalogError::Invariant {
            reason: "verification belongs to another logical service instance".to_owned(),
        });
    }
    Ok(())
}

fn live_attachment_id(
    registration: &CompleteAgentVerifiedServiceRegistration,
) -> Result<CompleteAgentLiveAttachmentId, CompleteAgentLiveCatalogError> {
    #[derive(Serialize)]
    struct Identity<'a> {
        schema: &'static str,
        logical_instance_id: &'a AgentServiceInstanceId,
        placement: &'a crate::CompleteAgentPlacement,
        remote_binding: &'a Option<crate::CompleteAgentRemoteBindingFact>,
    }

    let encoded = serde_json::to_vec(&Identity {
        schema: "agentdash.complete-agent-live-attachment/v1",
        logical_instance_id: &registration.instance_id,
        placement: &registration.placement,
        remote_binding: &registration.remote_binding,
    })
    .map_err(|error| CompleteAgentLiveCatalogError::Invariant {
        reason: format!("failed to encode attachment identity: {error}"),
    })?;
    CompleteAgentLiveAttachmentId::new(format!("attachment:{:x}", Sha256::digest(encoded))).map_err(
        |error| CompleteAgentLiveCatalogError::Invariant {
            reason: error.to_string(),
        },
    )
}

fn same_verified_facts(
    existing: &CompleteAgentLiveSelection,
    candidate: &CompleteAgentLiveSelection,
) -> bool {
    existing.target == candidate.target
        && existing.descriptor == candidate.descriptor
        && existing.verification == candidate.verification
        && existing.offer == candidate.offer
}
