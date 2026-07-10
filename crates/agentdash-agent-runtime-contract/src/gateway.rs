use async_trait::async_trait;

use crate::{
    OperationReceipt, RuntimeCommandEnvelope, RuntimeEventEnvelope, RuntimeEventSubscription,
    RuntimeExecuteError, RuntimeSnapshotError, RuntimeSnapshotQuery, RuntimeSnapshotResult,
    RuntimeSubscribeError,
};

#[async_trait]
pub trait RuntimeEventStream: Send {
    async fn next(&mut self) -> Option<Result<RuntimeEventEnvelope, RuntimeSubscribeError>>;
}

#[async_trait]
pub trait AgentRuntimeGateway: Send + Sync {
    async fn execute(
        &self,
        command: RuntimeCommandEnvelope,
    ) -> Result<OperationReceipt, RuntimeExecuteError>;

    async fn snapshot(
        &self,
        query: RuntimeSnapshotQuery,
    ) -> Result<RuntimeSnapshotResult, RuntimeSnapshotError>;

    async fn events(
        &self,
        subscription: RuntimeEventSubscription,
    ) -> Result<Box<dyn RuntimeEventStream>, RuntimeSubscribeError>;
}
