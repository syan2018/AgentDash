use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverThreadId, DriverTurnId, RuntimeBindingId, RuntimeDriverGeneration, RuntimeThreadId,
    RuntimeTurnId, ToolSetRevision,
};
use agentdash_integration_api::AuthIdentity;
use tokio::sync::RwLock;

use crate::tool::NativeToolEventContext;

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
    pub events: Arc<RwLock<Option<NativeToolEventContext>>>,
}
