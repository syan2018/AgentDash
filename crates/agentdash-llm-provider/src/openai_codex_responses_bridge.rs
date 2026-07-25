/// ChatGPT 账号登录版 Codex Responses Bridge
///
/// 使用 ChatGPT OAuth access token 直连 `chatgpt.com/backend-api/codex/responses`。
/// Responses 协议转换与流解析共享 OpenAI Responses bridge 的实现。
use std::pin::Pin;

use async_trait::async_trait;
use base64::Engine;

use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, LlmBridge, ProviderErrorClassification, StreamChunk,
};

use super::openai_responses_common::{
    ResponsesRequestOptions, build_responses_request_body, process_responses_stream,
};

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

pub struct OpenAiCodexResponsesBridge {
    client: reqwest::Client,
    base_url: String,
    credential: String,
    model_id: String,
}

impl OpenAiCodexResponsesBridge {
    pub fn new(credential: &str, model_id: &str, base_url: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url
                .unwrap_or(DEFAULT_CODEX_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            credential: credential.to_string(),
            model_id: model_id.to_string(),
        }
    }
}

#[async_trait]
impl LlmBridge for OpenAiCodexResponsesBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let client = self.client.clone();
        let url = resolve_codex_url(&self.base_url);
        let credential = self.credential.clone();
        let model_id = self.model_id.clone();

        super::spawn_bridge_stream(move |tx| async move {
            run_stream(&client, &url, &credential, &model_id, &request, &tx).await
        })
    }
}

async fn run_stream(
    client: &reqwest::Client,
    url: &str,
    credential: &str,
    model_id: &str,
    request: &BridgeRequest,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let auth = resolve_codex_auth(client, credential).await?;
    let body = build_request_body(model_id, request);

    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", auth.access_token))
        .header("chatgpt-account-id", auth.account_id)
        .header("originator", "agentdash")
        .header("OpenAI-Beta", "responses=experimental")
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .header("User-Agent", "agentdash")
        .body(
            serde_json::to_string(&body)
                .map_err(|error| BridgeError::RequestBuildFailed(error.to_string()))?,
        )
        .send()
        .await
        .map_err(|error| super::provider_transport_error("Codex HTTP 请求失败", error))?;

    let response = check_codex_api_response(response, model_id).await?;

    process_responses_stream(response, "读取 Codex 响应流失败", tx).await
}

async fn check_codex_api_response(
    response: reqwest::Response,
    model_id: &str,
) -> Result<reqwest::Response, BridgeError> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let headers = response.headers().clone();
    let body_text = response.text().await.unwrap_or_default();
    let display_body = friendly_codex_error(&body_text).unwrap_or_else(|| body_text.clone());
    let classification = classify_codex_api_error(status, &headers, &body_text);
    Err(BridgeError::provider(
        format!("Codex API 返回 {status}: {display_body}"),
        classification,
    )
    .with_provider_context("Codex API", model_id))
}

fn classify_codex_api_error(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
) -> ProviderErrorClassification {
    let classification = super::classify_http_provider_failure(status, headers, body);
    if !is_codex_usage_or_rate_limit_error(body) {
        return classification;
    }

    let mut fatal = ProviderErrorClassification::fatal().with_http_status(status.as_u16());
    if let Some(code) = classification.provider_code.clone() {
        fatal = fatal.with_provider_code(code);
    }
    if let Some(retry_after_ms) = classification.retry_after_ms {
        fatal = fatal.with_retry_after_ms(retry_after_ms);
    }
    fatal
}

fn resolve_codex_url(base_url: &str) -> String {
    let normalized = base_url.trim_end_matches('/');
    if normalized.ends_with("/codex/responses") {
        normalized.to_string()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    }
}

fn build_request_body(model_id: &str, request: &BridgeRequest) -> serde_json::Value {
    build_responses_request_body(model_id, request, ResponsesRequestOptions::codex())
}

struct CodexAuth {
    access_token: String,
    account_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct StoredCodexCredential {
    #[serde(default)]
    access: String,
    #[serde(default)]
    refresh: String,
    #[serde(default)]
    expires: Option<i64>,
    #[serde(default, alias = "accountId")]
    account_id: Option<String>,
}

async fn resolve_codex_auth(client: &reqwest::Client, raw: &str) -> Result<CodexAuth, BridgeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(super::provider_fatal_error(
            "OpenAI Codex provider 未配置登录凭据".to_string(),
            "codex_auth_missing",
        ));
    }

    if trimmed.starts_with('{') {
        let credential: StoredCodexCredential = serde_json::from_str(trimmed).map_err(|error| {
            BridgeError::RequestBuildFailed(format!("解析 Codex OAuth 凭据失败: {error}"))
        })?;
        let credential = refresh_if_needed(client, credential).await?;
        let account_id = credential
            .account_id
            .or_else(|| extract_account_id(&credential.access).ok())
            .ok_or_else(|| {
                BridgeError::RequestBuildFailed(
                    "Codex OAuth 凭据缺少 accountId，且 access token 中无法解析".to_string(),
                )
            })?;
        return Ok(CodexAuth {
            access_token: credential.access,
            account_id,
        });
    }

    let account_id = extract_account_id(trimmed).map_err(BridgeError::RequestBuildFailed)?;
    Ok(CodexAuth {
        access_token: trimmed.to_string(),
        account_id,
    })
}

async fn refresh_if_needed(
    client: &reqwest::Client,
    credential: StoredCodexCredential,
) -> Result<StoredCodexCredential, BridgeError> {
    let Some(expires) = credential.expires else {
        return Ok(credential);
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    if now_ms + 60_000 < expires {
        return Ok(credential);
    }
    if credential.refresh.trim().is_empty() {
        return Err(super::provider_fatal_error(
            "Codex OAuth access token 已过期，且没有 refresh token".to_string(),
            "codex_refresh_missing",
        ));
    }

    let response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", credential.refresh.as_str()),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|error| super::provider_transport_error("刷新 Codex token 失败", error))?;

    let response = super::check_http_response(response, "刷新 Codex token").await?;

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: String,
        expires_in: i64,
    }

    let token: TokenResponse = response.json().await.map_err(|error| {
        super::provider_fatal_error(
            format!("解析 Codex token 失败: {error}"),
            "codex_token_parse_failed",
        )
    })?;

    Ok(StoredCodexCredential {
        access: token.access_token,
        refresh: token.refresh_token,
        expires: Some(now_ms + token.expires_in * 1000),
        account_id: None,
    })
}

fn extract_account_id(token: &str) -> Result<String, String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| "Codex access token 不是合法 JWT".to_string())?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .map_err(|error| format!("解码 Codex access token 失败: {error}"))?;
    let json: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|error| format!("解析 Codex access token payload 失败: {error}"))?;
    json.get(JWT_CLAIM_PATH)
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Codex access token 中缺少 chatgpt_account_id".to_string())
}

fn friendly_codex_error(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let error = parsed.get("error")?;
    let code = error
        .get("code")
        .or_else(|| error.get("type"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if code.contains("usage_limit") || code.contains("rate_limit") {
        let plan = error
            .get("plan_type")
            .and_then(|value| value.as_str())
            .map(|value| format!("（{} plan）", value.to_ascii_lowercase()))
            .unwrap_or_default();
        return Some(format!("已达到 ChatGPT Codex 使用限制{plan}"));
    }
    error
        .get("message")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn is_codex_usage_or_rate_limit_error(raw: &str) -> bool {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };
    let Some(error) = parsed.get("error") else {
        return false;
    };
    let code = error
        .get("code")
        .or_else(|| error.get("type"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    code.contains("usage_limit") || code.contains("rate_limit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_bridge_test_support::{
        SYSTEM_PROMPT, TOOL_DESCRIPTION, TOOL_NAME, USER_PROMPT,
        assert_prompt_lanes_exclude_tool_metadata, bridge_request, serialized_body,
        tool_parameters,
    };
    use agentdash_agent::bridge::ProviderErrorKind;
    use agentdash_agent::types::{AgentMessage, ToolDefinition};

    fn jwt_with_account(account_id: &str) -> String {
        let payload = serde_json::json!({
            JWT_CLAIM_PATH: {
                "chatgpt_account_id": account_id,
            }
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn extracts_account_id_from_codex_access_token() {
        let token = jwt_with_account("acct_test");
        assert_eq!(extract_account_id(&token).unwrap(), "acct_test");
    }

    #[test]
    fn resolves_codex_endpoint_variants() {
        assert_eq!(
            resolve_codex_url("https://chatgpt.com/backend-api"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            resolve_codex_url("https://chatgpt.com/backend-api/codex"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[test]
    fn codex_responses_wire_body_keeps_tool_contract_structured_and_prompt_lanes_clean() {
        let body = serialized_body(build_request_body("gpt-5.5", &bridge_request()));

        assert_eq!(body["instructions"], SYSTEM_PROMPT);
        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"][0]["text"], USER_PROMPT);

        let tool = &body["tools"][0];
        assert_eq!(tool["name"], TOOL_NAME);
        assert_eq!(tool["description"], TOOL_DESCRIPTION);
        assert_eq!(tool["parameters"], tool_parameters());
        assert_eq!(tool["strict"], false);

        assert_prompt_lanes_exclude_tool_metadata(&[&body["instructions"], &body["input"]]);
    }

    #[test]
    fn builds_codex_body_with_instructions_not_system_input() {
        let body = build_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: Some("system".to_string()),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![],
                thinking_level: None,
            },
        );
        assert_eq!(body["instructions"], "system");
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
    }

    #[test]
    fn builds_codex_tool_with_boolean_strict_false() {
        let body = build_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: Some("system".to_string()),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![ToolDefinition {
                    name: "demo_tool".to_string(),
                    description: "Demo tool".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" }
                        },
                        "required": ["value"],
                        "additionalProperties": false
                    }),
                }],
                thinking_level: None,
            },
        );

        let tool = body["tools"]
            .as_array()
            .and_then(|tools| tools.first())
            .expect("tool should be serialized");
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["name"], "demo_tool");
        assert_eq!(tool["strict"], false);
        assert_eq!(tool["parameters"]["properties"]["value"]["type"], "string");
    }

    #[test]
    fn codex_rate_limit_friendly_error_stays_fatal() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "5".parse().unwrap());
        let classification = classify_codex_api_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            &headers,
            r#"{"error":{"code":"rate_limit_exceeded","plan_type":"Plus"}}"#,
        );

        assert_eq!(classification.kind, ProviderErrorKind::Fatal);
        assert_eq!(
            friendly_codex_error(r#"{"error":{"code":"rate_limit_exceeded","plan_type":"Plus"}}"#)
                .as_deref(),
            Some("已达到 ChatGPT Codex 使用限制（plus plan）")
        );
        assert_eq!(
            classification.provider_code.as_deref(),
            Some("rate_limit_exceeded")
        );
        assert_eq!(classification.retry_after_ms, Some(5_000));
    }
}
