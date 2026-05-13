use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

type OAuthFlowStore = Arc<Mutex<HashMap<String, OAuthFlowRecord>>>;

static OAUTH_FLOWS: OnceLock<OAuthFlowStore> = OnceLock::new();

#[derive(Clone)]
pub struct LocalOAuthProviderConfig {
    pub label: String,
    pub callback_host: String,
    pub callback_port: u16,
    pub callback_path: String,
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub extra_authorize_params: Vec<(String, String)>,
    pub timeout: Duration,
}

pub struct StartedOAuthFlow {
    pub flow_id: String,
    pub auth_url: String,
    pub expires_at: DateTime<Utc>,
    pub verifier: String,
    pub code_rx: oneshot::Receiver<Result<String, String>>,
}

#[derive(Debug, Clone)]
pub enum OAuthFlowStatus {
    Pending,
    Completed { message: String },
    Failed { message: String },
}

impl OAuthFlowStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
        }
    }

    pub fn message(&self) -> Option<String> {
        match self {
            Self::Pending => None,
            Self::Completed { message } | Self::Failed { message } => Some(message.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthFlowSnapshot {
    pub flow_id: String,
    pub status: OAuthFlowStatus,
}

struct OAuthFlowRecord {
    status: OAuthFlowStatus,
    cancel_tx: Option<oneshot::Sender<()>>,
}

pub async fn start_local_pkce_oauth_flow(
    config: LocalOAuthProviderConfig,
) -> Result<StartedOAuthFlow, String> {
    let flow_id = Uuid::new_v4().to_string();
    let state_token = Uuid::new_v4().to_string();
    let verifier = build_pkce_verifier();
    let challenge = build_pkce_challenge(&verifier);
    let auth_url = build_authorize_url(&config, &state_token, &challenge)?;
    let expires_at = Utc::now()
        + chrono::Duration::from_std(config.timeout)
            .map_err(|e| format!("OAuth 登录超时时间无效: {e}"))?;

    let listener = TcpListener::bind((config.callback_host.as_str(), config.callback_port))
        .await
        .map_err(|e| {
            format!(
                "无法启动 {} 登录回调服务 {}:{}: {e}",
                config.label, config.callback_host, config.callback_port
            )
        })?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let (code_tx, code_rx) = oneshot::channel();

    oauth_flows().lock().await.insert(
        flow_id.clone(),
        OAuthFlowRecord {
            status: OAuthFlowStatus::Pending,
            cancel_tx: Some(cancel_tx),
        },
    );

    tokio::spawn(run_callback_listener(
        config,
        flow_id.clone(),
        state_token,
        listener,
        cancel_rx,
        code_tx,
    ));

    Ok(StartedOAuthFlow {
        flow_id,
        auth_url,
        expires_at,
        verifier,
        code_rx,
    })
}

pub async fn get_flow_status(flow_id: &str) -> Result<OAuthFlowSnapshot, String> {
    let store = oauth_flows();
    let flows = store.lock().await;
    let flow = flows
        .get(flow_id)
        .ok_or_else(|| format!("OAuth 登录流程 {flow_id} 不存在"))?;
    Ok(OAuthFlowSnapshot {
        flow_id: flow_id.to_string(),
        status: flow.status.clone(),
    })
}

pub async fn complete_flow(flow_id: &str, message: impl Into<String>) {
    let store = oauth_flows();
    let mut flows = store.lock().await;
    if let Some(flow) = flows.get_mut(flow_id)
        && matches!(flow.status, OAuthFlowStatus::Pending)
    {
        flow.cancel_tx = None;
        flow.status = OAuthFlowStatus::Completed {
            message: message.into(),
        };
    }
}

pub async fn fail_flow(flow_id: &str, message: impl Into<String>) {
    let store = oauth_flows();
    let mut flows = store.lock().await;
    if let Some(flow) = flows.get_mut(flow_id)
        && matches!(flow.status, OAuthFlowStatus::Pending)
    {
        flow.cancel_tx = None;
        flow.status = OAuthFlowStatus::Failed {
            message: message.into(),
        };
    }
}

pub async fn cancel_flow(
    flow_id: &str,
    message: impl Into<String>,
) -> Result<OAuthFlowSnapshot, String> {
    let message = message.into();
    let store = oauth_flows();
    let mut flows = store.lock().await;
    let flow = flows
        .get_mut(flow_id)
        .ok_or_else(|| format!("OAuth 登录流程 {flow_id} 不存在"))?;
    if let Some(cancel_tx) = flow.cancel_tx.take() {
        let _ = cancel_tx.send(());
    }
    if matches!(flow.status, OAuthFlowStatus::Pending) {
        flow.status = OAuthFlowStatus::Failed {
            message: message.clone(),
        };
    }
    Ok(OAuthFlowSnapshot {
        flow_id: flow_id.to_string(),
        status: flow.status.clone(),
    })
}

async fn run_callback_listener(
    config: LocalOAuthProviderConfig,
    flow_id: String,
    expected_state: String,
    listener: TcpListener,
    cancel_rx: oneshot::Receiver<()>,
    code_tx: oneshot::Sender<Result<String, String>>,
) {
    let result = tokio::select! {
        accept_result = listener.accept() => {
            match accept_result {
                Ok((mut stream, _)) => handle_callback_stream(&mut stream, &config, &expected_state).await,
                Err(e) => Err(format!("接收 {} 登录回调失败: {e}", config.label)),
            }
        }
        _ = tokio::time::sleep(config.timeout) => {
            Err(format!("{} 登录超时", config.label))
        }
        _ = cancel_rx => {
            Err(format!("{} 登录已取消", config.label))
        }
    };

    if let Err(message) = &result {
        fail_flow(&flow_id, message.clone()).await;
    }
    let _ = code_tx.send(result);
}

async fn handle_callback_stream(
    stream: &mut tokio::net::TcpStream,
    config: &LocalOAuthProviderConfig,
    expected_state: &str,
) -> Result<String, String> {
    let mut buf = [0_u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("读取 {} 登录回调失败: {e}", config.label))?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| format!("{} 登录回调为空", config.label))?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("{} 登录回调请求格式无效", config.label))?;
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    if route != config.callback_path {
        write_oauth_html(stream, 404, &format!("{} 登录回调地址无效", config.label)).await;
        return Err(format!("{} 登录回调地址无效", config.label));
    }

    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        write_oauth_html(
            stream,
            400,
            &format!("{} 登录 state 校验失败", config.label),
        )
        .await;
        return Err(format!("{} 登录 state 校验失败", config.label));
    }
    let Some(code) = params.get("code").filter(|v| !v.is_empty()).cloned() else {
        write_oauth_html(stream, 400, &format!("{} 登录缺少授权码", config.label)).await;
        return Err(format!("{} 登录缺少授权码", config.label));
    };

    write_oauth_html(
        stream,
        200,
        &format!("{} 登录完成，可以关闭此窗口", config.label),
    )
    .await;
    Ok(code)
}

async fn write_oauth_html(stream: &mut tokio::net::TcpStream, status: u16, message: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>AgentDash OAuth 登录</title></head><body style=\"font-family:system-ui,sans-serif;margin:48px\"><h1>{message}</h1></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn oauth_flows() -> OAuthFlowStore {
    OAUTH_FLOWS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn build_pkce_verifier() -> String {
    format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub fn build_authorize_url(
    config: &LocalOAuthProviderConfig,
    state: &str,
    challenge: &str,
) -> Result<String, String> {
    let mut url =
        url::Url::parse(&config.authorize_url).map_err(|e| format!("OAuth 授权地址无效: {e}"))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &config.redirect_uri)
            .append_pair("scope", &config.scope)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        for (key, value) in &config.extra_authorize_params {
            query.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LocalOAuthProviderConfig {
        LocalOAuthProviderConfig {
            label: "测试 OAuth".to_string(),
            callback_host: "127.0.0.1".to_string(),
            callback_port: 1455,
            callback_path: "/auth/callback".to_string(),
            authorize_url: "https://example.com/oauth/authorize".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://localhost:1455/auth/callback".to_string(),
            scope: "openid offline_access".to_string(),
            extra_authorize_params: vec![("originator".to_string(), "agentdash".to_string())],
            timeout: Duration::from_secs(300),
        }
    }

    #[test]
    fn pkce_challenge_uses_sha256_base64url() {
        let challenge = build_pkce_challenge("verifier");
        assert_eq!(challenge, "iMnq5o6zALKXGivsnlom_0F5_WYda32GHkxlV7mq7hQ");
    }

    #[test]
    fn authorize_url_contains_common_pkce_params() {
        let url = build_authorize_url(&test_config(), "state", "challenge").unwrap();
        assert!(url.contains("client_id=client"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        assert!(url.contains("scope=openid+offline_access"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("originator=agentdash"));
    }
}
