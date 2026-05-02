use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo};
use agentdash_spi::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, PromptPayload,
};
use codex_app_server_protocol::{
    AskForApproval, ClientInfo, ClientNotification, ClientRequest, GetAccountParams,
    GetAccountResponse, InitializeCapabilities, InitializeParams, InitializeResponse,
    JSONRPCMessage, JSONRPCNotification, JSONRPCRequest, JSONRPCResponse, RequestId, SandboxMode,
    ThreadForkParams, ThreadForkResponse, ThreadStartParams, ThreadStartResponse, TurnStartParams,
    TurnStartResponse, UserInput,
};
use executors::{
    executors::{BaseCodingAgent, StandardCodingAgentExecutor as _},
    model_selector::PermissionPolicy,
    profile::{ExecutorConfigs, ExecutorProfileId},
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
use workspace_utils::command_ext::GroupSpawnNoWindowExt;

use crate::adapters::codex_config::to_codex_config;

const CODEX_EXECUTOR_ID: &str = "CODEX";

fn normalize_executor_id(executor: &str) -> String {
    executor.trim().replace('-', "_").to_ascii_uppercase()
}

fn is_codex_executor(executor: &str) -> bool {
    normalize_executor_id(executor) == CODEX_EXECUTOR_ID
}

type PendingResponseMap =
    Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<Value, ConnectorError>>>>>;

pub struct CodexBridgeConnector {
    default_repo_root: PathBuf,
    cancel_by_session: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl CodexBridgeConnector {
    /// 首阶段桥接：对外暴露独立 Codex connector，内部走原生 app-server 协议。
    /// 后续替换底层 SDK/运行时时，仅需继续演进该模块。
    pub fn new(default_repo_root: PathBuf) -> Self {
        Self {
            default_repo_root,
            cancel_by_session: Arc::new(Mutex::new(HashMap::new())),
        }
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

fn build_prompt_text(
    context: &ExecutionContext,
    prompt: &PromptPayload,
) -> Result<String, ConnectorError> {
    let user_text = prompt.to_fallback_text();
    // 过渡阶段沿用预渲染 system prompt，后续改为基于 Bundle 的原生渲染。
    #[allow(deprecated)]
    let prompt_text = if let Some(ref sys_prompt) = context.turn.assembled_system_prompt {
        format!("{sys_prompt}\n\n{user_text}")
    } else {
        user_text
    };
    let prompt_text = prompt_text.trim().to_string();
    if prompt_text.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "prompt payload 解析后为空".to_string(),
        ));
    }
    Ok(prompt_text)
}

fn build_thread_start_params(
    codex_config: &executors::profile::ExecutorConfig,
    working_directory: &PathBuf,
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
        Some(PermissionPolicy::Auto) => Some(AskForApproval::Never),
        Some(PermissionPolicy::Supervised) => Some(AskForApproval::UnlessTrusted),
        // 当前先保证协议通路可用；plan 专属协作模式后续单独补齐。
        Some(PermissionPolicy::Plan) => Some(AskForApproval::OnRequest),
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
        approval_policy: thread_start.approval_policy.clone(),
        sandbox: thread_start.sandbox.clone(),
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
                let _ = tx.send(Ok(wrap(BackboneEvent::ItemStarted(p)))).await;
            }
        }
        "item/completed" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::ItemCompleted(p)))).await;
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
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::TurnStarted(p)))).await;
            }
        }
        "turn/completed" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
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
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::TokenUsageUpdated(p)))).await;
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
        "context/compacted" => {
            if let Some(params) = notification.params
                && let Ok(p) = serde_json::from_value(params)
            {
                let _ = tx.send(Ok(wrap(BackboneEvent::ContextCompacted(p)))).await;
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
            tracing::debug!(
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
            supports_discovery: true,
            supports_variants: true,
            supports_model_override: true,
            supports_permission_policy: true,
        }
    }

    fn list_executors(&self) -> Vec<AgentInfo> {
        let configs = ExecutorConfigs::get_cached();
        let profile_id = ExecutorProfileId {
            executor: BaseCodingAgent::Codex,
            variant: None,
        };
        let available = configs
            .get_coding_agent(&profile_id)
            .map(|agent| agent.get_availability_info().is_available())
            .unwrap_or(false);

        let mut variants = configs
            .executors
            .get(&BaseCodingAgent::Codex)
            .map(|profile| profile.configurations.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        variants.sort();

        vec![AgentInfo {
            id: CODEX_EXECUTOR_ID.to_string(),
            name: "Codex".to_string(),
            variants,
            available,
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

        let profile_id = ExecutorProfileId {
            executor: BaseCodingAgent::Codex,
            variant: None,
        };
        let agent = ExecutorConfigs::get_cached()
            .get_coding_agent(&profile_id)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("找不到 Codex 执行器 profile".to_string())
            })?;

        let wd = working_dir.unwrap_or_else(|| self.default_repo_root.clone());
        agent
            .discover_options(Some(&wd), Some(&self.default_repo_root))
            .await
            .map_err(|e| ConnectorError::Runtime(format!("discover_options 失败: {e}")))
    }

    async fn has_live_session(&self, session_id: &str) -> bool {
        self.cancel_by_session.lock().await.contains_key(session_id)
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
            .args(["-y", "@openai/codex@0.121.0", "app-server"])
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

        let mut child = process
            .group_spawn_no_window()
            .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?;
        let stdout = child.inner().stdout.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stdout".to_string())
        })?;
        let stderr = child.inner().stderr.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stderr".to_string())
        })?;
        let stdin = child.inner().stdin.take().ok_or_else(|| {
            ConnectorError::SpawnFailed("Codex app-server 缺少 stdin".to_string())
        })?;

        let cancel_token = CancellationToken::new();
        self.cancel_by_session
            .lock()
            .await
            .insert(session_id.to_string(), cancel_token.clone());

        let (tx, rx) = mpsc::channel::<Result<BackboneEnvelope, ConnectorError>>(256);
        let (out_tx, mut out_rx) = mpsc::channel::<Value>(256);
        let pending: PendingResponseMap = Arc::new(Mutex::new(HashMap::new()));
        let request_counter = Arc::new(AtomicI64::new(1));

        let writer_tx = tx.clone();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(payload) = out_rx.recv().await {
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
        tokio::spawn(async move {
            let mut stdout_lines = BufReader::new(stdout).lines();
            loop {
                let line = match stdout_lines.next_line().await {
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

        tokio::spawn(async move {
            let mut stderr_lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr_lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                tracing::debug!("codex app-server stderr: {}", line.trim());
            }
        });

        let cancel_map = self.cancel_by_session.clone();
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
            cancel_map.lock().await.remove(&session_id_owned);
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
            let thread_id = if let Some(follow_up_session_id) = follow_up_session_id {
                let fork_request = ClientRequest::ThreadFork {
                    request_id: next_request_id(&request_counter),
                    params: build_thread_fork_params(
                        follow_up_session_id.to_string(),
                        &thread_start,
                    ),
                };
                let response: ThreadForkResponse =
                    send_rpc_request(&out_tx, &pending, fork_request).await?;
                response.thread.id
            } else {
                let start_request = ClientRequest::ThreadStart {
                    request_id: next_request_id(&request_counter),
                    params: thread_start,
                };
                let response: ThreadStartResponse =
                    send_rpc_request(&out_tx, &pending, start_request).await?;
                response.thread.id
            };

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
            let _: TurnStartResponse =
                send_rpc_request(&out_tx, &pending, turn_start_request).await?;
            Ok::<(), ConnectorError>(())
        }
        .await;

        if let Err(err) = handshake_result {
            cancel_token.cancel();
            self.cancel_by_session.lock().await.remove(session_id);
            return Err(err);
        }

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        if let Some(token) = self.cancel_by_session.lock().await.get(session_id).cloned() {
            token.cancel();
        }
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
