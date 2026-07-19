use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeCommand, ManagedRuntimeCommandAvailability,
    ManagedRuntimeCommandEnvelope, ManagedRuntimeCommandKind, ManagedRuntimeContentBlock,
    ManagedRuntimeGatewayError, ManagedRuntimeInteractionResponse, ManagedRuntimeOperationReceipt,
    ManagedRuntimeReadRequest, ManagedRuntimeUnavailabilityReason, RuntimeIdempotencyKey,
    RuntimeInteractionId, RuntimeOperationId, RuntimeProjectionRevision,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::AgentRunProductRuntimeBindingRepository;

#[derive(Debug, Clone)]
pub struct ProductRuntimeCommandClaimRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub request_digest: String,
    pub envelope: ManagedRuntimeCommandEnvelope,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProductRuntimeCommandClaimError {
    #[error(
        "Product Runtime client command `{client_command_id}` for {target:?} has a different request digest"
    )]
    RequestDigestConflict {
        target: AgentRunTarget,
        client_command_id: String,
    },
    #[error("Product Runtime command claim storage failed: {message}")]
    Storage { message: String },
}

#[async_trait]
pub trait ProductRuntimeCommandClaimRepository: Send + Sync {
    async fn load(
        &self,
        target: &AgentRunTarget,
        client_command_id: &str,
        request_digest: &str,
    ) -> Result<Option<ManagedRuntimeCommandEnvelope>, ProductRuntimeCommandClaimError>;

    async fn claim(
        &self,
        request: ProductRuntimeCommandClaimRequest,
    ) -> Result<ManagedRuntimeCommandEnvelope, ProductRuntimeCommandClaimError>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AgentRunProductCommand {
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt,
    RequestCompaction,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
}

impl AgentRunProductCommand {
    fn runtime_kind(&self, has_active_turn: bool) -> ManagedRuntimeCommandKind {
        match self {
            Self::SubmitInput { .. } if has_active_turn => ManagedRuntimeCommandKind::Steer,
            Self::SubmitInput { .. } => ManagedRuntimeCommandKind::SubmitInput,
            Self::Interrupt => ManagedRuntimeCommandKind::Interrupt,
            Self::RequestCompaction => ManagedRuntimeCommandKind::RequestCompaction,
            Self::ResolveInteraction { .. } => ManagedRuntimeCommandKind::ResolveInteraction,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunProductCommandRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub expected_revision: RuntimeProjectionRevision,
    pub command: AgentRunProductCommand,
}

#[derive(Debug, Error)]
pub enum AgentRunProductCommandError {
    #[error("AgentRun Product binding is missing")]
    TargetNotBound,
    #[error("AgentRun Product binding repository failed: {0}")]
    Binding(String),
    #[error("AgentRun Product binding does not match the requested target")]
    TargetMismatch,
    #[error("Managed Runtime snapshot does not match the committed Product binding")]
    RuntimeBindingMismatch,
    #[error("client command id is invalid")]
    InvalidClientCommandId,
    #[error("client command id is already bound to a different Product command")]
    ClientCommandConflict,
    #[error("Product Runtime command claim persistence failed")]
    ClaimPersistence {
        #[source]
        source: ProductRuntimeCommandClaimError,
    },
    #[error("Managed Runtime command {kind:?} is unavailable: {reason:?}")]
    CommandUnavailable {
        kind: ManagedRuntimeCommandKind,
        reason: Option<ManagedRuntimeUnavailabilityReason>,
    },
    #[error(
        "Managed Runtime availability evidence for {kind:?} was decided at {decided_at_revision:?}, snapshot is {snapshot_revision:?}"
    )]
    StaleAvailabilityEvidence {
        kind: ManagedRuntimeCommandKind,
        decided_at_revision: RuntimeProjectionRevision,
        snapshot_revision: RuntimeProjectionRevision,
    },
    #[error("Managed Runtime has no active turn for this command")]
    ActiveTurnMissing,
    #[error(transparent)]
    Runtime(#[from] ManagedRuntimeGatewayError),
}

pub struct AgentRunProductCommandFacade {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    claims: Arc<dyn ProductRuntimeCommandClaimRepository>,
}

impl AgentRunProductCommandFacade {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        claims: Arc<dyn ProductRuntimeCommandClaimRepository>,
    ) -> Self {
        Self {
            bindings,
            runtime,
            claims,
        }
    }

    pub async fn execute(
        &self,
        request: AgentRunProductCommandRequest,
    ) -> Result<ManagedRuntimeOperationReceipt, AgentRunProductCommandError> {
        let client_command_id = request.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(AgentRunProductCommandError::InvalidClientCommandId);
        }
        let request_digest = format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    "agentdash.product-command-request/v1",
                    &request.command,
                    request.expected_revision,
                ))
                .expect("Product command request is serializable"),
            )
        );
        if let Some(envelope) = self
            .claims
            .load(&request.target, client_command_id, &request_digest)
            .await
            .map_err(product_command_claim_error)?
        {
            return self.runtime.execute(envelope).await.map_err(Into::into);
        }
        let binding = self
            .bindings
            .load_product_binding(&request.target)
            .await
            .map_err(AgentRunProductCommandError::Binding)?
            .ok_or(AgentRunProductCommandError::TargetNotBound)?;
        if binding.target != request.target {
            return Err(AgentRunProductCommandError::TargetMismatch);
        }
        let snapshot = self
            .runtime
            .read(ManagedRuntimeReadRequest {
                thread_id: binding.runtime_thread_id.clone(),
            })
            .await?;
        if snapshot.thread_id != binding.runtime_thread_id
            || snapshot.source_binding.as_ref() != Some(&binding.source_binding)
        {
            return Err(AgentRunProductCommandError::RuntimeBindingMismatch);
        }
        if snapshot.revision != request.expected_revision {
            return Err(AgentRunProductCommandError::Runtime(
                ManagedRuntimeGatewayError::Conflict {
                    actual: snapshot.revision,
                },
            ));
        }
        let runtime_kind = request
            .command
            .runtime_kind(snapshot.active_turn_id.is_some());
        let Some(availability) = snapshot.command_availability.get(&runtime_kind) else {
            return Err(AgentRunProductCommandError::CommandUnavailable {
                kind: runtime_kind,
                reason: None,
            });
        };
        if availability.evidence().decided_at_revision != snapshot.revision {
            return Err(AgentRunProductCommandError::StaleAvailabilityEvidence {
                kind: runtime_kind,
                decided_at_revision: availability.evidence().decided_at_revision,
                snapshot_revision: snapshot.revision,
            });
        }
        if let ManagedRuntimeCommandAvailability::Unavailable { reason, .. } = availability {
            return Err(AgentRunProductCommandError::CommandUnavailable {
                kind: runtime_kind,
                reason: Some(*reason),
            });
        }
        let command = match request.command {
            AgentRunProductCommand::SubmitInput { content } => {
                if let Some(expected_turn_id) = snapshot.active_turn_id {
                    ManagedRuntimeCommand::Steer {
                        expected_turn_id,
                        content,
                    }
                } else {
                    ManagedRuntimeCommand::SubmitInput { content }
                }
            }
            AgentRunProductCommand::Interrupt => ManagedRuntimeCommand::Interrupt {
                expected_turn_id: snapshot
                    .active_turn_id
                    .ok_or(AgentRunProductCommandError::ActiveTurnMissing)?,
            },
            AgentRunProductCommand::RequestCompaction => ManagedRuntimeCommand::RequestCompaction,
            AgentRunProductCommand::ResolveInteraction {
                interaction_id,
                response,
            } => ManagedRuntimeCommand::ResolveInteraction {
                interaction_id,
                response,
            },
        };
        let identity = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    "agentdash.product-command-identity/v1",
                    request.target.run_id,
                    request.target.agent_id,
                    client_command_id,
                ))
                .expect("Product command identity is serializable"),
            )
        );
        let envelope = self
            .claims
            .claim(ProductRuntimeCommandClaimRequest {
                target: request.target,
                client_command_id: client_command_id.to_owned(),
                request_digest,
                envelope: ManagedRuntimeCommandEnvelope {
                    operation_id: RuntimeOperationId::new(format!("product-command:v1:{identity}"))
                        .map_err(|_| AgentRunProductCommandError::InvalidClientCommandId)?,
                    idempotency_key: RuntimeIdempotencyKey::new(format!(
                        "product-command-idempotency:v1:{identity}"
                    ))
                    .map_err(|_| AgentRunProductCommandError::InvalidClientCommandId)?,
                    thread_id: binding.runtime_thread_id,
                    expected_revision: Some(request.expected_revision),
                    command,
                },
            })
            .await
            .map_err(product_command_claim_error)?;
        self.runtime.execute(envelope).await.map_err(Into::into)
    }
}

fn product_command_claim_error(
    error: ProductRuntimeCommandClaimError,
) -> AgentRunProductCommandError {
    match error {
        ProductRuntimeCommandClaimError::RequestDigestConflict { .. } => {
            AgentRunProductCommandError::ClientCommandConflict
        }
        source @ ProductRuntimeCommandClaimError::Storage { .. } => {
            AgentRunProductCommandError::ClaimPersistence { source }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::Arc,
    };

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeChangePage, ManagedRuntimeChangesRequest,
        ManagedRuntimeCommandAvailability, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperationStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot,
        ManagedRuntimeSourceBindingEvidence, RuntimeChangeSequence, RuntimeSourceRef,
        RuntimeThreadId, RuntimeTurnId, SurfaceRevision,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::AgentRunProductRuntimeBinding;

    struct BindingRepository {
        binding: AgentRunProductRuntimeBinding,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for BindingRepository {
        async fn load_product_binding(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(Some(self.binding.clone()))
        }
    }

    struct IdempotentRuntime {
        snapshot: ManagedRuntimeSnapshot,
        accepted: Mutex<
            HashMap<
                String,
                (
                    ManagedRuntimeCommandEnvelope,
                    ManagedRuntimeOperationReceipt,
                ),
            >,
        >,
        observed: Mutex<Vec<ManagedRuntimeCommandEnvelope>>,
    }

    #[derive(Default)]
    struct FixtureRuntimeCommandClaims {
        claims: Mutex<HashMap<String, (String, ManagedRuntimeCommandEnvelope)>>,
    }

    #[async_trait]
    impl ProductRuntimeCommandClaimRepository for FixtureRuntimeCommandClaims {
        async fn load(
            &self,
            target: &AgentRunTarget,
            client_command_id: &str,
            request_digest: &str,
        ) -> Result<Option<ManagedRuntimeCommandEnvelope>, ProductRuntimeCommandClaimError>
        {
            let key = format!("{}:{}:{client_command_id}", target.run_id, target.agent_id);
            let claims = self.claims.lock().await;
            let Some((stored_digest, envelope)) = claims.get(&key) else {
                return Ok(None);
            };
            if stored_digest != request_digest {
                return Err(ProductRuntimeCommandClaimError::RequestDigestConflict {
                    target: target.clone(),
                    client_command_id: client_command_id.to_owned(),
                });
            }
            Ok(Some(envelope.clone()))
        }

        async fn claim(
            &self,
            request: ProductRuntimeCommandClaimRequest,
        ) -> Result<ManagedRuntimeCommandEnvelope, ProductRuntimeCommandClaimError> {
            let key = format!(
                "{}:{}:{}",
                request.target.run_id, request.target.agent_id, request.client_command_id
            );
            let mut claims = self.claims.lock().await;
            if let Some((stored_digest, envelope)) = claims.get(&key) {
                if stored_digest != &request.request_digest {
                    return Err(ProductRuntimeCommandClaimError::RequestDigestConflict {
                        target: request.target,
                        client_command_id: request.client_command_id,
                    });
                }
                return Ok(envelope.clone());
            }
            claims.insert(key, (request.request_digest, request.envelope.clone()));
            Ok(request.envelope)
        }
    }

    struct FailingRuntimeCommandClaims;

    #[async_trait]
    impl ProductRuntimeCommandClaimRepository for FailingRuntimeCommandClaims {
        async fn load(
            &self,
            _target: &AgentRunTarget,
            _client_command_id: &str,
            _request_digest: &str,
        ) -> Result<Option<ManagedRuntimeCommandEnvelope>, ProductRuntimeCommandClaimError>
        {
            Err(ProductRuntimeCommandClaimError::Storage {
                message: "claim store unavailable".to_owned(),
            })
        }

        async fn claim(
            &self,
            _request: ProductRuntimeCommandClaimRequest,
        ) -> Result<ManagedRuntimeCommandEnvelope, ProductRuntimeCommandClaimError> {
            Err(ProductRuntimeCommandClaimError::Storage {
                message: "claim store unavailable".to_owned(),
            })
        }
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for IdempotentRuntime {
        async fn execute(
            &self,
            envelope: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            self.observed.lock().await.push(envelope.clone());
            let key = envelope.idempotency_key.as_str().to_owned();
            let mut accepted = self.accepted.lock().await;
            if let Some((prior, receipt)) = accepted.get(&key) {
                if prior != &envelope {
                    return Err(ManagedRuntimeGatewayError::Invalid {
                        reason: "idempotency key is bound to a different command".to_owned(),
                    });
                }
                let mut duplicate = receipt.clone();
                duplicate.duplicate = true;
                return Ok(duplicate);
            }
            let receipt = ManagedRuntimeOperationReceipt {
                operation_id: envelope.operation_id.clone(),
                thread_id: envelope.thread_id.clone(),
                accepted_revision: self.snapshot.revision,
                status: ManagedRuntimeOperationStatus::Accepted,
                evidence: None,
                duplicate: false,
            };
            accepted.insert(key, (envelope, receipt.clone()));
            Ok(receipt)
        }

        async fn read(
            &self,
            _request: ManagedRuntimeReadRequest,
        ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError> {
            Ok(self.snapshot.clone())
        }

        async fn changes(
            &self,
            _request: ManagedRuntimeChangesRequest,
        ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
            Err(ManagedRuntimeGatewayError::Invalid {
                reason: "changes are not used by command facade tests".to_owned(),
            })
        }
    }

    fn fixture(
        active_turn: bool,
    ) -> (
        AgentRunTarget,
        AgentRunProductRuntimeBinding,
        ManagedRuntimeSnapshot,
    ) {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let thread_id = RuntimeThreadId::new("runtime-product-command").expect("thread");
        let source_binding = ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:product-command").expect("source"),
            committed_at_revision: RuntimeProjectionRevision(3),
            applied_surface_revision: SurfaceRevision(4),
            activated_at_revision: Some(RuntimeProjectionRevision(5)),
        };
        let mut command_availability = BTreeMap::new();
        for kind in [
            ManagedRuntimeCommandKind::SubmitInput,
            ManagedRuntimeCommandKind::Steer,
            ManagedRuntimeCommandKind::Interrupt,
            ManagedRuntimeCommandKind::RequestCompaction,
            ManagedRuntimeCommandKind::ResolveInteraction,
        ] {
            command_availability.insert(
                kind,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: ManagedRuntimeAvailabilityEvidence {
                        decided_at_revision: RuntimeProjectionRevision(7),
                        blocking_operation_id: None,
                        bound_surface_revision: None,
                        applied_surface_revision: None,
                    },
                },
            );
        }
        let snapshot = ManagedRuntimeSnapshot {
            thread_id: thread_id.clone(),
            revision: RuntimeProjectionRevision(7),
            latest_change_sequence: RuntimeChangeSequence(7),
            captured_at_ms: 10,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: active_turn.then(|| RuntimeTurnId::new("turn-active").expect("turn")),
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            thread_name: None,
            thread_name_source: None,
            operations: Vec::new(),
            source_binding: Some(source_binding.clone()),
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability,
        };
        let binding = AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: thread_id,
            source_binding,
        };
        (target, binding, snapshot)
    }

    fn facade(
        active_turn: bool,
    ) -> (
        AgentRunTarget,
        AgentRunProductCommandFacade,
        Arc<IdempotentRuntime>,
    ) {
        let (target, binding, snapshot) = fixture(active_turn);
        let runtime = Arc::new(IdempotentRuntime {
            snapshot,
            accepted: Mutex::new(HashMap::new()),
            observed: Mutex::new(Vec::new()),
        });
        let claims = Arc::new(FixtureRuntimeCommandClaims::default());
        (
            target,
            AgentRunProductCommandFacade::new(
                Arc::new(BindingRepository { binding }),
                runtime.clone(),
                claims,
            ),
            runtime,
        )
    }

    fn facade_from_parts(
        binding: AgentRunProductRuntimeBinding,
        snapshot: ManagedRuntimeSnapshot,
    ) -> AgentRunProductCommandFacade {
        AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository { binding }),
            Arc::new(IdempotentRuntime {
                snapshot,
                accepted: Mutex::new(HashMap::new()),
                observed: Mutex::new(Vec::new()),
            }),
            Arc::new(FixtureRuntimeCommandClaims::default()),
        )
    }

    fn request(
        target: AgentRunTarget,
        client_command_id: &str,
        command: AgentRunProductCommand,
    ) -> AgentRunProductCommandRequest {
        AgentRunProductCommandRequest {
            target,
            client_command_id: client_command_id.to_owned(),
            expected_revision: RuntimeProjectionRevision(7),
            command,
        }
    }

    fn commands() -> Vec<(bool, AgentRunProductCommand)> {
        vec![
            (
                false,
                AgentRunProductCommand::SubmitInput {
                    content: vec![ManagedRuntimeContentBlock::Text {
                        text: "hello".to_owned(),
                    }],
                },
            ),
            (true, AgentRunProductCommand::Interrupt),
            (false, AgentRunProductCommand::RequestCompaction),
            (
                false,
                AgentRunProductCommand::ResolveInteraction {
                    interaction_id: RuntimeInteractionId::new("interaction-1")
                        .expect("interaction"),
                    response: ManagedRuntimeInteractionResponse::Approved,
                },
            ),
        ]
    }

    #[tokio::test]
    async fn every_product_command_replays_same_client_and_payload() {
        for (active_turn, command) in commands() {
            let (target, facade, runtime) = facade(active_turn);
            let request = request(target, "client-stable", command);
            let first = facade.execute(request.clone()).await.expect("first");
            let replay = facade.execute(request).await.expect("replay");
            assert!(!first.duplicate);
            assert!(replay.duplicate);
            let observed = runtime.observed.lock().await;
            assert_eq!(observed.len(), 2);
            assert_eq!(observed[0], observed[1]);
        }
    }

    #[tokio::test]
    async fn same_client_with_different_command_or_content_conflicts() {
        let (target, facade, _) = facade(false);
        facade
            .execute(request(
                target.clone(),
                "client-conflict",
                AgentRunProductCommand::SubmitInput {
                    content: vec![ManagedRuntimeContentBlock::Text {
                        text: "first".to_owned(),
                    }],
                },
            ))
            .await
            .expect("first");
        let error = facade
            .execute(request(
                target,
                "client-conflict",
                AgentRunProductCommand::SubmitInput {
                    content: vec![ManagedRuntimeContentBlock::Text {
                        text: "different".to_owned(),
                    }],
                },
            ))
            .await
            .expect_err("different content must conflict");
        assert!(matches!(
            error,
            AgentRunProductCommandError::ClientCommandConflict
        ));
    }

    #[tokio::test]
    async fn rejects_binding_target_and_runtime_source_mismatch() {
        let (target, mut binding, snapshot) = fixture(false);
        binding.target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let error = facade_from_parts(binding, snapshot)
            .execute(request(
                target,
                "binding-target-mismatch",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("binding target fence");
        assert!(matches!(error, AgentRunProductCommandError::TargetMismatch));

        let (target, binding, mut snapshot) = fixture(false);
        snapshot.source_binding = Some(ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:different").expect("source"),
            ..binding.source_binding.clone()
        });
        let error = facade_from_parts(binding, snapshot)
            .execute(request(
                target,
                "binding-source-mismatch",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("source binding fence");
        assert!(matches!(
            error,
            AgentRunProductCommandError::RuntimeBindingMismatch
        ));
    }

    #[tokio::test]
    async fn rejects_expected_revision_conflict_before_claim() {
        let (target, binding, snapshot) = fixture(false);
        let mut request = request(
            target,
            "revision-conflict",
            AgentRunProductCommand::RequestCompaction,
        );
        request.expected_revision = RuntimeProjectionRevision(6);
        let error = facade_from_parts(binding, snapshot)
            .execute(request)
            .await
            .expect_err("revision conflict");
        assert!(matches!(
            error,
            AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::Conflict {
                actual: RuntimeProjectionRevision(7)
            })
        ));
    }

    #[tokio::test]
    async fn preserves_claim_persistence_error_category_and_source() {
        let (target, binding, snapshot) = fixture(false);
        let runtime = Arc::new(IdempotentRuntime {
            snapshot,
            accepted: Mutex::new(HashMap::new()),
            observed: Mutex::new(Vec::new()),
        });
        let facade = AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository { binding }),
            runtime,
            Arc::new(FailingRuntimeCommandClaims),
        );

        let error = facade
            .execute(request(
                target,
                "claim-storage-failure",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("claim persistence failure");
        assert!(matches!(
            error,
            AgentRunProductCommandError::ClaimPersistence {
                source: ProductRuntimeCommandClaimError::Storage { ref message },
            } if message == "claim store unavailable"
        ));
    }

    #[tokio::test]
    async fn rejects_missing_unavailable_and_stale_availability_evidence() {
        let (target, binding, mut snapshot) = fixture(false);
        snapshot
            .command_availability
            .remove(&ManagedRuntimeCommandKind::RequestCompaction);
        let error = facade_from_parts(binding, snapshot)
            .execute(request(
                target,
                "missing-availability",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("missing availability");
        assert!(matches!(
            error,
            AgentRunProductCommandError::CommandUnavailable {
                kind: ManagedRuntimeCommandKind::RequestCompaction,
                reason: None
            }
        ));

        let (target, binding, mut snapshot) = fixture(false);
        snapshot.command_availability.insert(
            ManagedRuntimeCommandKind::RequestCompaction,
            ManagedRuntimeCommandAvailability::Unavailable {
                reason: ManagedRuntimeUnavailabilityReason::OperationInFlight,
                evidence: ManagedRuntimeAvailabilityEvidence {
                    decided_at_revision: snapshot.revision,
                    blocking_operation_id: None,
                    bound_surface_revision: None,
                    applied_surface_revision: None,
                },
            },
        );
        let error = facade_from_parts(binding, snapshot)
            .execute(request(
                target,
                "unavailable",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("unavailable");
        assert!(matches!(
            error,
            AgentRunProductCommandError::CommandUnavailable {
                kind: ManagedRuntimeCommandKind::RequestCompaction,
                reason: Some(ManagedRuntimeUnavailabilityReason::OperationInFlight)
            }
        ));

        let (target, binding, mut snapshot) = fixture(false);
        snapshot.command_availability.insert(
            ManagedRuntimeCommandKind::RequestCompaction,
            ManagedRuntimeCommandAvailability::Available {
                evidence: ManagedRuntimeAvailabilityEvidence {
                    decided_at_revision: RuntimeProjectionRevision(6),
                    blocking_operation_id: None,
                    bound_surface_revision: None,
                    applied_surface_revision: None,
                },
            },
        );
        let error = facade_from_parts(binding, snapshot)
            .execute(request(
                target,
                "stale-availability",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect_err("stale availability evidence");
        assert!(matches!(
            error,
            AgentRunProductCommandError::StaleAvailabilityEvidence {
                kind: ManagedRuntimeCommandKind::RequestCompaction,
                decided_at_revision: RuntimeProjectionRevision(6),
                snapshot_revision: RuntimeProjectionRevision(7)
            }
        ));
    }

    #[tokio::test]
    async fn operation_identity_is_stable_across_facade_restart() {
        let (target, binding, snapshot) = fixture(false);
        let runtime_before = Arc::new(IdempotentRuntime {
            snapshot: snapshot.clone(),
            accepted: Mutex::new(HashMap::new()),
            observed: Mutex::new(Vec::new()),
        });
        let durable_claims = Arc::new(FixtureRuntimeCommandClaims::default());
        let before = AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository {
                binding: binding.clone(),
            }),
            runtime_before.clone(),
            durable_claims.clone(),
        );
        before
            .execute(request(
                target.clone(),
                "client-restart",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect("before restart");

        let runtime_after = Arc::new(IdempotentRuntime {
            snapshot,
            accepted: Mutex::new(HashMap::new()),
            observed: Mutex::new(Vec::new()),
        });
        let after = AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository { binding }),
            runtime_after.clone(),
            durable_claims,
        );
        after
            .execute(request(
                target,
                "client-restart",
                AgentRunProductCommand::RequestCompaction,
            ))
            .await
            .expect("after restart");

        let before = runtime_before.observed.lock().await;
        let after = runtime_after.observed.lock().await;
        assert_eq!(before[0].operation_id, after[0].operation_id);
        assert_eq!(before[0].idempotency_key, after[0].idempotency_key);
    }

    struct LostResponseRuntime {
        snapshot: Mutex<ManagedRuntimeSnapshot>,
        accepted: Mutex<Option<ManagedRuntimeCommandEnvelope>>,
        lose_first_response: Mutex<bool>,
        read_count: Mutex<usize>,
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for LostResponseRuntime {
        async fn execute(
            &self,
            envelope: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            let mut accepted = self.accepted.lock().await;
            if let Some(prior) = accepted.as_ref() {
                if prior != &envelope {
                    return Err(ManagedRuntimeGatewayError::Invalid {
                        reason: "replay envelope changed".to_owned(),
                    });
                }
                return Ok(ManagedRuntimeOperationReceipt {
                    operation_id: envelope.operation_id,
                    thread_id: envelope.thread_id,
                    accepted_revision: RuntimeProjectionRevision(8),
                    status: ManagedRuntimeOperationStatus::Accepted,
                    evidence: None,
                    duplicate: true,
                });
            }
            *accepted = Some(envelope);
            let mut snapshot = self.snapshot.lock().await;
            snapshot.revision = RuntimeProjectionRevision(8);
            snapshot.active_turn_id = Some(RuntimeTurnId::new("turn-after-accept").expect("turn"));
            if std::mem::take(&mut *self.lose_first_response.lock().await) {
                return Err(ManagedRuntimeGatewayError::Unavailable {
                    reason: "response lost after durable Runtime accept".to_owned(),
                });
            }
            unreachable!("first response must be lost")
        }

        async fn read(
            &self,
            _request: ManagedRuntimeReadRequest,
        ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError> {
            *self.read_count.lock().await += 1;
            Ok(self.snapshot.lock().await.clone())
        }

        async fn changes(
            &self,
            _request: ManagedRuntimeChangesRequest,
        ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
            Err(ManagedRuntimeGatewayError::NotFound)
        }
    }

    #[tokio::test]
    async fn lost_runtime_response_replays_claimed_envelope_before_latest_snapshot_gate() {
        let (target, binding, snapshot) = fixture(false);
        let runtime = Arc::new(LostResponseRuntime {
            snapshot: Mutex::new(snapshot),
            accepted: Mutex::new(None),
            lose_first_response: Mutex::new(true),
            read_count: Mutex::new(0),
        });
        let claims = Arc::new(FixtureRuntimeCommandClaims::default());
        let first_process = AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository {
                binding: binding.clone(),
            }),
            runtime.clone(),
            claims.clone(),
        );
        let command = request(
            target.clone(),
            "client-lost-response",
            AgentRunProductCommand::SubmitInput {
                content: vec![ManagedRuntimeContentBlock::Text {
                    text: "durable".to_owned(),
                }],
            },
        );
        assert!(matches!(
            first_process.execute(command.clone()).await,
            Err(AgentRunProductCommandError::Runtime(
                ManagedRuntimeGatewayError::Unavailable { .. }
            ))
        ));

        let restarted_process = AgentRunProductCommandFacade::new(
            Arc::new(BindingRepository { binding }),
            runtime.clone(),
            claims,
        );
        let replay = restarted_process
            .execute(command)
            .await
            .expect("durable envelope replay");
        assert!(replay.duplicate);
        assert_eq!(*runtime.read_count.lock().await, 1);
        let accepted = runtime.accepted.lock().await;
        assert!(matches!(
            accepted.as_ref().map(|envelope| &envelope.command),
            Some(ManagedRuntimeCommand::SubmitInput { .. })
        ));
    }
}
