use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU64, Ordering},
    },
};

use agentdash_process::{ProcessDomain, background_tokio_command_with_cwd};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    process::Child,
    sync::{Mutex, RwLock, oneshot},
};

use crate::{
    CodexAppServerObservation, CodexAppServerObservationPage, CodexAppServerTransport,
    CodexCompleteAgentTransportError,
};

const CODEX_APP_SERVER_PACKAGE: &str = "@openai/codex@0.144.1";
const OBSERVATION_RETENTION: usize = 4096;

type PendingResponse = oneshot::Sender<Result<Value, CodexCompleteAgentTransportError>>;
type ProcessInput = Box<dyn AsyncWrite + Unpin + Send>;
type ProcessOutput = Box<dyn AsyncRead + Unpin + Send>;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RpcInbound {
    Error(RpcError),
    Response(RpcResponse),
    Request(RpcServerRequest),
    Notification(RpcServerNotification),
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    id: Value,
    result: Value,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    id: Value,
    error: RpcErrorBody,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct RpcServerRequest {
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct RpcServerNotification {
    method: String,
    #[serde(default)]
    params: Value,
}

struct RecordedObservation {
    source_thread_id: String,
    observation: CodexAppServerObservation,
}

#[derive(Default)]
struct ObservationState {
    retained: VecDeque<RecordedObservation>,
    disconnected: bool,
}

/// Production JSON-RPC transport for one Codex App Server process.
///
/// It owns only process correlation and a bounded live notification tail. Codex ThreadStore
/// remains authoritative; a tail gap is reported so the Complete Agent service reconciles through
/// `thread/read`.
pub struct CodexProcessTransport {
    _child: Option<Mutex<Child>>,
    stdin: Mutex<ProcessInput>,
    next_request_id: AtomicI64,
    next_observation_sequence: AtomicU64,
    pending: Mutex<BTreeMap<i64, PendingResponse>>,
    observations: RwLock<ObservationState>,
}

impl CodexProcessTransport {
    pub(crate) fn spawn(cwd: &Path) -> Result<Arc<Self>, CodexCompleteAgentTransportError> {
        if !cwd.is_absolute() {
            return Err(CodexCompleteAgentTransportError::protocol(
                "Codex process cwd must be absolute",
            ));
        }
        let mut command =
            background_tokio_command_with_cwd(ProcessDomain::CodexAppServer, "npx", cwd);
        let mut child = command
            .args(["-y", CODEX_APP_SERVER_PACKAGE, "app-server"])
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("NPM_CONFIG_LOGLEVEL", "error")
            .env("NO_COLOR", "1")
            .spawn()
            .map_err(|error| {
                CodexCompleteAgentTransportError::unavailable(
                    format!("failed to spawn Codex App Server: {error}"),
                    false,
                )
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            CodexCompleteAgentTransportError::unavailable(
                "Codex App Server stdin is unavailable",
                false,
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CodexCompleteAgentTransportError::unavailable(
                "Codex App Server stdout is unavailable",
                false,
            )
        })?;
        Ok(Self::from_io(
            Some(child),
            Box::new(stdin),
            Box::new(stdout),
        ))
    }

    fn from_io(child: Option<Child>, stdin: ProcessInput, stdout: ProcessOutput) -> Arc<Self> {
        let transport = Arc::new(Self {
            _child: child.map(Mutex::new),
            stdin: Mutex::new(stdin),
            next_request_id: AtomicI64::new(1),
            next_observation_sequence: AtomicU64::new(1),
            pending: Mutex::new(BTreeMap::new()),
            observations: RwLock::new(ObservationState::default()),
        });
        let pump = Arc::downgrade(&transport);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                let inbound = match lines.next_line().await {
                    Ok(Some(line)) => serde_json::from_str::<RpcInbound>(&line).map_err(|error| {
                        CodexCompleteAgentTransportError::protocol(format!(
                            "invalid Codex JSON-RPC frame: {error}"
                        ))
                    }),
                    Ok(None) => Err(CodexCompleteAgentTransportError::unavailable(
                        "Codex App Server closed stdout",
                        true,
                    )),
                    Err(error) => Err(CodexCompleteAgentTransportError::unavailable(
                        format!("failed to read Codex App Server stdout: {error}"),
                        true,
                    )),
                };
                let Some(transport) = pump.upgrade() else {
                    break;
                };
                match inbound {
                    Ok(frame) => transport.handle_inbound(frame).await,
                    Err(error) => {
                        transport.disconnect(error).await;
                        break;
                    }
                }
            }
        });
        transport
    }

    async fn handle_inbound(&self, inbound: RpcInbound) {
        match inbound {
            RpcInbound::Response(response) => {
                if let Some(id) = response.id.as_i64()
                    && let Some(pending) = self.pending.lock().await.remove(&id)
                {
                    let _ = pending.send(Ok(response.result));
                }
            }
            RpcInbound::Error(error) => {
                if let Some(id) = error.id.as_i64()
                    && let Some(pending) = self.pending.lock().await.remove(&id)
                {
                    let _ = pending.send(Err(CodexCompleteAgentTransportError::protocol(format!(
                        "Codex RPC {}: {}",
                        error.error.code, error.error.message
                    ))));
                }
            }
            RpcInbound::Notification(notification) => {
                self.record_observation(notification.params, |sequence, params| {
                    CodexAppServerObservation::Notification {
                        sequence,
                        method: notification.method,
                        params,
                    }
                })
                .await;
            }
            RpcInbound::Request(request) => {
                self.record_observation(request.params, |sequence, params| {
                    CodexAppServerObservation::ServerRequest {
                        sequence,
                        request_id: request.id,
                        method: request.method,
                        params,
                    }
                })
                .await;
            }
        }
    }

    async fn record_observation(
        &self,
        params: Value,
        build: impl FnOnce(u64, Value) -> CodexAppServerObservation,
    ) {
        let Some(source_thread_id) = source_thread_id(&params) else {
            return;
        };
        let sequence = self
            .next_observation_sequence
            .fetch_add(1, Ordering::Relaxed);
        let mut state = self.observations.write().await;
        state.retained.push_back(RecordedObservation {
            source_thread_id: source_thread_id.to_owned(),
            observation: build(sequence, params),
        });
        while state.retained.len() > OBSERVATION_RETENTION {
            state.retained.pop_front();
        }
    }

    async fn disconnect(&self, error: CodexCompleteAgentTransportError) {
        self.observations.write().await.disconnected = true;
        let pending = std::mem::take(&mut *self.pending.lock().await);
        for (_, response) in pending {
            let _ = response.send(Err(error.clone()));
        }
    }

    async fn write(&self, payload: Value) -> Result<(), CodexCompleteAgentTransportError> {
        let mut encoded = serde_json::to_vec(&payload).map_err(|error| {
            CodexCompleteAgentTransportError::protocol(format!(
                "failed to encode Codex JSON-RPC frame: {error}"
            ))
        })?;
        encoded.push(b'\n');
        self.stdin
            .lock()
            .await
            .write_all(&encoded)
            .await
            .map_err(|error| {
                CodexCompleteAgentTransportError::unavailable(
                    format!("failed to write Codex JSON-RPC frame: {error}"),
                    true,
                )
            })
    }
}

#[async_trait]
impl CodexAppServerTransport for CodexProcessTransport {
    async fn request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, CodexCompleteAgentTransportError> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        if let Err(error) = self
            .write(json!({ "id": id, "method": method, "params": params }))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        receiver.await.unwrap_or_else(|_| {
            Err(CodexCompleteAgentTransportError::unavailable(
                "Codex response correlation closed",
                true,
            ))
        })
    }

    async fn respond(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), CodexCompleteAgentTransportError> {
        self.write(json!({ "id": request_id, "result": result }))
            .await
    }

    async fn notify(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), CodexCompleteAgentTransportError> {
        let mut notification =
            serde_json::Map::from_iter([("method".to_owned(), Value::String(method.to_owned()))]);
        if let Some(params) = params {
            notification.insert("params".to_owned(), params);
        }
        self.write(Value::Object(notification)).await
    }

    async fn observations(
        &self,
        source_thread_id: &str,
        after_sequence: Option<u64>,
        limit: u32,
    ) -> Result<CodexAppServerObservationPage, CodexCompleteAgentTransportError> {
        let state = self.observations.read().await;
        if state.disconnected {
            return Err(CodexCompleteAgentTransportError::unavailable(
                "Codex App Server observation stream is disconnected",
                true,
            ));
        }
        let after = after_sequence.unwrap_or(0);
        let first_retained = state
            .retained
            .front()
            .map(|entry| entry.observation.sequence())
            .unwrap_or(after.saturating_add(1));
        let gap = after_sequence.is_some() && after.saturating_add(1) < first_retained;
        let observations = state
            .retained
            .iter()
            .filter(|entry| {
                entry.source_thread_id == source_thread_id && entry.observation.sequence() > after
            })
            .take(limit as usize)
            .map(|entry| entry.observation.clone())
            .collect::<Vec<_>>();
        let next_sequence = observations.last().map(CodexAppServerObservation::sequence);
        Ok(CodexAppServerObservationPage {
            observations,
            next_sequence,
            gap,
        })
    }
}

fn source_thread_id(params: &Value) -> Option<&str> {
    ["/threadId", "/thread/id", "/thread/id/value", "/thread_id"]
        .into_iter()
        .find_map(|pointer| params.pointer(pointer).and_then(Value::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use agentdash_agent_service_api::{
        AgentServiceDefinitionId, AgentServiceErrorCode, AgentServiceInstanceId,
    };
    use tokio::io::{AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};

    use crate::{CodexCompleteAgentConfig, CodexCompleteAgentRegistration};

    fn config() -> CodexCompleteAgentConfig {
        let cwd = std::env::current_dir().expect("cwd");
        CodexCompleteAgentConfig {
            definition_id: AgentServiceDefinitionId::new("codex").expect("definition"),
            title: "Codex".to_owned(),
            cwd: cwd.clone(),
            model: None,
            model_provider: None,
            base_instructions: None,
            developer_instructions: None,
            runtime_workspace_roots: vec![cwd],
        }
    }

    fn fixture() -> (
        Arc<CodexProcessTransport>,
        tokio::io::Lines<BufReader<ReadHalf<DuplexStream>>>,
        WriteHalf<DuplexStream>,
    ) {
        let (client, server) = tokio::io::duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        (
            CodexProcessTransport::from_io(None, Box::new(client_write), Box::new(client_read)),
            BufReader::new(server_read).lines(),
            server_write,
        )
    }

    async fn write_frame(writer: &mut WriteHalf<DuplexStream>, frame: Value) {
        let mut bytes = serde_json::to_vec(&frame).expect("encode frame");
        bytes.push(b'\n');
        writer.write_all(&bytes).await.expect("write frame");
    }

    fn valid_initialize_response(id: Value) -> Value {
        json!({
            "id": id,
            "result": {
                "userAgent": "codex-test",
                "codexHome": std::env::current_dir().expect("cwd"),
                "platformFamily": "windows",
                "platformOs": "windows"
            }
        })
    }

    async fn next_frame(lines: &mut tokio::io::Lines<BufReader<ReadHalf<DuplexStream>>>) -> Value {
        let line = lines.next_line().await.expect("read frame").expect("frame");
        serde_json::from_str(&line).expect("decode frame")
    }

    async fn assert_no_frame(lines: &mut tokio::io::Lines<BufReader<ReadHalf<DuplexStream>>>) {
        match tokio::time::timeout(Duration::from_millis(30), lines.next_line()).await {
            Err(_) | Ok(Ok(None)) => {}
            Ok(Ok(Some(line))) => panic!("unexpected frame after failed initialize: {line}"),
            Ok(Err(error)) => panic!("failed while checking frame absence: {error}"),
        }
    }

    #[tokio::test]
    async fn registration_orders_initialize_initialized_then_first_thread_method() {
        let (transport, mut lines, mut writer) = fixture();
        let registration = tokio::spawn(CodexCompleteAgentRegistration::new(
            AgentServiceInstanceId::new("codex-instance").expect("instance"),
            config(),
            transport.clone(),
        ));

        let initialize = next_frame(&mut lines).await;
        assert_eq!(initialize["method"], "initialize");
        assert_eq!(initialize["params"]["clientInfo"]["name"], "agentdash");
        assert_eq!(initialize["params"]["clientInfo"]["title"], "AgentDash");
        assert_eq!(
            initialize["params"]["clientInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            initialize["params"]["capabilities"],
            json!({
                "experimentalApi": true,
                "requestAttestation": false,
                "optOutNotificationMethods": null
            })
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), lines.next_line())
                .await
                .is_err(),
            "initialized must wait for a valid initialize response"
        );

        write_frame(
            &mut writer,
            valid_initialize_response(initialize["id"].clone()),
        )
        .await;
        let initialized = next_frame(&mut lines).await;
        assert_eq!(initialized, json!({ "method": "initialized" }));
        registration
            .await
            .expect("registration task")
            .expect("ready registration");

        let first_thread = {
            let transport = transport.clone();
            tokio::spawn(async move {
                transport
                    .request("thread/read", json!({ "threadId": "thread-1" }))
                    .await
            })
        };
        let thread_read = next_frame(&mut lines).await;
        assert_eq!(thread_read["method"], "thread/read");
        write_frame(
            &mut writer,
            json!({ "id": thread_read["id"].clone(), "result": {} }),
        )
        .await;
        first_thread
            .await
            .expect("thread task")
            .expect("thread response");
    }

    #[tokio::test]
    async fn initialize_error_rejects_registration_without_thread_command() {
        let (transport, mut lines, mut writer) = fixture();
        let registration = tokio::spawn(CodexCompleteAgentRegistration::new(
            AgentServiceInstanceId::new("codex-instance").expect("instance"),
            config(),
            transport,
        ));
        let initialize = next_frame(&mut lines).await;
        write_frame(
            &mut writer,
            json!({
                "id": initialize["id"].clone(),
                "error": { "code": -32602, "message": "invalid client" }
            }),
        )
        .await;
        let error = match registration.await.expect("registration task") {
            Ok(_) => panic!("initialize error must reject registration"),
            Err(error) => error,
        };
        assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
        assert_no_frame(&mut lines).await;
    }

    #[tokio::test]
    async fn initialize_eof_rejects_registration_without_thread_command() {
        let (transport, mut lines, mut writer) = fixture();
        let registration = tokio::spawn(CodexCompleteAgentRegistration::new(
            AgentServiceInstanceId::new("codex-instance").expect("instance"),
            config(),
            transport,
        ));
        let initialize = next_frame(&mut lines).await;
        assert_eq!(initialize["method"], "initialize");
        writer.shutdown().await.expect("shutdown server output");
        drop(writer);
        let error = match registration.await.expect("registration task") {
            Ok(_) => panic!("initialize EOF must reject registration"),
            Err(error) => error,
        };
        assert_eq!(error.code, AgentServiceErrorCode::Unavailable);
        assert_no_frame(&mut lines).await;
    }

    #[tokio::test]
    async fn invalid_initialize_response_rejects_registration_without_initialized_or_thread() {
        let (transport, mut lines, mut writer) = fixture();
        let registration = tokio::spawn(CodexCompleteAgentRegistration::new(
            AgentServiceInstanceId::new("codex-instance").expect("instance"),
            config(),
            transport,
        ));
        let initialize = next_frame(&mut lines).await;
        write_frame(
            &mut writer,
            json!({ "id": initialize["id"].clone(), "result": { "capabilities": {} } }),
        )
        .await;
        let error = match registration.await.expect("registration task") {
            Ok(_) => panic!("invalid initialize response must reject registration"),
            Err(error) => error,
        };
        assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
        assert_no_frame(&mut lines).await;
    }

    #[test]
    fn source_thread_identity_is_read_without_vendor_dto_leakage() {
        assert_eq!(
            source_thread_id(&json!({ "threadId": "thread-1" })),
            Some("thread-1")
        );
        assert_eq!(
            source_thread_id(&json!({ "thread": { "id": "thread-2" } })),
            Some("thread-2")
        );
        assert_eq!(
            source_thread_id(&json!({ "turn": { "id": "turn-1" } })),
            None
        );
    }
}
