use std::collections::HashMap;
use std::sync::Arc;

use super::error::RuntimeInvocationError;
use super::provider::RuntimeProvider;
use super::types::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeActor, RuntimeContext,
    RuntimeInvocationRequest, RuntimeInvocationResult, RuntimeSurface,
};

pub struct RuntimeGateway {
    providers: HashMap<RuntimeActionKey, Arc<dyn RuntimeProvider>>,
    dynamic_providers: Vec<Arc<dyn RuntimeProvider>>,
}

impl RuntimeGateway {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            dynamic_providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn RuntimeProvider>) {
        self.providers
            .insert(provider.action_key().clone(), provider);
    }

    pub fn with_provider(mut self, provider: Arc<dyn RuntimeProvider>) -> Self {
        self.register(provider);
        self
    }

    pub fn register_dynamic(&mut self, provider: Arc<dyn RuntimeProvider>) {
        self.dynamic_providers.push(provider);
    }

    pub fn with_dynamic_provider(mut self, provider: Arc<dyn RuntimeProvider>) -> Self {
        self.register_dynamic(provider);
        self
    }

    pub fn get_provider(&self, action_key: &RuntimeActionKey) -> Option<Arc<dyn RuntimeProvider>> {
        self.providers.get(action_key).cloned()
    }

    pub fn action_descriptors(&self) -> Vec<RuntimeActionDescriptor> {
        self.providers
            .values()
            .map(|provider| provider.describe_action())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_surface_for_unchecked(&self, context: RuntimeContext) -> RuntimeSurface {
        let actions = self
            .providers
            .values()
            .filter(|provider| provider.action_kind() == context.action_kind())
            .map(|provider| provider.describe_action())
            .collect();
        RuntimeSurface { context, actions }
    }

    pub fn surface_for_actor(
        &self,
        actor: RuntimeActor,
        context: RuntimeContext,
    ) -> Result<RuntimeSurface, RuntimeInvocationError> {
        let trace = None;
        validate_actor_context(context.action_kind(), &actor, &context, trace)?;

        let actions = self
            .providers
            .values()
            .filter(|provider| provider.action_kind() == context.action_kind())
            .map(|provider| provider.describe_action())
            .collect();

        Ok(RuntimeSurface { context, actions })
    }

    pub async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationResult, RuntimeInvocationError> {
        let trace = request.trace.clone();
        let action_key = request.action_key.clone();

        let provider = self
            .providers
            .get(&request.action_key)
            .cloned()
            .or_else(|| {
                self.dynamic_providers
                    .iter()
                    .find(|provider| provider.supports(&request.action_key, &request.context))
                    .cloned()
            })
            .ok_or_else(|| RuntimeInvocationError::ProviderUnavailable {
                action_key: request.action_key.clone(),
                trace: Some(Box::new(trace.clone())),
            })?;

        validate_request(provider.as_ref(), &request)?;

        if !provider.supports(&request.action_key, &request.context) {
            return Err(RuntimeInvocationError::capability_denied(
                format!(
                    "provider 不支持当前上下文中的 action: {}",
                    request.action_key
                ),
                Some(trace),
            ));
        }

        let output = provider
            .invoke(request)
            .await
            .map_err(|error| error.with_trace_if_missing(trace.clone()))?;

        Ok(RuntimeInvocationResult {
            action_key,
            trace,
            output,
        })
    }
}

impl Default for RuntimeGateway {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_request(
    provider: &dyn RuntimeProvider,
    request: &RuntimeInvocationRequest,
) -> Result<(), RuntimeInvocationError> {
    validate_actor_context(
        provider.action_kind(),
        &request.actor,
        &request.context,
        Some(request.trace.clone()),
    )
}

fn validate_actor_context(
    action_kind: RuntimeActionKind,
    actor: &RuntimeActor,
    context: &RuntimeContext,
    trace: Option<super::types::RuntimeTrace>,
) -> Result<(), RuntimeInvocationError> {
    match action_kind {
        RuntimeActionKind::SessionRuntime => {
            validate_session_runtime_actor_context(actor, context, trace)
        }
        RuntimeActionKind::Setup => validate_setup_actor_context(actor, context, trace),
    }
}

fn validate_session_runtime_actor_context(
    actor: &RuntimeActor,
    context: &RuntimeContext,
    trace: Option<super::types::RuntimeTrace>,
) -> Result<(), RuntimeInvocationError> {
    let Some(context_session_id) = context.session_id() else {
        return Err(RuntimeInvocationError::invalid_request(
            "Session Runtime Action 必须绑定 Session context",
            trace,
        ));
    };
    if context_session_id.is_empty() {
        return Err(RuntimeInvocationError::invalid_request(
            "Session Runtime Action 的 session_id 不能为空",
            trace,
        ));
    }

    let Some(actor_session_id) = actor.session_id() else {
        return Err(RuntimeInvocationError::capability_denied(
            "Session Runtime Action 只能由绑定同一 Session 的 actor 调用",
            trace,
        ));
    };
    if actor_session_id != context_session_id {
        return Err(RuntimeInvocationError::capability_denied(
            "Runtime actor 与 Runtime context 的 session_id 不一致",
            trace,
        ));
    }

    Ok(())
}

fn validate_setup_actor_context(
    actor: &RuntimeActor,
    context: &RuntimeContext,
    trace: Option<super::types::RuntimeTrace>,
) -> Result<(), RuntimeInvocationError> {
    if !matches!(context, RuntimeContext::Setup { .. }) {
        return Err(RuntimeInvocationError::invalid_request(
            "Setup Action 必须使用 Setup context",
            trace,
        ));
    }
    if !actor.is_setup_actor() {
        return Err(RuntimeInvocationError::capability_denied(
            "Setup Action 只允许平台 UI 或 environment setup actor 调用",
            trace,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::runtime_gateway::{
        RuntimeActor, RuntimeInvocationErrorKind, RuntimeInvocationOutput,
        RuntimeInvocationRequest, RuntimeProvider,
    };

    struct FakeProvider {
        action_key: RuntimeActionKey,
        action_kind: RuntimeActionKind,
        fail: bool,
    }

    impl FakeProvider {
        fn new(action_key: &str, action_kind: RuntimeActionKind) -> Self {
            Self {
                action_key: RuntimeActionKey::parse(action_key).expect("valid action key"),
                action_kind,
                fail: false,
            }
        }

        fn failing(action_key: &str, action_kind: RuntimeActionKind) -> Self {
            Self {
                action_key: RuntimeActionKey::parse(action_key).expect("valid action key"),
                action_kind,
                fail: true,
            }
        }
    }

    #[async_trait]
    impl RuntimeProvider for FakeProvider {
        fn action_key(&self) -> &RuntimeActionKey {
            &self.action_key
        }

        fn action_kind(&self) -> RuntimeActionKind {
            self.action_kind
        }

        async fn invoke(
            &self,
            _request: RuntimeInvocationRequest,
        ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
            if self.fail {
                return Err(RuntimeInvocationError::provider_failed("boom", None));
            }
            Ok(RuntimeInvocationOutput::new(json!({ "ok": true })))
        }
    }

    fn session_request(action_key: &str) -> RuntimeInvocationRequest {
        RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(action_key).expect("valid action key"),
            RuntimeActor::AgentSession {
                session_id: "session-1".to_string(),
                agent_id: None,
            },
            RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: None,
                workspace_id: None,
            },
            json!({}),
        )
    }

    fn setup_request(action_key: &str) -> RuntimeInvocationRequest {
        RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(action_key).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("local".to_string()),
                root_ref: None,
            },
            json!({}),
        )
    }

    #[tokio::test]
    async fn unregistered_action_is_rejected() {
        let gateway = RuntimeGateway::new();

        let err = gateway
            .invoke(session_request("session.echo"))
            .await
            .expect_err("unregistered action should fail");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::ProviderUnavailable);
        assert!(err.trace().is_some(), "trace should be preserved");
    }

    #[tokio::test]
    async fn session_action_requires_session_context() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::new(
            "session.echo",
            RuntimeActionKind::SessionRuntime,
        )));
        let mut request = session_request("session.echo");
        request.context = RuntimeContext::Setup {
            project_id: None,
            workspace_id: None,
            backend_id: None,
            root_ref: None,
        };

        let err = gateway
            .invoke(request)
            .await
            .expect_err("session action without session context should fail");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::InvalidRequest);
    }

    #[tokio::test]
    async fn setup_action_rejects_session_actor() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::new(
            "workspace.detect",
            RuntimeActionKind::Setup,
        )));
        let mut request = setup_request("workspace.detect");
        request.actor = RuntimeActor::AgentSession {
            session_id: "session-1".to_string(),
            agent_id: None,
        };

        let err = gateway
            .invoke(request)
            .await
            .expect_err("session actor should not call setup action");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn provider_error_keeps_invocation_trace() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::failing(
            "session.echo",
            RuntimeActionKind::SessionRuntime,
        )));
        let request = session_request("session.echo");
        let expected_trace_id = request.trace.trace_id.clone();
        let expected_invocation_id = request.trace.invocation_id.clone();

        let err = gateway
            .invoke(request)
            .await
            .expect_err("provider failure should bubble up");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::ProviderFailed);
        let trace = err.trace().expect("trace should be attached");
        assert_eq!(trace.trace_id, expected_trace_id);
        assert_eq!(trace.invocation_id, expected_invocation_id);
    }

    #[tokio::test]
    async fn registered_session_action_invokes_provider() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::new(
            "session.echo",
            RuntimeActionKind::SessionRuntime,
        )));

        let result = gateway
            .invoke(session_request("session.echo"))
            .await
            .expect("registered action should invoke");

        assert_eq!(result.output.output, json!({ "ok": true }));
        assert_eq!(result.action_key.as_str(), "session.echo");
    }

    #[test]
    fn actor_aware_surface_returns_session_actions_for_bound_session_actor() {
        let gateway = RuntimeGateway::new()
            .with_provider(Arc::new(FakeProvider::new(
                "session.echo",
                RuntimeActionKind::SessionRuntime,
            )))
            .with_provider(Arc::new(FakeProvider::new(
                "workspace.detect",
                RuntimeActionKind::Setup,
            )));

        let surface = gateway
            .surface_for_actor(
                RuntimeActor::AgentSession {
                    session_id: "session-1".to_string(),
                    agent_id: None,
                },
                RuntimeContext::Session {
                    session_id: "session-1".to_string(),
                    project_id: None,
                    workspace_id: None,
                },
            )
            .expect("session actor should see session runtime actions");
        let keys = surface
            .actions
            .iter()
            .map(|action| action.action_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["session.echo"]);
    }

    #[test]
    fn actor_aware_surface_rejects_mismatched_session_actor() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::new(
            "session.echo",
            RuntimeActionKind::SessionRuntime,
        )));

        let err = gateway
            .surface_for_actor(
                RuntimeActor::AgentSession {
                    session_id: "session-a".to_string(),
                    agent_id: None,
                },
                RuntimeContext::Session {
                    session_id: "session-b".to_string(),
                    project_id: None,
                    workspace_id: None,
                },
            )
            .expect_err("mismatched session actor should not receive a surface");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[test]
    fn actor_aware_surface_returns_setup_actions_for_setup_actor() {
        let gateway = RuntimeGateway::new()
            .with_provider(Arc::new(FakeProvider::new(
                "session.echo",
                RuntimeActionKind::SessionRuntime,
            )))
            .with_provider(Arc::new(FakeProvider::new(
                "workspace.detect",
                RuntimeActionKind::Setup,
            )));

        let surface = gateway
            .surface_for_actor(
                RuntimeActor::PlatformUser { user_id: None },
                RuntimeContext::Setup {
                    project_id: None,
                    workspace_id: None,
                    backend_id: None,
                    root_ref: None,
                },
            )
            .expect("setup actor should see setup actions");
        let keys = surface
            .actions
            .iter()
            .map(|action| action.action_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["workspace.detect"]);
    }

    #[test]
    fn actor_aware_surface_rejects_session_actor_for_setup_context() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(FakeProvider::new(
            "workspace.detect",
            RuntimeActionKind::Setup,
        )));

        let err = gateway
            .surface_for_actor(
                RuntimeActor::AgentSession {
                    session_id: "session-1".to_string(),
                    agent_id: None,
                },
                RuntimeContext::Setup {
                    project_id: None,
                    workspace_id: None,
                    backend_id: None,
                    root_ref: None,
                },
            )
            .expect_err("session actor should not receive setup surface");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[test]
    fn debug_surface_for_unchecked_filters_only_by_context_kind() {
        let gateway = RuntimeGateway::new()
            .with_provider(Arc::new(FakeProvider::new(
                "session.echo",
                RuntimeActionKind::SessionRuntime,
            )))
            .with_provider(Arc::new(FakeProvider::new(
                "workspace.detect",
                RuntimeActionKind::Setup,
            )));

        let surface = gateway.debug_surface_for_unchecked(RuntimeContext::Session {
            session_id: String::new(),
            project_id: None,
            workspace_id: None,
        });
        let keys = surface
            .actions
            .iter()
            .map(|action| action.action_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["session.echo"]);
    }
}
