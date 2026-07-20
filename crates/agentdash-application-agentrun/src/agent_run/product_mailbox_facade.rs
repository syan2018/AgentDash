use std::{collections::BTreeMap, sync::Arc};

use agentdash_domain::{
    agent_run_mailbox::{AgentRunMailboxMessage, AgentRunMailboxState},
    agent_run_target::AgentRunTarget,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductMailboxSnapshotDigest(String);

impl ProductMailboxSnapshotDigest {
    pub fn new(value: impl Into<String>) -> Result<Self, ProductMailboxDigestError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(ProductMailboxDigestError::InvalidFormat);
        };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ProductMailboxDigestError::InvalidFormat);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProductMailboxDigestError {
    #[error("Product mailbox snapshot digest must be a sha256 hex digest")]
    InvalidFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxCommitEvidence {
    pub snapshot_digest: ProductMailboxSnapshotDigest,
    pub committed_at_ms: ProductMailboxCommittedAtMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProductMailboxCommittedAtMs(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductMailboxChangeOrigin {
    Command {
        client_command_id: String,
        command_kind: ProductMailboxCommandKind,
    },
    CanonicalReconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChange {
    pub change_id: Uuid,
    pub target: AgentRunTarget,
    pub sequence: u64,
    pub revision: u64,
    pub origin: ProductMailboxChangeOrigin,
    pub commit: ProductMailboxCommitEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChangePage {
    pub target: AgentRunTarget,
    pub changes: Vec<ProductMailboxChange>,
    pub next: u64,
    pub head: ProductMailboxCursor,
    pub head_commit: ProductMailboxCommitEvidence,
    pub gap: Option<ProductMailboxChangeGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMailboxChangeGap {
    pub requested_after: u64,
    pub earliest_available: u64,
    pub latest_available: u64,
    pub snapshot_revision: u64,
    pub snapshot_digest: ProductMailboxSnapshotDigest,
    pub detected_at_ms: ProductMailboxCommittedAtMs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductMailboxSnapshot {
    pub target: AgentRunTarget,
    pub cursor: ProductMailboxCursor,
    pub commit: ProductMailboxCommitEvidence,
    pub messages: Vec<AgentRunMailboxMessage>,
    pub state: Option<AgentRunMailboxState>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProductMailboxReadError {
    #[error("Product mailbox read target mismatch: expected {expected:?}, observed {observed:?}")]
    TargetMismatch {
        expected: AgentRunTarget,
        observed: AgentRunTarget,
    },
    #[error("Product mailbox message `{message_id}` was not found for {target:?}")]
    MessageNotFound {
        target: AgentRunTarget,
        message_id: Uuid,
    },
    #[error("Product mailbox change continuity is invalid: {message}")]
    InvalidContinuity { message: String },
    #[error(
        "Product mailbox change revision regressed at sequence {sequence}: previous {previous_revision}, observed {observed_revision}"
    )]
    RevisionRegression {
        sequence: u64,
        previous_revision: u64,
        observed_revision: u64,
    },
    #[error("Product mailbox read storage failed: {message}")]
    Storage { message: String },
}

#[async_trait]
pub trait ProductMailboxReadRepository: Send + Sync {
    /// Reads messages and state in one database snapshot, computes the canonical digest,
    /// reconciles the Product head/change, and returns the cursor for that exact state.
    async fn snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ProductMailboxSnapshot, ProductMailboxReadError>;

    /// Reconciles external canonical mutations before reading the ordered Product change log.
    async fn changes(
        &self,
        target: &AgentRunTarget,
        after: u64,
        limit: usize,
    ) -> Result<ProductMailboxChangePage, ProductMailboxReadError>;

    async fn content(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<Option<serde_json::Value>, ProductMailboxReadError>;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductMailboxCommandKind {
    Promote,
    Delete,
    Move,
    Resume,
}

impl ProductMailboxCommand {
    pub fn kind(&self) -> ProductMailboxCommandKind {
        match self {
            Self::Promote { .. } => ProductMailboxCommandKind::Promote,
            Self::Delete { .. } => ProductMailboxCommandKind::Delete,
            Self::Move { .. } => ProductMailboxCommandKind::Move,
            Self::Resume => ProductMailboxCommandKind::Resume,
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
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub revision: u64,
    pub latest_change_sequence: u64,
    pub commit: ProductMailboxCommitEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMailboxCommandOutcome {
    pub receipt: ProductMailboxCommandReceipt,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct ProductMailboxDurableCommand {
    pub target: AgentRunTarget,
    pub client_command_id: String,
    pub request_digest: String,
    pub command: ProductMailboxCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductMailboxInvalidMoveReason {
    SelfAnchor,
    CrossPriorityLane,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProductMailboxCommandRepositoryError {
    #[error(
        "Product mailbox client command `{client_command_id}` for {target:?} has a different request digest"
    )]
    RequestDigestConflict {
        target: AgentRunTarget,
        client_command_id: String,
    },
    #[error(
        "Product mailbox command target mismatch: expected {expected:?}, observed {observed:?}"
    )]
    TargetMismatch {
        expected: AgentRunTarget,
        observed: AgentRunTarget,
    },
    #[error("Product mailbox message `{message_id}` was not found for {target:?}")]
    MessageNotFound {
        target: AgentRunTarget,
        message_id: Uuid,
    },
    #[error(
        "Product mailbox move for message `{message_id}` relative to anchor `{anchor_message_id}` is invalid for {target:?}: {reason:?}"
    )]
    InvalidMove {
        target: AgentRunTarget,
        message_id: Uuid,
        anchor_message_id: Uuid,
        reason: ProductMailboxInvalidMoveReason,
    },
    #[error("Product mailbox receipt is non-terminal for client command `{client_command_id}`")]
    NonTerminalReceipt { client_command_id: String },
    #[error("Product mailbox command storage failed: {message}")]
    Storage { message: String },
}

#[async_trait]
pub trait ProductMailboxCommandRepository: Send + Sync {
    /// Target fences, mutation, head/change advancement, and terminal receipt commit form one UoW.
    async fn execute(
        &self,
        command: ProductMailboxDurableCommand,
    ) -> Result<ProductMailboxCommandOutcome, ProductMailboxCommandRepositoryError>;
}

#[derive(Debug, Error)]
pub enum ProductMailboxError {
    #[error("AgentRun Product binding is missing")]
    TargetNotBound,
    #[error("AgentRun Product binding repository failed: {0}")]
    Binding(String),
    #[error("AgentRun Product binding target mismatch")]
    BindingTargetMismatch,
    #[error("Product mailbox input is invalid: {0}")]
    Invalid(String),
    #[error(transparent)]
    Read(#[from] ProductMailboxReadError),
    #[error(transparent)]
    Command(#[from] ProductMailboxCommandRepositoryError),
    #[error("Product mailbox snapshot digest does not match canonical state")]
    SnapshotDigestMismatch,
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
        let snapshot = self.reads.snapshot(&target).await?;
        validate_snapshot(&target, &snapshot)?;
        Ok(snapshot)
    }

    pub async fn changes(
        &self,
        target: AgentRunTarget,
        after: u64,
        limit: usize,
    ) -> Result<ProductMailboxChangePage, ProductMailboxError> {
        self.require_binding(&target).await?;
        let page = self.reads.changes(&target, after, limit).await?;
        validate_change_page(&target, after, &page)?;
        Ok(page)
    }

    pub async fn content(
        &self,
        target: AgentRunTarget,
        message_id: Uuid,
    ) -> Result<serde_json::Value, ProductMailboxError> {
        self.require_binding(&target).await?;
        self.reads
            .content(&target, message_id)
            .await?
            .ok_or_else(|| ProductMailboxReadError::MessageNotFound { target, message_id }.into())
    }

    pub async fn execute(
        &self,
        request: ProductMailboxCommandRequest,
    ) -> Result<ProductMailboxCommandOutcome, ProductMailboxError> {
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
                serde_json::to_vec(&(
                    "agentdash.product-mailbox-command/v1",
                    &request.target,
                    &request.command,
                ))
                .expect("Product mailbox command is serializable")
            )
        );
        let outcome = self
            .commands
            .execute(ProductMailboxDurableCommand {
                target: request.target.clone(),
                client_command_id: client_command_id.to_owned(),
                request_digest,
                command: request.command,
            })
            .await?;
        if outcome.receipt.target != request.target {
            return Err(ProductMailboxCommandRepositoryError::TargetMismatch {
                expected: request.target,
                observed: outcome.receipt.target,
            }
            .into());
        }
        Ok(outcome)
    }

    async fn require_binding(&self, target: &AgentRunTarget) -> Result<(), ProductMailboxError> {
        let binding = self
            .bindings
            .load_product_binding(target)
            .await
            .map_err(ProductMailboxError::Binding)?
            .ok_or(ProductMailboxError::TargetNotBound)?;
        if binding.target != *target {
            return Err(ProductMailboxError::BindingTargetMismatch);
        }
        Ok(())
    }
}

fn validate_snapshot(
    target: &AgentRunTarget,
    snapshot: &ProductMailboxSnapshot,
) -> Result<(), ProductMailboxError> {
    if snapshot.target != *target {
        return Err(ProductMailboxReadError::TargetMismatch {
            expected: target.clone(),
            observed: snapshot.target.clone(),
        }
        .into());
    }
    for message in &snapshot.messages {
        let observed = AgentRunTarget {
            run_id: message.run_id,
            agent_id: message.agent_id,
        };
        if observed != *target {
            return Err(ProductMailboxReadError::TargetMismatch {
                expected: target.clone(),
                observed,
            }
            .into());
        }
    }
    if let Some(state) = &snapshot.state {
        let observed = AgentRunTarget {
            run_id: state.run_id,
            agent_id: state.agent_id,
        };
        if observed != *target {
            return Err(ProductMailboxReadError::TargetMismatch {
                expected: target.clone(),
                observed,
            }
            .into());
        }
    }
    if canonical_product_mailbox_digest(&snapshot.messages, snapshot.state.as_ref())
        != snapshot.commit.snapshot_digest
    {
        return Err(ProductMailboxError::SnapshotDigestMismatch);
    }
    Ok(())
}

fn validate_change_page(
    target: &AgentRunTarget,
    after: u64,
    page: &ProductMailboxChangePage,
) -> Result<(), ProductMailboxError> {
    if page.target != *target {
        return Err(ProductMailboxReadError::TargetMismatch {
            expected: target.clone(),
            observed: page.target.clone(),
        }
        .into());
    }
    if after > page.head.latest_change_sequence {
        return Err(ProductMailboxReadError::InvalidContinuity {
            message: format!(
                "requested cursor {after} is ahead of Product mailbox head {}",
                page.head.latest_change_sequence
            ),
        }
        .into());
    }
    if let Some(gap) = &page.gap {
        let first_requested =
            after
                .checked_add(1)
                .ok_or_else(|| ProductMailboxReadError::InvalidContinuity {
                    message: "requested cursor overflow".to_owned(),
                })?;
        if gap.requested_after != after
            || gap.earliest_available <= first_requested
            || gap.earliest_available > gap.latest_available
            || !page.changes.is_empty()
            || page.next != gap.latest_available
            || gap.latest_available != page.head.latest_change_sequence
            || gap.snapshot_revision != page.head.revision
            || gap.snapshot_digest != page.head_commit.snapshot_digest
            || gap.detected_at_ms < page.head_commit.committed_at_ms
        {
            return Err(ProductMailboxReadError::InvalidContinuity {
                message: "gap page evidence is inconsistent".to_owned(),
            }
            .into());
        }
        return Ok(());
    }
    if page.changes.is_empty() && after < page.head.latest_change_sequence {
        return Err(ProductMailboxReadError::InvalidContinuity {
            message: "change page omitted retained changes or required gap evidence".to_owned(),
        }
        .into());
    }
    let mut expected =
        after
            .checked_add(1)
            .ok_or_else(|| ProductMailboxReadError::InvalidContinuity {
                message: "requested cursor overflow".to_owned(),
            })?;
    let mut previous_revision = None;
    for change in &page.changes {
        if change.target != *target || change.sequence != expected {
            return Err(ProductMailboxReadError::InvalidContinuity {
                message: format!("expected sequence {expected}, observed {}", change.sequence),
            }
            .into());
        }
        if let Some(previous_revision) = previous_revision
            && change.revision < previous_revision
        {
            return Err(ProductMailboxReadError::RevisionRegression {
                sequence: change.sequence,
                previous_revision,
                observed_revision: change.revision,
            }
            .into());
        }
        if change.revision > page.head.revision {
            return Err(ProductMailboxReadError::InvalidContinuity {
                message: format!(
                    "change revision {} exceeds Product mailbox head revision {}",
                    change.revision, page.head.revision
                ),
            }
            .into());
        }
        previous_revision = Some(change.revision);
        expected =
            expected
                .checked_add(1)
                .ok_or_else(|| ProductMailboxReadError::InvalidContinuity {
                    message: "change cursor overflow".to_owned(),
                })?;
    }
    let expected_next = page
        .changes
        .last()
        .map(|change| change.sequence)
        .unwrap_or(after);
    if page.next != expected_next {
        return Err(ProductMailboxReadError::InvalidContinuity {
            message: "page next cursor does not match the final change".to_owned(),
        }
        .into());
    }
    if page.next > page.head.latest_change_sequence {
        return Err(ProductMailboxReadError::InvalidContinuity {
            message: "page cursor exceeds Product mailbox head".to_owned(),
        }
        .into());
    }
    if page.next == page.head.latest_change_sequence
        && let Some(change) = page.changes.last()
        && (change.revision != page.head.revision || change.commit != page.head_commit)
    {
        return Err(ProductMailboxReadError::InvalidContinuity {
            message: "final change does not match Product mailbox head evidence".to_owned(),
        }
        .into());
    }
    Ok(())
}

pub fn canonical_product_mailbox_digest(
    messages: &[AgentRunMailboxMessage],
    state: Option<&AgentRunMailboxState>,
) -> ProductMailboxSnapshotDigest {
    let mut messages = messages.iter().collect::<Vec<_>>();
    messages.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.order_key.cmp(&right.order_key))
            .then_with(|| left.id.cmp(&right.id))
    });
    let canonical_messages = messages
        .into_iter()
        .map(canonical_mailbox_message)
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
    let canonical = normalize_json(serde_json::json!({
        "schema": "agentdash.product-mailbox-snapshot/v1",
        "messages": canonical_messages,
        "state": canonical_state,
    }));
    let bytes =
        serde_json::to_vec(&canonical).expect("canonical Product mailbox snapshot is serializable");
    ProductMailboxSnapshotDigest::new(format!("sha256:{:x}", Sha256::digest(bytes)))
        .expect("sha256 digest")
}

fn canonical_mailbox_message(message: &AgentRunMailboxMessage) -> serde_json::Value {
    normalize_json(serde_json::json!({
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
    }))
}

fn normalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(normalize_json).collect())
        }
        serde_json::Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, normalize_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        scalar => scalar,
    }
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
    use agentdash_domain::agent_run_mailbox::{
        ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
        MailboxMessageStatus, MailboxSourceIdentity,
    };
    use tokio::sync::Mutex;

    use super::*;
    use crate::agent_run::AgentRunProductRuntimeBinding;

    #[derive(Clone)]
    struct FixtureReceipt {
        request_digest: String,
        receipt: ProductMailboxCommandReceipt,
    }

    #[derive(Clone)]
    struct FixtureMailboxState {
        messages: Vec<AgentRunMailboxMessage>,
        mailbox_state: Option<AgentRunMailboxState>,
        cursor: ProductMailboxCursor,
        head: Option<ProductMailboxCommitEvidence>,
        changes: Vec<ProductMailboxChange>,
        receipts: HashMap<String, FixtureReceipt>,
        clock_ms: i64,
        retention: usize,
    }

    struct FixtureProductMailboxStore {
        state: Mutex<FixtureMailboxState>,
        fail_before_commit_once: AtomicBool,
    }

    impl FixtureProductMailboxStore {
        fn new(target: &AgentRunTarget) -> Self {
            Self {
                state: Mutex::new(FixtureMailboxState {
                    messages: vec![
                        mailbox_message(target, Uuid::from_u128(1), 0, 0, "first"),
                        mailbox_message(target, Uuid::from_u128(2), 0, 1024, "second"),
                        mailbox_message(target, Uuid::from_u128(3), 0, 2048, "third"),
                    ],
                    mailbox_state: Some(AgentRunMailboxState {
                        run_id: target.run_id,
                        agent_id: target.agent_id,
                        paused: true,
                        pause_reason: Some("manual".to_owned()),
                        pause_message: Some("paused".to_owned()),
                        updated_at: "1970-01-01T00:00:01Z".parse().expect("timestamp"),
                    }),
                    cursor: ProductMailboxCursor::default(),
                    head: None,
                    changes: Vec::new(),
                    receipts: HashMap::new(),
                    clock_ms: 1_000,
                    retention: 16,
                }),
                fail_before_commit_once: AtomicBool::new(false),
            }
        }

        async fn external_mutation(&self, target: &AgentRunTarget, preview: &str) {
            let mut state = self.state.lock().await;
            state.clock_ms += 1;
            let message = state
                .messages
                .iter_mut()
                .find(|message| message.run_id == target.run_id)
                .expect("fixture target");
            message.preview = preview.to_owned();
        }

        async fn raw_state(&self) -> FixtureMailboxState {
            self.state.lock().await.clone()
        }
    }

    #[async_trait]
    impl ProductMailboxReadRepository for FixtureProductMailboxStore {
        async fn snapshot(
            &self,
            target: &AgentRunTarget,
        ) -> Result<ProductMailboxSnapshot, ProductMailboxReadError> {
            let mut state = self.state.lock().await;
            reconcile(
                &mut state,
                target,
                ProductMailboxChangeOrigin::CanonicalReconcile,
            )?;
            snapshot_from_state(&state, target)
        }

        async fn changes(
            &self,
            target: &AgentRunTarget,
            after: u64,
            limit: usize,
        ) -> Result<ProductMailboxChangePage, ProductMailboxReadError> {
            let mut state = self.state.lock().await;
            reconcile(
                &mut state,
                target,
                ProductMailboxChangeOrigin::CanonicalReconcile,
            )?;
            let earliest = state
                .changes
                .first()
                .map(|change| change.sequence)
                .unwrap_or(state.cursor.latest_change_sequence.saturating_add(1));
            if after.saturating_add(1) < earliest {
                let head = state.head.as_ref().expect("reconciled head");
                return Ok(ProductMailboxChangePage {
                    target: target.clone(),
                    changes: Vec::new(),
                    next: state.cursor.latest_change_sequence,
                    head: state.cursor,
                    head_commit: head.clone(),
                    gap: Some(ProductMailboxChangeGap {
                        requested_after: after,
                        earliest_available: earliest,
                        latest_available: state.cursor.latest_change_sequence,
                        snapshot_revision: state.cursor.revision,
                        snapshot_digest: head.snapshot_digest.clone(),
                        detected_at_ms: committed_at(state.clock_ms),
                    }),
                });
            }
            let changes = state
                .changes
                .iter()
                .filter(|change| change.sequence > after)
                .take(limit)
                .cloned()
                .collect::<Vec<_>>();
            Ok(ProductMailboxChangePage {
                target: target.clone(),
                next: changes
                    .last()
                    .map(|change| change.sequence)
                    .unwrap_or(after),
                changes,
                head: state.cursor,
                head_commit: state.head.clone().expect("reconciled head"),
                gap: None,
            })
        }

        async fn content(
            &self,
            target: &AgentRunTarget,
            message_id: Uuid,
        ) -> Result<Option<serde_json::Value>, ProductMailboxReadError> {
            let state = self.state.lock().await;
            let message = state
                .messages
                .iter()
                .find(|message| message.id == message_id);
            match message {
                Some(message)
                    if message.run_id == target.run_id && message.agent_id == target.agent_id =>
                {
                    Ok(message.payload_json.clone())
                }
                Some(message) => Err(ProductMailboxReadError::TargetMismatch {
                    expected: target.clone(),
                    observed: AgentRunTarget {
                        run_id: message.run_id,
                        agent_id: message.agent_id,
                    },
                }),
                None => Ok(None),
            }
        }
    }

    #[async_trait]
    impl ProductMailboxCommandRepository for FixtureProductMailboxStore {
        async fn execute(
            &self,
            command: ProductMailboxDurableCommand,
        ) -> Result<ProductMailboxCommandOutcome, ProductMailboxCommandRepositoryError> {
            let mut locked = self.state.lock().await;
            let key = format!(
                "{}:{}:{}",
                command.target.run_id, command.target.agent_id, command.client_command_id
            );
            if let Some(stored) = locked.receipts.get(&key) {
                if stored.request_digest != command.request_digest {
                    return Err(
                        ProductMailboxCommandRepositoryError::RequestDigestConflict {
                            target: command.target,
                            client_command_id: command.client_command_id,
                        },
                    );
                }
                return Ok(ProductMailboxCommandOutcome {
                    receipt: stored.receipt.clone(),
                    replayed: true,
                });
            }

            let mut working = locked.clone();
            validate_command_targets(&working, &command)?;
            working.clock_ms += 1;
            apply_command(&mut working, &command)?;
            reconcile(
                &mut working,
                &command.target,
                ProductMailboxChangeOrigin::Command {
                    client_command_id: command.client_command_id.clone(),
                    command_kind: command.command.kind(),
                },
            )
            .map_err(read_to_command_error)?;
            let head = working.head.clone().expect("command reconciled head");
            let receipt = ProductMailboxCommandReceipt {
                target: command.target.clone(),
                client_command_id: command.client_command_id.clone(),
                revision: working.cursor.revision,
                latest_change_sequence: working.cursor.latest_change_sequence,
                commit: head,
            };
            working.receipts.insert(
                key,
                FixtureReceipt {
                    request_digest: command.request_digest,
                    receipt: receipt.clone(),
                },
            );
            if self.fail_before_commit_once.swap(false, Ordering::SeqCst) {
                return Err(ProductMailboxCommandRepositoryError::Storage {
                    message: "injected crash before commit".to_owned(),
                });
            }
            *locked = working;
            Ok(ProductMailboxCommandOutcome {
                receipt,
                replayed: false,
            })
        }
    }

    fn validate_command_targets(
        state: &FixtureMailboxState,
        command: &ProductMailboxDurableCommand,
    ) -> Result<(), ProductMailboxCommandRepositoryError> {
        let mut ids = Vec::new();
        match command.command {
            ProductMailboxCommand::Promote { message_id }
            | ProductMailboxCommand::Delete { message_id } => ids.push(message_id),
            ProductMailboxCommand::Move {
                message_id,
                after_message_id,
            } => {
                ids.push(message_id);
                if let Some(after_message_id) = after_message_id {
                    ids.push(after_message_id);
                }
            }
            ProductMailboxCommand::Resume => {
                if let Some(mailbox_state) = &state.mailbox_state {
                    let observed = AgentRunTarget {
                        run_id: mailbox_state.run_id,
                        agent_id: mailbox_state.agent_id,
                    };
                    if observed != command.target {
                        return Err(ProductMailboxCommandRepositoryError::TargetMismatch {
                            expected: command.target.clone(),
                            observed,
                        });
                    }
                }
            }
        }
        for message_id in ids {
            let message = state
                .messages
                .iter()
                .find(|message| message.id == message_id)
                .ok_or_else(|| ProductMailboxCommandRepositoryError::MessageNotFound {
                    target: command.target.clone(),
                    message_id,
                })?;
            let observed = AgentRunTarget {
                run_id: message.run_id,
                agent_id: message.agent_id,
            };
            if observed != command.target {
                return Err(ProductMailboxCommandRepositoryError::TargetMismatch {
                    expected: command.target.clone(),
                    observed,
                });
            }
        }
        if let ProductMailboxCommand::Move {
            message_id,
            after_message_id: Some(anchor_message_id),
        } = command.command
        {
            if message_id == anchor_message_id {
                return Err(ProductMailboxCommandRepositoryError::InvalidMove {
                    target: command.target.clone(),
                    message_id,
                    anchor_message_id,
                    reason: ProductMailboxInvalidMoveReason::SelfAnchor,
                });
            }
            let message = state
                .messages
                .iter()
                .find(|message| message.id == message_id)
                .expect("target validated");
            let anchor = state
                .messages
                .iter()
                .find(|message| message.id == anchor_message_id)
                .expect("anchor validated");
            if message.priority != anchor.priority {
                return Err(ProductMailboxCommandRepositoryError::InvalidMove {
                    target: command.target.clone(),
                    message_id,
                    anchor_message_id,
                    reason: ProductMailboxInvalidMoveReason::CrossPriorityLane,
                });
            }
        }
        Ok(())
    }

    fn apply_command(
        state: &mut FixtureMailboxState,
        command: &ProductMailboxDurableCommand,
    ) -> Result<(), ProductMailboxCommandRepositoryError> {
        match command.command {
            ProductMailboxCommand::Promote { message_id } => {
                let message = state
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .expect("target validated");
                message.delivery = MailboxDelivery::SteerActiveTurn {
                    stop_effect: agentdash_domain::agent_run_mailbox::SteeringStopEffect::None,
                };
                message.barrier = ConsumptionBarrier::AgentLoopTurnBoundary;
                message.drain_mode = MailboxDrainMode::All;
                message.priority = 100;
            }
            ProductMailboxCommand::Delete { message_id } => {
                let message = state
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .expect("target validated");
                message.status = MailboxMessageStatus::Deleted;
                message.payload_json = None;
                message.deleted_at = Some("1970-01-01T00:00:02Z".parse().expect("timestamp"));
            }
            ProductMailboxCommand::Move {
                message_id,
                after_message_id,
            } => {
                let from = state
                    .messages
                    .iter()
                    .position(|message| message.id == message_id)
                    .expect("target validated");
                let moved = state.messages.remove(from);
                let destination = after_message_id
                    .map(|anchor| {
                        state
                            .messages
                            .iter()
                            .position(|message| message.id == anchor)
                            .expect("anchor validated")
                            + 1
                    })
                    .unwrap_or(0);
                state.messages.insert(destination, moved);
                for (index, message) in state.messages.iter_mut().enumerate() {
                    message.order_key = i64::try_from(index).expect("fixture order") * 1024;
                }
            }
            ProductMailboxCommand::Resume => {
                let mailbox_state = state.mailbox_state.as_mut().ok_or_else(|| {
                    ProductMailboxCommandRepositoryError::Storage {
                        message: "fixture mailbox state missing".to_owned(),
                    }
                })?;
                mailbox_state.paused = false;
                mailbox_state.pause_reason = None;
                mailbox_state.pause_message = None;
            }
        }
        Ok(())
    }

    fn reconcile(
        state: &mut FixtureMailboxState,
        target: &AgentRunTarget,
        origin: ProductMailboxChangeOrigin,
    ) -> Result<(), ProductMailboxReadError> {
        for message in &state.messages {
            let observed = AgentRunTarget {
                run_id: message.run_id,
                agent_id: message.agent_id,
            };
            if observed != *target {
                return Err(ProductMailboxReadError::TargetMismatch {
                    expected: target.clone(),
                    observed,
                });
            }
        }
        if let Some(mailbox_state) = &state.mailbox_state {
            let observed = AgentRunTarget {
                run_id: mailbox_state.run_id,
                agent_id: mailbox_state.agent_id,
            };
            if observed != *target {
                return Err(ProductMailboxReadError::TargetMismatch {
                    expected: target.clone(),
                    observed,
                });
            }
        }
        let digest =
            canonical_product_mailbox_digest(&state.messages, state.mailbox_state.as_ref());
        if state
            .head
            .as_ref()
            .is_some_and(|head| head.snapshot_digest == digest)
        {
            return Ok(());
        }
        state.cursor.revision = state.cursor.revision.checked_add(1).ok_or_else(|| {
            ProductMailboxReadError::Storage {
                message: "revision overflow".to_owned(),
            }
        })?;
        state.cursor.latest_change_sequence = state
            .cursor
            .latest_change_sequence
            .checked_add(1)
            .ok_or_else(|| ProductMailboxReadError::Storage {
                message: "sequence overflow".to_owned(),
            })?;
        let commit = ProductMailboxCommitEvidence {
            snapshot_digest: digest,
            committed_at_ms: committed_at(state.clock_ms),
        };
        state.changes.push(ProductMailboxChange {
            change_id: Uuid::new_v4(),
            target: target.clone(),
            sequence: state.cursor.latest_change_sequence,
            revision: state.cursor.revision,
            origin,
            commit: commit.clone(),
        });
        state.head = Some(commit);
        if state.changes.len() > state.retention {
            let remove = state.changes.len() - state.retention;
            state.changes.drain(0..remove);
        }
        Ok(())
    }

    fn snapshot_from_state(
        state: &FixtureMailboxState,
        target: &AgentRunTarget,
    ) -> Result<ProductMailboxSnapshot, ProductMailboxReadError> {
        let mut messages = state.messages.clone();
        messages.sort_by(product_mailbox_order);
        Ok(ProductMailboxSnapshot {
            target: target.clone(),
            cursor: state.cursor,
            commit: state
                .head
                .clone()
                .ok_or_else(|| ProductMailboxReadError::Storage {
                    message: "head missing after reconcile".to_owned(),
                })?,
            messages,
            state: state.mailbox_state.clone(),
        })
    }

    fn read_to_command_error(
        error: ProductMailboxReadError,
    ) -> ProductMailboxCommandRepositoryError {
        match error {
            ProductMailboxReadError::TargetMismatch { expected, observed } => {
                ProductMailboxCommandRepositoryError::TargetMismatch { expected, observed }
            }
            ProductMailboxReadError::MessageNotFound { target, message_id } => {
                ProductMailboxCommandRepositoryError::MessageNotFound { target, message_id }
            }
            ProductMailboxReadError::InvalidContinuity { message }
            | ProductMailboxReadError::Storage { message } => {
                ProductMailboxCommandRepositoryError::Storage { message }
            }
            ProductMailboxReadError::RevisionRegression {
                sequence,
                previous_revision,
                observed_revision,
            } => ProductMailboxCommandRepositoryError::Storage {
                message: format!(
                    "change revision regressed at sequence {sequence}: previous {previous_revision}, observed {observed_revision}"
                ),
            },
        }
    }

    struct FixtureBindingRepository {
        binding: AgentRunProductRuntimeBinding,
    }

    #[async_trait]
    impl AgentRunProductRuntimeBindingRepository for FixtureBindingRepository {
        async fn load_product_binding(
            &self,
            _target: &AgentRunTarget,
        ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
            Ok(Some(self.binding.clone()))
        }
    }

    fn fixture() -> (
        AgentRunTarget,
        Arc<FixtureProductMailboxStore>,
        ProductMailboxFacade,
    ) {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(100),
            agent_id: Uuid::from_u128(101),
        };
        let binding = AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: RuntimeThreadId::new("mailbox-thread").expect("thread"),
            launch_frame: crate::agent_run::ProductAgentFrameRef {
                frame_id: Uuid::new_v4(),
                agent_id: target.agent_id,
                revision: 1,
            },
            execution_profile_digest: "sha256:mailbox-profile".to_owned(),
            source_binding: ManagedRuntimeSourceBindingEvidence {
                source_ref: RuntimeSourceRef::new("source:mailbox").expect("source"),
                committed_at_revision: RuntimeProjectionRevision(1),
                applied_surface_revision: SurfaceRevision(1),
                activated_at_revision: Some(RuntimeProjectionRevision(1)),
            },
        };
        let store = Arc::new(FixtureProductMailboxStore::new(&target));
        let facade = ProductMailboxFacade::new(
            Arc::new(FixtureBindingRepository { binding }),
            store.clone(),
            store.clone(),
        );
        (target, store, facade)
    }

    fn request(
        target: &AgentRunTarget,
        client_command_id: &str,
        command: ProductMailboxCommand,
    ) -> ProductMailboxCommandRequest {
        ProductMailboxCommandRequest {
            target: target.clone(),
            client_command_id: client_command_id.to_owned(),
            command,
        }
    }

    fn mailbox_message(
        target: &AgentRunTarget,
        id: Uuid,
        priority: i32,
        order_key: i64,
        preview: &str,
    ) -> AgentRunMailboxMessage {
        AgentRunMailboxMessage {
            id,
            run_id: target.run_id,
            agent_id: target.agent_id,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::AgentRunTurnBoundary,
            drain_mode: MailboxDrainMode::One,
            status: MailboxMessageStatus::Queued,
            priority,
            order_key,
            source_dedup_key: Some(format!("message:{id}")),
            delivery_request_digest: format!("sha256:{}", id.simple()),
            accepted_runtime_operation_id: None,
            reconcile_required: false,
            claim_token: None,
            claimed_at: None,
            claim_expires_at: None,
            payload_json: Some(serde_json::json!([{ "type": "text", "text": preview }])),
            launch_planning_input: None,
            preview: preview.to_owned(),
            has_images: false,
            retain_payload: true,
            attempt_count: 0,
            last_error: None,
            created_at: "1970-01-01T00:00:01Z".parse().expect("timestamp"),
            updated_at: "1970-01-01T00:00:01Z".parse().expect("timestamp"),
            consumed_at: None,
            deleted_at: None,
        }
    }

    fn committed_at(ms: i64) -> ProductMailboxCommittedAtMs {
        ProductMailboxCommittedAtMs(u64::try_from(ms).expect("non-negative fixture clock"))
    }

    fn commit_evidence(hex: char, ms: i64) -> ProductMailboxCommitEvidence {
        ProductMailboxCommitEvidence {
            snapshot_digest: ProductMailboxSnapshotDigest::new(format!(
                "sha256:{}",
                hex.to_string().repeat(64)
            ))
            .expect("fixture digest"),
            committed_at_ms: committed_at(ms),
        }
    }

    fn change(
        target: &AgentRunTarget,
        sequence: u64,
        revision: u64,
        commit: ProductMailboxCommitEvidence,
    ) -> ProductMailboxChange {
        ProductMailboxChange {
            change_id: Uuid::from_u128(u128::from(sequence)),
            target: target.clone(),
            sequence,
            revision,
            origin: ProductMailboxChangeOrigin::CanonicalReconcile,
            commit,
        }
    }

    fn product_mailbox_order(
        left: &AgentRunMailboxMessage,
        right: &AgentRunMailboxMessage,
    ) -> std::cmp::Ordering {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.order_key.cmp(&right.order_key))
            .then_with(|| left.id.cmp(&right.id))
    }

    #[tokio::test]
    async fn promote_delete_move_and_resume_commit_mutation_receipt_head_and_change_together() {
        let (target, store, facade) = fixture();
        let initial = facade.snapshot(target.clone()).await.expect("initial");
        let initial_sequence = initial.cursor.latest_change_sequence;

        let promote = facade
            .execute(request(
                &target,
                "promote",
                ProductMailboxCommand::Promote {
                    message_id: Uuid::from_u128(2),
                },
            ))
            .await
            .expect("promote");
        let promoted = facade.snapshot(target.clone()).await.expect("promoted");
        assert_eq!(promoted.messages[0].id, Uuid::from_u128(2));
        assert!(matches!(
            promoted.messages[0].delivery,
            MailboxDelivery::SteerActiveTurn { .. }
        ));
        assert_eq!(
            promoted.messages[0].barrier,
            ConsumptionBarrier::AgentLoopTurnBoundary
        );
        assert_eq!(promoted.messages[0].drain_mode, MailboxDrainMode::All);
        assert_eq!(
            promote.receipt.commit.snapshot_digest,
            promoted.commit.snapshot_digest
        );

        let moved = facade
            .execute(request(
                &target,
                "move",
                ProductMailboxCommand::Move {
                    message_id: Uuid::from_u128(3),
                    after_message_id: None,
                },
            ))
            .await
            .expect("move");
        let after_move = facade.snapshot(target.clone()).await.expect("after move");
        assert_eq!(after_move.messages[0].id, Uuid::from_u128(2));
        assert_eq!(
            after_move
                .messages
                .iter()
                .filter(|message| message.priority == 0)
                .min_by_key(|message| message.order_key)
                .expect("moved message")
                .id,
            Uuid::from_u128(3)
        );
        assert_eq!(
            moved.receipt.commit.snapshot_digest,
            after_move.commit.snapshot_digest
        );

        let deleted = facade
            .execute(request(
                &target,
                "delete",
                ProductMailboxCommand::Delete {
                    message_id: Uuid::from_u128(1),
                },
            ))
            .await
            .expect("delete");
        let after_delete = facade.snapshot(target.clone()).await.expect("after delete");
        let deleted_message = after_delete
            .messages
            .iter()
            .find(|message| message.id == Uuid::from_u128(1))
            .expect("deleted row");
        assert_eq!(deleted_message.status, MailboxMessageStatus::Deleted);
        assert!(deleted_message.payload_json.is_none());
        assert_eq!(
            deleted.receipt.commit.snapshot_digest,
            after_delete.commit.snapshot_digest
        );

        let resumed = facade
            .execute(request(&target, "resume", ProductMailboxCommand::Resume))
            .await
            .expect("resume");
        let after_resume = facade.snapshot(target.clone()).await.expect("after resume");
        assert!(!after_resume.state.as_ref().expect("state").paused);
        assert_eq!(
            resumed.receipt.commit.snapshot_digest,
            after_resume.commit.snapshot_digest
        );

        let raw = store.raw_state().await;
        assert_eq!(raw.receipts.len(), 4);
        assert_eq!(raw.cursor.latest_change_sequence, initial_sequence + 4);
        for receipt in raw.receipts.values() {
            let change = raw
                .changes
                .iter()
                .find(|change| change.sequence == receipt.receipt.latest_change_sequence)
                .expect("receipt change retained");
            assert_eq!(
                receipt.receipt.commit.snapshot_digest,
                change.commit.snapshot_digest
            );
        }
    }

    #[tokio::test]
    async fn crash_before_commit_rolls_back_mutation_head_change_and_receipt() {
        let (target, store, facade) = fixture();
        let before = facade.snapshot(target.clone()).await.expect("before");
        store.fail_before_commit_once.store(true, Ordering::SeqCst);
        let command = request(
            &target,
            "crash",
            ProductMailboxCommand::Delete {
                message_id: Uuid::from_u128(1),
            },
        );
        assert!(matches!(
            facade.execute(command.clone()).await,
            Err(ProductMailboxError::Command(
                ProductMailboxCommandRepositoryError::Storage { .. }
            ))
        ));
        let after_crash = facade.snapshot(target.clone()).await.expect("after crash");
        assert_eq!(after_crash.cursor, before.cursor);
        assert_eq!(after_crash.commit, before.commit);
        assert_eq!(
            after_crash
                .messages
                .iter()
                .find(|message| message.id == Uuid::from_u128(1))
                .expect("message")
                .status,
            MailboxMessageStatus::Queued
        );
        assert!(store.raw_state().await.receipts.is_empty());

        let accepted = facade.execute(command.clone()).await.expect("retry");
        let replay = facade.execute(command).await.expect("replay");
        assert!(!accepted.replayed);
        assert!(replay.replayed);
        assert_eq!(accepted.receipt, replay.receipt);
    }

    #[tokio::test]
    async fn same_client_different_payload_conflicts_without_mutation() {
        let (target, store, facade) = fixture();
        facade
            .execute(request(
                &target,
                "same-client",
                ProductMailboxCommand::Promote {
                    message_id: Uuid::from_u128(1),
                },
            ))
            .await
            .expect("first");
        let before = store.raw_state().await;
        let error = facade
            .execute(request(
                &target,
                "same-client",
                ProductMailboxCommand::Delete {
                    message_id: Uuid::from_u128(1),
                },
            ))
            .await
            .expect_err("conflict");
        assert!(matches!(
            error,
            ProductMailboxError::Command(
                ProductMailboxCommandRepositoryError::RequestDigestConflict { .. }
            )
        ));
        let after = store.raw_state().await;
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.messages, before.messages);
        assert_eq!(after.receipts.len(), before.receipts.len());
    }

    #[tokio::test]
    async fn same_lane_move_replays_exact_receipt_and_preserves_digest_conflict() {
        let (target, store, facade) = fixture();
        let command = request(
            &target,
            "same-lane-move",
            ProductMailboxCommand::Move {
                message_id: Uuid::from_u128(1),
                after_message_id: Some(Uuid::from_u128(2)),
            },
        );
        let accepted = facade
            .execute(command.clone())
            .await
            .expect("same-lane move");
        let moved = facade.snapshot(target.clone()).await.expect("moved");
        assert_eq!(
            moved
                .messages
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![Uuid::from_u128(2), Uuid::from_u128(1), Uuid::from_u128(3)]
        );

        let replay = facade.execute(command).await.expect("exact replay");
        assert!(!accepted.replayed);
        assert!(replay.replayed);
        assert_eq!(accepted.receipt, replay.receipt);
        let before_conflict = store.raw_state().await;
        let conflict = facade
            .execute(request(
                &target,
                "same-lane-move",
                ProductMailboxCommand::Move {
                    message_id: Uuid::from_u128(1),
                    after_message_id: Some(Uuid::from_u128(3)),
                },
            ))
            .await
            .expect_err("different anchor conflicts");
        assert!(matches!(
            conflict,
            ProductMailboxError::Command(
                ProductMailboxCommandRepositoryError::RequestDigestConflict { .. }
            )
        ));
        let after_conflict = store.raw_state().await;
        assert_eq!(after_conflict.messages, before_conflict.messages);
        assert_eq!(after_conflict.cursor, before_conflict.cursor);
        assert_eq!(after_conflict.head, before_conflict.head);
        assert_eq!(after_conflict.changes, before_conflict.changes);
        assert_eq!(
            after_conflict.receipts.len(),
            before_conflict.receipts.len()
        );
    }

    #[tokio::test]
    async fn cross_lane_move_is_typed_and_leaves_all_durable_facts_unchanged() {
        let (target, store, facade) = fixture();
        {
            let mut state = store.state.lock().await;
            state.messages[1].priority = 100;
        }
        let before = store.raw_state().await;
        let error = facade
            .execute(request(
                &target,
                "cross-lane-move",
                ProductMailboxCommand::Move {
                    message_id: Uuid::from_u128(1),
                    after_message_id: Some(Uuid::from_u128(2)),
                },
            ))
            .await
            .expect_err("cross-priority lane");
        assert!(matches!(
            error,
            ProductMailboxError::Command(
                ProductMailboxCommandRepositoryError::InvalidMove {
                    target: ref error_target,
                    message_id,
                    anchor_message_id,
                    reason: ProductMailboxInvalidMoveReason::CrossPriorityLane,
                }
            ) if error_target == &target
                && message_id == Uuid::from_u128(1)
                && anchor_message_id == Uuid::from_u128(2)
        ));
        let after = store.raw_state().await;
        assert_eq!(after.messages, before.messages);
        assert_eq!(after.mailbox_state, before.mailbox_state);
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.head, before.head);
        assert_eq!(after.changes, before.changes);
        assert_eq!(after.receipts.len(), before.receipts.len());
        assert_eq!(after.clock_ms, before.clock_ms);
    }

    #[tokio::test]
    async fn self_anchor_move_is_typed_before_mutation() {
        let (target, store, facade) = fixture();
        let before = store.raw_state().await;
        let error = facade
            .execute(request(
                &target,
                "self-anchor-move",
                ProductMailboxCommand::Move {
                    message_id: Uuid::from_u128(1),
                    after_message_id: Some(Uuid::from_u128(1)),
                },
            ))
            .await
            .expect_err("self anchor");
        assert!(matches!(
            error,
            ProductMailboxError::Command(ProductMailboxCommandRepositoryError::InvalidMove {
                reason: ProductMailboxInvalidMoveReason::SelfAnchor,
                ..
            })
        ));
        let after = store.raw_state().await;
        assert_eq!(after.messages, before.messages);
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.head, before.head);
        assert_eq!(after.changes, before.changes);
        assert!(after.receipts.is_empty());
    }

    #[tokio::test]
    async fn cross_target_delete_and_move_anchor_are_rejected_before_any_mutation() {
        let (target, store, facade) = fixture();
        let foreign = AgentRunTarget {
            run_id: Uuid::from_u128(200),
            agent_id: Uuid::from_u128(201),
        };
        {
            let mut state = store.state.lock().await;
            state.messages.push(mailbox_message(
                &foreign,
                Uuid::from_u128(99),
                0,
                4096,
                "foreign",
            ));
        }
        let before = store.raw_state().await;
        for command in [
            ProductMailboxCommand::Delete {
                message_id: Uuid::from_u128(99),
            },
            ProductMailboxCommand::Move {
                message_id: Uuid::from_u128(1),
                after_message_id: Some(Uuid::from_u128(99)),
            },
        ] {
            let error = facade
                .execute(request(&target, &format!("{command:?}"), command))
                .await
                .expect_err("target mismatch");
            assert!(matches!(
                error,
                ProductMailboxError::Command(
                    ProductMailboxCommandRepositoryError::TargetMismatch { .. }
                )
            ));
        }
        let after = store.raw_state().await;
        assert_eq!(after.messages, before.messages);
        assert_eq!(after.cursor, before.cursor);
        assert!(after.receipts.is_empty());
    }

    #[tokio::test]
    async fn external_canonical_mutation_reconciles_one_real_snapshot_change() {
        let (target, store, facade) = fixture();
        let initial = facade.snapshot(target.clone()).await.expect("initial");
        store.external_mutation(&target, "external").await;
        let reconciled = facade.snapshot(target.clone()).await.expect("reconciled");
        assert_eq!(reconciled.cursor.revision, initial.cursor.revision + 1);
        assert_eq!(
            reconciled.cursor.latest_change_sequence,
            initial.cursor.latest_change_sequence + 1
        );
        assert_eq!(reconciled.messages[0].preview, "external");
        let page = facade
            .changes(target, initial.cursor.latest_change_sequence, 256)
            .await
            .expect("changes");
        assert_eq!(page.changes.len(), 1);
        assert_eq!(
            page.changes[0].commit.snapshot_digest,
            reconciled.commit.snapshot_digest
        );
        assert!(matches!(
            page.changes[0].origin,
            ProductMailboxChangeOrigin::CanonicalReconcile
        ));
    }

    #[tokio::test]
    async fn strict_sequence_paging_and_retention_gap_are_typed() {
        let (target, store, facade) = fixture();
        store.state.lock().await.retention = 3;
        let initial = facade.snapshot(target.clone()).await.expect("initial");
        for index in 0..5 {
            store
                .external_mutation(&target, &format!("external-{index}"))
                .await;
            facade.snapshot(target.clone()).await.expect("reconcile");
        }
        let gap = facade
            .changes(target.clone(), initial.cursor.latest_change_sequence, 2)
            .await
            .expect("gap");
        let evidence = gap.gap.expect("retention gap");
        assert_eq!(evidence.latest_available, gap.next);
        assert!(gap.changes.is_empty());

        let page = facade
            .changes(target, evidence.earliest_available - 1, 2)
            .await
            .expect("page");
        assert!(page.gap.is_none());
        assert_eq!(page.changes.len(), 2);
        assert_eq!(page.changes[1].sequence, page.changes[0].sequence + 1);
        assert_eq!(page.next, page.changes[1].sequence);
    }

    #[test]
    fn validates_normal_page_revision_and_head_evidence() {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(400),
            agent_id: Uuid::from_u128(401),
        };
        let first_commit = commit_evidence('a', 10);
        let head_commit = commit_evidence('b', 11);
        let page = ProductMailboxChangePage {
            target: target.clone(),
            changes: vec![
                change(&target, 5, 7, first_commit),
                change(&target, 6, 8, head_commit.clone()),
            ],
            next: 6,
            head: ProductMailboxCursor {
                revision: 8,
                latest_change_sequence: 6,
            },
            head_commit,
            gap: None,
        };

        validate_change_page(&target, 4, &page).expect("valid page");
    }

    #[test]
    fn rejects_revision_regression_with_typed_error() {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(410),
            agent_id: Uuid::from_u128(411),
        };
        let head_commit = commit_evidence('c', 12);
        let page = ProductMailboxChangePage {
            target: target.clone(),
            changes: vec![
                change(&target, 5, 8, commit_evidence('b', 11)),
                change(&target, 6, 7, head_commit.clone()),
            ],
            next: 6,
            head: ProductMailboxCursor {
                revision: 8,
                latest_change_sequence: 6,
            },
            head_commit,
            gap: None,
        };

        assert!(matches!(
            validate_change_page(&target, 4, &page),
            Err(ProductMailboxError::Read(
                ProductMailboxReadError::RevisionRegression {
                    sequence: 6,
                    previous_revision: 8,
                    observed_revision: 7,
                }
            ))
        ));
    }

    #[test]
    fn rejects_pseudo_gap_and_gap_that_disagrees_with_head() {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(420),
            agent_id: Uuid::from_u128(421),
        };
        let head_commit = commit_evidence('d', 20);
        let head = ProductMailboxCursor {
            revision: 20,
            latest_change_sequence: 10,
        };
        let pseudo_gap = ProductMailboxChangePage {
            target: target.clone(),
            changes: Vec::new(),
            next: 10,
            head,
            head_commit: head_commit.clone(),
            gap: Some(ProductMailboxChangeGap {
                requested_after: 8,
                earliest_available: 9,
                latest_available: 10,
                snapshot_revision: 20,
                snapshot_digest: head_commit.snapshot_digest.clone(),
                detected_at_ms: committed_at(20),
            }),
        };
        assert!(matches!(
            validate_change_page(&target, 8, &pseudo_gap),
            Err(ProductMailboxError::Read(
                ProductMailboxReadError::InvalidContinuity { .. }
            ))
        ));

        let mismatched_head = ProductMailboxChangePage {
            gap: Some(ProductMailboxChangeGap {
                requested_after: 7,
                earliest_available: 9,
                latest_available: 10,
                snapshot_revision: 19,
                snapshot_digest: head_commit.snapshot_digest.clone(),
                detected_at_ms: committed_at(20),
            }),
            ..pseudo_gap
        };
        assert!(matches!(
            validate_change_page(&target, 7, &mismatched_head),
            Err(ProductMailboxError::Read(
                ProductMailboxReadError::InvalidContinuity { .. }
            ))
        ));
    }

    #[test]
    fn rejects_page_that_omits_required_gap_evidence() {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(430),
            agent_id: Uuid::from_u128(431),
        };
        let page = ProductMailboxChangePage {
            target: target.clone(),
            changes: Vec::new(),
            next: 5,
            head: ProductMailboxCursor {
                revision: 10,
                latest_change_sequence: 10,
            },
            head_commit: commit_evidence('e', 30),
            gap: None,
        };

        assert!(matches!(
            validate_change_page(&target, 5, &page),
            Err(ProductMailboxError::Read(
                ProductMailboxReadError::InvalidContinuity { .. }
            ))
        ));
    }

    #[test]
    fn canonical_digest_sorts_by_product_order_and_stable_id_and_normalizes_json_keys() {
        let target = AgentRunTarget {
            run_id: Uuid::from_u128(300),
            agent_id: Uuid::from_u128(301),
        };
        let mut first = mailbox_message(&target, Uuid::from_u128(2), 0, 0, "same");
        first.source.metadata = Some(serde_json::json!({ "z": 1, "a": { "y": 2, "b": 3 } }));
        let mut second = mailbox_message(&target, Uuid::from_u128(1), 0, 0, "same");
        second.source.metadata = Some(serde_json::json!({ "a": { "b": 3, "y": 2 }, "z": 1 }));
        let left = canonical_product_mailbox_digest(&[first.clone(), second.clone()], None);
        let right = canonical_product_mailbox_digest(&[second, first], None);
        assert_eq!(left, right);
        assert!(left.as_str().starts_with("sha256:"));
    }

    #[tokio::test]
    async fn facade_rejects_mixed_target_snapshot_even_with_repository_cursor() {
        let (target, store, facade) = fixture();
        facade.snapshot(target.clone()).await.expect("initial");
        {
            let mut state = store.state.lock().await;
            state.messages[0].run_id = Uuid::from_u128(999);
        }
        let error = facade.snapshot(target).await.expect_err("mixed target");
        assert!(matches!(
            error,
            ProductMailboxError::Read(ProductMailboxReadError::TargetMismatch { .. })
        ));
    }
}
