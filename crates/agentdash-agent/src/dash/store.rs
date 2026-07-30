use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, AgentHistoryReplayer, CommandId, CommandOutcome,
    CommandStatus, CompactionId, DashCommand, DashCommandKind, DashLifecycle, HistoryContribution,
    HistoryEntryId, HistoryError, HistoryPayload, LifecycleError, ProjectedAgentHistoryEntry,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSettlement {
    pub command_id: CommandId,
    pub outcome: CommandOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentCommit {
    pub expected_head: Option<HistoryEntryId>,
    pub command_settlement: Option<CommandSettlement>,
    pub history: Vec<HistoryContribution>,
    pub enqueue_commands: Vec<DashCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashChangeCursor {
    pub revision: u64,
}

impl DashChangeCursor {
    pub fn new(revision: u64) -> Self {
        Self { revision }
    }

    pub fn encode(&self) -> String {
        self.revision.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentChange {
    pub cursor: DashChangeCursor,
    pub head: Option<HistoryEntryId>,
    pub source_digest: String,
    pub entry: AgentHistoryEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashAgentStore {
    history: AgentHistory,
    lifecycle: DashLifecycle,
    #[serde(skip)]
    pending_history_projections: Vec<ProjectedAgentHistoryEntry>,
}

impl DashAgentStore {
    pub fn new(history: AgentHistory) -> Result<Self, StoreError> {
        history.state()?;
        Ok(Self {
            history,
            lifecycle: DashLifecycle::default(),
            pending_history_projections: Vec::new(),
        })
    }

    pub fn history(&self) -> &AgentHistory {
        &self.history
    }

    pub fn lifecycle(&self) -> &DashLifecycle {
        &self.lifecycle
    }

    pub fn take_pending_history_projections(&mut self) -> Vec<ProjectedAgentHistoryEntry> {
        std::mem::take(&mut self.pending_history_projections)
    }

    pub fn changes(&self) -> Result<Vec<DashAgentChange>, StoreError> {
        let mut replay = AgentHistoryReplayer::new(&self.history);
        self.history
            .entries()
            .iter()
            .map(|entry| {
                replay.apply(entry)?;
                Ok(DashAgentChange {
                    cursor: DashChangeCursor::new(entry.sequence),
                    head: Some(entry.entry_id.clone()),
                    source_digest: replay.source_digest(),
                    entry: entry.clone(),
                })
            })
            .collect()
    }

    pub fn claim_next_command(&mut self) -> Result<Option<DashCommand>, StoreError> {
        Ok(self.lifecycle.promote_next()?)
    }

    pub fn command_status(&self, command_id: &CommandId) -> Option<CommandStatus> {
        self.lifecycle.status(command_id)
    }

    pub fn begin_compaction(
        &mut self,
        command: DashCommand,
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
            history: vec![HistoryContribution {
                entry_id: started_entry_id,
                payload: HistoryPayload::CompactionStarted {
                    compaction_id: compaction_id.clone(),
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
        compaction_id: CompactionId,
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
        let mode = compaction.mode;
        let revision = super::history::compaction_context_revision(
            &compaction_id,
            &compaction.source_digest,
            &summary,
            retained_from.as_ref(),
        );
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
        let compacted_entries = entries[..retained_index]
            .iter()
            .filter(is_recipe_entry)
            .collect::<Vec<_>>();
        let retained_entries = entries[retained_index..]
            .iter()
            .filter(is_recipe_entry)
            .collect::<Vec<_>>();
        let created_at_ms = crate::model::message::now_millis();
        let tokens_before = history_state
            .token_usage
            .last
            .map_or(0, |usage| usage.input_tokens);
        let source_start_event_seq = compacted_entries.first().map(|entry| entry.sequence);
        let source_end_event_seq = compacted_entries.last().map(|entry| entry.sequence);
        let first_kept_event_seq = retained_entries.first().map(|entry| entry.sequence);
        let summary_frame = super::history::accepted_compaction_summary_frame(
            &compaction_id,
            &revision,
            &summary,
            mode,
            tokens_before,
            u32::try_from(compacted_entries.len()).unwrap_or(u32::MAX),
            source_start_event_seq,
            source_end_event_seq,
            first_kept_event_seq,
            created_at_ms,
        );
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: Some(CommandSettlement {
                command_id,
                outcome: CommandOutcome::Succeeded,
            }),
            history: vec![
                HistoryContribution {
                    entry_id: applied_entry_id,
                    payload: HistoryPayload::CompactionApplied {
                        compaction_id: compaction_id.clone(),
                        context_revision: revision,
                        summary_frame: Box::new(summary_frame),
                        retained_from,
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
        compaction_id: CompactionId,
        entry_id: HistoryEntryId,
    ) -> Result<(), StoreError> {
        self.commit(DashAgentCommit {
            expected_head: self.history.head().cloned(),
            command_settlement: Some(CommandSettlement {
                command_id,
                outcome: CommandOutcome::Failed,
            }),
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
        for command in commit.enqueue_commands {
            staged.lifecycle.enqueue(command)?;
        }
        let projections = staged.history.append_batch_projected(commit.history)?;
        let appended = projections
            .iter()
            .map(|projection| projection.entry.clone())
            .collect();
        staged.pending_history_projections.extend(projections);
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
