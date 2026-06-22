use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agentdash_relay::*;
use codex_utils_output_truncation::{
    TruncationPolicy, approx_tokens_from_byte_count, truncate_text,
};
use codex_utils_pty::{
    DEFAULT_OUTPUT_BYTES_CAP, ProcessHandle, SpawnedProcess, TerminalSize, spawn_pipe_process,
    spawn_pty_process,
};
use tokio::sync::{Mutex, Notify, mpsc};

use crate::tool_executor::{ToolError, ToolExecutor};

const DEFAULT_YIELD_TIME_MS: u64 = 1_000;
const DEFAULT_READ_WAIT_MS: u64 = 0;
const DEFAULT_WRITE_WAIT_MS: u64 = 250;
const MAX_SHELL_SESSIONS: usize = 32;
const EXIT_OUTPUT_GRACE_MS: u64 = 50;

#[derive(Clone)]
pub struct ShellSessionManager {
    inner: Arc<Mutex<ShellSessionTable>>,
    tool_executor: ToolExecutor,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

#[derive(Default)]
struct ShellSessionTable {
    sessions: HashMap<String, ShellSession>,
}

struct ShellSession {
    session_id: String,
    call_id: Option<String>,
    terminal_id: Option<String>,
    state: ToolShellSessionState,
    exit_code: Option<i32>,
    handle: Arc<ProcessHandle>,
    buffer: RetainedOutputBuffer,
    live_output: LiveOutputEventBudget,
    notify: Arc<Notify>,
    created_at: Instant,
    updated_at: Instant,
    exited_at: Option<Instant>,
}

struct ShellSpawnSpec {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: HashMap<String, String>,
    tty: bool,
    cols: u16,
    rows: u16,
    call_id: Option<String>,
    terminal_id: Option<String>,
    max_output_bytes: usize,
    timeout_ms: Option<u64>,
}

struct LiveOutputEventBudget {
    max_bytes: usize,
    emitted_bytes: usize,
    omitted_bytes: usize,
    omitted_chunks: usize,
    truncation_notice_sent: bool,
}

impl LiveOutputEventBudget {
    fn new(session_max_bytes: usize) -> Self {
        Self {
            max_bytes: session_max_bytes.min(LIVE_OUTPUT_EVENT_MAX_BYTES).max(1),
            emitted_bytes: 0,
            omitted_bytes: 0,
            omitted_chunks: 0,
            truncation_notice_sent: false,
        }
    }

    fn push(&mut self, data: &str) -> Option<(String, ToolShellTruncationInfo)> {
        if data.is_empty() {
            return Some((String::new(), ToolShellTruncationInfo::default()));
        }

        let remaining = self.max_bytes.saturating_sub(self.emitted_bytes);
        if remaining == 0 {
            self.record_omitted(data.len());
            return self.truncation_notice_once();
        }

        let (mut bounded, truncation) = truncate_live_output_text(data, remaining);
        self.emitted_bytes = self.emitted_bytes.saturating_add(bounded.len());
        if truncation.truncated {
            self.record_omitted(truncation.omitted_bytes);
            append_output_truncation_notice(&mut bounded, self.omitted_bytes);
            return Some((bounded, self.truncation()));
        }

        Some((bounded, ToolShellTruncationInfo::default()))
    }

    fn record_omitted(&mut self, bytes: usize) {
        self.omitted_bytes = self.omitted_bytes.saturating_add(bytes);
        self.omitted_chunks = self.omitted_chunks.saturating_add(1);
    }

    fn truncation_notice_once(&mut self) -> Option<(String, ToolShellTruncationInfo)> {
        if self.truncation_notice_sent {
            return None;
        }
        self.truncation_notice_sent = true;
        let mut data = String::new();
        append_output_truncation_notice(&mut data, self.omitted_bytes);
        Some((data, self.truncation()))
    }

    fn truncation(&self) -> ToolShellTruncationInfo {
        ToolShellTruncationInfo {
            truncated: self.omitted_chunks > 0,
            omitted_bytes: self.omitted_bytes,
            omitted_chunks: self.omitted_chunks,
            omitted_tokens_estimate: if self.omitted_bytes > 0 {
                usize::try_from(approx_tokens_from_byte_count(self.omitted_bytes)).ok()
            } else {
                None
            },
        }
    }
}

fn append_output_truncation_notice(output: &mut String, omitted_bytes: usize) {
    if !output.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    output.push_str(&format!(
        "[output truncated: omitted_bytes={omitted_bytes}]\n"
    ));
}

#[derive(Default)]
struct RetainedOutputBuffer {
    head: Vec<ToolShellOutputChunk>,
    tail: VecDeque<ToolShellOutputChunk>,
    head_bytes: usize,
    tail_bytes: usize,
    omitted_bytes: usize,
    omitted_chunks: usize,
    next_seq: u64,
    max_bytes: usize,
}

impl RetainedOutputBuffer {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes: max_bytes.max(1),
            ..Self::default()
        }
    }

    fn push(&mut self, stream: ShellOutputStream, data: String) -> ToolShellOutputChunk {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let chunk = ToolShellOutputChunk { seq, stream, data };
        let size = chunk.data.len();
        let head_limit = (self.max_bytes / 4).max(1);
        if self.head.is_empty() && size > self.max_bytes {
            let mut retained = chunk.clone();
            retained.data = truncate_text(&retained.data, TruncationPolicy::Bytes(self.max_bytes));
            self.head_bytes = retained.data.len();
            self.omitted_bytes = size.saturating_sub(retained.data.len());
            self.omitted_chunks = 1;
            self.head.push(retained);
            return chunk;
        }

        if self.head_bytes < head_limit {
            self.head_bytes = self.head_bytes.saturating_add(size);
            self.head.push(chunk.clone());
            return chunk;
        }

        self.tail_bytes = self.tail_bytes.saturating_add(size);
        self.tail.push_back(chunk.clone());
        let tail_limit = self.max_bytes.saturating_sub(self.head_bytes).max(1);
        while self.tail_bytes > tail_limit {
            let Some(removed) = self.tail.pop_front() else {
                break;
            };
            self.tail_bytes = self.tail_bytes.saturating_sub(removed.data.len());
            self.omitted_bytes = self.omitted_bytes.saturating_add(removed.data.len());
            self.omitted_chunks = self.omitted_chunks.saturating_add(1);
        }
        chunk
    }

    fn chunks_after(
        &self,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
    ) -> Vec<ToolShellOutputChunk> {
        let mut out = Vec::new();
        let mut used = 0usize;
        let max_bytes = max_bytes.unwrap_or(usize::MAX);
        for chunk in self.head.iter().chain(self.tail.iter()) {
            if after_seq.is_some_and(|seq| chunk.seq <= seq) {
                continue;
            }
            let size = chunk.data.len();
            if !out.is_empty() && used.saturating_add(size) > max_bytes {
                break;
            }
            used = used.saturating_add(size);
            out.push(chunk.clone());
            if used >= max_bytes {
                break;
            }
        }
        out
    }

    fn truncation(&self) -> ToolShellTruncationInfo {
        ToolShellTruncationInfo {
            truncated: self.omitted_chunks > 0,
            omitted_bytes: self.omitted_bytes,
            omitted_chunks: self.omitted_chunks,
            omitted_tokens_estimate: if self.omitted_bytes > 0 {
                usize::try_from(approx_tokens_from_byte_count(self.omitted_bytes)).ok()
            } else {
                None
            },
        }
    }
}

impl ShellSessionManager {
    pub fn new(tool_executor: ToolExecutor, event_tx: mpsc::UnboundedSender<RelayMessage>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ShellSessionTable::default())),
            tool_executor,
            event_tx,
        }
    }

    pub async fn start_shell(
        &self,
        payload: ToolShellExecPayload,
    ) -> Result<ToolShellExecResponse, ToolError> {
        let cwd = self
            .tool_executor
            .process_executor()
            .resolve_cwd(&payload.mount_root_ref, payload.cwd.as_deref())?;
        let (program, args) = shell_program_and_args(&payload.command);
        let session_id = RelayMessage::new_id("shell");
        let spec = ShellSpawnSpec {
            program,
            args,
            cwd,
            env: current_env(),
            tty: payload.tty,
            cols: 80,
            rows: 24,
            call_id: Some(payload.call_id.clone()),
            terminal_id: None,
            max_output_bytes: payload.max_output_bytes.unwrap_or(DEFAULT_OUTPUT_BYTES_CAP),
            timeout_ms: payload.timeout_ms,
        };
        self.spawn_session(session_id.clone(), spec).await?;
        let read = self
            .read_session(
                &session_id,
                None,
                Some(payload.yield_time_ms.unwrap_or(DEFAULT_YIELD_TIME_MS)),
                payload.max_output_bytes,
            )
            .await
            .map_err(|message| ToolError::InvalidPath(message))?;
        let (stdout, stderr, pty) = split_output(&read.chunks);
        Ok(ToolShellExecResponse {
            call_id: payload.call_id,
            session_id: session_id.clone(),
            terminal_id: Some(session_id),
            state: read.state,
            exit_code: read.exit_code,
            stdout,
            stderr,
            pty,
            chunks: read.chunks,
            next_seq: read.next_seq,
            truncation: read.truncation,
        })
    }

    pub async fn spawn_terminal(
        &self,
        payload: &TerminalSpawnPayload,
        workspace_root: &Path,
    ) -> Result<TerminalSpawnResponse, String> {
        let cwd = resolve_terminal_cwd(workspace_root, payload.cwd.as_deref())?;
        let shell = payload.shell.clone().unwrap_or_else(default_shell);
        let session_id = payload.terminal_id.clone();
        let spec = ShellSpawnSpec {
            program: shell,
            args: Vec::new(),
            cwd,
            env: current_env(),
            tty: true,
            cols: payload.cols,
            rows: payload.rows,
            call_id: None,
            terminal_id: Some(payload.terminal_id.clone()),
            max_output_bytes: DEFAULT_OUTPUT_BYTES_CAP,
            timeout_ms: None,
        };
        self.spawn_session(session_id, spec)
            .await
            .map_err(|error| error.to_string())?;
        Ok(TerminalSpawnResponse {
            terminal_id: payload.terminal_id.clone(),
            process_id: None,
        })
    }

    pub async fn read_session(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        wait_ms: Option<u64>,
        max_bytes: Option<usize>,
    ) -> Result<ToolShellReadResponse, String> {
        let wait_ms = wait_ms.unwrap_or(DEFAULT_READ_WAIT_MS);
        let deadline = Instant::now() + Duration::from_millis(wait_ms);

        loop {
            let (snapshot, should_wait, notify) = {
                let table = self.inner.lock().await;
                let session = table
                    .sessions
                    .get(session_id)
                    .ok_or_else(|| format!("shell session not found: {session_id}"))?;
                let snapshot = shell_read_snapshot(session, after_seq, max_bytes);
                let should_wait = snapshot.chunks.is_empty()
                    && !is_terminal_shell_state(snapshot.state)
                    && Instant::now() < deadline;
                (snapshot, should_wait, Arc::clone(&session.notify))
            };

            if !should_wait {
                return Ok(snapshot);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return self.snapshot_now(session_id, after_seq, max_bytes).await;
            }
            let _ = tokio::time::timeout(remaining, notify.notified()).await;
        }
    }

    pub async fn input_shell(
        &self,
        payload: ToolShellInputPayload,
    ) -> Result<ToolShellInputResponse, String> {
        let (handle, after_seq) = {
            let table = self.inner.lock().await;
            let session = table
                .sessions
                .get(&payload.session_id)
                .ok_or_else(|| format!("shell session not found: {}", payload.session_id))?;
            (
                Arc::clone(&session.handle),
                session.buffer.next_seq.checked_sub(1),
            )
        };

        let mut accepted = true;
        let mut stdin_closed = false;
        if !payload.data.is_empty() {
            let writer = handle.writer_sender();
            if writer.send(payload.data.into_bytes()).await.is_err() {
                accepted = false;
                stdin_closed = true;
            }
        }

        let read_wait = if payload.wait_ms.is_some() {
            payload.wait_ms
        } else if accepted {
            Some(DEFAULT_WRITE_WAIT_MS)
        } else {
            Some(0)
        };
        let read = self
            .read_session(&payload.session_id, after_seq, read_wait, payload.max_bytes)
            .await?;
        Ok(ToolShellInputResponse {
            session_id: payload.session_id,
            accepted,
            stdin_closed,
            read,
        })
    }

    pub async fn terminate_shell(
        &self,
        payload: ToolShellTerminatePayload,
    ) -> Result<ToolShellTerminateResponse, String> {
        let mut event = None;
        let (status, state, exit_code) = {
            let mut table = self.inner.lock().await;
            let Some(session) = table.sessions.get_mut(&payload.session_id) else {
                return Ok(ToolShellTerminateResponse {
                    session_id: payload.session_id,
                    status: ToolShellTerminateStatus::UnknownSession,
                    state: ToolShellSessionState::Closed,
                    exit_code: None,
                });
            };
            if is_terminal_shell_state(session.state) {
                (
                    ToolShellTerminateStatus::AlreadyExited,
                    session.state,
                    session.exit_code,
                )
            } else {
                session.handle.request_terminate();
                session.state = ToolShellSessionState::Killed;
                session.exited_at = Some(Instant::now());
                session.updated_at = Instant::now();
                session.notify.notify_waiters();
                if let Some(terminal_id) = session.terminal_id.clone() {
                    event = Some(TerminalStateChangedPayload {
                        terminal_id,
                        state: TerminalProcessState::Killed,
                        exit_code: None,
                        message: Some("terminate requested".to_string()),
                    });
                }
                (
                    ToolShellTerminateStatus::Killed,
                    session.state,
                    session.exit_code,
                )
            }
        };
        if let Some(payload) = event {
            let _ = self.event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload,
            });
        }
        Ok(ToolShellTerminateResponse {
            session_id: payload.session_id,
            status,
            state,
            exit_code,
        })
    }

    pub async fn resize_terminal(&self, payload: &TerminalResizePayload) -> Result<(), String> {
        let handle = {
            let table = self.inner.lock().await;
            let session = table
                .sessions
                .get(&payload.terminal_id)
                .ok_or_else(|| format!("terminal not found: {}", payload.terminal_id))?;
            Arc::clone(&session.handle)
        };
        handle
            .resize(TerminalSize {
                rows: payload.rows,
                cols: payload.cols,
            })
            .map_err(|error| format!("resize failed: {error}"))
    }

    async fn spawn_session(
        &self,
        session_id: String,
        spec: ShellSpawnSpec,
    ) -> Result<(), ToolError> {
        self.prune_finished_sessions().await?;
        let spawned = if spec.tty {
            spawn_pty_process(
                &spec.program,
                &spec.args,
                &spec.cwd,
                &spec.env,
                &None,
                TerminalSize {
                    rows: spec.rows,
                    cols: spec.cols,
                },
            )
            .await
        } else {
            spawn_pipe_process(&spec.program, &spec.args, &spec.cwd, &spec.env, &None).await
        }
        .map_err(|error| ToolError::Io(std::io::Error::other(error)))?;

        self.insert_spawned_session(session_id, spec, spawned).await;
        Ok(())
    }

    async fn insert_spawned_session(
        &self,
        session_id: String,
        spec: ShellSpawnSpec,
        spawned: SpawnedProcess,
    ) {
        let SpawnedProcess {
            session,
            mut stdout_rx,
            mut stderr_rx,
            exit_rx,
        } = spawned;
        let handle = Arc::new(session);
        let notify = Arc::new(Notify::new());
        let now = Instant::now();
        let terminal_id = spec.terminal_id.clone();
        let session = ShellSession {
            session_id: session_id.clone(),
            call_id: spec.call_id.clone(),
            terminal_id: terminal_id.clone(),
            state: ToolShellSessionState::Running,
            exit_code: None,
            handle,
            buffer: RetainedOutputBuffer::new(spec.max_output_bytes),
            live_output: LiveOutputEventBudget::new(spec.max_output_bytes),
            notify,
            created_at: now,
            updated_at: now,
            exited_at: None,
        };
        self.inner
            .lock()
            .await
            .sessions
            .insert(session_id.clone(), session);

        if let Some(terminal_id) = terminal_id {
            let _ = self.event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload: TerminalStateChangedPayload {
                    terminal_id,
                    state: TerminalProcessState::Running,
                    exit_code: None,
                    message: None,
                },
            });
        }

        if let Some(timeout_ms) = spec.timeout_ms {
            let timeout_manager = self.clone();
            let timeout_session_id = session_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
                timeout_manager.mark_timed_out(&timeout_session_id).await;
            });
        }

        let stdout_manager = self.clone();
        let stdout_session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(bytes) = stdout_rx.recv().await {
                stdout_manager
                    .push_output(&stdout_session_id, ShellOutputStream::Stdout, bytes)
                    .await;
            }
        });

        let stderr_manager = self.clone();
        let stderr_session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(bytes) = stderr_rx.recv().await {
                stderr_manager
                    .push_output(&stderr_session_id, ShellOutputStream::Stderr, bytes)
                    .await;
            }
        });

        let exit_manager = self.clone();
        tokio::spawn(async move {
            let code = exit_rx.await.unwrap_or(-1);
            tokio::time::sleep(Duration::from_millis(EXIT_OUTPUT_GRACE_MS)).await;
            exit_manager.mark_exited(&session_id, code).await;
        });
    }

    async fn push_output(&self, session_id: &str, stream: ShellOutputStream, bytes: Vec<u8>) {
        let data = String::from_utf8_lossy(&bytes).to_string();
        let (chunk, live_output, call_id, terminal_id, notify) = {
            let mut table = self.inner.lock().await;
            let Some(session) = table.sessions.get_mut(session_id) else {
                return;
            };
            let stream =
                if session.terminal_id.is_some() && matches!(stream, ShellOutputStream::Stdout) {
                    ShellOutputStream::Pty
                } else {
                    stream
                };
            let chunk = session.buffer.push(stream, data.clone());
            let live_output = session.live_output.push(&chunk.data);
            session.updated_at = Instant::now();
            (
                chunk,
                live_output,
                session.call_id.clone(),
                session.terminal_id.clone(),
                Arc::clone(&session.notify),
            )
        };

        if let (Some(call_id), Some((delta, truncation))) = (call_id, live_output.clone()) {
            let _ = self.event_tx.send(RelayMessage::EventToolShellOutput {
                id: RelayMessage::new_id("shell-out"),
                payload: ToolShellOutputPayload {
                    call_id,
                    delta,
                    stream: chunk.stream,
                    truncation,
                },
            });
        }
        if let (Some(terminal_id), Some((data, truncation))) = (terminal_id, live_output) {
            let _ = self.event_tx.send(RelayMessage::EventTerminalOutput {
                id: RelayMessage::new_id("term-out"),
                payload: TerminalOutputPayload {
                    terminal_id,
                    data,
                    truncation,
                },
            });
        }
        notify.notify_waiters();
    }

    async fn mark_exited(&self, session_id: &str, code: i32) {
        let event = {
            let mut table = self.inner.lock().await;
            let Some(session) = table.sessions.get_mut(session_id) else {
                return;
            };
            if matches!(
                session.state,
                ToolShellSessionState::Killed | ToolShellSessionState::TimedOut
            ) {
                session.exit_code = Some(code);
            } else {
                session.state = if code == 0 {
                    ToolShellSessionState::Completed
                } else {
                    ToolShellSessionState::Failed
                };
                session.exit_code = Some(code);
            }
            session.exited_at = Some(Instant::now());
            session.updated_at = Instant::now();
            session.notify.notify_waiters();
            session.terminal_id.clone().map(|terminal_id| {
                let state = match session.state {
                    ToolShellSessionState::Killed => TerminalProcessState::Killed,
                    _ => TerminalProcessState::Exited,
                };
                TerminalStateChangedPayload {
                    terminal_id,
                    state,
                    exit_code: Some(code),
                    message: None,
                }
            })
        };
        if let Some(payload) = event {
            let _ = self.event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload,
            });
        }
    }

    async fn mark_timed_out(&self, session_id: &str) {
        let event = {
            let mut table = self.inner.lock().await;
            let Some(session) = table.sessions.get_mut(session_id) else {
                return;
            };
            if is_terminal_shell_state(session.state) {
                return;
            }
            session.handle.request_terminate();
            session.state = ToolShellSessionState::TimedOut;
            session.exited_at = Some(Instant::now());
            session.updated_at = Instant::now();
            session.notify.notify_waiters();
            session
                .terminal_id
                .clone()
                .map(|terminal_id| TerminalStateChangedPayload {
                    terminal_id,
                    state: TerminalProcessState::Killed,
                    exit_code: None,
                    message: Some("timeout reached".to_string()),
                })
        };
        if let Some(payload) = event {
            let _ = self.event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload,
            });
        }
    }

    async fn snapshot_now(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
    ) -> Result<ToolShellReadResponse, String> {
        let table = self.inner.lock().await;
        let session = table
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("shell session not found: {session_id}"))?;
        Ok(shell_read_snapshot(session, after_seq, max_bytes))
    }

    async fn prune_finished_sessions(&self) -> Result<(), ToolError> {
        let mut table = self.inner.lock().await;
        if table.sessions.len() < MAX_SHELL_SESSIONS {
            return Ok(());
        }

        let mut finished = table
            .sessions
            .iter()
            .filter(|(_, session)| is_terminal_shell_state(session.state))
            .map(|(id, session)| (id.clone(), session.exited_at.unwrap_or(session.created_at)))
            .collect::<Vec<_>>();
        finished.sort_by_key(|(_, exited_at)| *exited_at);
        while table.sessions.len() >= MAX_SHELL_SESSIONS {
            let Some((id, _)) = finished.first().cloned() else {
                return Err(ToolError::InvalidPath(format!(
                    "shell session 数量达到上限 {MAX_SHELL_SESSIONS}"
                )));
            };
            table.sessions.remove(&id);
            finished.remove(0);
        }
        Ok(())
    }
}

fn shell_read_snapshot(
    session: &ShellSession,
    after_seq: Option<u64>,
    max_bytes: Option<usize>,
) -> ToolShellReadResponse {
    ToolShellReadResponse {
        session_id: session.session_id.clone(),
        state: session.state,
        exit_code: session.exit_code,
        chunks: session.buffer.chunks_after(after_seq, max_bytes),
        next_seq: session.buffer.next_seq,
        truncation: session.buffer.truncation(),
    }
}

fn split_output(chunks: &[ToolShellOutputChunk]) -> (String, String, String) {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut pty = String::new();
    for chunk in chunks {
        match chunk.stream {
            ShellOutputStream::Stdout => stdout.push_str(&chunk.data),
            ShellOutputStream::Stderr => stderr.push_str(&chunk.data),
            ShellOutputStream::Pty => pty.push_str(&chunk.data),
        }
    }
    (stdout, stderr, pty)
}

fn is_terminal_shell_state(state: ToolShellSessionState) -> bool {
    matches!(
        state,
        ToolShellSessionState::Completed
            | ToolShellSessionState::Failed
            | ToolShellSessionState::TimedOut
            | ToolShellSessionState::Killed
            | ToolShellSessionState::Lost
            | ToolShellSessionState::Closed
    )
}

fn shell_program_and_args(command: &str) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        let command = format!(
            "$OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = $OutputEncoding; {command}"
        );
        (
            "powershell.exe".to_string(),
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-Command".to_string(),
                command,
            ],
        )
    }

    #[cfg(not(windows))]
    {
        (
            "sh".to_string(),
            vec!["-c".to_string(), command.to_string()],
        )
    }
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

fn current_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn resolve_terminal_cwd(workspace_root: &Path, cwd: Option<&str>) -> Result<PathBuf, String> {
    let candidate = cwd
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = Path::new(value);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                workspace_root.join(path)
            }
        })
        .unwrap_or_else(|| workspace_root.to_path_buf());

    let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
        format!(
            "terminal cwd 不存在或不可访问: {} ({error})",
            candidate.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!("terminal cwd 不是目录: {}", candidate.display()));
    }
    if !canonical.starts_with(workspace_root) {
        return Err(format!(
            "terminal cwd 越过 workspace 边界: {}",
            candidate.display()
        ));
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_print_then_sleep() -> String {
        if cfg!(windows) {
            "Write-Output ready; Start-Sleep -Seconds 8; Write-Output done".to_string()
        } else {
            "printf 'ready\\n'; sleep 3; printf 'done\\n'".to_string()
        }
    }

    fn command_sleep_long() -> String {
        if cfg!(windows) {
            "Start-Sleep -Seconds 5; Write-Output late".to_string()
        } else {
            "sleep 5; printf 'late\\n'".to_string()
        }
    }

    async fn wait_until_terminal(
        manager: &ShellSessionManager,
        session_id: &str,
        mut after_seq: Option<u64>,
    ) -> ToolShellReadResponse {
        let mut latest = manager
            .read_session(session_id, after_seq, Some(5_000), Some(16 * 1024))
            .await
            .expect("read shell session");
        for _ in 0..20 {
            if is_terminal_shell_state(latest.state) {
                return latest;
            }
            after_seq = latest.next_seq.checked_sub(1);
            latest = manager
                .read_session(session_id, after_seq, Some(5_000), Some(16 * 1024))
                .await
                .expect("read shell session");
        }
        latest
    }

    #[tokio::test]
    async fn start_shell_returns_running_after_yield_and_retains_tail() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let manager = ShellSessionManager::new(
            ToolExecutor::new(vec![workspace.path().to_path_buf()]),
            event_tx,
        );

        let response = manager
            .start_shell(ToolShellExecPayload {
                call_id: "call-1".to_string(),
                command: command_print_then_sleep(),
                mount_root_ref: workspace.path().to_string_lossy().to_string(),
                cwd: None,
                timeout_ms: None,
                yield_time_ms: Some(1_500),
                max_output_bytes: Some(16 * 1024),
                tty: false,
            })
            .await
            .expect("start shell");

        assert_eq!(
            response.state,
            ToolShellSessionState::Running,
            "response={response:?}"
        );
        assert!(response.exit_code.is_none());

        let read = wait_until_terminal(
            &manager,
            &response.session_id,
            response.next_seq.checked_sub(1),
        )
        .await;
        assert_eq!(read.state, ToolShellSessionState::Completed);
        let final_snapshot = manager
            .read_session(&response.session_id, None, Some(0), Some(16 * 1024))
            .await
            .expect("read retained output");
        let (stdout, _, _) = split_output(&final_snapshot.chunks);
        assert!(stdout.contains("ready"));
        assert!(stdout.contains("done"));
        assert_eq!(read.exit_code, Some(0));
    }

    #[tokio::test]
    async fn input_shell_writes_stdin_and_reads_output() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let manager = ShellSessionManager::new(
            ToolExecutor::new(vec![workspace.path().to_path_buf()]),
            event_tx,
        );
        let command = if cfg!(windows) {
            "$line = [Console]::In.ReadLine(); Write-Output \"echo:$line\""
        } else {
            "read line; printf 'echo:%s\\n' \"$line\""
        };

        let response = manager
            .start_shell(ToolShellExecPayload {
                call_id: "call-stdin".to_string(),
                command: command.to_string(),
                mount_root_ref: workspace.path().to_string_lossy().to_string(),
                cwd: None,
                timeout_ms: None,
                yield_time_ms: Some(10),
                max_output_bytes: Some(16 * 1024),
                tty: false,
            })
            .await
            .expect("start shell");

        let input = manager
            .input_shell(ToolShellInputPayload {
                session_id: response.session_id,
                data: "hello\n".to_string(),
                wait_ms: Some(2_000),
                max_bytes: Some(16 * 1024),
            })
            .await
            .expect("input");

        assert!(input.accepted);
        let (stdout, _, _) = split_output(&input.read.chunks);
        assert!(stdout.contains("echo:hello"));
    }

    #[tokio::test]
    async fn timeout_ms_marks_running_session_timed_out() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let manager = ShellSessionManager::new(
            ToolExecutor::new(vec![workspace.path().to_path_buf()]),
            event_tx,
        );

        let response = manager
            .start_shell(ToolShellExecPayload {
                call_id: "call-timeout".to_string(),
                command: command_sleep_long(),
                mount_root_ref: workspace.path().to_string_lossy().to_string(),
                cwd: None,
                timeout_ms: Some(300),
                yield_time_ms: Some(50),
                max_output_bytes: Some(16 * 1024),
                tty: false,
            })
            .await
            .expect("start shell");

        assert_eq!(response.state, ToolShellSessionState::Running);

        let read = wait_until_terminal(
            &manager,
            &response.session_id,
            response.next_seq.checked_sub(1),
        )
        .await;
        assert_eq!(read.state, ToolShellSessionState::TimedOut);
    }

    #[tokio::test]
    async fn retained_buffer_reports_truncation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let manager = ShellSessionManager::new(
            ToolExecutor::new(vec![workspace.path().to_path_buf()]),
            event_tx,
        );
        let command = if cfg!(windows) {
            "1..40 | ForEach-Object { Write-Output ('line' + $_) }"
        } else {
            "for i in $(seq 1 40); do echo line$i; done"
        };

        let response = manager
            .start_shell(ToolShellExecPayload {
                call_id: "call-trunc".to_string(),
                command: command.to_string(),
                mount_root_ref: workspace.path().to_string_lossy().to_string(),
                cwd: None,
                timeout_ms: None,
                yield_time_ms: Some(2_000),
                max_output_bytes: Some(80),
                tty: false,
            })
            .await
            .expect("start shell");

        let read = wait_until_terminal(
            &manager,
            &response.session_id,
            response.next_seq.checked_sub(1),
        )
        .await;
        assert!(read.truncation.truncated, "read={read:?}");
        assert!(read.truncation.omitted_bytes > 0);
    }

    #[test]
    fn live_output_budget_bounds_single_chunk_and_stops_repeated_omissions() {
        let mut budget = LiveOutputEventBudget::new(32);
        let (first, first_truncation) = budget
            .push(&"x".repeat(128))
            .expect("first oversized chunk should emit preview");
        assert!(first.len() <= 96, "first={first:?}");
        assert!(first.contains("output truncated"));
        assert!(first_truncation.truncated);
        assert!(first_truncation.omitted_bytes > 0);

        let (second, second_truncation) = budget
            .push("another omitted chunk")
            .expect("first fully omitted chunk should emit notice");
        assert!(second.contains("output truncated"));
        assert!(second_truncation.truncated);

        assert!(
            budget.push("one more omitted chunk").is_none(),
            "subsequent omissions should not keep emitting live events"
        );
    }
}
