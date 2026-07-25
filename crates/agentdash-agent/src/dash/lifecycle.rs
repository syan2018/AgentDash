use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{CompactionId, CompactionMode};

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

string_id!(CommandId);
string_id!(EffectId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDependency {
    pub command_id: CommandId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DashCommandKind {
    SubmitInput {
        input_id: String,
        content: String,
    },
    RequestCompaction {
        compaction_id: CompactionId,
        mode: CompactionMode,
    },
    ContinueAfterCompaction {
        input_id: String,
        content: String,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashCommand {
    pub command_id: CommandId,
    pub kind: DashCommandKind,
    pub dependency: Option<CommandDependency>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Queued,
    Active,
    Succeeded,
    Failed,
    Lost,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Succeeded,
    Failed,
    Lost,
}

impl From<CommandOutcome> for CommandStatus {
    fn from(value: CommandOutcome) -> Self {
        match value {
            CommandOutcome::Succeeded => Self::Succeeded,
            CommandOutcome::Failed => Self::Failed,
            CommandOutcome::Lost => Self::Lost,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectOutcome {
    Applied,
    Failed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashExecutionConsistency {
    Current,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CommandRecord {
    command: DashCommand,
    status: CommandStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashLifecycle {
    commands: BTreeMap<CommandId, CommandRecord>,
    queue: VecDeque<CommandId>,
    active: Option<CommandId>,
    effects: BTreeMap<EffectId, EffectOutcome>,
    pub consistency: DashExecutionConsistency,
}

impl Default for DashLifecycle {
    fn default() -> Self {
        Self {
            commands: BTreeMap::new(),
            queue: VecDeque::new(),
            active: None,
            effects: BTreeMap::new(),
            consistency: DashExecutionConsistency::Current,
        }
    }
}

impl DashLifecycle {
    pub fn command_ids(&self) -> impl Iterator<Item = &CommandId> {
        self.commands.keys()
    }

    pub fn effect_ids(&self) -> impl Iterator<Item = &EffectId> {
        self.effects.keys()
    }

    pub fn enqueue(&mut self, command: DashCommand) -> Result<(), LifecycleError> {
        if let Some(existing) = self.commands.get(&command.command_id) {
            return if existing.command == command {
                Ok(())
            } else {
                Err(LifecycleError::ConflictingCommand(command.command_id))
            };
        }
        self.queue.push_back(command.command_id.clone());
        self.commands.insert(
            command.command_id.clone(),
            CommandRecord {
                command,
                status: CommandStatus::Queued,
            },
        );
        Ok(())
    }

    pub fn active(&self) -> Option<&CommandId> {
        self.active.as_ref()
    }

    pub fn status(&self, command_id: &CommandId) -> Option<CommandStatus> {
        self.commands.get(command_id).map(|record| record.status)
    }

    pub fn command(&self, command_id: &CommandId) -> Option<&DashCommand> {
        self.commands.get(command_id).map(|record| &record.command)
    }

    pub fn effect(&self, effect_id: &EffectId) -> Option<EffectOutcome> {
        self.effects.get(effect_id).copied()
    }

    pub fn promote_next(&mut self) -> Result<Option<DashCommand>, LifecycleError> {
        if self.active.is_some() {
            return Ok(None);
        }

        let candidates = self.queue.len();
        for _ in 0..candidates {
            let command_id = self.queue.pop_front().expect("queue length is stable");
            let dependency = self
                .commands
                .get(&command_id)
                .expect("queued command exists")
                .command
                .dependency
                .clone();

            match dependency.and_then(|dependency| {
                self.commands
                    .get(&dependency.command_id)
                    .map(|record| record.status)
            }) {
                None | Some(CommandStatus::Succeeded) => {
                    let record = self.commands.get_mut(&command_id).expect("command exists");
                    record.status = CommandStatus::Active;
                    self.active = Some(command_id);
                    return Ok(Some(record.command.clone()));
                }
                Some(CommandStatus::Failed) => {
                    self.commands
                        .get_mut(&command_id)
                        .expect("command exists")
                        .status = CommandStatus::Failed;
                }
                Some(CommandStatus::Lost | CommandStatus::Blocked) => {
                    self.commands
                        .get_mut(&command_id)
                        .expect("command exists")
                        .status = CommandStatus::Blocked;
                    self.consistency = DashExecutionConsistency::Lost;
                }
                Some(CommandStatus::Queued | CommandStatus::Active) => {
                    self.queue.push_back(command_id);
                }
            }
        }
        Ok(None)
    }

    pub fn settle_active(
        &mut self,
        command_id: &CommandId,
        outcome: CommandOutcome,
    ) -> Result<(), LifecycleError> {
        if self.active.as_ref() != Some(command_id) {
            return Err(LifecycleError::CommandNotActive(command_id.clone()));
        }
        self.commands
            .get_mut(command_id)
            .expect("active command exists")
            .status = outcome.into();
        self.active = None;

        if outcome == CommandOutcome::Lost {
            self.consistency = DashExecutionConsistency::Lost;
        }
        self.terminalize_dependents(command_id, outcome);
        Ok(())
    }

    pub fn settle_effect(
        &mut self,
        effect_id: EffectId,
        outcome: EffectOutcome,
    ) -> Result<(), LifecycleError> {
        if let Some(existing) = self.effects.get(&effect_id) {
            return if *existing == outcome {
                Ok(())
            } else {
                Err(LifecycleError::ConflictingEffect(effect_id))
            };
        }
        if outcome == EffectOutcome::Lost {
            self.consistency = DashExecutionConsistency::Lost;
        }
        self.effects.insert(effect_id, outcome);
        Ok(())
    }

    fn terminalize_dependents(&mut self, command_id: &CommandId, outcome: CommandOutcome) {
        if outcome == CommandOutcome::Succeeded {
            return;
        }
        for record in self.commands.values_mut() {
            if record.status != CommandStatus::Queued
                || record
                    .command
                    .dependency
                    .as_ref()
                    .is_none_or(|dependency| dependency.command_id != *command_id)
            {
                continue;
            }
            record.status = if outcome == CommandOutcome::Failed {
                CommandStatus::Failed
            } else {
                CommandStatus::Blocked
            };
        }
        self.queue.retain(|candidate| {
            self.commands
                .get(candidate)
                .is_some_and(|record| record.status == CommandStatus::Queued)
        });
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("command identity was reused with conflicting content: {0:?}")]
    ConflictingCommand(CommandId),
    #[error("command is not active: {0:?}")]
    CommandNotActive(CommandId),
    #[error("effect identity was reused with a conflicting terminal: {0:?}")]
    ConflictingEffect(EffectId),
}
