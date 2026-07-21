use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{DashExecutionFailure, DashToolDefinition};
use thiserror::Error;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

string_id!(AgentSessionId);
string_id!(BranchId);
string_id!(HistoryEntryId);
string_id!(AgentTurnId);
string_id!(AgentItemId);
string_id!(InteractionId);
string_id!(CompactionId);
string_id!(ContextRevision);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextMode {
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeliveryFidelity {
    TypedNative,
    CanonicalRendered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialContextContribution {
    pub kind: String,
    pub payload: String,
    pub authority: String,
    pub source_revision: String,
    pub digest: String,
}

impl InitialContextContribution {
    /// Renders the contribution exactly where Dash materializes its provider system prompt.
    ///
    /// The native history retains the typed kind and original payload so adapters can also
    /// project the same accepted contribution into platform ContextFrame presentation.
    pub fn render_for_prompt(&self) -> String {
        let title = match self.kind.as_str() {
            "compact_summary" => "Compaction Summary",
            "workflow_context" => "Workflow Context",
            "constraint_set" => "Constraint Set",
            _ => self.kind.as_str(),
        };
        format!("## AgentDash Initial Context: {title}\n{}", self.payload)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialContextInstallation {
    pub package_id: String,
    pub package_digest: String,
    pub mode: InitialContextMode,
    pub fidelity: ContextDeliveryFidelity,
    pub contributions: Vec<InitialContextContribution>,
}

impl InitialContextInstallation {
    pub fn render_for_prompt(&self) -> String {
        self.contributions
            .iter()
            .map(InitialContextContribution::render_for_prompt)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// The exact prompt/tool surface materialized by the Dash agent.
///
/// This belongs to the concrete agent source: Product surface contributions are only input
/// intent, while this value records what Dash actually accepted for provider execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashSurface {
    pub revision: u64,
    pub digest: String,
    pub system_prompt: String,
    pub tools: Vec<DashToolDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    AssistantMessage,
    ToolCall,
    ToolResult,
    Interaction,
    ContextCompaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionMode {
    Manual,
    AutomaticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryPayload {
    InitialContextInstalled {
        installation: InitialContextInstallation,
    },
    SurfaceApplied {
        surface: DashSurface,
    },
    SurfaceRevoked {
        surface: DashSurface,
    },
    ThreadNameChanged {
        thread_name: String,
    },
    InputAccepted {
        input_id: String,
        content: String,
    },
    TurnStarted {
        turn_id: AgentTurnId,
    },
    ItemStarted {
        turn_id: AgentTurnId,
        item_id: AgentItemId,
        kind: ItemKind,
    },
    ItemCompleted {
        turn_id: AgentTurnId,
        item_id: AgentItemId,
    },
    AgentOutput {
        turn_id: AgentTurnId,
        item_id: Option<AgentItemId>,
        content: String,
    },
    ToolCall {
        turn_id: AgentTurnId,
        item_id: AgentItemId,
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        turn_id: AgentTurnId,
        item_id: AgentItemId,
        content: String,
        is_error: bool,
    },
    InteractionRequested {
        turn_id: AgentTurnId,
        item_id: Option<AgentItemId>,
        interaction_id: InteractionId,
        prompt: String,
    },
    InteractionResolved {
        interaction_id: InteractionId,
        response: String,
    },
    CompactionStarted {
        compaction_id: CompactionId,
        mode: CompactionMode,
        source_head: Option<HistoryEntryId>,
        source_digest: String,
    },
    CompactionApplied {
        compaction_id: CompactionId,
        revision: ContextRevision,
        summary: String,
        retained_from: Option<HistoryEntryId>,
        source_digest: String,
    },
    CompactionCompleted {
        compaction_id: CompactionId,
    },
    CompactionFailed {
        compaction_id: CompactionId,
        error: String,
        lost: bool,
    },
    TurnCompleted {
        turn_id: AgentTurnId,
    },
    TurnFailed {
        turn_id: AgentTurnId,
        error: DashExecutionFailure,
        lost: bool,
    },
    TurnInterrupted {
        turn_id: AgentTurnId,
    },
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryContribution {
    pub entry_id: HistoryEntryId,
    pub payload: HistoryPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHistoryEntry {
    pub entry_id: HistoryEntryId,
    pub sequence: u64,
    pub parent_entry_id: Option<HistoryEntryId>,
    pub payload: HistoryPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForkCutoff {
    Head,
    CompletedTurn { turn_id: AgentTurnId },
    CompletedItem { item_id: AgentItemId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkLineage {
    pub parent_session_id: AgentSessionId,
    pub parent_branch_id: BranchId,
    pub source_head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub cutoff: ForkCutoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHistory {
    pub session_id: AgentSessionId,
    pub branch_id: BranchId,
    pub lineage: Option<ForkLineage>,
    entries: Vec<AgentHistoryEntry>,
}

impl AgentHistory {
    pub fn empty(session_id: AgentSessionId, branch_id: BranchId) -> Self {
        Self {
            session_id,
            branch_id,
            lineage: None,
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[AgentHistoryEntry] {
        &self.entries
    }

    pub fn head(&self) -> Option<&HistoryEntryId> {
        self.entries.last().map(|entry| &entry.entry_id)
    }

    pub fn state(&self) -> Result<AgentHistoryState, HistoryError> {
        fold_history(self)
    }

    pub fn state_at(&self, sequence: u64) -> Result<AgentHistoryState, HistoryError> {
        if sequence > self.entries.len() as u64 {
            return Err(HistoryError::SequenceOutOfRange {
                requested: sequence,
                available: self.entries.len() as u64,
            });
        }
        let mut prefix = self.clone();
        prefix.entries.truncate(sequence as usize);
        fold_history(&prefix)
    }

    pub(crate) fn truncate_after(&mut self, sequence: u64) -> Result<(), HistoryError> {
        if sequence > self.entries.len() as u64 {
            return Err(HistoryError::SequenceOutOfRange {
                requested: sequence,
                available: self.entries.len() as u64,
            });
        }
        self.entries.truncate(sequence as usize);
        Ok(())
    }

    pub fn append(
        &mut self,
        contribution: HistoryContribution,
    ) -> Result<AgentHistoryEntry, HistoryError> {
        let appended = self.append_batch(vec![contribution])?;
        Ok(appended.into_iter().next().expect("single append"))
    }

    pub fn append_batch(
        &mut self,
        contributions: Vec<HistoryContribution>,
    ) -> Result<Vec<AgentHistoryEntry>, HistoryError> {
        let mut staged = self.clone();
        let first_new = staged.entries.len();
        for contribution in contributions {
            let entry = AgentHistoryEntry {
                entry_id: contribution.entry_id,
                sequence: staged.entries.len() as u64 + 1,
                parent_entry_id: staged.head().cloned(),
                payload: contribution.payload,
            };
            staged.entries.push(entry);
        }
        fold_history(&staged)?;
        let appended = staged.entries[first_new..].to_vec();
        *self = staged;
        Ok(appended)
    }

    pub fn digest(&self) -> String {
        digest_entries(&self.entries)
    }

    pub fn fork(
        &self,
        child_session_id: AgentSessionId,
        child_branch_id: BranchId,
        cutoff: ForkCutoff,
    ) -> Result<Self, HistoryError> {
        let entry_count = match &cutoff {
            ForkCutoff::Head => self.entries.len(),
            ForkCutoff::CompletedTurn { turn_id } => self
                .entries
                .iter()
                .position(|entry| {
                    matches!(
                        &entry.payload,
                        HistoryPayload::TurnCompleted { turn_id: candidate }
                            if candidate == turn_id
                    )
                })
                .map(|index| index + 1)
                .ok_or_else(|| HistoryError::UnknownForkCutoff {
                    coordinate: turn_id.0.clone(),
                })?,
            ForkCutoff::CompletedItem { item_id } => self
                .entries
                .iter()
                .position(|entry| {
                    matches!(
                        &entry.payload,
                        HistoryPayload::ItemCompleted { item_id: candidate, .. }
                            if candidate == item_id
                    )
                })
                .map(|index| index + 1)
                .ok_or_else(|| HistoryError::UnknownForkCutoff {
                    coordinate: item_id.0.clone(),
                })?,
        };
        let entries = self.entries[..entry_count].to_vec();
        let source_head = entries.last().map(|entry| entry.entry_id.clone());
        let source_digest = digest_entries(&entries);
        let child = Self {
            session_id: child_session_id,
            branch_id: child_branch_id,
            lineage: Some(ForkLineage {
                parent_session_id: self.session_id.clone(),
                parent_branch_id: self.branch_id.clone(),
                source_head,
                source_digest,
                cutoff,
            }),
            entries,
        };
        fold_history(&child)?;
        Ok(child)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Active,
    Completed,
    Failed,
    Lost,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnState {
    pub status: ActivityStatus,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemState {
    pub turn_id: AgentTurnId,
    pub kind: ItemKind,
    pub status: ActivityStatus,
    pub details: ItemDetails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemDetails {
    Pending,
    AssistantMessage {
        content: String,
    },
    ToolActivity {
        call_id: String,
        name: String,
        arguments: String,
        result: Option<ToolActivityResult>,
    },
    Interaction {
        prompt: String,
    },
    ContextCompaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActivityResult {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionState {
    pub turn_id: AgentTurnId,
    pub item_id: Option<AgentItemId>,
    pub prompt: String,
    pub response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionState {
    pub mode: CompactionMode,
    pub status: ActivityStatus,
    pub revision: Option<ContextRevision>,
    pub summary: Option<String>,
    pub retained_from: Option<HistoryEntryId>,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHistoryState {
    pub session_id: AgentSessionId,
    pub branch_id: BranchId,
    pub head: Option<HistoryEntryId>,
    pub entry_count: u64,
    pub status: SessionStatus,
    pub initial_context: Option<InitialContextInstallation>,
    pub surface: Option<DashSurface>,
    pub thread_name: Option<String>,
    pub accepted_inputs: Vec<String>,
    pub active_turn: Option<AgentTurnId>,
    pub active_compaction: Option<CompactionId>,
    pub turns: BTreeMap<AgentTurnId, TurnState>,
    pub items: BTreeMap<AgentItemId, ItemState>,
    pub interactions: BTreeMap<InteractionId, InteractionState>,
    pub compactions: BTreeMap<CompactionId, CompactionState>,
}

pub fn fold_history(history: &AgentHistory) -> Result<AgentHistoryState, HistoryError> {
    let mut state = AgentHistoryState {
        session_id: history.session_id.clone(),
        branch_id: history.branch_id.clone(),
        head: None,
        entry_count: 0,
        status: SessionStatus::Open,
        initial_context: None,
        surface: None,
        thread_name: None,
        accepted_inputs: Vec::new(),
        active_turn: None,
        active_compaction: None,
        turns: BTreeMap::new(),
        items: BTreeMap::new(),
        interactions: BTreeMap::new(),
        compactions: BTreeMap::new(),
    };
    let mut ids = BTreeSet::new();

    for (index, entry) in history.entries.iter().enumerate() {
        let expected_sequence = index as u64 + 1;
        if entry.sequence != expected_sequence {
            return Err(HistoryError::SequenceGap {
                expected: expected_sequence,
                actual: entry.sequence,
            });
        }
        if entry.parent_entry_id != state.head {
            return Err(HistoryError::ParentMismatch {
                entry_id: entry.entry_id.clone(),
                expected: state.head,
                actual: entry.parent_entry_id.clone(),
            });
        }
        if !ids.insert(entry.entry_id.clone()) {
            return Err(HistoryError::DuplicateEntry(entry.entry_id.clone()));
        }
        if state.status == SessionStatus::Closed {
            return Err(HistoryError::ClosedSessionMutation);
        }
        if let HistoryPayload::CompactionStarted { source_digest, .. } = &entry.payload
            && source_digest != &digest_entries(&history.entries[..index])
        {
            return Err(HistoryError::CompactionSourceDigestMismatch);
        }

        apply_payload(&mut state, &entry.payload)?;
        state.head = Some(entry.entry_id.clone());
        state.entry_count = entry.sequence;
    }
    Ok(state)
}

fn apply_payload(
    state: &mut AgentHistoryState,
    payload: &HistoryPayload,
) -> Result<(), HistoryError> {
    match payload {
        HistoryPayload::InitialContextInstalled { installation } => {
            if state.initial_context.is_some() || state.entry_count > 0 {
                return Err(HistoryError::InitialContextNotFirst);
            }
            state.initial_context = Some(installation.clone());
        }
        HistoryPayload::SurfaceApplied { surface } => {
            if state
                .surface
                .as_ref()
                .is_some_and(|current| surface.revision < current.revision)
            {
                return Err(HistoryError::SurfaceRevisionMovedBackwards);
            }
            state.surface = Some(surface.clone());
        }
        HistoryPayload::SurfaceRevoked { surface } => {
            if state.surface.as_ref() != Some(surface) {
                return Err(HistoryError::SurfaceRevisionMismatch);
            }
            state.surface = None;
        }
        HistoryPayload::ThreadNameChanged { thread_name } => {
            let thread_name = thread_name.trim();
            if thread_name.is_empty() {
                return Err(HistoryError::InvalidThreadName);
            }
            state.thread_name = Some(thread_name.to_owned());
        }
        HistoryPayload::InputAccepted { input_id, .. } => {
            state.accepted_inputs.push(input_id.clone());
        }
        HistoryPayload::TurnStarted { turn_id } => {
            ensure_idle(state)?;
            if state
                .turns
                .insert(
                    turn_id.clone(),
                    TurnState {
                        status: ActivityStatus::Active,
                        output: None,
                    },
                )
                .is_some()
            {
                return Err(HistoryError::DuplicateTurn(turn_id.clone()));
            }
            state.active_turn = Some(turn_id.clone());
        }
        HistoryPayload::ItemStarted {
            turn_id,
            item_id,
            kind,
        } => {
            ensure_active_turn(state, turn_id)?;
            if state
                .items
                .insert(
                    item_id.clone(),
                    ItemState {
                        turn_id: turn_id.clone(),
                        kind: *kind,
                        status: ActivityStatus::Active,
                        details: match kind {
                            ItemKind::ContextCompaction => ItemDetails::ContextCompaction,
                            _ => ItemDetails::Pending,
                        },
                    },
                )
                .is_some()
            {
                return Err(HistoryError::DuplicateItem(item_id.clone()));
            }
        }
        HistoryPayload::ItemCompleted { turn_id, item_id } => {
            ensure_active_turn(state, turn_id)?;
            let item = state
                .items
                .get_mut(item_id)
                .ok_or_else(|| HistoryError::UnknownItem(item_id.clone()))?;
            if item.turn_id != *turn_id || item.status != ActivityStatus::Active {
                return Err(HistoryError::InvalidItemTransition(item_id.clone()));
            }
            item.status = ActivityStatus::Completed;
        }
        HistoryPayload::AgentOutput {
            turn_id,
            item_id,
            content,
        } => {
            ensure_active_turn(state, turn_id)?;
            state
                .turns
                .get_mut(turn_id)
                .expect("active turn exists")
                .output = Some(content.clone());
            if let Some(item_id) = item_id {
                let item = state
                    .items
                    .get_mut(item_id)
                    .ok_or_else(|| HistoryError::UnknownItem(item_id.clone()))?;
                item.details = ItemDetails::AssistantMessage {
                    content: content.clone(),
                };
            }
        }
        HistoryPayload::ToolCall {
            turn_id,
            item_id,
            call_id,
            name,
            arguments,
        } => {
            ensure_active_turn(state, turn_id)?;
            let item = state
                .items
                .get_mut(item_id)
                .ok_or_else(|| HistoryError::UnknownItem(item_id.clone()))?;
            if item.turn_id != *turn_id {
                return Err(HistoryError::InvalidItemTransition(item_id.clone()));
            }
            item.details = ItemDetails::ToolActivity {
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                result: None,
            };
        }
        HistoryPayload::ToolResult {
            turn_id,
            item_id,
            content,
            is_error,
        } => {
            ensure_active_turn(state, turn_id)?;
            let item = state
                .items
                .get_mut(item_id)
                .ok_or_else(|| HistoryError::UnknownItem(item_id.clone()))?;
            if item.turn_id != *turn_id {
                return Err(HistoryError::InvalidItemTransition(item_id.clone()));
            }
            let ItemDetails::ToolActivity {
                call_id,
                name,
                arguments,
                ..
            } = &item.details
            else {
                return Err(HistoryError::InvalidItemTransition(item_id.clone()));
            };
            item.details = ItemDetails::ToolActivity {
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                result: Some(ToolActivityResult {
                    content: content.clone(),
                    is_error: *is_error,
                }),
            };
        }
        HistoryPayload::InteractionRequested {
            turn_id,
            item_id,
            interaction_id,
            prompt,
        } => {
            ensure_active_turn(state, turn_id)?;
            if let Some(item_id) = item_id {
                let item = state
                    .items
                    .get_mut(item_id)
                    .ok_or_else(|| HistoryError::UnknownItem(item_id.clone()))?;
                item.details = ItemDetails::Interaction {
                    prompt: prompt.clone(),
                };
            }
            if state
                .interactions
                .insert(
                    interaction_id.clone(),
                    InteractionState {
                        turn_id: turn_id.clone(),
                        item_id: item_id.clone(),
                        prompt: prompt.clone(),
                        response: None,
                    },
                )
                .is_some()
            {
                return Err(HistoryError::DuplicateInteraction(interaction_id.clone()));
            }
        }
        HistoryPayload::InteractionResolved {
            interaction_id,
            response,
        } => {
            let interaction = state
                .interactions
                .get_mut(interaction_id)
                .ok_or_else(|| HistoryError::UnknownInteraction(interaction_id.clone()))?;
            if interaction.response.is_some() {
                return Err(HistoryError::InteractionAlreadyResolved(
                    interaction_id.clone(),
                ));
            }
            interaction.response = Some(response.clone());
        }
        HistoryPayload::CompactionStarted {
            compaction_id,
            mode,
            source_head,
            source_digest,
        } => {
            ensure_idle(state)?;
            if source_head != &state.head {
                return Err(HistoryError::CompactionSourceHeadMismatch);
            }
            if state
                .compactions
                .insert(
                    compaction_id.clone(),
                    CompactionState {
                        mode: *mode,
                        status: ActivityStatus::Active,
                        revision: None,
                        summary: None,
                        retained_from: None,
                        source_digest: source_digest.clone(),
                    },
                )
                .is_some()
            {
                return Err(HistoryError::DuplicateCompaction(compaction_id.clone()));
            }
            state.active_compaction = Some(compaction_id.clone());
        }
        HistoryPayload::CompactionApplied {
            compaction_id,
            revision,
            summary,
            retained_from,
            source_digest,
        } => {
            ensure_active_compaction(state, compaction_id)?;
            let compaction = state
                .compactions
                .get_mut(compaction_id)
                .expect("active compaction exists");
            if compaction.source_digest != *source_digest || compaction.revision.is_some() {
                return Err(HistoryError::InvalidCompactionTransition(
                    compaction_id.clone(),
                ));
            }
            compaction.revision = Some(revision.clone());
            compaction.summary = Some(summary.clone());
            compaction.retained_from = retained_from.clone();
        }
        HistoryPayload::CompactionCompleted { compaction_id } => {
            ensure_active_compaction(state, compaction_id)?;
            let compaction = state
                .compactions
                .get_mut(compaction_id)
                .expect("active compaction exists");
            if compaction.revision.is_none() {
                return Err(HistoryError::InvalidCompactionTransition(
                    compaction_id.clone(),
                ));
            }
            compaction.status = ActivityStatus::Completed;
            state.active_compaction = None;
        }
        HistoryPayload::CompactionFailed {
            compaction_id,
            lost,
            ..
        } => {
            ensure_active_compaction(state, compaction_id)?;
            state
                .compactions
                .get_mut(compaction_id)
                .expect("active compaction exists")
                .status = if *lost {
                ActivityStatus::Lost
            } else {
                ActivityStatus::Failed
            };
            state.active_compaction = None;
        }
        HistoryPayload::TurnCompleted { turn_id } => {
            terminalize_turn(state, turn_id, ActivityStatus::Completed)?;
        }
        HistoryPayload::TurnFailed {
            turn_id,
            error: _,
            lost,
        } => {
            terminalize_turn(
                state,
                turn_id,
                if *lost {
                    ActivityStatus::Lost
                } else {
                    ActivityStatus::Failed
                },
            )?;
        }
        HistoryPayload::TurnInterrupted { turn_id } => {
            terminalize_turn(state, turn_id, ActivityStatus::Interrupted)?;
        }
        HistoryPayload::Closed => {
            ensure_idle(state)?;
            state.status = SessionStatus::Closed;
        }
    }
    Ok(())
}

fn ensure_idle(state: &AgentHistoryState) -> Result<(), HistoryError> {
    if state.active_turn.is_some() || state.active_compaction.is_some() {
        Err(HistoryError::ActivityAlreadyActive)
    } else {
        Ok(())
    }
}

fn ensure_active_turn(
    state: &AgentHistoryState,
    turn_id: &AgentTurnId,
) -> Result<(), HistoryError> {
    if state.active_turn.as_ref() == Some(turn_id) {
        Ok(())
    } else {
        Err(HistoryError::TurnNotActive(turn_id.clone()))
    }
}

fn ensure_active_compaction(
    state: &AgentHistoryState,
    compaction_id: &CompactionId,
) -> Result<(), HistoryError> {
    if state.active_compaction.as_ref() == Some(compaction_id) {
        Ok(())
    } else {
        Err(HistoryError::CompactionNotActive(compaction_id.clone()))
    }
}

fn terminalize_turn(
    state: &mut AgentHistoryState,
    turn_id: &AgentTurnId,
    terminal: ActivityStatus,
) -> Result<(), HistoryError> {
    ensure_active_turn(state, turn_id)?;
    if state
        .items
        .values()
        .any(|item| item.turn_id == *turn_id && item.status == ActivityStatus::Active)
    {
        return Err(HistoryError::TurnHasActiveItems(turn_id.clone()));
    }
    state
        .turns
        .get_mut(turn_id)
        .expect("active turn exists")
        .status = terminal;
    state.active_turn = None;
    Ok(())
}

fn digest_entries(entries: &[AgentHistoryEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        let bytes = serde_json::to_vec(entry).expect("history entry serialization is infallible");
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HistoryError {
    #[error("history sequence gap: expected {expected}, got {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    #[error("history sequence {requested} exceeds available {available}")]
    SequenceOutOfRange { requested: u64, available: u64 },
    #[error("history entry {entry_id:?} has wrong parent")]
    ParentMismatch {
        entry_id: HistoryEntryId,
        expected: Option<HistoryEntryId>,
        actual: Option<HistoryEntryId>,
    },
    #[error("duplicate history entry {0:?}")]
    DuplicateEntry(HistoryEntryId),
    #[error("initial context must be the first and unique history contribution")]
    InitialContextNotFirst,
    #[error("Dash Agent surface revision moved backwards")]
    SurfaceRevisionMovedBackwards,
    #[error("Dash Agent surface revision does not match")]
    SurfaceRevisionMismatch,
    #[error("Dash Agent thread name must not be blank")]
    InvalidThreadName,
    #[error("session is closed")]
    ClosedSessionMutation,
    #[error("another history activity is active")]
    ActivityAlreadyActive,
    #[error("duplicate turn {0:?}")]
    DuplicateTurn(AgentTurnId),
    #[error("turn is not active: {0:?}")]
    TurnNotActive(AgentTurnId),
    #[error("turn still has active items: {0:?}")]
    TurnHasActiveItems(AgentTurnId),
    #[error("duplicate item {0:?}")]
    DuplicateItem(AgentItemId),
    #[error("unknown item {0:?}")]
    UnknownItem(AgentItemId),
    #[error("invalid item transition {0:?}")]
    InvalidItemTransition(AgentItemId),
    #[error("duplicate interaction {0:?}")]
    DuplicateInteraction(InteractionId),
    #[error("unknown interaction {0:?}")]
    UnknownInteraction(InteractionId),
    #[error("interaction already resolved {0:?}")]
    InteractionAlreadyResolved(InteractionId),
    #[error("duplicate compaction {0:?}")]
    DuplicateCompaction(CompactionId),
    #[error("compaction is not active: {0:?}")]
    CompactionNotActive(CompactionId),
    #[error("compaction source head does not match current history head")]
    CompactionSourceHeadMismatch,
    #[error("compaction source digest does not match the immutable history prefix")]
    CompactionSourceDigestMismatch,
    #[error("invalid compaction transition {0:?}")]
    InvalidCompactionTransition(CompactionId),
    #[error("unknown exact fork cutoff {coordinate}")]
    UnknownForkCutoff { coordinate: String },
}
