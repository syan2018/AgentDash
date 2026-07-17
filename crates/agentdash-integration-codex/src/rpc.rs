use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcRequest<'a> {
    pub id: i64,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RpcNotification<'a> {
    pub method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum RpcInbound {
    Error(RpcError),
    Response(RpcResponse),
    Request(RpcServerRequest),
    Notification(RpcServerNotification),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcResponse {
    pub id: Value,
    pub result: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcError {
    pub id: Value,
    pub error: RpcErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcErrorBody {
    pub code: i64,
    pub message: String,
    #[allow(dead_code)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcServerRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcServerNotification {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub(crate) fn response(id: Value, result: Value) -> Value {
    serde_json::json!({ "id": id, "result": result })
}

pub(crate) fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    serde_json::json!({ "id": id, "error": { "code": code, "message": message.into() } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_server_request_from_notification() {
        let request: RpcInbound = serde_json::from_value(serde_json::json!({
            "id": 7,
            "method": "item/tool/requestUserInput",
            "params": { "threadId": "t" }
        }))
        .expect("request");
        assert!(matches!(request, RpcInbound::Request(_)));

        let notification: RpcInbound = serde_json::from_value(serde_json::json!({
            "method": "turn/started",
            "params": { "threadId": "t" }
        }))
        .expect("notification");
        assert!(matches!(notification, RpcInbound::Notification(_)));
    }
}
