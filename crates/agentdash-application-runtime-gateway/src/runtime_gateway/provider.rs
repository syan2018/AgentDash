use async_trait::async_trait;

use super::error::RuntimeInvocationError;
use super::types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeActor, RuntimeContext,
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

    async fn supports_action(
        &self,
        action_key: &RuntimeActionKey,
        context: &RuntimeContext,
    ) -> Result<bool, RuntimeInvocationError> {
        Ok(self.supports(action_key, context))
    }

    async fn discover_actions(
        &self,
        _actor: &RuntimeActor,
        context: &RuntimeContext,
    ) -> Result<Vec<RuntimeActionDescriptor>, RuntimeInvocationError> {
        if self.action_kind() == context.action_kind() {
            Ok(vec![self.describe_action()])
        } else {
            Ok(Vec::new())
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError>;
}
