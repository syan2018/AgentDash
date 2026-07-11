use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverThreadId, DriverTurnId, HookPoint, RuntimeBindingId,
    RuntimeDriverGeneration, RuntimeItemId, RuntimeThreadId, RuntimeTurnId,
};
use agentdash_integration_api::{
    AgentRuntimeHookCallback, AuthIdentity, DriverHookBinding, DriverHookDecision,
    DriverHookInvocation,
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
    task::JoinHandle,
};

#[derive(Debug, Error)]
pub(crate) enum HookBridgeError {
    #[error("could not bind native hook callback bridge: {0}")]
    Bind(#[from] std::io::Error),
}

pub(crate) struct HookBridgeLease {
    pub endpoint: String,
    task: JoinHandle<()>,
    source_thread_id: Arc<RwLock<Option<DriverThreadId>>>,
}

impl HookBridgeLease {
    pub async fn bind_source_thread(&self, source_thread_id: DriverThreadId) {
        *self.source_thread_id.write().await = Some(source_thread_id);
    }
}

impl Drop for HookBridgeLease {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) async fn start_hook_bridge(
    callback: Arc<dyn AgentRuntimeHookCallback>,
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    bindings: Vec<DriverHookBinding>,
    runtime_thread_id: RuntimeThreadId,
    authorization_identity: Option<AuthIdentity>,
) -> Result<HookBridgeLease, HookBridgeError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let expected_path = format!("/hook/{token}");
    let endpoint = format!("http://{address}{expected_path}");
    let source_thread_id = Arc::new(RwLock::new(None));
    let decisions = Arc::new(Mutex::new(
        std::collections::BTreeMap::<String, Value>::new(),
    ));
    let context = Arc::new(HookEvaluationContext {
        callback,
        binding_id,
        generation,
        bindings,
        runtime_thread_id,
        authorization_identity,
        source_thread_id: source_thread_id.clone(),
        decisions,
    });
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let expected_path = expected_path.clone();
            let context = context.clone();
            tokio::spawn(async move {
                let _ = serve(stream, &expected_path, context).await;
            });
        }
    });
    Ok(HookBridgeLease {
        endpoint,
        task,
        source_thread_id,
    })
}

struct HookEvaluationContext {
    callback: Arc<dyn AgentRuntimeHookCallback>,
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    bindings: Vec<DriverHookBinding>,
    runtime_thread_id: RuntimeThreadId,
    authorization_identity: Option<AuthIdentity>,
    source_thread_id: Arc<RwLock<Option<DriverThreadId>>>,
    decisions: Arc<Mutex<std::collections::BTreeMap<String, Value>>>,
}

async fn serve(
    mut stream: TcpStream,
    expected_path: &str,
    context: Arc<HookEvaluationContext>,
) -> Result<(), std::io::Error> {
    let request = read_request(&mut stream).await?;
    let (status, body) = match request {
        Some(request) if request.path == expected_path => {
            match evaluate(context.as_ref(), request.body).await {
                Ok(value) => (200, value.to_string()),
                Err(message) => (500, json!({ "error": message }).to_string()),
            }
        }
        Some(_) => (404, json!({ "error": "not found" }).to_string()),
        None => (400, json!({ "error": "invalid request" }).to_string()),
    };
    let reason = if status == 200 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await
}

struct HttpRequest {
    path: String,
    body: Value,
}

async fn read_request(stream: &mut TcpStream) -> Result<Option<HttpRequest>, std::io::Error> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > 1024 * 1024 {
            return Ok(None);
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = headers.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    if request_parts.next() != Some("POST") {
        return Ok(None);
    }
    let path = request_parts.next().unwrap_or_default().to_string();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() - header_end < content_length {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = serde_json::from_slice(&bytes[header_end..header_end + content_length]).ok();
    Ok(body.map(|body| HttpRequest { path, body }))
}

async fn evaluate(context: &HookEvaluationContext, payload: Value) -> Result<Value, String> {
    let key = decision_key(&payload);
    let mut decisions = context.decisions.lock().await;
    if let Some(decision) = decisions.get(&key) {
        return Ok(decision.clone());
    }
    let decision = evaluate_uncached(context, payload).await?;
    if decisions.len() >= 1024
        && let Some(oldest) = decisions.keys().next().cloned()
    {
        decisions.remove(&oldest);
    }
    decisions.insert(key, decision.clone());
    Ok(decision)
}

async fn evaluate_uncached(
    context: &HookEvaluationContext,
    payload: Value,
) -> Result<Value, String> {
    let event_name = payload
        .get("hook_event_name")
        .or_else(|| payload.get("hookEventName"))
        .and_then(Value::as_str)
        .ok_or_else(|| "hook payload misses event name".to_string())?;
    let point =
        point(event_name).ok_or_else(|| format!("unsupported Codex hook event {event_name}"))?;
    let selected = context
        .bindings
        .iter()
        .filter(|binding| binding.point == point)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(json!({ "continue": true }));
    }
    let source_thread_id = context
        .source_thread_id
        .read()
        .await
        .clone()
        .ok_or_else(|| "Codex source thread is not bound yet".to_string())?;
    let source_turn_id =
        string(&payload, &["turn_id", "turnId"]).and_then(|value| DriverTurnId::new(value).ok());
    let source_item_id = string(&payload, &["tool_use_id", "item_id", "itemId"])
        .and_then(|value| DriverItemId::new(value).ok());
    let mut merged = Value::Object(serde_json::Map::new());
    for binding in selected {
        let decision = context
            .callback
            .execute(DriverHookInvocation {
                thread_id: context.runtime_thread_id.clone(),
                turn_id: source_turn_id
                    .as_ref()
                    .and_then(|value| RuntimeTurnId::new(value.to_string()).ok()),
                item_id: source_item_id
                    .as_ref()
                    .and_then(|value| RuntimeItemId::new(value.to_string()).ok()),
                binding_id: context.binding_id.clone(),
                generation: context.generation,
                source_thread_id: source_thread_id.clone(),
                source_turn_id: source_turn_id.clone(),
                source_item_id: source_item_id.clone(),
                definition_id: binding.definition_id.clone(),
                point,
                payload: payload.clone(),
                authorization_identity: context.authorization_identity.clone(),
            })
            .await
            .map_err(|error| error.to_string())?;
        match decision {
            DriverHookDecision::Continue { payload } => {
                merge(&mut merged, continue_output(point, &payload))
            }
            DriverHookDecision::Block { reason } => return Ok(block_output(point, reason)),
            DriverHookDecision::InteractionRequired { reason, .. } => {
                return Ok(block_output(point, reason));
            }
        }
    }
    if merged.as_object().is_some_and(|map| map.is_empty()) {
        Ok(json!({ "continue": true }))
    } else {
        Ok(merged)
    }
}

fn decision_key(payload: &Value) -> String {
    for key in ["hook_run_id", "hookRunId", "run_id", "runId"] {
        if let Some(value) = payload.get(key).and_then(Value::as_str) {
            return format!("run:{value}");
        }
    }
    format!("payload:{payload}")
}

fn block_output(point: HookPoint, reason: String) -> Value {
    match point {
        HookPoint::BeforeTool => json!({
            "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason }
        }),
        HookPoint::BeforeContextCompact => {
            json!({ "continue": false, "stopReason": reason })
        }
        HookPoint::BeforeStop => json!({ "continue": false, "stopReason": reason }),
        _ => json!({ "decision": "block", "reason": reason }),
    }
}

fn continue_output(point: HookPoint, payload: &Value) -> Value {
    let additional_context = payload.get("additional_context").and_then(Value::as_str);
    match point {
        HookPoint::BeforeTool => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": payload.get("rewritten_input").cloned(),
                "additionalContext": additional_context
            }
        }),
        HookPoint::AfterTool => json!({
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "updatedMCPToolOutput": payload.get("rewritten_result").cloned(),
                "additionalContext": additional_context
            }
        }),
        HookPoint::BeforeStop => payload
            .get("continue_turn")
            .and_then(Value::as_str)
            .map(|reason| json!({ "decision": "block", "reason": reason }))
            .unwrap_or_else(|| json!({ "continue": true })),
        _ => additional_context
            .map(|context| json!({ "systemMessage": context }))
            .unwrap_or_else(|| json!({ "continue": true })),
    }
}

fn point(name: &str) -> Option<HookPoint> {
    Some(match name {
        "PreToolUse" => HookPoint::BeforeTool,
        "PermissionRequest" => HookPoint::BeforeTool,
        "PostToolUse" => HookPoint::AfterTool,
        "PreCompact" => HookPoint::BeforeContextCompact,
        "PostCompact" => HookPoint::AfterContextCompact,
        "SessionStart" => HookPoint::AfterThreadStart,
        "UserPromptSubmit" => HookPoint::BeforeTurn,
        "Stop" => HookPoint::BeforeStop,
        _ => return None,
    })
}

fn string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn merge(target: &mut Value, incoming: Value) {
    match (target, incoming) {
        (Value::Object(target), Value::Object(incoming)) => target.extend(incoming),
        (target, incoming) => *target = incoming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime_contract::{
        HookAction, HookDefinitionId, HookFailurePolicy, SemanticStrength,
    };
    use agentdash_integration_api::{DriverHookCallbackError, DriverHookInvocation};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct BlockingCallback(AtomicUsize);

    #[async_trait]
    impl AgentRuntimeHookCallback for BlockingCallback {
        async fn execute(
            &self,
            _request: DriverHookInvocation,
        ) -> Result<DriverHookDecision, DriverHookCallbackError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(DriverHookDecision::Block {
                reason: "policy denied".to_string(),
            })
        }
    }

    #[test]
    fn pre_tool_block_uses_codex_native_synchronous_decision_shape() {
        let output = block_output(HookPoint::BeforeTool, "denied".to_string());
        assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            output["hookSpecificOutput"]["permissionDecisionReason"],
            "denied"
        );
    }

    #[test]
    fn continue_decisions_translate_canonical_rewrites_without_vendor_payload_leak() {
        let pre = continue_output(
            HookPoint::BeforeTool,
            &json!({
                "rewritten_input": { "path": "safe" }, "vendorDecision": "ignored"
            }),
        );
        assert_eq!(pre["hookSpecificOutput"]["updatedInput"]["path"], "safe");
        assert!(pre.get("vendorDecision").is_none());
        let stop = continue_output(
            HookPoint::BeforeStop,
            &json!({ "continue_turn": "run tests" }),
        );
        assert_eq!(stop["decision"], "block");
        assert_eq!(stop["reason"], "run tests");
    }

    #[tokio::test]
    async fn native_bridge_correlates_and_returns_synchronous_decision() {
        let callback = Arc::new(BlockingCallback(AtomicUsize::new(0)));
        let lease = start_hook_bridge(
            callback.clone(),
            RuntimeBindingId::new("binding-1").unwrap(),
            RuntimeDriverGeneration(3),
            vec![DriverHookBinding {
                definition_id: HookDefinitionId::new("hook-1").unwrap(),
                point: HookPoint::BeforeTool,
                actions: vec![HookAction::Block],
                strength: SemanticStrength::ExactSynchronous,
                failure_policy: HookFailurePolicy::FailClosed,
                required: true,
            }],
            RuntimeThreadId::new("runtime-thread-1").unwrap(),
            None,
        )
        .await
        .expect("bridge");
        lease
            .bind_source_thread(DriverThreadId::new("thread-1").unwrap())
            .await;
        let endpoint = lease.endpoint.strip_prefix("http://").unwrap();
        let (address, path) = endpoint.split_once('/').unwrap();
        let body = json!({
            "hook_event_name": "PreToolUse", "thread_id": "thread-1",
            "turn_id": "turn-1", "tool_use_id": "item-1", "hook_run_id": "run-1"
        })
        .to_string();
        let mut stream = TcpStream::connect(address).await.expect("connect");
        stream
            .write_all(
                format!(
                    "POST /{path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).await.expect("read");
        assert!(response.contains("200 OK"));
        assert!(response.contains("permissionDecisionReason"));
        assert!(response.contains("policy denied"));

        let mut replay = TcpStream::connect(address).await.expect("replay connect");
        replay
            .write_all(
                format!(
                    "POST /{path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .expect("replay write");
        let mut replay_response = String::new();
        replay
            .read_to_string(&mut replay_response)
            .await
            .expect("replay read");
        assert!(replay_response.contains("policy denied"));
        assert_eq!(callback.0.load(Ordering::SeqCst), 1);
    }
}
