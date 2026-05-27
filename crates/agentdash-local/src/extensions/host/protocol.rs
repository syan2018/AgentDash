use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(super) struct RunnerRequest<'a> {
    pub kind: &'static str,
    pub id: &'a str,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct RunnerMessage {
    pub kind: String,
    pub id: Option<String>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub level: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RunnerHostApiResponse<'a> {
    pub kind: &'static str,
    pub id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<'a> RunnerHostApiResponse<'a> {
    pub fn result(id: &'a str, result: Value) -> Self {
        Self {
            kind: "host_api_response",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: &'a str, error: String) -> Self {
        Self {
            kind: "host_api_response",
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn golden_serializes_activate_request() {
        let request = RunnerRequest {
            kind: "request",
            id: "local-1",
            method: "activate",
            params: json!({
                "extension_key": "local-hello",
                "bundle_path": "cache/local-hello/dist/extension.js",
                "manifest": {
                    "manifest_version": "2",
                    "extension_id": "local-hello",
                },
            }),
        };

        assert_eq!(
            serde_json::to_string(&request).expect("serialize"),
            r#"{"kind":"request","id":"local-1","method":"activate","params":{"extension_key":"local-hello","bundle_path":"cache/local-hello/dist/extension.js","manifest":{"manifest_version":"2","extension_id":"local-hello"}}}"#,
        );
    }

    #[test]
    fn golden_serializes_invoke_requests() {
        let action = RunnerRequest {
            kind: "request",
            id: "local-2",
            method: "invoke_action",
            params: json!({
                "action_key": "local-hello.profile",
                "input": { "source": "panel" },
            }),
        };
        let channel = RunnerRequest {
            kind: "request",
            id: "local-3",
            method: "invoke_channel",
            params: json!({
                "channel_key": "local-hello.api",
                "method": "echo",
                "input": { "source": "panel" },
            }),
        };

        assert_eq!(
            serde_json::to_string(&action).expect("serialize action"),
            r#"{"kind":"request","id":"local-2","method":"invoke_action","params":{"action_key":"local-hello.profile","input":{"source":"panel"}}}"#,
        );
        assert_eq!(
            serde_json::to_string(&channel).expect("serialize channel"),
            r#"{"kind":"request","id":"local-3","method":"invoke_channel","params":{"channel_key":"local-hello.api","method":"echo","input":{"source":"panel"}}}"#,
        );
    }

    #[test]
    fn golden_deserializes_host_api_request() {
        let message: RunnerMessage = serde_json::from_str(
            r#"{"kind":"host_api_request","id":"host-api-1","method":"local.get_profile","params":{"action_key":"local-hello.profile","extension_key":"local-hello"}}"#,
        )
        .expect("deserialize");

        assert_eq!(message.kind, "host_api_request");
        assert_eq!(message.id.as_deref(), Some("host-api-1"));
        assert_eq!(message.method.as_deref(), Some("local.get_profile"));
        assert_eq!(
            message.params,
            Some(json!({
                "action_key": "local-hello.profile",
                "extension_key": "local-hello",
            })),
        );
    }

    #[test]
    fn golden_serializes_host_api_response() {
        let ok = RunnerHostApiResponse::result("host-api-1", json!({ "username": "user" }));
        let err = RunnerHostApiResponse::error(
            "host-api-2",
            "extension host 权限拒绝: local.profile.read".to_string(),
        );

        assert_eq!(
            serde_json::to_string(&ok).expect("serialize ok"),
            r#"{"kind":"host_api_response","id":"host-api-1","result":{"username":"user"}}"#,
        );
        assert_eq!(
            serde_json::to_string(&err).expect("serialize error"),
            r#"{"kind":"host_api_response","id":"host-api-2","error":"extension host 权限拒绝: local.profile.read"}"#,
        );
    }
}
