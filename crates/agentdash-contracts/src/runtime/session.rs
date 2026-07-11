use serde::{Deserialize, Serialize};
use ts_rs::TS;

use agentdash_agent_types::MessageRef;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionMessageRefDto {
    pub turn_id: String,
    pub entry_index: u32,
}

impl From<MessageRef> for SessionMessageRefDto {
    fn from(value: MessageRef) -> Self {
        Self {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        }
    }
}
