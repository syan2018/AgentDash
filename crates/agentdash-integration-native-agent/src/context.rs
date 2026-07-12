use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverThreadId, DriverTurnId, RuntimeBindingId, RuntimeDriverGeneration,
    RuntimeItemId, RuntimeThreadId, RuntimeTurnId, ToolSetRevision,
};
use agentdash_integration_api::AuthIdentity;
use tokio::sync::RwLock;

#[derive(Clone)]
pub(crate) struct NativeBindingContext {
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub runtime_thread_id: RuntimeThreadId,
    pub authorization_identity: Option<AuthIdentity>,
}

#[derive(Clone)]
pub(crate) struct NativeToolCallContext {
    pub active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    pub active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    pub tool_set_revision: ToolSetRevision,
    pub item_identities: Arc<RwLock<BTreeMap<(DriverTurnId, DriverItemId), RuntimeItemId>>>,
}
