use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, AgentHistoryReplayer, AgentTurnId, CommandId, CommandOutcome,
    CommandStatus, CompactionCheckpoint, CompactionId, CompactionToolPairMembership,
    CompactionUsageEvidence, ContextRevision, DashCommand, DashCommandKind, DashLifecycle,
    EffectId, EffectOutcome, HistoryContribution, HistoryEntryId, HistoryError, HistoryPayload,
    LifecycleError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSettlement {
    pub command_id: CommandId,
    pub outcome: CommandOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSettlement {
    pub effect_id: EffectId,
    pub outcome: EffectOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentCommit {
    pub expected_head: Option<HistoryEntryId>,
    pub command_settlement: Option<CommandSettlement>,
    pub effect_settlements: Vec<EffectSettlement>,
    pub history: Vec<HistoryContribution>,
    pub enqueue_commands: Vec<DashCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashChangeCursor {
    pub revision: u64,
    pub ordinal: u16,
}

impl DashChangeCursor {
    pub fn new(revision: u64, ordinal: u16) -> Self {
        Self { revision, ordinal }
    }

    pub fn encode(&self) -> String {
        format!("{}:{}", self.revision, self.ordinal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DashAgentChangePayload {
    HistoryEntry { entry: AgentHistoryEntry },
    ActiveTurnChanged { active_turn_id: Option<AgentTurnId> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentChange {
    pub cursor: DashChangeCursor,
    pub head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub payload: DashAgentChangePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashExecutionInspection {
    pub command_status: Option<CommandStatus>,
    pub effect_outcome: Option<EffectOutcome>,
    pub history_head: Option<HistoryEntryId>,
    pub consistency: super::DashExecutionConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentStore {
    history: AgentHistory,
    lifecycle: DashLifecycle,
    changes: Vec<DashAgentChange>,
}

impl DashAgentStore {
    pub fn new(history: AgentHistory) -> Result<Self, StoreError> {
        history.state()?;
        Ok(Self {
            history,
            lifecycle: DashLifecycle::default(),
            changes: Vec::new(),
        })
    }

    pub fn history(&self) -> &AgentHistory {
        &self.history
    }

    pub fn lifecycle(&self) -> &DashLifecycle {
        &self.lifecycle
    }

    pub fn changes(&self) -> &[DashAgentChange] {
        &self.changes
    }

    pub fn claim_next_command(&mut self) -> Result<Option<DashCommand>, StoreError> {
        Ok(self.lifecycle.promote_next()?)
    }

    pub fn command_status(&self, command_id: &CommandId) -> Option<CommandStatus> {
        self.lifecycle.status(command_id)
    }

    pub fn effect_outcome(&self, effect_id: &EffectId) -> Option<EffectOutcome> {
        self.lifecycle.effect(effect_id)
    }

    pub fn inspect_execution(
        &self,
        command_id: &CommandId,
        effect_id: &EffectId,
    ) -> DashExecutionInspection {
        DashExecutionInspection {
            command_status: self.command_status(command_id),
            effect_outcome: self.effect_outcome(effect_id),
            history_head: self.history.head().cloned(),
            consistency: self.lifecycle.consistency,
        }
    }

    pub fn begin_compaction(
        &mut self,
        command: DashCommand,
        operation_id: EffectId,
        started_entry_id: HistoryEntryId,
    ) -> Result<CompactionId, StoreError> {
        let (compaction_id, mode) = match &command.kind {
            DashCommandKind::RequestCompaction {
                compaction_id,
                mode,
            } => (compaction_id.clone(), *mode),
            _ => return Err(StoreError::NotCompactionCommand(command.command_id)),
        };
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: None,
            effect_settlements: vec![],
            history: vec![],
            enqueue_commands: vec![command.clone()],
        })?;
        let claimed = self.claim_next_command()?;
        if claimed.as_ref().map(|value| &value.command_id) != Some(&command.command_id) {
            return Err(StoreError::CommandNotPromoted(command.command_id));
        }
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: None,
            effect_settlements: vec![],
            history: vec![HistoryContribution {
                entry_id: started_entry_id,
                payload: HistoryPayload::CompactionStarted {
                    compaction_id: compaction_id.clone(),
                    operation_id,
                    mode,
                    source_head: self.history.head().cloned(),
                    source_digest: self.history.digest(),
                    started_at_ms: crate::model::message::now_millis(),
                },
            }],
            enqueue_commands: vec![],
        })?;
        Ok(compaction_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_compaction(
        &mut self,
        command_id: CommandId,
        effect_id: EffectId,
        compaction_id: CompactionId,
        revision: ContextRevision,
        summary: String,
        retained_from: Option<HistoryEntryId>,
        applied_entry_id: HistoryEntryId,
        completed_entry_id: HistoryEntryId,
    ) -> Result<(), StoreError> {
        let history_state = self.history.state()?;
        let compaction = history_state
            .compactions
            .get(&compaction_id)
            .ok_or_else(|| StoreError::UnknownCompaction(compaction_id.clone()))?;
        let source_digest = compaction.source_digest.clone();
        let operation_id = compaction.operation_id.clone();
        let mode = compaction.mode;
        let entries = self.history.entries();
        let retained_index = retained_from
            .as_ref()
            .and_then(|id| entries.iter().position(|entry| &entry.entry_id == id))
            .unwrap_or(entries.len());
        let is_recipe_entry = |entry: &&AgentHistoryEntry| {
            matches!(
                entry.payload,
                HistoryPayload::InputAccepted { .. }
                    | HistoryPayload::AgentOutput { .. }
                    | HistoryPayload::ToolCall { .. }
                    | HistoryPayload::ToolResult { .. }
            )
        };
        let compacted_entry_ids = entries[..retained_index]
            .iter()
            .filter(is_recipe_entry)
            .map(|entry| entry.entry_id.clone())
            .collect::<Vec<_>>();
        let retained_entry_ids = entries[retained_index..]
            .iter()
            .filter(is_recipe_entry)
            .map(|entry| entry.entry_id.clone())
            .collect::<Vec<_>>();
        let mut tool_calls = std::collections::BTreeMap::new();
        let mut tool_pairs = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            match &entry.payload {
                HistoryPayload::ToolCall {
                    item_id, call_id, ..
                } => {
                    tool_calls.insert(
                        item_id.clone(),
                        (
                            entry.entry_id.clone(),
                            call_id.clone(),
                            index >= retained_index,
                        ),
                    );
                }
                HistoryPayload::ToolResult { item_id, .. } => {
                    if let Some((call_entry_id, call_id, retained)) = tool_calls.remove(item_id) {
                        tool_pairs.push(CompactionToolPairMembership {
                            call_entry_id,
                            result_entry_id: Some(entry.entry_id.clone()),
                            call_id,
                            retained,
                        });
                    }
                }
                _ => {}
            }
        }
        tool_pairs.extend(
            tool_calls
                .into_values()
                .map(
                    |(call_entry_id, call_id, retained)| CompactionToolPairMembership {
                        call_entry_id,
                        result_entry_id: None,
                        call_id,
                        retained,
                    },
                ),
        );
        let created_at_ms = crate::model::message::now_millis();
        let usage = history_state
            .token_usage
            .last
            .map(|usage| CompactionUsageEvidence {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                context_window: usage.context_window,
                observed_turn_id: usage.turn_id,
            });
        let tokens_before = usage.as_ref().map_or(0, |usage| usage.input_tokens);
        let source_start_event_seq = compacted_entry_ids
            .first()
            .and_then(|id| entries.iter().find(|entry| &entry.entry_id == id))
            .map(|entry| entry.sequence);
        let source_end_event_seq = compacted_entry_ids
            .last()
            .and_then(|id| entries.iter().find(|entry| &entry.entry_id == id))
            .map(|entry| entry.sequence);
        let first_kept_event_seq = retained_entry_ids
            .first()
            .and_then(|id| entries.iter().find(|entry| &entry.entry_id == id))
            .map(|entry| entry.sequence);
        let summary_frame = super::history::accepted_compaction_summary_frame(
            &compaction_id,
            &revision,
            &summary,
            mode,
            tokens_before,
            u32::try_from(compacted_entry_ids.len()).unwrap_or(u32::MAX),
            source_start_event_seq,
            source_end_event_seq,
            first_kept_event_seq,
            created_at_ms,
        );
        let source_head = entries
            .iter()
            .find_map(|entry| match &entry.payload {
                HistoryPayload::CompactionStarted {
                    compaction_id: started,
                    source_head,
                    ..
                } if started == &compaction_id => Some(source_head.clone()),
                _ => None,
            })
            .flatten();
        let base_history_revision = source_head
            .as_ref()
            .and_then(|id| entries.iter().find(|entry| &entry.entry_id == id))
            .map_or(0, |entry| entry.sequence);
        let applied_history_revision = history_state.entry_count + 1;
        let checkpoint_digest = format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    &operation_id,
                    &revision,
                    base_history_revision,
                    applied_history_revision,
                    &source_head,
                    &source_digest,
                    &summary_frame,
                    &compacted_entry_ids,
                    &retained_from,
                    &retained_entry_ids,
                    &tool_pairs,
                    &usage,
                    created_at_ms,
                ))
                .expect("typed compaction checkpoint serialization cannot fail")
            )
        );
        let checkpoint = CompactionCheckpoint {
            operation_id,
            context_revision: revision,
            base_history_revision,
            applied_history_revision,
            source_head,
            source_digest,
            summary,
            summary_frame,
            compacted_entry_ids,
            retained_from,
            retained_entry_ids,
            tool_pairs,
            checkpoint_digest,
            usage,
            created_at_ms,
        };
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: Some(CommandSettlement {
                command_id,
                outcome: CommandOutcome::Succeeded,
            }),
            effect_settlements: vec![EffectSettlement {
                effect_id,
                outcome: EffectOutcome::Applied,
            }],
            history: vec![
                HistoryContribution {
                    entry_id: applied_entry_id,
                    payload: HistoryPayload::CompactionApplied {
                        compaction_id: compaction_id.clone(),
                        checkpoint,
                    },
                },
                HistoryContribution {
                    entry_id: completed_entry_id,
                    payload: HistoryPayload::CompactionCompleted {
                        compaction_id,
                        completed_at_ms: crate::model::message::now_millis(),
                    },
                },
            ],
            enqueue_commands: vec![],
        })?;
        Ok(())
    }

    pub fn mark_compaction_side_effect_started(
        &mut self,
        compaction_id: CompactionId,
        entry_id: HistoryEntryId,
    ) -> Result<(), StoreError> {
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: None,
            effect_settlements: vec![],
            history: vec![HistoryContribution {
                entry_id,
                payload: HistoryPayload::CompactionSideEffectStarted {
                    compaction_id,
                    started_at_ms: crate::model::message::now_millis(),
                },
            }],
            enqueue_commands: vec![],
        })?;
        Ok(())
    }

    pub fn cancel_compaction(
        &mut self,
        command_id: CommandId,
        effect_id: EffectId,
        compaction_id: CompactionId,
        entry_id: HistoryEntryId,
    ) -> Result<(), StoreError> {
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: Some(CommandSettlement {
                command_id,
                outcome: CommandOutcome::Failed,
            }),
            effect_settlements: vec![EffectSettlement {
                effect_id,
                outcome: EffectOutcome::Failed,
            }],
            history: vec![HistoryContribution {
                entry_id,
                payload: HistoryPayload::CompactionCancelled {
                    compaction_id,
                    completed_at_ms: crate::model::message::now_millis(),
                },
            }],
            enqueue_commands: vec![],
        })?;
        Ok(())
    }

    pub fn fail_compaction(
        &mut self,
        command_id: CommandId,
        effect_id: EffectId,
        compaction_id: CompactionId,
        failed_entry_id: HistoryEntryId,
        error: String,
        lost: bool,
    ) -> Result<(), StoreError> {
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: Some(CommandSettlement {
                command_id,
                outcome: if lost {
                    CommandOutcome::Lost
                } else {
                    CommandOutcome::Failed
                },
            }),
            effect_settlements: vec![EffectSettlement {
                effect_id,
                outcome: if lost {
                    EffectOutcome::Lost
                } else {
                    EffectOutcome::Failed
                },
            }],
            history: vec![HistoryContribution {
                entry_id: failed_entry_id,
                payload: HistoryPayload::CompactionFailed {
                    compaction_id,
                    error,
                    lost,
                    completed_at_ms: crate::model::message::now_millis(),
                },
            }],
            enqueue_commands: vec![],
        })?;
        Ok(())
    }

    pub fn commit(
        &mut self,
        commit: DashAgentCommit,
    ) -> Result<Vec<AgentHistoryEntry>, StoreError> {
        if self.history.head() != commit.expected_head.as_ref() {
            return Err(StoreError::HeadConflict {
                expected: commit.expected_head,
                actual: self.history.head().cloned(),
            });
        }

        let mut staged = self.clone();
        if let Some(settlement) = commit.command_settlement {
            staged
                .lifecycle
                .settle_active(&settlement.command_id, settlement.outcome)?;
        }
        for settlement in commit.effect_settlements {
            staged
                .lifecycle
                .settle_effect(settlement.effect_id, settlement.outcome)?;
        }
        for command in commit.enqueue_commands {
            staged.lifecycle.enqueue(command)?;
        }
        let appended = staged.history.append_batch(commit.history)?;
        let first_appended_sequence = appended.first().map(|entry| entry.sequence);
        let mut replay = AgentHistoryReplayer::new(&staged.history);
        let mut previous_active_turn = None;
        for entry in staged.history.entries() {
            let active_turn = replay.apply(entry)?.active_turn.clone();
            if first_appended_sequence.is_none_or(|first| entry.sequence < first) {
                previous_active_turn = active_turn;
                continue;
            }
            let source_digest = replay.source_digest();
            staged.changes.push(DashAgentChange {
                cursor: DashChangeCursor::new(entry.sequence, 0),
                head: Some(entry.entry_id.clone()),
                source_digest: source_digest.clone(),
                payload: DashAgentChangePayload::HistoryEntry {
                    entry: entry.clone(),
                },
            });
            if previous_active_turn.as_ref() != active_turn.as_ref() {
                staged.changes.push(DashAgentChange {
                    cursor: DashChangeCursor::new(entry.sequence, 1),
                    head: Some(entry.entry_id.clone()),
                    source_digest,
                    payload: DashAgentChangePayload::ActiveTurnChanged {
                        active_turn_id: active_turn.clone(),
                    },
                });
            }
            previous_active_turn = active_turn;
        }
        *self = staged;
        Ok(appended)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StoreError {
    #[error("Dash Agent history head conflict")]
    HeadConflict {
        expected: Option<HistoryEntryId>,
        actual: Option<HistoryEntryId>,
    },
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error("command is not a compaction command: {0:?}")]
    NotCompactionCommand(CommandId),
    #[error("compaction command was queued behind another active command: {0:?}")]
    CommandNotPromoted(CommandId),
    #[error("unknown compaction: {0:?}")]
    UnknownCompaction(CompactionId),
}
