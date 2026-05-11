use async_trait::async_trait;

use super::error::RuntimeInvocationError;
use super::types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeContext,
    RuntimeInvocationOutput, RuntimeInvocationRequest,
};

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    fn action_key(&self) -> &RuntimeActionKey;

    fn action_kind(&self) -> RuntimeActionKind;

    fn describe_action(&self) -> RuntimeActionDescriptor {
        RuntimeActionDescriptor::new(self.action_key().clone(), self.action_kind())
    }

    fn supports(&self, action_key: &RuntimeActionKey, context: &RuntimeContext) -> bool {
        self.action_key() == action_key && self.action_kind() == context.action_kind()
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError>;
}
