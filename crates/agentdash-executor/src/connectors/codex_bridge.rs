use agentdash_diagnostics::{diag, Subsystem};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
    PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload,
};
use codex_app_server_protocol::{
    AskForApproval, ClientInfo, ClientNotification, ClientRequest, GetAccountParams,
    GetAccountResponse, InitializeCapabilities, InitializeParams, InitializeResponse,
    JSONRPCMessage, JSONRPCNotification, JSONRPCRequest, JSONRPCResponse, RequestId, SandboxMode,
    Thread, ThreadForkParams, ThreadForkResponse, ThreadNameUpdatedNotification, ThreadStartParams,
    ThreadStartResponse, TurnStartParams, TurnStartResponse, TurnSteerParams, TurnSteerResponse,
    UserInput,
};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, mpsc, oneshot},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::adapters::codex_config::{CodexExecutorConfig, CodexPermissionPolicy, to_codex_config};
use crate::connectors::context_frame_render::compose_prompt_text;

const CODEX_EXECUTOR_ID: &str = "CODEX";
const CODEX_SOURCE_TITLE: &str = "codex";

fn normalize_executor_id(executor: &str) -> String {
    executor.trim().replace('-', "_").to_ascii_uppercase()
}

fn is_codex_executor(executor: &str) -> bool {
    normalize_executor_id(executor) == CODEX_EXECUTOR_ID
}

type PendingResponseMap =
    Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<Value, ConnectorError>>>>>;

#[derive(Clone)]
struct CodexLiveSession {
    cancel_token: CancellationToken,
    out_tx: mpsc::Sender<Value>,
    pending: PendingResponseMap,
    request_counter: Arc<AtomicI64>,
    thread_id: Arc<Mutex<Option<String>>>,
    active_turn_id: Arc<Mutex<Option<String>>>,
}

/// 包裹返回给消费者的事件流：当消费者提前丢弃 stream 时，
/// 触发 `cancel_token`，使 writer/stdout/stderr 三个循环退出、
/// waiter 杀掉子进程，避免遗留孤儿 `npx codex app-server`。
struct CancelOnDropStream<S> {
    inner: S,
    cancel_token: CancellationToken,
}

impl<S> Drop for CancelOnDropStream<S> {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl<S: futures::Stream + Unpin> futures::Stream for CancelOnDropStream<S> {
    type Item = S::Item;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

pub struct CodexBridgeConnector {
    live_sessions: Arc<Mutex<HashMap<String, CodexLiveSession>>>,
}

impl CodexBridgeConnector {
    /// 首阶段桥接：对外暴露独立 Codex connector，内部走原生 app-server 协议。
    /// 后续替换底层 SDK/运行时时，仅需继续演进该模块。
    pub fn new() -> Self {
        Self {
            live_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for CodexBridgeConnector {
    fn default() -> Self {
        Self::new()
    }
}

fn connector_type_label(connector_type: ConnectorType) -> &'static str {
    match connector_type {
        ConnectorType::LocalExecutor => "local_executor",
        ConnectorType::RemoteAcpBackend => "remote_acp_backend",
    }
}

fn make_envelope(
    event: BackboneEvent,
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
) -> BackboneEnvelope {
    BackboneEnvelope::new(event, session_id, source.clone()).with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

fn make_source_session_title_envelope(
    session_id: &str,
    source_info: &SourceInfo,
    turn_id: &str,
    executor_session_id: String,
    title: String,
    preview: Option<String>,
) -> Option<BackboneEnvelope> {
    let title = title.trim();
    if title.is_empty()
        || preview
            .as_deref()
            .is_some_and(|value| value.trim() == title)
    {
        return None;
    }

    Some(make_envelope(
        BackboneEvent::Platform(PlatformEvent::SourceSessionTitleUpdated {
            executor_session_id: Some(executor_session_id),
            title: title.to_string(),
            preview,
            source: CODEX_SOURCE_TITLE.to_string(),
        }),
        session_id,
        source_info,
        turn_id,
    ))
}

fn make_thread_source_title_envelope(
    session_id: &str,
    source_info: &SourceInfo,
    turn_id: &str,
    thread: &Thread,
) -> Option<BackboneEnvelope> {
    make_source_session_title_envelope(
        session_id,
        source_info,
        turn_id,
        thread.id.clone(),
        thread.name.clone()?,
        Some(thread.preview.clone()),
    )
}

fn build_prompt_text(
    context: &ExecutionContext,
    prompt: &PromptPayload,
) -> Result<String, ConnectorError> {
    let user_text = prompt.to_fallback_text();
    let prompt_text = compose_prompt_text(&user_text, &context.turn.context_frames);
    if prompt_text.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "prompt payload 解析后为空".to_string(),
        ));
    }
    Ok(prompt_text)
}

fn executable_available(name: &str) -> bool {
    let name_path = Path::new(name);
    if name_path.components().count() > 1 {
        return name_path.is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    for dir in env::split_paths(&paths) {
        for candidate in executable_candidates(name) {
            if dir.join(candidate).is_file() {
                return true;
            }
        }
    }
    false
}

fn executable_candidates(name: &str) -> Vec<String> {
    if cfg!(windows) {
        if Path::new(name).extension().is_some() {
            vec![name.to_string()]
        } else {
            vec![
                format!("{name}.exe"),
                format!("{name}.cmd"),
                format!("{name}.bat"),
                name.to_string(),
            ]
        }
    } else {
        vec![name.to_string()]
    }
}

fn spawn_codex_process(
    command: &mut tokio::process::Command,
) -> std::io::Result<tokio::process::Child> {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()
}

fn codex_discovery_patch() -> json_patch::Patch {
    serde_json::from_value(serde_json::json!([
        { "op": "replace", "path": "/options/model_selector/providers", "value": [] },
        { "op": "replace", "path": "/options/model_selector/models", "value": [] },
        { "op": "replace", "path": "/options/model_selector/default_model", "value": null },
        { "op": "replace", "path": "/options/model_selector/agents", "value": [] },
        { "op": "replace", "path": "/options/model_selector/permissions", "value": ["AUTO", "SUPERVISED", "PLAN"] },
        { "op": "replace", "path": "/options/loading_models", "value": false },
        { "op": "replace", "path": "/options/loading_agents", "value": false },
        { "op": "replace", "path": "/options/loading_slash_commands", "value": false },
        { "op": "replace", "path": "/options/slash_commands", "value": [] }
    ]))
    .expect("static codex discovery patch must be valid")
}

fn build_thread_start_params(
    codex_config: &CodexExecutorConfig,
    working_directory: &Path,
) -> ThreadStartParams {
    let mut config_overrides = HashMap::new();
    if let Some(reasoning_id) = codex_config.reasoning_id.as_deref() {
        let normalized = reasoning_id.trim().to_ascii_lowercase();
        if matches!(normalized.as_str(), "low" | "medium" | "high" | "xhigh") {
            config_overrides.insert(
                "model_reasoning_effort".to_string(),
                Value::String(normalized),
            );
        }
    }
    let config = if config_overrides.is_empty() {
        None
    } else {
        Some(config_overrides)
    };

    let approval_policy = match codex_config.permission_policy {
        Some(CodexPermissionPolicy::Auto) => Some(AskForApproval::Never),
        Some(CodexPermissionPolicy::Supervised) => Some(AskForApproval::UnlessTrusted),
        // 当前先保证协议通路可用；plan 专属协作模式后续单独补齐。
        Some(CodexPermissionPolicy::Plan) => Some(AskForApproval::OnRequest),
        None => Some(AskForApproval::OnRequest),
    };

    let model = codex_config.model_id.as_deref().map(|m| {
        m.strip_suffix("-fast")
            .map_or_else(|| m.to_string(), str::to_string)
    });

    ThreadStartParams {
        model,
        cwd: Some(working_directory.to_string_lossy().to_string()),
        approval_policy,
        sandbox: Some(SandboxMode::WorkspaceWrite),
        config,
        ..Default::default()
    }
}

fn build_thread_fork_params(
    thread_id: String,
    thread_start: &ThreadStartParams,
) -> ThreadForkParams {
    ThreadForkParams {
        thread_id,
        model: thread_start.model.clone(),
        model_provider: thread_start.model_provider.clone(),
        service_tier: thread_start.service_tier.clone(),
        cwd: thread_start.cwd.clone(),
        approval_policy: thread_start.approval_policy,
        sandbox: thread_start.sandbox,
        config: thread_start.config.clone(),
        base_instructions: thread_start.base_instructions.clone(),
        developer_instructions: thread_start.developer_instructions.clone(),
        ..Default::default()
    }
}

fn next_request_id(counter: &AtomicI64) -> RequestId {
    RequestId::Integer(counter.fetch_add(1, Ordering::SeqCst))
}

async fn send_rpc_notification(
    out_tx: &mpsc::Sender<Value>,
    notification: ClientNotification,
) -> Result<(), ConnectorError> {
    out_tx
        .send(serde_json::to_value(notification)?)
        .await
        .map_err(|_| ConnectorError::Runtime("Codex app-server 已断开，无法发送通知".to_string()))
}

async fn send_rpc_request<R>(
    out_tx: &mpsc::Sender<Value>,
    pending: &PendingResponseMap,
    request: ClientRequest,
) -> Result<R, ConnectorError>
where
    R: DeserializeOwned,
{
    let request_id = request.id().clone();
    let (tx, rx) = oneshot::channel::<Result<Value, ConnectorError>>();
    pending.lock().await.insert(request_id.clone(), tx);

    if out_tx.send(serde_json::to_value(request)?).await.is_err() {
        pending.lock().await.remove(&request_id);
        return Err(ConnectorError::Runtime(
            "Codex app-server 已断开，无法发送请求".to_string(),
        ));
    }

    let response = rx.await.map_err(|_| {
        ConnectorError::Runtime(format!("等待 Codex app-server 响应失败: {request_id}"))
    })?;
    let payload = response?;
    serde_json::from_value(payload)
        .map_err(|e| ConnectorError::Runtime(format!("解析 Codex app-server 响应失败: {e}")))
}

async fn send_server_response(
    out_tx: &mpsc::Sender<Value>,
    request_id: RequestId,
    result: Value,
) -> Result<(), ConnectorError> {
    let response = JSONRPCResponse {
        id: request_id,
        result,
    };
    out_tx
        .send(serde_json::to_value(response)?)
        .await
        .map_err(|_| ConnectorError::Runtime("Codex app-server 已断开，无法回传响应".to_string()))
}

/// 1:1 映射 Codex 原生 JSON-RPC notification → BackboneEvent。
async fn handle_server_notification(
    notification: JSONRPCNotification,
    session_id: &str,
    tx: &mpsc::Sender<Result<BackboneEnvelope, ConnectorError>>,
    source: &SourceInfo,
    turn_id: &str,
    active_turn_id: &Arc<Mutex<Option<String>>>,
) {
    let wrap = |event: BackboneEvent| make_envelope(event, session_id, source, turn_id);

    match notification.method.as_str() {
        "item/agentMessage/delta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::AgentMessageDelta(p)))).await;
            }
        }
        "item/reasoning/textDelta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ReasoningTextDelta(p))))
                    .await;
            }
        }
        "item/reasoning/summaryTextDelta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ReasoningSummaryDelta(p))))
                    .await;
            }
        }
        "item/started" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ItemStarted(
                        ItemStartedNotification::from_codex(p),
                    ))))
                    .await;
            }
        }
        "item/completed" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ItemCompleted(
                        ItemCompletedNotification::from_codex(p),
                    ))))
                    .await;
            }
        }
        "item/commandExecution/outputDelta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::CommandOutputDelta(p))))
                    .await;
            }
        }
        "item/fileChange/outputDelta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::FileChangeDelta(p)))).await;
            }
        }
        "item/mcpToolCall/progress" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::McpToolCallProgress(p))))
                    .await;
            }
        }
        "turn/started" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value::<
                    codex_app_server_protocol::TurnStartedNotification,
                >(params)
            {
                *active_turn_id.lock().await = Some(p.turn.id.clone());
                let _ = tx.send(Ok(wrap(BackboneEvent::TurnStarted(p)))).await;
            }
        }
        "turn/completed" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value::<
                    codex_app_server_protocol::TurnCompletedNotification,
                >(params)
            {
                let completed_turn_id = p.turn.id.clone();
                let mut active = active_turn_id.lock().await;
                if active.as_deref() == Some(completed_turn_id.as_str()) {
                    *active = None;
                }
                drop(active);
                let _ = tx.send(Ok(wrap(BackboneEvent::TurnCompleted(p)))).await;
            }
        }
        "turn/diff/updated" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::TurnDiffUpdated(p)))).await;
            }
        }
        "turn/plan/updated" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::TurnPlanUpdated(p)))).await;
            }
        }
        "turn/plan/delta" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::PlanDelta(p)))).await;
            }
        }
        "thread/tokenUsage/updated" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value::<
                    codex_app_server_protocol::ThreadTokenUsageUpdatedNotification,
                >(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::TokenUsageUpdated(p.into()))))
                    .await;
            }
        }
        "thread/status/changed" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ThreadStatusChanged(p))))
                    .await;
            }
        }
        "thread/name/updated" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value::<ThreadNameUpdatedNotification>(params)
                && let Some(thread_name) = p.thread_name
                && let Some(envelope) = make_source_session_title_envelope(
                    session_id,
                    source,
                    turn_id,
                    p.thread_id,
                    thread_name,
                    None,
                )
            {
                let _ = tx.send(Ok(envelope)).await;
            }
        }
        "thread/compacted" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx
                    .send(Ok(wrap(BackboneEvent::ExecutorContextCompacted(p))))
                    .await;
            }
        }
        "error" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::Error(p)))).await;
            }
        }
        _ => {
            diag!(Debug, Subsystem::AgentRun,
        
                "codex bridge: unhandled notification method={}",
                notification.method
            );
        }
    }
}

/// 处理 server → client 请求。当前仍自动审批，后续接入正式审批链路后
/// 改为产出 `BackboneEvent::ApprovalRequest` 并等待 application 层决策。
async fn handle_server_request(
    request: JSONRPCRequest,
    out_tx: &mpsc::Sender<Value>,
) -> Result<(), ConnectorError> {
    let result = match request.method.as_str() {
        "item/commandExecution/requestApproval" => json!({ "decision": "acceptForSession" }),
        "item/fileChange/requestApproval" => json!({ "decision": "acceptForSession" }),
        "item/tool/requestUserInput" => json!({ "answers": {} }),
        _ => Value::Null,
    };
    send_server_response(out_tx, request.id, result).await
}

#[async_trait::async_trait]
impl AgentConnector for CodexBridgeConnector {
    fn connector_id(&self) -> &'static str {
        "codex-bridge"
    }

    fn connector_type(&self) -> ConnectorType {
        ConnectorType::LocalExecutor
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            supports_cancel: true,
            supports_steering: true,
            supports_discovery: true,
            supports_variants: false,
            supports_model_override: true,
            supports_permission_policy: true,
            supports_source_session_title: true,
        }
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
        vec![AgentInfo {
            id: CODEX_EXECUTOR_ID.to_string(),
            name: "Codex".to_string(),
            variants: Vec::new(),
            available: executable_available("npx"),
        }]
    }

    async fn discover_options_stream(
        &self,
        executor: &str,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        if !is_codex_executor(executor) {
            return Err(ConnectorError::InvalidConfig(format!(
                "Codex bridge 不支持执行器: {executor}"
            )));
        }

        let _ = working_dir;
        Ok(Box::pin(futures::stream::once(async {
            codex_discovery_patch()
        })))
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.live_sessions.lock().await.contains_key(session_id)
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError> {
        let codex_config = to_codex_config(&context.session.executor_config).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "执行器 '{}' 不是有效的 Codex bridge 执行器",
                context.session.executor_config.executor
            ))
        })?;
        let prompt_text = build_prompt_text(&context, prompt)?;

        let mut process = tokio::process::Command::new("npx");
        process
            .args(["-y", "@openai/codex@0.124.0", "app-server"])
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(&context.session.working_directory)
            .env("NPM_CONFIG_LOGLEVEL", "error")
            .env("NODE_NO_WARNINGS", "1")
            .env("NO_COLOR", "1")
            .env("RUST_LOG", "error")
            .envs(context.session.environment_variables.clone());

        let mut child = spawn_codex_process(&mut process)
            .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stderr".to_string())
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stdin".to_string())
        })?;

        let cancel_token = CancellationToken::new();
        let (tx, rx) = mpsc::channel::<Result<BackboneEnvelope, ConnectorError>>(256);
        let (out_tx, mut out_rx) = mpsc::channel::<Value>(256);
        let pending: PendingResponseMap = Arc::new(Mutex::new(HashMap::new()));
        let request_counter = Arc::new(AtomicI64::new(1));
        let live_thread_id = Arc::new(Mutex::new(None));
        let live_active_turn_id = Arc::new(Mutex::new(None));
        self.live_sessions.lock().await.insert(
            session_id.to_string(),
            CodexLiveSession {
                cancel_token: cancel_token.clone(),
                out_tx: out_tx.clone(),
                pending: pending.clone(),
                request_counter: request_counter.clone(),
                thread_id: live_thread_id.clone(),
                active_turn_id: live_active_turn_id.clone(),
            },
        );

        let writer_tx = tx.clone();
        let writer_cancel = cancel_token.clone();
        tokio::spawn(async move {
            let mut stdin = stdin;
            loop {
                let payload = tokio::select! {
                    biased;
                    _ = writer_cancel.cancelled() => break,
                    payload = out_rx.recv() => match payload {
                        Some(payload) => payload,
                        None => break,
                    },
                };
                let encoded = match serde_json::to_string(&payload) {
                    Ok(encoded) => encoded,
                    Err(e) => {
                        let _ = writer_tx
                            .send(Err(ConnectorError::Runtime(format!(
                                "序列化 Codex RPC 消息失败: {e}"
                            ))))
                            .await;
                        break;
                    }
                };
                if stdin.write_all(encoded.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    let _ = writer_tx
                        .send(Err(ConnectorError::Runtime(
                            "写入 Codex app-server stdin 失败".to_string(),
                        )))
                        .await;
                    break;
                }
            }
        });

        let source = SourceInfo {
            connector_id: self.connector_id().to_string(),
            connector_type: connector_type_label(self.connector_type()).to_string(),
            executor_id: Some(context.session.executor_config.executor.to_string()),
        };
        let turn_id = context.session.turn_id.clone();
        let stream_session_id = session_id.to_string();

        let read_pending = pending.clone();
        let read_out_tx = out_tx.clone();
        let read_tx = tx.clone();
        let read_source = source.clone();
        let read_turn_id = turn_id.clone();
        let read_session_id = stream_session_id.clone();
        let read_cancel = cancel_token.clone();
        let read_active_turn_id = live_active_turn_id.clone();
        tokio::spawn(async move {
            let mut stdout_lines = BufReader::new(stdout).lines();
            loop {
                let next = tokio::select! {
                    biased;
                    _ = read_cancel.cancelled() => break,
                    next = stdout_lines.next_line() => next,
                };
                let line = match next {
                    Ok(Some(line)) => line,
                    Ok(None) => break,
                    Err(e) => {
                        let _ = read_tx
                            .send(Err(ConnectorError::Runtime(format!(
                                "读取 Codex app-server stdout 失败: {e}"
                            ))))
                            .await;
                        break;
                    }
                };

                if line.trim().is_empty() {
                    continue;
                }

                let message = match serde_json::from_str::<JSONRPCMessage>(&line) {
                    Ok(message) => message,
                    Err(_) => continue,
                };

                match message {
                    JSONRPCMessage::Response(response) => {
                        if let Some(waiter) = read_pending.lock().await.remove(&response.id) {
                            let _ = waiter.send(Ok(response.result));
                        }
                    }
                    JSONRPCMessage::Error(error) => {
                        if let Some(waiter) = read_pending.lock().await.remove(&error.id) {
                            let _ = waiter.send(Err(ConnectorError::Runtime(error.error.message)));
                        }
                    }
                    JSONRPCMessage::Request(request) => {
                        if let Err(e) = handle_server_request(request, &read_out_tx).await {
                            let _ = read_tx.send(Err(e)).await;
                        }
                    }
                    JSONRPCMessage::Notification(notification) => {
                        handle_server_notification(
                            notification,
                            &read_session_id,
                            &read_tx,
                            &read_source,
                            &read_turn_id,
                            &read_active_turn_id,
                        )
                        .await;
                    }
                }
            }

            let mut pending = read_pending.lock().await;
            for (_, waiter) in pending.drain() {
                let _ = waiter.send(Err(ConnectorError::Runtime(
                    "Codex app-server 连接已关闭".to_string(),
                )));
            }
        });

        let stderr_cancel = cancel_token.clone();
        tokio::spawn(async move {
            let mut stderr_lines = BufReader::new(stderr).lines();
            loop {
                let next = tokio::select! {
                    biased;
                    _ = stderr_cancel.cancelled() => break,
                    next = stderr_lines.next_line() => next,
                };
                match next {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        diag!(Debug, Subsystem::AgentRun,
        "codex app-server stderr: {}", line.trim());
                    }
                    _ => break,
                }
            }
        });

        let live_sessions = self.live_sessions.clone();
        let session_id_owned = session_id.to_string();
        let wait_cancel = cancel_token.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = wait_cancel.cancelled() => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
                _ = child.wait() => {}
            }
            live_sessions.lock().await.remove(&session_id_owned);
        });

        let handshake_result = async {
            let initialize_request = ClientRequest::Initialize {
                request_id: next_request_id(&request_counter),
                params: InitializeParams {
                    client_info: ClientInfo {
                        name: "agentdash-codex-bridge".to_string(),
                        title: None,
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    },
                    capabilities: Some(InitializeCapabilities {
                        experimental_api: true,
                        ..Default::default()
                    }),
                },
            };
            let _: InitializeResponse =
                send_rpc_request(&out_tx, &pending, initialize_request).await?;
            send_rpc_notification(&out_tx, ClientNotification::Initialized).await?;

            let get_account_request = ClientRequest::GetAccount {
                request_id: next_request_id(&request_counter),
                params: GetAccountParams {
                    refresh_token: false,
                },
            };
            let account: GetAccountResponse =
                send_rpc_request(&out_tx, &pending, get_account_request).await?;
            if account.requires_openai_auth && account.account.is_none() {
                return Err(ConnectorError::Runtime(
                    "Codex 未登录，请先完成 Codex 认证".to_string(),
                ));
            }

            let thread_start =
                build_thread_start_params(&codex_config, &context.session.working_directory);
            let (thread_id, source_title_envelope) =
                if let Some(follow_up_session_id) = follow_up_session_id {
                    let fork_request = ClientRequest::ThreadFork {
                        request_id: next_request_id(&request_counter),
                        params: build_thread_fork_params(
                            follow_up_session_id.to_string(),
                            &thread_start,
                        ),
                    };
                    let response: ThreadForkResponse =
                        send_rpc_request(&out_tx, &pending, fork_request).await?;
                    let source_title_envelope = make_thread_source_title_envelope(
                        session_id,
                        &source,
                        &turn_id,
                        &response.thread,
                    );
                    (response.thread.id, source_title_envelope)
                } else {
                    let start_request = ClientRequest::ThreadStart {
                        request_id: next_request_id(&request_counter),
                        params: thread_start,
                    };
                    let response: ThreadStartResponse =
                        send_rpc_request(&out_tx, &pending, start_request).await?;
                    let source_title_envelope = make_thread_source_title_envelope(
                        session_id,
                        &source,
                        &turn_id,
                        &response.thread,
                    );
                    (response.thread.id, source_title_envelope)
                };
            *live_thread_id.lock().await = Some(thread_id.clone());

            let _ = tx
                .send(Ok(make_envelope(
                    BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
                        executor_session_id: thread_id.clone(),
                    }),
                    session_id,
                    &source,
                    &turn_id,
                )))
                .await;
            if let Some(envelope) = source_title_envelope {
                let _ = tx.send(Ok(envelope)).await;
            }

            let turn_start_request = ClientRequest::TurnStart {
                request_id: next_request_id(&request_counter),
                params: TurnStartParams {
                    thread_id,
                    input: vec![UserInput::Text {
                        text: prompt_text,
                        text_elements: vec![],
                    }],
                    ..Default::default()
                },
            };
            let turn_start: TurnStartResponse =
                send_rpc_request(&out_tx, &pending, turn_start_request).await?;
            *live_active_turn_id.lock().await = Some(turn_start.turn.id);
            Ok::<(), ConnectorError>(())
        }
        .await;

        if let Err(err) = handshake_result {
            cancel_token.cancel();
            self.live_sessions.lock().await.remove(session_id);
            return Err(err);
        }

        Ok(Box::pin(CancelOnDropStream {
            inner: ReceiverStream::new(rx),
            cancel_token,
        }))
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        if let Some(session) = self.live_sessions.lock().await.get(session_id).cloned() {
            session.cancel_token.cancel();
        }
        Ok(())
    }

    async fn steer_session(
        &self,
        session_id: &str,
        expected_turn_id: &str,
        input: Vec<UserInput>,
    ) -> Result<(), ConnectorError> {
        let session = self
            .live_sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                ConnectorError::Runtime(format!(
                    "session `{session_id}` 当前没有活跃的 Codex app-server，无法运行中 steer"
                ))
            })?;
        let thread_id = session.thread_id.lock().await.clone().ok_or_else(|| {
            ConnectorError::Runtime("Codex thread 尚未就绪，无法 steer".to_string())
        })?;
        let active_turn_id = session.active_turn_id.lock().await.clone().ok_or_else(|| {
            ConnectorError::Runtime("Codex 当前没有 active turn，无法 steer".to_string())
        })?;
        if active_turn_id != expected_turn_id {
            return Err(ConnectorError::Runtime(format!(
                "Codex active turn 不匹配: expected={expected_turn_id}, actual={active_turn_id}"
            )));
        }
        let request = ClientRequest::TurnSteer {
            request_id: next_request_id(&session.request_counter),
            params: TurnSteerParams {
                thread_id,
                input,
                responsesapi_client_metadata: None,
                expected_turn_id: expected_turn_id.to_string(),
                additional_context: None,
                client_user_message_id: None,
            },
        };
        let response: TurnSteerResponse =
            send_rpc_request(&session.out_tx, &session.pending, request).await?;
        *session.active_turn_id.lock().await = Some(response.turn_id);
        Ok(())
    }

    async fn approve_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "当前 Codex bridge 尚未接入正式审批恢复链路".to_string(),
        ))
    }

    async fn reject_tool_call(
        &self,
        _session_id: &str,
        _tool_call_id: &str,
        _reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(
            "当前 Codex bridge 尚未接入正式审批恢复链路".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn thread_name_updated_maps_to_source_session_title_event() {
        let (tx, mut rx) = mpsc::channel(1);
        let source = SourceInfo {
            connector_id: "codex-bridge".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("CODEX".to_string()),
        };
        let active_turn_id = Arc::new(Mutex::new(None));

        handle_server_notification(
            JSONRPCNotification {
                method: "thread/name/updated".to_string(),
                params: Some(json!({
                    "threadId": "thread-1",
                    "threadName": "Codex Title",
                })),
            },
            "sess-1",
            &tx,
            &source,
            "turn-1",
            &active_turn_id,
        )
        .await;

        let envelope = rx
            .recv()
            .await
            .expect("notification should emit an event")
            .expect("event should be ok");
        match envelope.event {
            BackboneEvent::Platform(PlatformEvent::SourceSessionTitleUpdated {
                executor_session_id,
                title,
                preview,
                source,
            }) => {
                assert_eq!(executor_session_id.as_deref(), Some("thread-1"));
                assert_eq!(title, "Codex Title");
                assert_eq!(preview, None);
                assert_eq!(source, CODEX_SOURCE_TITLE);
            }
            event => panic!("expected source title event, got {event:?}"),
        }
    }

    #[tokio::test]
    async fn thread_compacted_maps_to_executor_context_compacted_event() {
        let (tx, mut rx) = mpsc::channel(1);
        let source = SourceInfo {
            connector_id: "codex-bridge".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("CODEX".to_string()),
        };
        let active_turn_id = Arc::new(Mutex::new(None));

        handle_server_notification(
            JSONRPCNotification {
                method: "thread/compacted".to_string(),
                params: Some(json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                })),
            },
            "sess-1",
            &tx,
            &source,
            "turn-1",
            &active_turn_id,
        )
        .await;

        let envelope = rx
            .recv()
            .await
            .expect("notification should emit an event")
            .expect("event should be ok");
        match envelope.event {
            BackboneEvent::ExecutorContextCompacted(payload) => {
                assert_eq!(payload.thread_id, "thread-1");
                assert_eq!(payload.turn_id, "turn-1");
            }
            event => panic!("expected executor context compacted event, got {event:?}"),
        }
    }
}
