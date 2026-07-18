use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    AgentHistory, AgentHistoryEntry, AgentHistoryState, AgentTurnId, CommandId, CommandOutcome,
    CommandStatus, CompactionId, ContextRevision, DashCommand, DashCommandKind, DashLifecycle,
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
    pub state: AgentHistoryState,
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
                    mode,
                    source_head: self.history.head().cloned(),
                    source_digest: self.history.digest(),
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
        let source_digest = self
            .history
            .state()?
            .compactions
            .get(&compaction_id)
            .ok_or_else(|| StoreError::UnknownCompaction(compaction_id.clone()))?
            .source_digest
            .clone();
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
                        revision,
                        summary,
                        retained_from,
                        source_digest,
                    },
                },
                HistoryContribution {
                    entry_id: completed_entry_id,
                    payload: HistoryPayload::CompactionCompleted { compaction_id },
                },
            ],
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
        for entry in &appended {
            let previous_state = if entry.sequence == 1 {
                None
            } else {
                Some(staged.history.state_at(entry.sequence - 1)?)
            };
            let state = staged.history.state_at(entry.sequence)?;
            let source_digest = {
                let mut prefix = staged.history.clone();
                prefix.truncate_after(entry.sequence)?;
                prefix.digest()
            };
            staged.changes.push(DashAgentChange {
                cursor: DashChangeCursor::new(entry.sequence, 0),
                head: Some(entry.entry_id.clone()),
                source_digest: source_digest.clone(),
                state: state.clone(),
                payload: DashAgentChangePayload::HistoryEntry {
                    entry: entry.clone(),
                },
            });
            if previous_state
                .as_ref()
                .and_then(|previous| previous.active_turn.as_ref())
                != state.active_turn.as_ref()
            {
                staged.changes.push(DashAgentChange {
                    cursor: DashChangeCursor::new(entry.sequence, 1),
                    head: Some(entry.entry_id.clone()),
                    source_digest,
                    state: state.clone(),
                    payload: DashAgentChangePayload::ActiveTurnChanged {
                        active_turn_id: state.active_turn,
                    },
                });
            }
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
