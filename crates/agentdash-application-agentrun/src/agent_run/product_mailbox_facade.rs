use std::sync::Arc;

use agentdash_domain::{
    agent_run_mailbox::{AgentRunMailboxMessage, AgentRunMailboxState},
    agent_run_target::AgentRunTarget,
    workflow::AgentRunCommandKind,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::AgentRunProductRuntimeBindingRepository;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxCursor {
    pub revision: u64,
    pub latest_change_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChange {
    pub change_id: Uuid,
    pub sequence: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChangePage {
    pub changes: Vec<ProductMailboxChange>,
    pub next: u64,
    pub gap: Option<ProductMailboxChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChangeGap {
    pub requested_after: u64,
    pub earliest_available: u64,
    pub latest_available: u64,
    pub snapshot_revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductMailboxSnapshot {
    pub target: AgentRunTarget,
    pub cursor: ProductMailboxCursor,
    pub messages: Vec<AgentRunMailboxMessage>,
    pub state: Option<AgentRunMailboxState>,
}

#[async_trait]
pub trait ProductMailboxReadRepository: Send + Sync {
    /// Reads messages and state in one database snapshot, reconciles their canonical digest
    /// against the Product head, and returns the matching cursor atomically.
    async fn snapshot(&self, target: &AgentRunTarget) -> Result<ProductMailboxSnapshot, String>;

    async fn changes(
        &self,
        target: &AgentRunTarget,
        after: u64,
        limit: usize,
    ) -> Result<ProductMailboxChangePage, String>;

    async fn content(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<Option<serde_json::Value>, String>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProductMailboxCommand {
    Promote {
        message_id: Uuid,
    },
    Delete {
        message_id: Uuid,
    },
    Move {
        message_id: Uuid,
        after_message_id: Option<Uuid>,
    },
    Resume,
}

impl ProductMailboxCommand {
    fn receipt_kind(&self) -> AgentRunCommandKind {
        match self {
            Self::Promote { .. } => AgentRunCommandKind::MailboxPromote,
            Self::Delete { .. } => AgentRunCommandKind::MailboxDelete,
            Self::Move { .. } => AgentRunCommandKind::MailboxMove,
            Self::Resume => AgentRunCommandKind::MailboxResume,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductMailboxCommandRequest {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub command: ProductMailboxCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxCommandReceipt {
    pub client_command_id: String,
    pub duplicate: bool,
    pub revision: u64,
    pub latest_change_sequence: u64,
}

#[derive(Debug, Clone)]
pub struct ProductMailboxDurableCommand {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub request_digest: String,
    pub command_kind: AgentRunCommandKind,
    pub command: ProductMailboxCommand,
}

#[async_trait]
pub trait ProductMailboxCommandRepository: Send + Sync {
    async fn execute(
        &self,
        command: ProductMailboxDurableCommand,
    ) -> Result<ProductMailboxCommandReceipt, String>;
}

#[derive(Debug, Error)]
pub enum ProductMailboxError {
    #[error("AgentRun Product binding is missing")]
    TargetNotBound,
    #[error("Product mailbox input is invalid: {0}")]
    Invalid(String),
    #[error("Product mailbox command conflicts with a previous client command: {0}")]
    Conflict(String),
    #[error("Product mailbox entity was not found: {0}")]
    NotFound(String),
    #[error("Product mailbox repository failed: {0}")]
    Repository(String),
}

pub struct ProductMailboxFacade {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    reads: Arc<dyn ProductMailboxReadRepository>,
    commands: Arc<dyn ProductMailboxCommandRepository>,
}

impl ProductMailboxFacade {
    pub fn new(
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        reads: Arc<dyn ProductMailboxReadRepository>,
        commands: Arc<dyn ProductMailboxCommandRepository>,
    ) -> Self {
        Self {
            bindings,
            reads,
            commands,
        }
    }

    pub async fn snapshot(
        &self,
        target: AgentRunTarget,
    ) -> Result<ProductMailboxSnapshot, ProductMailboxError> {
        self.require_binding(&target).await?;
        self.reads
            .snapshot(&target)
            .await
            .map_err(ProductMailboxError::Repository)
    }

    pub async fn changes(
        &self,
        target: AgentRunTarget,
        after: u64,
        limit: usize,
    ) -> Result<ProductMailboxChangePage, ProductMailboxError> {
        self.require_binding(&target).await?;
        self.reads
            .changes(&target, after, limit)
            .await
            .map_err(ProductMailboxError::Repository)
    }

    pub async fn content(
        &self,
        target: AgentRunTarget,
        message_id: Uuid,
    ) -> Result<serde_json::Value, ProductMailboxError> {
        self.require_binding(&target).await?;
        self.reads
            .content(&target, message_id)
            .await
            .map_err(ProductMailboxError::Repository)?
            .ok_or_else(|| ProductMailboxError::NotFound(message_id.to_string()))
    }

    pub async fn execute(
        &self,
        request: ProductMailboxCommandRequest,
    ) -> Result<ProductMailboxCommandReceipt, ProductMailboxError> {
        let client_command_id = request.client_command_id.trim();
        if client_command_id.is_empty() || client_command_id.len() > 256 {
            return Err(ProductMailboxError::Invalid(
                "client_command_id 无效".to_owned(),
            ));
        }
        self.require_binding(&request.target).await?;
        let request_digest = format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_vec(&request.command)
                    .expect("Product mailbox command is serializable")
            )
        );
        self.commands
            .execute(ProductMailboxDurableCommand {
                target: request.target,
                client_command_id: client_command_id.to_owned(),
                request_digest,
                command_kind: request.command.receipt_kind(),
                command: request.command,
            })
            .await
            .map_err(|error| {
                if error.starts_with("conflict:") {
                    ProductMailboxError::Conflict(error)
                } else if error.starts_with("not_found:") {
                    ProductMailboxError::NotFound(error)
                } else {
                    ProductMailboxError::Repository(error)
                }
            })
    }

    async fn require_binding(&self, target: &AgentRunTarget) -> Result<(), ProductMailboxError> {
        self.bindings
            .load_product_binding(target)
            .await
            .map_err(ProductMailboxError::Repository)?
            .ok_or(ProductMailboxError::TargetNotBound)?;
        Ok(())
    }
}

pub fn canonical_product_mailbox_digest(
    messages: &[AgentRunMailboxMessage],
    state: Option<&AgentRunMailboxState>,
) -> String {
    let canonical_messages = messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "id": message.id,
                "run_id": message.run_id,
                "agent_id": message.agent_id,
                "origin": message.origin.as_str(),
                "source": {
                    "namespace": message.source.namespace,
                    "kind": message.source.kind,
                    "source_ref": message.source.source_ref,
                    "correlation_ref": message.source.correlation_ref,
                    "actor": message.source.actor,
                    "route": message.source.route,
                    "display_label_key": message.source.display_label_key,
                    "metadata": message.source.metadata,
                },
                "delivery": {
                    "kind": message.delivery.kind(),
                    "value": message.delivery.to_json(),
                },
                "barrier": message.barrier.as_str(),
                "drain_mode": message.drain_mode.as_str(),
                "status": message.status.as_str(),
                "priority": message.priority,
                "order_key": message.order_key,
                "source_dedup_key": message.source_dedup_key,
                "delivery_request_digest": message.delivery_request_digest,
                "accepted_runtime_operation_id": message.accepted_runtime_operation_id,
                "reconcile_required": message.reconcile_required,
                "claim_token": message.claim_token,
                "claimed_at": message.claimed_at,
                "claim_expires_at": message.claim_expires_at,
                "payload_json": message.payload_json,
                "launch_planning_input": message.launch_planning_input,
                "preview": message.preview,
                "has_images": message.has_images,
                "retain_payload": message.retain_payload,
                "attempt_count": message.attempt_count,
                "last_error": message.last_error,
                "created_at": message.created_at,
                "updated_at": message.updated_at,
                "consumed_at": message.consumed_at,
                "deleted_at": message.deleted_at,
            })
        })
        .collect::<Vec<_>>();
    let canonical_state = state.map(|state| {
        serde_json::json!({
            "run_id": state.run_id,
            "agent_id": state.agent_id,
            "paused": state.paused,
            "pause_reason": state.pause_reason,
            "pause_message": state.pause_message,
            "updated_at": state.updated_at,
        })
    });
    let canonical = serde_json::to_vec(&serde_json::json!({
        "schema": "agentdash.product-mailbox-snapshot/v1",
        "messages": canonical_messages,
        "state": canonical_state,
    }))
    .expect("canonical Product mailbox snapshot is serializable");
    format!("sha256:{:x}", Sha256::digest(canonical))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeSourceBindingEvidence, RuntimeProjectionRevision, RuntimeSourceRef,
        RuntimeThreadId, SurfaceRevision,
    };
    use tokio::sync::Mutex;

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

    #[derive(Default)]
    struct AtomicCommandFixture {
        receipts: Mutex<HashMap<String, (String, ProductMailboxCommandReceipt)>>,
        cursor: Mutex<ProductMailboxCursor>,
        fail_before_commit_once: AtomicBool,
        commits: Mutex<u64>,
    }

    #[async_trait]
    impl ProductMailboxCommandRepository for AtomicCommandFixture {
        async fn execute(
            &self,
            command: ProductMailboxDurableCommand,
        ) -> Result<ProductMailboxCommandReceipt, String> {
            let key = format!(
                "{}:{}:{}",
                command.target.run_id, command.target.agent_id, command.client_command_id
            );
            let mut receipts = self.receipts.lock().await;
            if let Some((digest, receipt)) = receipts.get(&key) {
                if digest != &command.request_digest {
                    return Err("conflict: request digest differs".to_owned());
                }
                return Ok(ProductMailboxCommandReceipt {
                    duplicate: true,
                    ..receipt.clone()
                });
            }
            if self.fail_before_commit_once.swap(false, Ordering::SeqCst) {
                return Err("injected crash before atomic commit".to_owned());
            }
            let mut cursor = self.cursor.lock().await;
            cursor.revision += 1;
            cursor.latest_change_sequence += 1;
            let receipt = ProductMailboxCommandReceipt {
                client_command_id: command.client_command_id,
                duplicate: false,
                revision: cursor.revision,
                latest_change_sequence: cursor.latest_change_sequence,
            };
            receipts.insert(key, (command.request_digest, receipt.clone()));
            *self.commits.lock().await += 1;
            Ok(receipt)
        }
    }

    struct AtomicReadFixture {
        target: AgentRunTarget,
        state: Mutex<(ProductMailboxCursor, Vec<ProductMailboxChange>)>,
    }

    impl AtomicReadFixture {
        async fn external_mutation(&self) {
            let mut state = self.state.lock().await;
            state.0.revision += 1;
            state.0.latest_change_sequence += 1;
            let cursor = state.0;
            state.1.push(ProductMailboxChange {
                change_id: Uuid::new_v4(),
                sequence: cursor.latest_change_sequence,
                revision: cursor.revision,
            });
        }
    }

    #[async_trait]
    impl ProductMailboxReadRepository for AtomicReadFixture {
        async fn snapshot(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<ProductMailboxSnapshot, String> {
            let state = self.state.lock().await;
            Ok(ProductMailboxSnapshot {
                target: self.target.clone(),
                cursor: state.0,
                messages: Vec::new(),
                state: None,
            })
        }

        async fn changes(
            &self,
            _target: &AgentRunTarget,
            after: u64,
            limit: usize,
        ) -> Result<ProductMailboxChangePage, String> {
            let state = self.state.lock().await;
            let changes = state
                .1
                .iter()
                .filter(|change| change.sequence > after)
                .take(limit)
                .cloned()
                .collect::<Vec<_>>();
            Ok(ProductMailboxChangePage {
                next: changes
                    .last()
                    .map(|change| change.sequence)
                    .unwrap_or(after),
                changes,
                gap: None,
            })
        }

        async fn content(
            &self,
            _target: &AgentRunTarget,
            _message_id: Uuid,
        ) -> Result<Option<serde_json::Value>, String> {
            Ok(None)
        }
    }

    fn fixture() -> (
        AgentRunTarget,
        Arc<AtomicReadFixture>,
        Arc<AtomicCommandFixture>,
        ProductMailboxFacade,
    ) {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let binding = AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: RuntimeThreadId::new("mailbox-thread").expect("thread"),
            source_binding: ManagedRuntimeSourceBindingEvidence {
                source_ref: RuntimeSourceRef::new("source:mailbox").expect("source"),
                committed_at_revision: RuntimeProjectionRevision(1),
                applied_surface_revision: SurfaceRevision(1),
                activated_at_revision: Some(RuntimeProjectionRevision(1)),
            },
        };
        let reads = Arc::new(AtomicReadFixture {
            target: target.clone(),
            state: Mutex::new((ProductMailboxCursor::default(), Vec::new())),
        });
        let commands = Arc::new(AtomicCommandFixture::default());
        let facade = ProductMailboxFacade::new(
            Arc::new(BindingRepository { binding }),
            reads.clone(),
            commands.clone(),
        );
        (target, reads, commands, facade)
    }

    #[tokio::test]
    async fn crash_before_uow_commit_retries_once_and_replays_terminal_receipt() {
        let (target, _, commands, facade) = fixture();
        commands
            .fail_before_commit_once
            .store(true, Ordering::SeqCst);
        let request = ProductMailboxCommandRequest {
            target,
            client_command_id: "mailbox-crash".to_owned(),
            command: ProductMailboxCommand::Resume,
        };
        assert!(matches!(
            facade.execute(request.clone()).await,
            Err(ProductMailboxError::Repository(_))
        ));
        let accepted = facade.execute(request.clone()).await.expect("retry");
        let replay = facade.execute(request).await.expect("replay");
        assert_eq!(accepted.revision, 1);
        assert!(replay.duplicate);
        assert_eq!(replay.revision, accepted.revision);
        assert_eq!(*commands.commits.lock().await, 1);
    }

    #[tokio::test]
    async fn same_mailbox_client_with_different_payload_conflicts() {
        let (target, _, _, facade) = fixture();
        facade
            .execute(ProductMailboxCommandRequest {
                target: target.clone(),
                client_command_id: "mailbox-conflict".to_owned(),
                command: ProductMailboxCommand::Resume,
            })
            .await
            .expect("first");
        let error = facade
            .execute(ProductMailboxCommandRequest {
                target,
                client_command_id: "mailbox-conflict".to_owned(),
                command: ProductMailboxCommand::Delete {
                    message_id: Uuid::new_v4(),
                },
            })
            .await
            .expect_err("digest conflict");
        assert!(matches!(error, ProductMailboxError::Conflict(_)));
    }

    #[tokio::test]
    async fn snapshot_and_change_cursor_stay_continuous_under_concurrent_reads() {
        let (target, reads, _, facade) = fixture();
        for _ in 0..32 {
            let (_, snapshot) =
                tokio::join!(reads.external_mutation(), facade.snapshot(target.clone()),);
            let snapshot = snapshot.expect("snapshot");
            assert_eq!(
                snapshot.cursor.revision,
                snapshot.cursor.latest_change_sequence
            );
        }
        let snapshot = facade.snapshot(target.clone()).await.expect("snapshot");
        let page = facade.changes(target, 0, 256).await.expect("changes");
        assert_eq!(page.next, snapshot.cursor.latest_change_sequence);
        assert_eq!(page.changes.len() as u64, page.next);
        assert!(page.gap.is_none());
    }
}
