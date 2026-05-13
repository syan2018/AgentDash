/// ChatGPT 账号登录版 Codex Responses Bridge
///
/// 对齐 pi-mono 的 `openai-codex-responses.ts` 主路径：使用 ChatGPT OAuth
/// access token 直连 `chatgpt.com/backend-api/codex/responses`，并按 Codex
/// 要求补充 `chatgpt-account-id` / `originator` / SSE headers。
use std::pin::Pin;

use async_trait::async_trait;
use base64::Engine;
use tokio_stream::wrappers::ReceiverStream;

use agentdash_agent::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, LlmBridge, StreamChunk, ToolCallDeltaContent,
};
use agentdash_agent::types::{
    AgentMessage, ContentPart, StopReason, TokenUsage, ToolCallInfo, now_millis,
};

use super::sse::SseParser;

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
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        let client = self.client.clone();
        let url = resolve_codex_url(&self.base_url);
        let credential = self.credential.clone();
        let model_id = self.model_id.clone();

        tokio::spawn(async move {
            if let Err(e) = run_stream(&client, &url, &credential, &model_id, &request, &tx).await {
                let _ = tx.send(StreamChunk::Error(e)).await;
            }
        });

        Box::pin(ReceiverStream::new(rx))
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
                .map_err(|e| BridgeError::RequestBuildFailed(e.to_string()))?,
        )
        .send()
        .await
        .map_err(|e| BridgeError::CompletionFailed(format!("Codex HTTP 请求失败: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(BridgeError::CompletionFailed(format!(
            "Codex API 返回 {status}: {}",
            friendly_codex_error(&body_text).unwrap_or(body_text)
        )));
    }

    let mut parser = SseParser::new();
    let mut state = StreamState::default();
    let mut response = response;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| BridgeError::CompletionFailed(format!("读取 Codex 响应流失败: {e}")))?
    {
        let text = String::from_utf8_lossy(&chunk);
        for event in parser.feed(&text) {
            process_sse_event(&event, &mut state, tx).await?;
        }
    }
    if let Some(trailing) = parser.flush() {
        process_sse_event(&trailing, &mut state, tx).await?;
    }

    let message = state.into_agent_message();
    let content_parts = match &message {
        AgentMessage::Assistant { content, .. } => content.clone(),
        _ => Vec::new(),
    };
    let usage = match &message {
        AgentMessage::Assistant { usage, .. } => usage.clone().unwrap_or_default(),
        _ => TokenUsage::default(),
    };

    let _ = tx
        .send(StreamChunk::Done(BridgeResponse {
            message,
            raw_content: content_parts,
            usage,
        }))
        .await;

    Ok(())
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
    let mut body = serde_json::json!({
        "model": model_id,
        "store": false,
        "stream": true,
        "instructions": request.system_prompt.as_deref().filter(|v| !v.is_empty()).unwrap_or("You are a helpful assistant."),
        "input": convert_input(request),
        "text": { "verbosity": "low" },
        "include": ["reasoning.encrypted_content"],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
    });

    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                    "strict": null,
                })
            })
            .collect();
        body["tools"] = serde_json::Value::Array(tools);
    }

    body
}

fn convert_input(request: &BridgeRequest) -> Vec<serde_json::Value> {
    let mut input = Vec::new();

    for msg in &request.messages {
        match msg {
            AgentMessage::User { content, .. } => {
                let parts: Vec<serde_json::Value> = content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => {
                            Some(serde_json::json!({ "type": "input_text", "text": text }))
                        }
                        _ => None,
                    })
                    .collect();
                if !parts.is_empty() {
                    input.push(serde_json::json!({ "role": "user", "content": parts }));
                }
            }
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            } => {}
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    input.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
                        "status": "completed",
                    }));
                }
                for tc in tool_calls {
                    let call_id = tc.call_id.as_deref().unwrap_or(&tc.id);
                    input.push(serde_json::json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": tc.name,
                        "arguments": tc.arguments.to_string(),
                    }));
                }
            }
            AgentMessage::ToolResult {
                tool_call_id,
                call_id,
                content,
                ..
            } => {
                let text = content
                    .iter()
                    .filter_map(ContentPart::extract_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let id = call_id.as_deref().unwrap_or(tool_call_id);
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": text,
                }));
            }
            AgentMessage::CompactionSummary { summary, .. } => {
                if !summary.is_empty() {
                    input.push(serde_json::json!({
                        "role": "user",
                        "content": [{ "type": "input_text", "text": format!("<summary>\n{summary}\n</summary>") }],
                    }));
                }
            }
        }
    }

    input
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
        return Err(BridgeError::CompletionFailed(
            "OpenAI Codex provider 未配置登录凭据".to_string(),
        ));
    }

    if trimmed.starts_with('{') {
        let credential: StoredCodexCredential = serde_json::from_str(trimmed).map_err(|e| {
            BridgeError::RequestBuildFailed(format!("解析 Codex OAuth 凭据失败: {e}"))
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
        return Err(BridgeError::CompletionFailed(
            "Codex OAuth access token 已过期，且没有 refresh token".to_string(),
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
        .map_err(|e| BridgeError::CompletionFailed(format!("刷新 Codex token 失败: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(BridgeError::CompletionFailed(format!(
            "刷新 Codex token 返回 {status}: {body_text}"
        )));
    }

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: String,
        expires_in: i64,
    }

    let token: TokenResponse = response
        .json()
        .await
        .map_err(|e| BridgeError::CompletionFailed(format!("解析 Codex token 失败: {e}")))?;

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
        .map_err(|e| format!("解码 Codex access token 失败: {e}"))?;
    let json: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| format!("解析 Codex access token payload 失败: {e}"))?;
    json.get(JWT_CLAIM_PATH)
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Codex access token 中缺少 chatgpt_account_id".to_string())
}

fn friendly_codex_error(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let error = parsed.get("error")?;
    let code = error
        .get("code")
        .or_else(|| error.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if code.contains("usage_limit") || code.contains("rate_limit") {
        let plan = error
            .get("plan_type")
            .and_then(|v| v.as_str())
            .map(|v| format!("（{} plan）", v.to_ascii_lowercase()))
            .unwrap_or_default();
        return Some(format!("已达到 ChatGPT Codex 使用限制{plan}"));
    }
    error
        .get("message")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

struct PendingFunctionCall {
    call_id: String,
    item_id: String,
    name: String,
    arguments_buf: String,
}

#[derive(Default)]
struct StreamState {
    content_parts: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    usage: TokenUsage,
    text_buf: String,
    reasoning_buf: String,
    pending_fc: Option<PendingFunctionCall>,
}

impl StreamState {
    fn finish_current_text(&mut self) {
        if !self.text_buf.is_empty() {
            self.content_parts
                .push(ContentPart::text(std::mem::take(&mut self.text_buf)));
        }
    }

    fn finish_current_reasoning(&mut self) {
        if !self.reasoning_buf.is_empty() {
            self.content_parts.push(ContentPart::reasoning(
                std::mem::take(&mut self.reasoning_buf),
                None,
                None,
            ));
        }
    }

    fn finish_current_fc(&mut self) {
        if let Some(fc) = self.pending_fc.take() {
            let arguments = serde_json::from_str(&fc.arguments_buf)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let combined_id = format!("{}|{}", fc.call_id, fc.item_id);
            self.tool_calls.push(ToolCallInfo {
                id: combined_id,
                call_id: Some(fc.call_id),
                name: fc.name,
                arguments,
            });
        }
    }

    fn into_agent_message(mut self) -> AgentMessage {
        self.finish_current_text();
        self.finish_current_reasoning();
        self.finish_current_fc();
        AgentMessage::Assistant {
            content: self.content_parts,
            tool_calls: self.tool_calls,
            stop_reason: None,
            error_message: None,
            usage: Some(self.usage),
            timestamp: Some(now_millis()),
        }
    }
}

async fn process_sse_event(
    sse: &super::sse::SseEvent,
    state: &mut StreamState,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    if sse.data.trim() == "[DONE]" {
        return Ok(());
    }
    let data: serde_json::Value = match serde_json::from_str(&sse.data) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let event_type = sse
        .event
        .as_deref()
        .or_else(|| data.get("type").and_then(|v| v.as_str()))
        .unwrap_or("");

    match event_type {
        "response.output_item.added" => {
            let item = data.get("item").unwrap_or(&serde_json::Value::Null);
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match item_type {
                "reasoning" => {
                    state.finish_current_text();
                    state.finish_current_fc();
                }
                "message" => {
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                }
                "function_call" => {
                    state.finish_current_text();
                    state.finish_current_reasoning();
                    state.finish_current_fc();
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let item_id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let combined_id = format!("{call_id}|{item_id}");
                    let _ = tx
                        .send(StreamChunk::ToolCallDelta {
                            id: combined_id,
                            content: ToolCallDeltaContent::Name(name.clone()),
                        })
                        .await;
                    state.pending_fc = Some(PendingFunctionCall {
                        call_id,
                        item_id,
                        name,
                        arguments_buf: String::new(),
                    });
                }
                _ => {}
            }
        }
        "response.output_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|v| v.as_str()) {
                state.text_buf.push_str(delta);
                let _ = tx.send(StreamChunk::TextDelta(delta.to_string())).await;
            }
        }
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|v| v.as_str()) {
                state.reasoning_buf.push_str(delta);
                let _ = tx
                    .send(StreamChunk::ReasoningDelta {
                        id: None,
                        text: delta.to_string(),
                        signature: None,
                    })
                    .await;
            }
        }
        "response.function_call_arguments.delta" => {
            if let Some(delta) = data.get("delta").and_then(|v| v.as_str())
                && let Some(ref mut fc) = state.pending_fc
            {
                fc.arguments_buf.push_str(delta);
                let combined_id = format!("{}|{}", fc.call_id, fc.item_id);
                let _ = tx
                    .send(StreamChunk::ToolCallDelta {
                        id: combined_id,
                        content: ToolCallDeltaContent::Arguments(delta.to_string()),
                    })
                    .await;
            }
        }
        "response.output_item.done" => {
            let item = data.get("item").unwrap_or(&serde_json::Value::Null);
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match item_type {
                "message" => state.finish_current_text(),
                "reasoning" => state.finish_current_reasoning(),
                "function_call" => {
                    if let Some(ref mut fc) = state.pending_fc
                        && let Some(args_str) = item.get("arguments").and_then(|v| v.as_str())
                    {
                        fc.arguments_buf = args_str.to_string();
                    }
                    state.finish_current_fc();
                }
                _ => {}
            }
        }
        "response.completed" | "response.done" | "response.incomplete" => {
            if let Some(usage) = data.get("response").and_then(|v| v.get("usage")) {
                let input_tokens = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cached = usage
                    .get("input_tokens_details")
                    .and_then(|v| v.get("cached_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                state.usage.input = input_tokens.saturating_sub(cached);
                state.usage.output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
        }
        "response.failed" | "error" => {
            let msg = data
                .get("response")
                .and_then(|v| v.get("error"))
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    data.get("error")
                        .and_then(|v| v.get("message"))
                        .and_then(|v| v.as_str())
                })
                .or_else(|| data.get("message").and_then(|v| v.as_str()))
                .unwrap_or("unknown Codex error");
            return Err(BridgeError::CompletionFailed(msg.to_string()));
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn builds_codex_body_with_instructions_not_system_input() {
        let body = build_request_body(
            "gpt-5.5",
            &BridgeRequest {
                system_prompt: Some("system".to_string()),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![],
            },
        );
        assert_eq!(body["instructions"], "system");
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
    }
}
