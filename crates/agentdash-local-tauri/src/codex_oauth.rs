use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use agentdash_contracts::llm_provider::{
    CodexOAuthStatusResponse, CompleteCodexOAuthRequest, FailCodexOAuthRequest,
    PrepareCodexOAuthRequest, StartCodexOAuthResponse,
};
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

const CODEX_OAUTH_CALLBACK_PORT: u16 = 1455;
const CODEX_OAUTH_CALLBACK_PATH: &str = "/auth/callback";
const CODEX_OAUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CODEX_OAUTH_TIMEOUT_SECS: u64 = 5 * 60;

type LocalCodexOAuthStore = Arc<Mutex<HashMap<String, LocalCodexOAuthFlow>>>;

static LOCAL_CODEX_OAUTH_FLOWS: OnceLock<LocalCodexOAuthStore> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopCodexOAuthTarget {
    GlobalProvider,
    UserByok,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DesktopCodexOAuthStartRequest {
    pub api_origin: String,
    pub access_token: String,
    pub provider_id: String,
    pub target: DesktopCodexOAuthTarget,
}

struct LocalCodexOAuthFlow {
    api_origin: String,
    access_token: String,
    cancel_tx: oneshot::Sender<()>,
}

#[tauri::command]
pub async fn codex_oauth_start(
    request: DesktopCodexOAuthStartRequest,
) -> Result<StartCodexOAuthResponse, String> {
    let api_origin = normalize_api_origin(&request.api_origin)?;
    let access_token = request.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err("Codex OAuth 需要当前登录 token".to_string());
    }
    if request.provider_id.trim().is_empty() {
        return Err("Codex OAuth provider_id 不能为空".to_string());
    }

    let ipv4_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, CODEX_OAUTH_CALLBACK_PORT))
        .await
        .map_err(|error| {
            format!(
                "无法启动 Codex 登录回调服务 127.0.0.1:{}: {error}",
                CODEX_OAUTH_CALLBACK_PORT
            )
        })?;
    let ipv6_listener = TcpListener::bind((Ipv6Addr::LOCALHOST, CODEX_OAUTH_CALLBACK_PORT))
        .await
        .ok();

    let state = Uuid::new_v4().to_string();
    let verifier = build_pkce_verifier();
    let challenge = build_pkce_challenge(&verifier);
    let prepare = PrepareCodexOAuthRequest {
        state: state.clone(),
        code_challenge: challenge,
        redirect_uri: CODEX_OAUTH_REDIRECT_URI.to_string(),
    };
    let flow = prepare_remote_flow(&api_origin, &access_token, &request, prepare).await?;
    let flow_id = flow.flow_id.clone();
    let (cancel_tx, cancel_rx) = oneshot::channel();

    local_codex_oauth_flows().lock().await.insert(
        flow_id.clone(),
        LocalCodexOAuthFlow {
            api_origin: api_origin.clone(),
            access_token: access_token.clone(),
            cancel_tx,
        },
    );

    tokio::spawn(run_local_codex_oauth_flow(
        flow_id,
        api_origin,
        access_token,
        state,
        verifier,
        ipv4_listener,
        ipv6_listener,
        cancel_rx,
    ));

    Ok(flow)
}

#[tauri::command]
pub async fn codex_oauth_cancel(flow_id: String) -> Result<CodexOAuthStatusResponse, String> {
    let flow = local_codex_oauth_flows().lock().await.remove(&flow_id);
    let Some(flow) = flow else {
        return Err(format!("Codex OAuth flow {flow_id} 不存在"));
    };
    let _ = flow.cancel_tx.send(());
    cancel_remote_flow(&flow.api_origin, &flow.access_token, &flow_id).await
}

async fn run_local_codex_oauth_flow(
    flow_id: String,
    api_origin: String,
    access_token: String,
    expected_state: String,
    verifier: String,
    ipv4_listener: TcpListener,
    ipv6_listener: Option<TcpListener>,
    cancel_rx: oneshot::Receiver<()>,
) {
    let result = await_callback(
        ipv4_listener,
        ipv6_listener,
        expected_state.as_str(),
        cancel_rx,
    )
    .await;

    match result {
        Ok(code) => {
            let complete = CompleteCodexOAuthRequest {
                code,
                state: expected_state,
                code_verifier: verifier,
                redirect_uri: CODEX_OAUTH_REDIRECT_URI.to_string(),
            };
            let _ = complete_remote_flow(&api_origin, &access_token, &flow_id, complete).await;
        }
        Err(message) if message == "Codex 登录已取消" => {}
        Err(message) => {
            let _ = fail_remote_flow(&api_origin, &access_token, &flow_id, &message).await;
        }
    }

    local_codex_oauth_flows().lock().await.remove(&flow_id);
}

async fn await_callback(
    ipv4_listener: TcpListener,
    ipv6_listener: Option<TcpListener>,
    expected_state: &str,
    cancel_rx: oneshot::Receiver<()>,
) -> Result<String, String> {
    let timeout = tokio::time::sleep(Duration::from_secs(CODEX_OAUTH_TIMEOUT_SECS));
    tokio::pin!(timeout);

    if let Some(ipv6_listener) = ipv6_listener {
        tokio::select! {
            result = accept_callback(ipv4_listener, expected_state) => result,
            result = accept_callback(ipv6_listener, expected_state) => result,
            _ = &mut timeout => Err("Codex 登录已超时".to_string()),
            _ = cancel_rx => Err("Codex 登录已取消".to_string()),
        }
    } else {
        tokio::select! {
            result = accept_callback(ipv4_listener, expected_state) => result,
            _ = &mut timeout => Err("Codex 登录已超时".to_string()),
            _ = cancel_rx => Err("Codex 登录已取消".to_string()),
        }
    }
}

async fn accept_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|error| format!("接收 Codex 登录回调失败: {error}"))?;
    handle_callback_stream(&mut stream, expected_state).await
}

async fn handle_callback_stream(
    stream: &mut TcpStream,
    expected_state: &str,
) -> Result<String, String> {
    let mut buf = [0_u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|error| format!("读取 Codex 登录回调失败: {error}"))?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| "Codex 登录回调为空".to_string())?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "Codex 登录回调请求格式无效".to_string())?;
    let callback_url = reqwest::Url::parse(&format!("http://localhost{path}"))
        .map_err(|_| "Codex 登录回调地址无效".to_string())?;
    if callback_url.path() != CODEX_OAUTH_CALLBACK_PATH {
        write_oauth_html(stream, 404, "Codex 登录回调地址无效").await;
        return Err("Codex 登录回调地址无效".to_string());
    }

    let state = callback_url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
    if state.as_deref() != Some(expected_state) {
        write_oauth_html(stream, 400, "Codex 登录 state 校验失败").await;
        return Err("Codex 登录 state 校验失败".to_string());
    }

    let code = callback_url
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
        .filter(|value| !value.is_empty());
    let Some(code) = code else {
        write_oauth_html(stream, 400, "Codex 登录缺少授权码").await;
        return Err("Codex 登录缺少授权码".to_string());
    };

    write_oauth_html(stream, 200, "Codex 登录完成，可以关闭此窗口").await;
    Ok(code)
}

async fn prepare_remote_flow(
    api_origin: &str,
    access_token: &str,
    request: &DesktopCodexOAuthStartRequest,
    prepare: PrepareCodexOAuthRequest,
) -> Result<StartCodexOAuthResponse, String> {
    let target_path = match request.target {
        DesktopCodexOAuthTarget::GlobalProvider => {
            format!(
                "/api/llm-providers/{}/codex-oauth/desktop/prepare",
                request.provider_id
            )
        }
        DesktopCodexOAuthTarget::UserByok => format!(
            "/api/llm-providers/{}/user-credential/codex-oauth/desktop/prepare",
            request.provider_id
        ),
    };
    post_json(
        api_origin,
        access_token,
        &target_path,
        &prepare,
        "启动 Codex OAuth",
    )
    .await
}

async fn complete_remote_flow(
    api_origin: &str,
    access_token: &str,
    flow_id: &str,
    complete: CompleteCodexOAuthRequest,
) -> Result<CodexOAuthStatusResponse, String> {
    post_json(
        api_origin,
        access_token,
        &format!("/api/llm-providers/codex-oauth/{flow_id}/complete"),
        &complete,
        "完成 Codex OAuth",
    )
    .await
}

async fn fail_remote_flow(
    api_origin: &str,
    access_token: &str,
    flow_id: &str,
    message: &str,
) -> Result<CodexOAuthStatusResponse, String> {
    post_json(
        api_origin,
        access_token,
        &format!("/api/llm-providers/codex-oauth/{flow_id}/fail"),
        &FailCodexOAuthRequest {
            message: message.to_string(),
        },
        "标记 Codex OAuth 失败",
    )
    .await
}

async fn cancel_remote_flow(
    api_origin: &str,
    access_token: &str,
    flow_id: &str,
) -> Result<CodexOAuthStatusResponse, String> {
    let client = reqwest::Client::new();
    let response = client
        .post(build_api_url(
            api_origin,
            &format!("/api/llm-providers/codex-oauth/{flow_id}/cancel"),
        )?)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| format!("取消 Codex OAuth 请求失败: {error}"))?;
    response_json(response, "取消 Codex OAuth").await
}

async fn post_json<TRequest, TResponse>(
    api_origin: &str,
    access_token: &str,
    path: &str,
    body: &TRequest,
    operation: &str,
) -> Result<TResponse, String>
where
    TRequest: serde::Serialize + ?Sized,
    TResponse: serde::de::DeserializeOwned,
{
    let client = reqwest::Client::new();
    let response = client
        .post(build_api_url(api_origin, path)?)
        .bearer_auth(access_token)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{operation} 请求失败: {error}"))?;
    response_json(response, operation).await
}

async fn response_json<TResponse>(
    response: reqwest::Response,
    operation: &str,
) -> Result<TResponse, String>
where
    TResponse: serde::de::DeserializeOwned,
{
    if !response.status().is_success() {
        return Err(format!("{operation} 返回 {}", response.status()));
    }
    response
        .json::<TResponse>()
        .await
        .map_err(|error| format!("解析 {operation} 响应失败: {error}"))
}

fn build_api_url(api_origin: &str, path: &str) -> Result<String, String> {
    Ok(format!(
        "{}{}",
        normalize_api_origin(api_origin)?,
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        }
    ))
}

fn normalize_api_origin(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Dashboard API origin 不能为空".to_string());
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|error| format!("Dashboard API origin 无效: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Dashboard API origin 只支持 http/https".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Dashboard API origin 不能包含认证信息".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Dashboard API origin 缺少 host".to_string())?;
    let mut origin = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Ok(origin)
}

fn build_pkce_verifier() -> String {
    format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

async fn write_oauth_html(stream: &mut TcpStream, status: u16, message: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>AgentDash Codex OAuth</title></head><body style=\"font-family:system-ui,sans-serif;margin:48px\"><h1>{message}</h1></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn local_codex_oauth_flows() -> LocalCodexOAuthStore {
    LOCAL_CODEX_OAUTH_FLOWS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn exercise_callback_request(
        request: &'static str,
        expected_state: &'static str,
    ) -> (Result<String, String>, String) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            handle_callback_stream(&mut stream, expected_state).await
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        (task.await.unwrap(), response)
    }

    #[test]
    fn pkce_challenge_matches_existing_codex_flow() {
        assert_eq!(
            build_pkce_challenge("verifier"),
            "iMnq5o6zALKXGivsnlom_0F5_WYda32GHkxlV7mq7hQ"
        );
    }

    #[test]
    fn normalize_api_origin_strips_path_query_and_credentials() {
        assert_eq!(
            normalize_api_origin(" https://app.example.test/api?x=1 ").unwrap(),
            "https://app.example.test"
        );
        assert!(normalize_api_origin("https://user:pass@app.example.test").is_err());
    }

    #[tokio::test]
    async fn callback_stream_accepts_valid_code_and_state() {
        let (result, response) = exercise_callback_request(
            "GET /auth/callback?code=code-ok&state=state-ok HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "state-ok",
        )
        .await;

        assert_eq!(result.unwrap(), "code-ok");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
    }

    #[tokio::test]
    async fn callback_stream_rejects_state_mismatch() {
        let (result, response) = exercise_callback_request(
            "GET /auth/callback?code=code-ok&state=state-wrong HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "state-ok",
        )
        .await;

        assert_eq!(result.unwrap_err(), "Codex 登录 state 校验失败");
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }

    #[tokio::test]
    async fn callback_stream_rejects_missing_code() {
        let (result, response) = exercise_callback_request(
            "GET /auth/callback?state=state-ok HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "state-ok",
        )
        .await;

        assert_eq!(result.unwrap_err(), "Codex 登录缺少授权码");
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }
}
