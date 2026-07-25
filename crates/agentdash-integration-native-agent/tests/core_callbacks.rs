use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

use agentdash_agent::dash::{
    DashBeforeToolDecision, DashToolCall, DashToolCallbacks, DashToolResult,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentHookAction, AgentHookDecision,
    AgentHookDefinitionId, AgentHookInvocation, AgentHookPoint, AgentHookTiming,
    AgentHostCallbackError, AgentHostCallbackErrorCode, AgentHostCallbacks, AgentPayloadDigest,
    AgentProfileDigest, AgentSourceCoordinate, AgentSurfaceContributionPayload, AgentSurfaceDigest,
    AgentSurfaceRevision, AgentSurfaceRoute, AgentSurfaceSemanticFacet, AgentToolInvocation,
    AgentToolResult, BoundAgentSurface, BoundAgentSurfaceContribution, SemanticFidelity,
};
use agentdash_integration_native_agent::DashAgentCoreToolCallbacks;
use async_trait::async_trait;

#[derive(Default)]
struct RecordingCallbacks {
    tools: Mutex<Vec<AgentToolInvocation>>,
    hooks: Mutex<Vec<AgentHookInvocation>>,
    decisions: Mutex<VecDeque<AgentHookDecision>>,
    tool_output: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl AgentHostCallbacks for RecordingCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.tools.lock().unwrap().push(call);
        Ok(AgentToolResult::Completed {
            output: self
                .tool_output
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| serde_json::json!({"value": 7})),
        })
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        self.hooks.lock().unwrap().push(call);
        Ok(self
            .decisions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(AgentHookDecision::Allow))
    }
}

#[tokio::test]
async fn completed_vfs_result_preserves_typed_text_instead_of_serializing_the_envelope() {
    let host = Arc::new(RecordingCallbacks::default());
    *host.tool_output.lock().unwrap() = Some(serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": "file: main://README.md\n1 | first\n2 | second"
            }
        ],
        "is_error": false,
        "details": {
            "kind": "fs_read"
        }
    }));
    let callbacks = DashAgentCoreToolCallbacks::from_bound_surface(
        host,
        AgentCallbackRouteId::new("route-read-result").unwrap(),
        AgentBindingGeneration(1),
        AgentSourceCoordinate::new("source-read-result").unwrap(),
        5_000,
        &BoundAgentSurface {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("read-surface").unwrap(),
            offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
            contributions: Vec::new(),
        },
    );

    let result = callbacks
        .invoke(
            &agentdash_agent::dash::AgentTurnId::new("turn-read-result"),
            DashToolCall {
                call_id: "item-read-result".into(),
                name: "fs_read".into(),
                arguments: serde_json::json!({"path": "main://README.md"}),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        result.text(),
        "file: main://README.md\n1 | first\n2 | second"
    );
    assert_eq!(result.details, Some(serde_json::json!({"kind": "fs_read"})));
}

fn hook_surface() -> BoundAgentSurface {
    BoundAgentSurface {
        revision: AgentSurfaceRevision(1),
        digest: AgentSurfaceDigest::new("hook-surface").unwrap(),
        offer_profile_digest: AgentProfileDigest::new("dash-agent-profile-v1").unwrap(),
        contributions: vec![
            hook_contribution(
                "before",
                AgentHookPoint::BeforeTool,
                AgentHookTiming::Before,
                AgentHookAction::RewriteInput,
            ),
            hook_contribution(
                "after",
                AgentHookPoint::AfterTool,
                AgentHookTiming::After,
                AgentHookAction::RewriteResult,
            ),
        ],
    }
}

fn hook_contribution(
    id: &str,
    point: AgentHookPoint,
    timing: AgentHookTiming,
    action: AgentHookAction,
) -> BoundAgentSurfaceContribution {
    BoundAgentSurfaceContribution {
        key: format!("hook:{id}"),
        required: true,
        route: AgentSurfaceRoute::AgentNativeCallback,
        fidelity: SemanticFidelity::Exact,
        semantics: AgentSurfaceSemanticFacet::Hook(
            agentdash_agent_service_api::AgentHookSemanticFacet {
                point,
                timing,
                blocking: agentdash_agent_service_api::AgentHookBlockingSemantics::Blocking {
                    fidelity: SemanticFidelity::Exact,
                },
                mutations: std::collections::BTreeMap::new(),
                effects: std::collections::BTreeMap::new(),
            },
        ),
        payload: AgentSurfaceContributionPayload::Hook {
            definition_id: AgentHookDefinitionId::new(id).unwrap(),
            point,
            timing,
            actions: BTreeSet::from([AgentHookAction::AllowOrDeny, action]),
            deadline_ms: 2_000,
        },
        payload_digest: AgentPayloadDigest::new(format!("sha256:{id}")).unwrap(),
    }
}

#[tokio::test]
async fn before_and_after_hooks_materialize_once_and_rewrite_typed_values() {
    let host = Arc::new(RecordingCallbacks::default());
    host.decisions.lock().unwrap().extend([
        AgentHookDecision::ReplaceInput {
            input: serde_json::json!({"arguments": {"rewritten": true}}),
        },
        AgentHookDecision::ReplaceResult {
            result: serde_json::json!({"content": "rewritten-result", "is_error": false}),
        },
    ]);
    let callbacks = DashAgentCoreToolCallbacks::from_bound_surface(
        host.clone(),
        AgentCallbackRouteId::new("route-hooks").unwrap(),
        AgentBindingGeneration(9),
        AgentSourceCoordinate::new("source-hooks").unwrap(),
        5_000,
        &hook_surface(),
    );
    let turn = agentdash_agent::dash::AgentTurnId::new("turn-hooks");
    let call = DashToolCall {
        call_id: "item-hooks".into(),
        name: "read".into(),
        arguments: serde_json::json!({"original": true}),
    };
    let DashBeforeToolDecision::Invoke { call } = callbacks.before_tool(&turn, call).await.unwrap()
    else {
        panic!("before hook should rewrite and invoke");
    };
    assert_eq!(call.arguments, serde_json::json!({"rewritten": true}));
    let result = callbacks
        .after_tool(
            &turn,
            &call,
            DashToolResult {
                call_id: call.call_id.clone(),
                content: vec![agentdash_agent::ContentPart::text("original-result")],
                is_error: false,
                details: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.text(), "rewritten-result");
    let hooks = host.hooks.lock().unwrap();
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0].point, AgentHookPoint::BeforeTool);
    assert_eq!(hooks[1].point, AgentHookPoint::AfterTool);
    assert_eq!(hooks[0].meta.binding_generation, AgentBindingGeneration(9));
    assert_eq!(hooks[0].meta.source.as_str(), "source-hooks");
    assert_eq!(hooks[0].meta.turn_id.as_str(), "turn-hooks");
    assert_eq!(
        hooks[0].meta.item_id.as_ref().unwrap().as_str(),
        "item-hooks"
    );
    assert!(hooks[0].meta.deadline_at_ms > 0);
}

#[tokio::test]
async fn before_hook_deny_skips_tool_invocation() {
    let host = Arc::new(RecordingCallbacks::default());
    host.decisions
        .lock()
        .unwrap()
        .push_back(AgentHookDecision::Deny {
            reason: "policy".into(),
        });
    let callbacks = DashAgentCoreToolCallbacks::from_bound_surface(
        host.clone(),
        AgentCallbackRouteId::new("route-deny").unwrap(),
        AgentBindingGeneration(3),
        AgentSourceCoordinate::new("source-deny").unwrap(),
        5_000,
        &hook_surface(),
    );
    let decision = callbacks
        .before_tool(
            &agentdash_agent::dash::AgentTurnId::new("turn-deny"),
            DashToolCall {
                call_id: "item-deny".into(),
                name: "write".into(),
                arguments: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        decision,
        DashBeforeToolDecision::Deny {
            result: DashToolResult { is_error: true, .. }
        }
    ));
    assert!(host.tools.lock().unwrap().is_empty());
    assert_eq!(host.hooks.lock().unwrap().len(), 1);
}

struct RejectingHookCallbacks {
    code: AgentHostCallbackErrorCode,
}

#[async_trait]
impl AgentHostCallbacks for RejectingHookCallbacks {
    async fn invoke_tool(
        &self,
        _: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        unreachable!("hook rejection happens before tool invocation")
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Err(AgentHostCallbackError::new(
            self.code,
            format!("{:?}", self.code),
            false,
        ))
    }
}

#[tokio::test]
async fn stale_generation_and_deadline_failures_cross_the_typed_hook_boundary() {
    for (code, label) in [
        (
            AgentHostCallbackErrorCode::StaleBindingGeneration,
            "StaleBindingGeneration",
        ),
        (
            AgentHostCallbackErrorCode::DeadlineExceeded,
            "DeadlineExceeded",
        ),
    ] {
        let callbacks = DashAgentCoreToolCallbacks::from_bound_surface(
            Arc::new(RejectingHookCallbacks { code }),
            AgentCallbackRouteId::new("route-rejected").unwrap(),
            AgentBindingGeneration(4),
            AgentSourceCoordinate::new("source-rejected").unwrap(),
            1,
            &hook_surface(),
        );
        let error = callbacks
            .before_tool(
                &agentdash_agent::dash::AgentTurnId::new("turn-rejected"),
                DashToolCall {
                    call_id: "item-rejected".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains(label));
    }
}

#[tokio::test]
async fn core_tool_calls_only_cross_the_typed_host_callback_route() {
    let host = Arc::new(RecordingCallbacks::default());
    let callbacks = DashAgentCoreToolCallbacks::new(
        host.clone(),
        AgentCallbackRouteId::new("route-1").unwrap(),
        AgentBindingGeneration(7),
        AgentSourceCoordinate::new("source-1").unwrap(),
        9_000,
    );

    let result = callbacks
        .invoke(
            &agentdash_agent::dash::AgentTurnId::new("turn-1"),
            DashToolCall {
                call_id: "item-1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "README.md"}),
            },
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    let calls = host.tools.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].meta.binding_generation, AgentBindingGeneration(7));
    assert_eq!(calls[0].meta.turn_id.as_str(), "turn-1");
    assert_eq!(calls[0].meta.item_id.as_ref().unwrap().as_str(), "item-1");
    assert_eq!(calls[0].meta.effect_id.as_str(), "tool:item-1");
}
