use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_platform_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_platform_spi::{AgentTool, CapabilityState, RuntimeVfsOperation, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::platform_shell::{PlatformShell, PlatformShellCwd};
use crate::inline_persistence::InlineContentOverlay;
use crate::rewrite::find_mount_uri_candidates;
use crate::runtime_tool_execution::{
    VfsToolContent, VfsToolExecutionError, VfsToolExecutionResult, VfsToolUpdateSink,
};
use crate::service::{VfsService, ensure_runtime_vfs_access};
use crate::tools::common::{SharedRuntimeVfs, resolve_uri_path};
use crate::tools::{legacy_error, legacy_result, legacy_update_sink};
use crate::{
    ExecRequest, MaterializationRewrite, RewriteShellCommandOutput, ShellSessionReadRequest,
    ShellSessionResizeRequest, ShellSessionSnapshot, ShellSessionTerminateRequest,
    ShellSessionWriteRequest, VfsMaterializationService, format_mount_uri, resolve_mount,
};

const SHELL_EXEC_RESULT_OUTPUT_MAX_BYTES: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTerminalOwner {
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub runtime_thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone)]
pub struct ShellTerminalRegistration {
    pub owner: ShellTerminalOwner,
    pub terminal_id: String,
    pub mount_id: String,
    pub backend_id: String,
    pub cwd: String,
    pub capability: String,
}

pub struct ShellTerminalOutputSnapshot<'a> {
    pub terminal_id: &'a str,
    pub state: &'a str,
    pub exit_code: Option<i32>,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub pty: &'a str,
    pub next_seq: Option<u64>,
    pub truncated: bool,
    pub omitted_bytes: usize,
    /// `Some` means the output came from a cursor-based control read and contains incremental
    /// chunks. `None` means stdout/stderr/pty are a complete snapshot (the start response).
    pub chunks: Option<&'a [crate::ShellSessionOutputChunk]>,
}

pub trait ShellTerminalRegistry: Send + Sync {
    fn register_shell_terminal(&self, registration: ShellTerminalRegistration);
    fn resolve_shell_terminal(&self, terminal_id: &str) -> Option<ShellTerminalRegistration>;
    fn record_shell_terminal_output_snapshot(&self, snapshot: ShellTerminalOutputSnapshot<'_>);
    fn remove_shell_terminal(&self, terminal_id: &str);
}

#[derive(Clone)]
pub struct ShellExecExecutor {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    terminal_registry: Option<Arc<dyn ShellTerminalRegistry>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    terminal_owner: Option<ShellTerminalOwner>,
    session_id: String,
    turn_id: Option<String>,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    capability_state: CapabilityState,
}
impl ShellExecExecutor {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self {
            service,
            vfs,
            shell_output_registry: None,
            terminal_registry: None,
            materialization: None,
            terminal_owner: None,
            session_id: "session".to_string(),
            turn_id: None,
            overlay: None,
            identity: None,
            capability_state: CapabilityState::default(),
        }
    }

    pub fn with_shell_output_registry(
        mut self,
        registry: Arc<agentdash_relay::ShellOutputRegistry>,
    ) -> Self {
        self.shell_output_registry = Some(registry);
        self
    }

    pub fn with_terminal_registry(mut self, registry: Arc<dyn ShellTerminalRegistry>) -> Self {
        self.terminal_registry = Some(registry);
        self
    }

    pub fn with_terminal_owner(mut self, owner: ShellTerminalOwner) -> Self {
        self.terminal_owner = Some(owner);
        self
    }

    pub fn with_materialization_context(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
        session_id: String,
        turn_id: Option<String>,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        self.materialization = materialization;
        self.session_id = session_id;
        self.turn_id = turn_id;
        self.overlay = overlay;
        self.identity = identity;
        self
    }

    pub fn with_capability_state(mut self, capability_state: CapabilityState) -> Self {
        self.capability_state = capability_state;
        self
    }

    fn record_shell_session_snapshot(&self, terminal_id: &str, snapshot: &ShellSessionSnapshot) {
        let (stdout, stderr, pty) = shell_session_output_streams(&snapshot.chunks);
        if let Some(registry) = &self.terminal_registry {
            registry.record_shell_terminal_output_snapshot(ShellTerminalOutputSnapshot {
                terminal_id,
                state: &snapshot.state,
                exit_code: snapshot.exit_code,
                stdout: &stdout,
                stderr: &stderr,
                pty: &pty,
                next_seq: Some(snapshot.next_seq),
                truncated: snapshot.truncated,
                omitted_bytes: snapshot.omitted_bytes,
                chunks: Some(&snapshot.chunks),
            });
        }
    }

    async fn execute_control_operation(
        &self,
        params: &ShellExecParams,
        vfs: &agentdash_platform_spi::Vfs,
        access_policy: &agentdash_platform_spi::RuntimeVfsAccessPolicy,
        cancel: &CancellationToken,
    ) -> Result<VfsToolExecutionResult, VfsToolExecutionError> {
        let terminal_id = required_terminal_id(params)?;
        let registration = self
            .terminal_registry
            .as_ref()
            .and_then(|registry| registry.resolve_shell_terminal(&terminal_id))
            .ok_or_else(|| {
                VfsToolExecutionError::ExecutionFailed(format!(
                    "shell_exec 未找到可续接终端: {terminal_id}"
                ))
            })?;
        let owner = self.terminal_owner.as_ref().ok_or_else(|| {
            VfsToolExecutionError::ExecutionFailed(
                "shell_exec continuation missing canonical AgentRun owner".to_string(),
            )
        })?;
        if &registration.owner != owner {
            return Err(VfsToolExecutionError::ExecutionFailed(format!(
                "shell_exec terminal {terminal_id} does not belong to the current AgentRun"
            )));
        }

        match params.operation {
            ShellExecOperation::Read => {
                let snapshot = cancellable(
                    cancel,
                    self.service.shell_session_read_with_policy(
                        vfs,
                        Some(access_policy),
                        &registration.mount_id,
                        &ShellSessionReadRequest {
                            terminal_id: terminal_id.clone(),
                            after_seq: params.after_seq,
                            wait_ms: params.wait_ms,
                            max_bytes: params.max_bytes,
                        },
                    ),
                )
                .await?;
                self.record_shell_session_snapshot(&terminal_id, &snapshot);
                Ok(shell_session_snapshot_result(
                    "read",
                    &terminal_id,
                    &registration.cwd,
                    &snapshot,
                    Vec::new(),
                ))
            }
            ShellExecOperation::Write => {
                let write = cancellable(
                    cancel,
                    self.service.shell_session_write_with_policy(
                        vfs,
                        Some(access_policy),
                        &registration.mount_id,
                        &ShellSessionWriteRequest {
                            terminal_id: terminal_id.clone(),
                            data: params.data.clone().unwrap_or_default(),
                            close_stdin: params.close_stdin,
                            wait_ms: params.wait_ms,
                            max_bytes: params.max_bytes,
                        },
                    ),
                )
                .await?;
                self.record_shell_session_snapshot(&terminal_id, &write.snapshot);
                Ok(shell_session_snapshot_result(
                    "write",
                    &terminal_id,
                    &registration.cwd,
                    &write.snapshot,
                    vec![
                        format!("accepted: {}", write.accepted),
                        format!("stdin_closed: {}", write.stdin_closed),
                    ],
                ))
            }
            ShellExecOperation::Status => {
                let snapshot = cancellable(
                    cancel,
                    self.service.shell_session_read_with_policy(
                        vfs,
                        Some(access_policy),
                        &registration.mount_id,
                        &ShellSessionReadRequest {
                            terminal_id: terminal_id.clone(),
                            after_seq: Some(u64::MAX),
                            wait_ms: Some(0),
                            max_bytes: Some(0),
                        },
                    ),
                )
                .await?;
                self.record_shell_session_snapshot(&terminal_id, &snapshot);
                Ok(shell_session_snapshot_result(
                    "status",
                    &terminal_id,
                    &registration.cwd,
                    &snapshot,
                    Vec::new(),
                ))
            }
            ShellExecOperation::Resize => {
                let cols = params.cols.ok_or_else(|| {
                    VfsToolExecutionError::InvalidArguments(
                        "shell_exec.resize requires cols".to_string(),
                    )
                })?;
                let rows = params.rows.ok_or_else(|| {
                    VfsToolExecutionError::InvalidArguments(
                        "shell_exec.resize requires rows".to_string(),
                    )
                })?;
                cancellable(
                    cancel,
                    self.service.shell_session_resize_with_policy(
                        vfs,
                        Some(access_policy),
                        &registration.mount_id,
                        &ShellSessionResizeRequest {
                            terminal_id: terminal_id.clone(),
                            cols,
                            rows,
                        },
                    ),
                )
                .await?;
                Ok(VfsToolExecutionResult {
                    content: vec![VfsToolContent::text(format!(
                        "operation: resize\nterminal_id: {terminal_id}\ncols: {cols}\nrows: {rows}\nstatus: resized"
                    ))],
                    is_error: false,
                    details: Some(serde_json::json!({
                        "type": "shell_exec",
                        "operation": "resize",
                        "terminal_id": terminal_id,
                        "cols": cols,
                        "rows": rows,
                        "status": "resized",
                    })),
                })
            }
            ShellExecOperation::Terminate => {
                let result = cancellable(
                    cancel,
                    self.service.shell_session_terminate_with_policy(
                        vfs,
                        Some(access_policy),
                        &registration.mount_id,
                        &ShellSessionTerminateRequest {
                            terminal_id: terminal_id.clone(),
                        },
                    ),
                )
                .await?;
                Ok(VfsToolExecutionResult {
                    content: vec![VfsToolContent::text(shell_session_terminate_text(
                        &terminal_id,
                        &result.status,
                        &result.state,
                        result.exit_code,
                    ))],
                    is_error: false,
                    details: Some(serde_json::json!({
                        "type": "shell_exec",
                        "operation": "terminate",
                        "terminal_id": terminal_id,
                        "status": result.status,
                        "state": result.state,
                        "exit_code": result.exit_code,
                    })),
                })
            }
            ShellExecOperation::Start => unreachable!("start handled by execute"),
        }
    }
}

#[derive(Clone)]
pub struct ShellExecTool {
    executor: ShellExecExecutor,
}

impl ShellExecTool {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self {
            executor: ShellExecExecutor::new(service, vfs),
        }
    }

    pub fn with_shell_output_registry(
        mut self,
        registry: Arc<agentdash_relay::ShellOutputRegistry>,
    ) -> Self {
        self.executor = self.executor.with_shell_output_registry(registry);
        self
    }

    pub fn with_terminal_registry(mut self, registry: Arc<dyn ShellTerminalRegistry>) -> Self {
        self.executor = self.executor.with_terminal_registry(registry);
        self
    }

    pub fn with_terminal_owner(mut self, owner: ShellTerminalOwner) -> Self {
        self.executor = self.executor.with_terminal_owner(owner);
        self
    }

    pub fn with_materialization_context(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
        session_id: String,
        turn_id: Option<String>,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        self.executor = self.executor.with_materialization_context(
            materialization,
            session_id,
            turn_id,
            overlay,
            identity,
        );
        self
    }

    pub fn with_capability_state(mut self, capability_state: CapabilityState) -> Self {
        self.executor = self.executor.with_capability_state(capability_state);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShellExecOperation {
    #[default]
    Start,
    Read,
    Write,
    Status,
    Resize,
    Terminate,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellExecParams {
    /// Instruction-style operation. Defaults to `start` for backwards-compatible command execution.
    #[serde(default)]
    pub operation: ShellExecOperation,
    /// Working directory in `mount_id://relative/path` format for OS shell execution. Omit it to use the platform shell; use `platform://` explicitly to force the platform shell.
    pub cwd: Option<String>,
    /// The shell command to execute for `operation=start`.
    pub command: Option<String>,
    /// Hard process timeout in seconds. If omitted, the process may continue as a background session after the initial yield.
    pub timeout_secs: Option<u64>,
    /// Canonical terminal id returned by `operation=start`; required for read/write/status/resize/terminate.
    pub terminal_id: Option<String>,
    /// For read/write: return chunks with seq greater than this value.
    pub after_seq: Option<u64>,
    /// start/read/write 返回当前输出或状态前的等待窗口，省略时使用 runtime 默认值。
    pub wait_ms: Option<u64>,
    /// For read/write: maximum bytes of retained output to return.
    pub max_bytes: Option<usize>,
    /// For start: retained output buffer cap. For read/write, use max_bytes instead.
    pub max_output_bytes: Option<usize>,
    /// For write: bytes/text sent to stdin. Empty string means poll/read without writing bytes.
    pub data: Option<String>,
    /// For write: close stdin after writing data.
    #[serde(default)]
    pub close_stdin: bool,
    /// For start: run under a PTY; for resize: target columns.
    #[serde(default)]
    pub tty: bool,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

impl ShellExecExecutor {
    pub async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<VfsToolUpdateSink>,
    ) -> Result<VfsToolExecutionResult, VfsToolExecutionError> {
        if cancel.is_cancelled() {
            return Err(VfsToolExecutionError::Cancelled);
        }
        let params: ShellExecParams = serde_json::from_value(args).map_err(|error| {
            VfsToolExecutionError::InvalidArguments(format!("invalid arguments: {error}"))
        })?;
        let state = self.vfs.snapshot_state().await;
        let vfs = state.vfs;
        let access_policy = state.access_policy;
        if params.operation != ShellExecOperation::Start {
            return self
                .execute_control_operation(&params, &vfs, &access_policy, &cancel)
                .await;
        }
        let command = required_start_command(&params)?;
        if let Some(platform_cwd) = PlatformShellCwd::from_param(params.cwd.as_deref())
            .map_err(VfsToolExecutionError::ExecutionFailed)?
        {
            let platform_shell = PlatformShell::new(
                self.service.clone(),
                &vfs,
                &access_policy,
                platform_cwd,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
                &self.capability_state,
            );
            let result = tokio::select! {
                _ = cancel.cancelled() => return Err(VfsToolExecutionError::Cancelled),
                result = platform_shell.execute(&command) => result,
            };
            let aggregated_output = if result.stderr.is_empty() {
                result.stdout.clone()
            } else if result.stdout.is_empty() {
                result.stderr.clone()
            } else {
                format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr)
            };
            let mut details = result.details.clone();
            if let Some(object) = details.as_object_mut() {
                object.insert("original_command".to_string(), serde_json::json!(command));
                object.insert("cwd".to_string(), serde_json::json!(result.cwd));
                object.insert("exit_code".to_string(), serde_json::json!(result.exit_code));
                object.insert(
                    "aggregated_output".to_string(),
                    serde_json::json!(aggregated_output),
                );
            }
            return Ok(VfsToolExecutionResult {
                content: vec![VfsToolContent::text(platform_shell_result_text(
                    &command,
                    &result.cwd,
                    Some(result.exit_code),
                    "completed",
                    None,
                    None,
                    &result.stdout,
                    &result.stderr,
                ))],
                is_error: result.exit_code != 0,
                details: Some(details),
            });
        }
        let cwd_param = params
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                VfsToolExecutionError::ExecutionFailed(
                    "shell_exec.cwd 留空时应进入 platform shell；真实 OS shell cwd 必须显式使用 mount_id://relative/path"
                        .to_string(),
                )
            })?;
        if !cwd_param.contains("://") {
            return Err(VfsToolExecutionError::ExecutionFailed(format!(
                "shell_exec.cwd 必须留空使用 platform shell，或显式使用 mount_id://relative/path 指向 exec mount；收到 `{cwd_param}`"
            )));
        }
        let target =
            resolve_uri_path(&vfs, cwd_param).map_err(VfsToolExecutionError::ExecutionFailed)?;
        let cwd = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path.clone()
        };
        let display_cwd = format_mount_uri(&target.mount_id, &cwd_for_display(&cwd));
        let exec_mount = resolve_mount(
            &vfs,
            &target.mount_id,
            agentdash_platform_spi::MountCapability::Exec,
        )
        .map_err(VfsToolExecutionError::ExecutionFailed)?;
        ensure_runtime_vfs_access(
            &access_policy,
            &target.mount_id,
            &target.path,
            RuntimeVfsOperation::Exec,
        )
        .map_err(|error| VfsToolExecutionError::ExecutionFailed(error.to_string()))?;

        let rewrite_output = if let Some(materialization) = &self.materialization {
            cancellable(
                &cancel,
                materialization.rewrite_shell_command_with_policy(
                    crate::RewriteShellCommandInput {
                        vfs: &vfs,
                        exec_mount_id: &target.mount_id,
                        command: &command,
                        session_id: &self.session_id,
                        turn_id: self.turn_id.as_deref(),
                        tool_call_id: Some(tool_call_id),
                        overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                        identity: self.identity.as_ref(),
                    },
                    &access_policy,
                    &cwd,
                ),
            )
            .await?
        } else {
            RewriteShellCommandOutput {
                command: command.clone(),
                rewrites: Vec::new(),
            }
        };
        if !rewrite_output.rewrites.is_empty() {
            diag!(Info, Subsystem::Vfs,

                exec_mount_id = %exec_mount.id,
                rewrite_count = rewrite_output.rewrites.len(),
                "shell_exec command 中的 VFS URI 已物化并重写"
            );
            if let Some(on_update) = &on_update {
                on_update(vfs_uri_rewrite_notice(
                    &command,
                    &rewrite_output.command,
                    &rewrite_output.rewrites,
                ));
            }
        }
        let rewritten_command = rewrite_output.command.clone();
        if let Some(message) = unresolved_vfs_uri_message(&rewritten_command, &vfs) {
            return Err(VfsToolExecutionError::ExecutionFailed(message));
        }

        let streaming_call_id = self
            .shell_output_registry
            .as_ref()
            .map(|_| agentdash_relay::RelayMessage::new_id("stream-call"));

        // 注册流式输出通道 + 转发任务
        let forward_handle = if let (Some(registry), Some(call_id), Some(on_update)) =
            (&self.shell_output_registry, &streaming_call_id, &on_update)
        {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(call_id, tx);
            let cb = on_update.clone();
            Some(tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    let truncated = chunk.truncation.truncated;
                    let omitted_bytes = chunk.truncation.omitted_bytes;
                    cb(VfsToolExecutionResult {
                        content: vec![VfsToolContent::text(chunk.delta)],
                        is_error: false,
                        details: Some(serde_json::json!({
                            "type": "shell_output",
                            "stream": chunk.stream,
                            "truncated": truncated,
                            "omitted_bytes": omitted_bytes,
                        })),
                    });
                }
            }))
        } else {
            None
        };

        let registry = self.terminal_registry.as_ref().ok_or_else(|| {
            VfsToolExecutionError::ExecutionFailed(
                "shell_exec start missing terminal continuation registry".to_string(),
            )
        })?;
        let owner = self.terminal_owner.clone().ok_or_else(|| {
            VfsToolExecutionError::ExecutionFailed(
                "shell_exec start missing canonical AgentRun owner".to_string(),
            )
        })?;
        let terminal_id = agentdash_relay::RelayMessage::new_id("term");
        registry.register_shell_terminal(ShellTerminalRegistration {
            owner,
            terminal_id: terminal_id.clone(),
            mount_id: target.mount_id.clone(),
            backend_id: exec_mount.backend_id.clone(),
            cwd: display_cwd.clone(),
            capability: if params.tty {
                "interactive".to_string()
            } else {
                "read_only_output".to_string()
            },
        });

        let exec_result = cancellable(
            &cancel,
            self.service.exec_with_policy(
                &vfs,
                Some(&access_policy),
                &ExecRequest {
                    mount_id: target.mount_id.clone(),
                    cwd: cwd.clone(),
                    command: rewritten_command.clone(),
                    timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)),
                    terminal_id: Some(terminal_id.clone()),
                    streaming_call_id: streaming_call_id.clone(),
                    yield_time_ms: params.wait_ms,
                    max_output_bytes: params.max_output_bytes,
                    tty: params.tty,
                    cols: params.cols,
                    rows: params.rows,
                },
            ),
        )
        .await;

        // 清理通道
        if let Some(ref call_id) = streaming_call_id
            && let Some(registry) = &self.shell_output_registry
        {
            registry.unregister(call_id);
        }
        if let Some(handle) = forward_handle {
            handle.abort();
        }
        let result = match exec_result {
            Ok(result) => result,
            Err(error) => {
                registry.remove_shell_terminal(&terminal_id);
                return Err(error);
            }
        };

        let exit_code = result.exit_code;
        let merged = if !result.pty.trim().is_empty() {
            result.pty.clone()
        } else if result.stderr.trim().is_empty() {
            result.stdout.clone()
        } else if result.stdout.trim().is_empty() {
            format!("[stderr]\n{}", result.stderr)
        } else {
            format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr)
        };
        let (merged, extra_truncation) =
            agentdash_relay::truncate_live_output_text(&merged, SHELL_EXEC_RESULT_OUTPUT_MAX_BYTES);
        let result_truncated = result.truncated || extra_truncation.truncated;
        let result_omitted_bytes = result
            .omitted_bytes
            .saturating_add(extra_truncation.omitted_bytes);
        if let Some(registry) = &self.terminal_registry {
            registry.record_shell_terminal_output_snapshot(ShellTerminalOutputSnapshot {
                terminal_id: &terminal_id,
                state: &result.state,
                exit_code: result.exit_code,
                stdout: &result.stdout,
                stderr: &result.stderr,
                pty: &result.pty,
                next_seq: result.next_seq,
                truncated: result_truncated,
                omitted_bytes: result_omitted_bytes,
                chunks: None,
            });
        }
        Ok(VfsToolExecutionResult {
            content: vec![VfsToolContent::text(shell_exec_result_text(
                &command,
                &rewritten_command,
                &display_cwd,
                result.exit_code,
                &result.state,
                result.terminal_id.as_deref(),
                result.next_seq,
                &merged,
                !rewrite_output.rewrites.is_empty(),
                result_truncated,
                result_omitted_bytes,
            ))],
            is_error: exit_code.is_some_and(|code| code != 0),
            details: shell_exec_result_details(
                &command,
                &rewritten_command,
                &rewrite_output.rewrites,
                &result,
                &display_cwd,
                &merged,
                result_truncated,
                result_omitted_bytes,
            ),
        })
    }
}

#[async_trait]
impl AgentTool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "Execute and control a shell command through one instruction-style tool.\n\
         \n\
         Usage:\n\
         - operation defaults to `start`; use `read`, `write`, `status`, `resize`, or `terminate` to continue a running command.\n\
         - Omit cwd to run the platform shell: a restricted VFS-backed command set that supports pwd, ls, cat, cp, mv, rm, and echo.\n\
         - Use cwd=`platform://` to explicitly run the same platform shell.\n\
         - Use cwd=`mount_id://relative/path` to run the command in the real OS shell environment of an exec-capable mount.\n\
         - start returns terminal_id; pass that same terminal_id to read/write/status/resize/terminate. Do not look for a separate session id.\n\
         - start and read default to a 10000 ms wait window so quick commands usually return completed output directly.\n\
         - read returns retained output chunks after after_seq and may wait up to wait_ms.\n\
         - write sends data to stdin, optionally close_stdin=true, then returns newly available output.\n\
         - status is a zero-output state snapshot for the terminal_id.\n\
         - Platform shell commands operate on VFS paths and never start an OS process.\n\
         - Platform shell supports VFS command primitives plus narrow `>` redirection for `echo` and `cat`; shell operators, variables, globbing, and command substitution are not expanded or executed.\n\
         - stdout and stderr are returned separately, labeled as [stdout] and [stderr].\n\
         - The exit code is included in the output; non-zero exit codes are flagged as errors.\n\
         - timeout_secs is a hard process timeout for real OS shell execution; long-running commands return a background session after the initial yield.\n\
         - Prefer dedicated tools (fs_read, fs_glob, fs_grep) for focused read/search work."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ShellExecParams>()
    }

    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::Command)
    }

    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_shell_exec_lifecycle".to_string())
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<agentdash_agent_types::AgentToolResult, agentdash_agent_types::AgentToolError> {
        self.executor
            .execute(tool_call_id, args, cancel, legacy_update_sink(on_update))
            .await
            .map(legacy_result)
            .map_err(legacy_error)
    }
}

async fn cancellable<T, E>(
    cancel: &CancellationToken,
    future: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, VfsToolExecutionError>
where
    E: std::fmt::Display,
{
    tokio::select! {
        _ = cancel.cancelled() => Err(VfsToolExecutionError::Cancelled),
        result = future => result.map_err(|error| VfsToolExecutionError::ExecutionFailed(error.to_string())),
    }
}

fn required_start_command(params: &ShellExecParams) -> Result<String, VfsToolExecutionError> {
    params
        .command
        .as_deref()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            VfsToolExecutionError::InvalidArguments(
                "shell_exec.start requires non-empty command".to_string(),
            )
        })
}

fn required_terminal_id(params: &ShellExecParams) -> Result<String, VfsToolExecutionError> {
    params
        .terminal_id
        .as_deref()
        .map(str::trim)
        .filter(|terminal_id| !terminal_id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            VfsToolExecutionError::InvalidArguments(
                "shell_exec continuation operation requires terminal_id".to_string(),
            )
        })
}

fn vfs_uri_rewrite_notice(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> VfsToolExecutionResult {
    VfsToolExecutionResult {
        content: vec![VfsToolContent::text(format_vfs_uri_rewrite_notice(
            rewritten_command,
            rewrites,
        ))],
        is_error: false,
        details: Some(vfs_uri_rewrite_details(
            original_command,
            rewritten_command,
            rewrites,
        )),
    }
}

fn format_vfs_uri_rewrite_notice(
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> String {
    let mut lines = vec![format!(
        "vfs_uri_rewrite: {} URI(s) materialized",
        rewrites.len()
    )];
    for rewrite in rewrites {
        lines.push(format!("{} -> {}", rewrite.source_uri, rewrite.local_path));
    }
    lines.push(format!("executed_command: {rewritten_command}"));
    lines.join("\n")
}

fn vfs_uri_rewrite_details(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> serde_json::Value {
    serde_json::json!({
        "type": "vfs_uri_rewrite",
        "original_command": original_command,
        "executed_command": rewritten_command,
        "rewritten_command": rewritten_command,
        "rewrite_count": rewrites.len(),
        "rewrites": rewrites.iter().map(|rewrite| {
            serde_json::json!({
                "source_uri": rewrite.source_uri,
                "local_path": rewrite.local_path,
            })
        }).collect::<Vec<_>>(),
    })
}

fn shell_session_snapshot_result(
    operation: &str,
    terminal_id: &str,
    cwd: &str,
    snapshot: &ShellSessionSnapshot,
    extra_lines: Vec<String>,
) -> VfsToolExecutionResult {
    let merged = merge_shell_session_chunks(&snapshot.chunks);
    let (merged, extra_truncation) =
        agentdash_relay::truncate_live_output_text(&merged, SHELL_EXEC_RESULT_OUTPUT_MAX_BYTES);
    let truncated = snapshot.truncated || extra_truncation.truncated;
    let omitted_bytes = snapshot
        .omitted_bytes
        .saturating_add(extra_truncation.omitted_bytes);
    let mut lines = vec![
        format!("operation: {operation}"),
        format!("terminal_id: {terminal_id}"),
        format!("cwd: {cwd}"),
        format!("state: {}", snapshot.state),
    ];
    if let Some(exit_code) = snapshot.exit_code {
        lines.push(format!("exit_code: {exit_code}"));
    }
    lines.push(format!("next_seq: {}", snapshot.next_seq));
    lines.extend(extra_lines);
    if truncated {
        lines.push(format!(
            "output_truncated: true (omitted_bytes={omitted_bytes})"
        ));
    }
    if !merged.is_empty() {
        lines.push(merged.clone());
    }
    VfsToolExecutionResult {
        content: vec![VfsToolContent::text(lines.join("\n"))],
        is_error: snapshot.exit_code.is_some_and(|code| code != 0),
        details: Some(serde_json::json!({
            "type": "shell_exec",
            "operation": operation,
            "terminal_id": terminal_id,
            "cwd": cwd,
            "state": snapshot.state.as_str(),
            "aggregated_output": merged,
            "exit_code": snapshot.exit_code,
            "next_seq": snapshot.next_seq,
            "truncated": truncated,
            "omitted_bytes": omitted_bytes,
        })),
    }
}

fn merge_shell_session_chunks(chunks: &[crate::ShellSessionOutputChunk]) -> String {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut pty = String::new();
    for chunk in chunks {
        match chunk.stream.as_str() {
            "pty" => pty.push_str(&chunk.data),
            "stderr" => stderr.push_str(&chunk.data),
            _ => stdout.push_str(&chunk.data),
        }
    }
    if !pty.trim().is_empty() {
        pty
    } else if stderr.trim().is_empty() {
        stdout
    } else if stdout.trim().is_empty() {
        format!("[stderr]\n{stderr}")
    } else {
        format!("[stdout]\n{stdout}\n\n[stderr]\n{stderr}")
    }
}

fn shell_session_output_streams(
    chunks: &[crate::ShellSessionOutputChunk],
) -> (String, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut pty = Vec::new();
    for chunk in chunks {
        match chunk.stream.as_str() {
            "stderr" => stderr.push(chunk.data.as_str()),
            "pty" => pty.push(chunk.data.as_str()),
            _ => stdout.push(chunk.data.as_str()),
        }
    }
    (stdout.concat(), stderr.concat(), pty.concat())
}

fn shell_session_terminate_text(
    terminal_id: &str,
    status: &str,
    state: &str,
    exit_code: Option<i32>,
) -> String {
    let mut lines = vec![
        "operation: terminate".to_string(),
        format!("terminal_id: {terminal_id}"),
        format!("status: {status}"),
        format!("state: {state}"),
    ];
    if let Some(exit_code) = exit_code {
        lines.push(format!("exit_code: {exit_code}"));
    }
    lines.join("\n")
}

#[allow(clippy::too_many_arguments)]
fn shell_exec_result_text(
    original_command: &str,
    rewritten_command: &str,
    display_cwd: &str,
    exit_code: Option<i32>,
    state: &str,
    terminal_id: Option<&str>,
    next_seq: Option<u64>,
    merged_output: &str,
    has_rewrite: bool,
    truncated: bool,
    omitted_bytes: usize,
) -> String {
    let mut lines = vec![format!("command: {original_command}")];
    if has_rewrite {
        lines.push(format!("executed_command: {rewritten_command}"));
    }
    lines.push(format!("cwd: {display_cwd}"));
    lines.push(format!("state: {state}"));
    if let Some(exit_code) = exit_code {
        lines.push(format!("exit_code: {exit_code}"));
    }
    if let Some(terminal_id) = terminal_id {
        lines.push(format!("terminal_id: {terminal_id}"));
    }
    if let Some(next_seq) = next_seq {
        lines.push(format!("next_seq: {next_seq}"));
    }
    if truncated {
        lines.push(format!(
            "output_truncated: true (omitted_bytes={omitted_bytes})"
        ));
    }
    if !merged_output.is_empty() {
        lines.push(merged_output.to_string());
    }
    lines.join("\n")
}

fn cwd_for_display(cwd: &str) -> String {
    if cwd == "." {
        String::new()
    } else {
        cwd.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn platform_shell_result_text(
    command: &str,
    cwd: &str,
    exit_code: Option<i32>,
    state: &str,
    session_id: Option<&str>,
    next_seq: Option<u64>,
    stdout: &str,
    stderr: &str,
) -> String {
    let mut lines = vec![
        format!("command: {command}"),
        format!("cwd: {cwd}"),
        format!("state: {state}"),
    ];
    if let Some(exit_code) = exit_code {
        lines.push(format!("exit_code: {exit_code}"));
    }
    if let Some(session_id) = session_id {
        lines.push(format!("session_id: {session_id}"));
    }
    if let Some(next_seq) = next_seq {
        lines.push(format!("next_seq: {next_seq}"));
    }
    if !stdout.is_empty() {
        lines.push(stdout.to_string());
    }
    if !stderr.is_empty() {
        lines.push(format!("[stderr]\n{stderr}"));
    }
    lines.join("\n")
}

fn shell_exec_result_details(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
    result: &crate::ExecResult,
    cwd: &str,
    aggregated_output: &str,
    truncated: bool,
    omitted_bytes: usize,
) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "shell_exec",
        "operation": "start",
        "original_command": original_command,
        "executed_command": rewritten_command,
        "state": result.state.as_str(),
        "exit_code": result.exit_code,
        "cwd": cwd,
        "aggregated_output": aggregated_output,
        "terminal_id": result.terminal_id.as_deref(),
        "next_seq": result.next_seq,
        "truncated": truncated,
        "omitted_bytes": omitted_bytes,
        "rewrite": (!rewrites.is_empty()).then(|| vfs_uri_rewrite_details(original_command, rewritten_command, rewrites)),
    }))
}

fn unresolved_vfs_uri_message(command: &str, vfs: &agentdash_platform_spi::Vfs) -> Option<String> {
    let mut unresolved = unresolved_current_mount_uris(command, vfs);
    unresolved.extend(unresolved_reserved_vfs_uris(command));
    unresolved.sort();
    unresolved.dedup();
    if unresolved.is_empty() {
        return None;
    }

    Some(format!(
        "shell_exec 拒绝执行：命令中仍包含未物化的 VFS URI: {}。这类 URI 不能直接交给本机 shell 执行，否则会被当作普通路径/参数并可能超时；请确认当前 session VFS 包含对应 mount，且物化 rewrite 已在下发前成功。",
        unresolved.join(", ")
    ))
}

fn unresolved_current_mount_uris(command: &str, vfs: &agentdash_platform_spi::Vfs) -> Vec<String> {
    let mount_ids = vfs
        .mounts
        .iter()
        .map(|mount| mount.id.clone())
        .collect::<Vec<_>>();
    find_mount_uri_candidates(command, &mount_ids)
        .into_iter()
        .map(|candidate| candidate.value)
        .collect()
}

fn unresolved_reserved_vfs_uris(command: &str) -> Vec<String> {
    const RESERVED_VFS_SCHEMES: &[&str] = &["skill-assets", "lifecycle"];
    let mount_ids = RESERVED_VFS_SCHEMES
        .iter()
        .map(|scheme| scheme.to_string())
        .collect::<Vec<_>>();
    find_mount_uri_candidates(command, &mount_ids)
        .into_iter()
        .map(|candidate| candidate.value)
        .collect()
}
#[cfg(test)]
mod shell_exec_rewrite_tests {
    use super::*;
    use crate::{
        ListOptions, ListResult, MountError, MountOperationContext, MountProvider,
        MountProviderRegistryBuilder, ReadResult, SearchQuery, SearchResult,
        ShellSessionOutputChunk, ShellSessionWriteResult,
    };
    use agentdash_platform_spi::{Mount, Vfs};
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedTerminalSnapshot {
        state: String,
        stdout: String,
        stderr: String,
        pty: String,
        next_seq: Option<u64>,
    }

    #[derive(Default)]
    struct RecordingShellTerminalRegistry {
        registration: Mutex<Option<ShellTerminalRegistration>>,
        snapshots: Mutex<Vec<CapturedTerminalSnapshot>>,
    }

    impl ShellTerminalRegistry for RecordingShellTerminalRegistry {
        fn register_shell_terminal(&self, registration: ShellTerminalRegistration) {
            *self.registration.lock().expect("registration lock") = Some(registration);
        }

        fn resolve_shell_terminal(&self, terminal_id: &str) -> Option<ShellTerminalRegistration> {
            self.registration
                .lock()
                .expect("registration lock")
                .as_ref()
                .filter(|registration| registration.terminal_id == terminal_id)
                .cloned()
        }

        fn record_shell_terminal_output_snapshot(&self, snapshot: ShellTerminalOutputSnapshot<'_>) {
            self.snapshots
                .lock()
                .expect("snapshot lock")
                .push(CapturedTerminalSnapshot {
                    state: snapshot.state.to_string(),
                    stdout: snapshot.stdout.to_string(),
                    stderr: snapshot.stderr.to_string(),
                    pty: snapshot.pty.to_string(),
                    next_seq: snapshot.next_seq,
                });
        }

        fn remove_shell_terminal(&self, terminal_id: &str) {
            let mut registration = self.registration.lock().expect("registration lock");
            if registration
                .as_ref()
                .is_some_and(|registration| registration.terminal_id == terminal_id)
            {
                *registration = None;
            }
        }
    }

    struct ShellLifecycleProvider;

    #[async_trait::async_trait]
    impl MountProvider for ShellLifecycleProvider {
        fn provider_id(&self) -> &str {
            "shell_lifecycle"
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "list", "exec"]
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            Err(MountError::NotSupported(
                "shell lifecycle fixture".to_string(),
            ))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported(
                "shell lifecycle fixture".to_string(),
            ))
        }

        async fn list(
            &self,
            _mount: &Mount,
            _options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            Ok(ListResult {
                entries: Vec::new(),
            })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            Ok(SearchResult::default())
        }

        async fn exec(
            &self,
            _mount: &Mount,
            request: &ExecRequest,
            _ctx: &MountOperationContext,
        ) -> Result<crate::ExecResult, MountError> {
            Ok(crate::ExecResult {
                state: "running".to_string(),
                exit_code: None,
                stdout: "started\n".to_string(),
                stderr: String::new(),
                pty: String::new(),
                session_id: request.terminal_id.clone(),
                terminal_id: request.terminal_id.clone(),
                next_seq: Some(1),
                truncated: false,
                omitted_bytes: 0,
            })
        }

        async fn shell_session_read(
            &self,
            _mount: &Mount,
            _request: &ShellSessionReadRequest,
            _ctx: &MountOperationContext,
        ) -> Result<ShellSessionSnapshot, MountError> {
            Ok(ShellSessionSnapshot {
                state: "completed".to_string(),
                exit_code: Some(0),
                chunks: vec![ShellSessionOutputChunk {
                    seq: 2,
                    stream: "stdout".to_string(),
                    data: "retained completion\n".to_string(),
                }],
                next_seq: 3,
                truncated: false,
                omitted_bytes: 0,
            })
        }

        async fn shell_session_write(
            &self,
            _mount: &Mount,
            _request: &ShellSessionWriteRequest,
            _ctx: &MountOperationContext,
        ) -> Result<ShellSessionWriteResult, MountError> {
            Ok(ShellSessionWriteResult {
                accepted: true,
                stdin_closed: false,
                snapshot: ShellSessionSnapshot {
                    state: "running".to_string(),
                    exit_code: None,
                    chunks: vec![ShellSessionOutputChunk {
                        seq: 1,
                        stream: "pty".to_string(),
                        data: "input accepted\n".to_string(),
                    }],
                    next_seq: 2,
                    truncated: false,
                    omitted_bytes: 0,
                },
            })
        }
    }

    fn test_shell_tool() -> ShellExecExecutor {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: crate::PROVIDER_RELAY_FS.to_string(),
                backend_id: "local-dev-1".to_string(),
                root_ref: "D:\\workspace".to_string(),
                capabilities: vec![
                    agentdash_platform_spi::MountCapability::Read,
                    agentdash_platform_spi::MountCapability::Exec,
                ],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        ShellExecExecutor::new(
            Arc::new(VfsService::new(Arc::new(
                MountProviderRegistryBuilder::new().build(),
            ))),
            SharedRuntimeVfs::new(vfs),
        )
        .with_terminal_owner(ShellTerminalOwner {
            run_id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
            agent_id: uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                .expect("agent id"),
            runtime_thread_id: "runtime-thread-shell".parse().expect("runtime thread"),
        })
        .with_terminal_registry(Arc::new(RecordingShellTerminalRegistry::default()))
    }

    fn rewrite() -> MaterializationRewrite {
        MaterializationRewrite {
            source_uri: "skill-assets://skills/abc-user-lookup/scripts/lookup.py".to_string(),
            local_path: "C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py".to_string(),
        }
    }

    fn exec_result_fixture() -> crate::ExecResult {
        crate::ExecResult {
            state: "completed".to_string(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            pty: String::new(),
            session_id: None,
            terminal_id: None,
            next_seq: None,
            truncated: false,
            omitted_bytes: 0,
        }
    }

    #[test]
    fn rewrite_notice_exposes_mapping_and_rewritten_command() {
        let rewrites = vec![rewrite()];
        let result = vfs_uri_rewrite_notice(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao",
            &rewrites,
        );

        assert!(!result.is_error);
        let text = result.content[0].extract_text().expect("text content");
        assert!(text.contains("vfs_uri_rewrite"));
        assert!(text.contains("skill-assets://skills/abc-user-lookup/scripts/lookup.py"));
        assert!(text.contains("executed_command:"));
        let details = result.details.expect("details");
        assert_eq!(details["type"], "vfs_uri_rewrite");
        assert_eq!(
            details["executed_command"],
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
        );
        assert_eq!(details["rewrite_count"], 1);
        assert_eq!(
            details["rewrites"][0]["source_uri"],
            "skill-assets://skills/abc-user-lookup/scripts/lookup.py"
        );
    }

    #[test]
    fn shell_exec_result_shows_rewritten_command_only_when_rewritten() {
        let rewritten = shell_exec_result_text(
            "python skill-assets://skills/foo/scripts/run.py",
            "python \"C:\\agentdash\\materialized\\readonly\\skill-assets\\skills\\foo\\scripts\\run.py\"",
            "main://",
            Some(0),
            "completed",
            None,
            None,
            "ok",
            true,
            false,
            0,
        );
        assert!(rewritten.contains("executed_command:"));

        let plain = shell_exec_result_text(
            "echo ok",
            "echo ok",
            "main://",
            Some(0),
            "completed",
            None,
            None,
            "ok",
            false,
            false,
            0,
        );
        assert!(!plain.contains("executed_command:"));
    }

    #[test]
    fn shell_exec_result_uses_terminal_id_as_public_continuation_ref() {
        let text = shell_exec_result_text(
            "sleep 30",
            "sleep 30",
            "main://",
            None,
            "running",
            Some("term-123"),
            Some(4),
            "ready",
            false,
            false,
            0,
        );

        assert!(text.contains("terminal_id: term-123"));
        assert!(!text.contains("session_id:"));
    }

    #[test]
    fn shell_exec_params_default_to_start_operation() {
        let params: ShellExecParams =
            serde_json::from_value(serde_json::json!({ "command": "echo ok" })).expect("params");

        assert_eq!(params.operation, ShellExecOperation::Start);
        assert_eq!(params.command.as_deref(), Some("echo ok"));
    }

    #[tokio::test]
    async fn shell_exec_empty_cwd_uses_platform_shell() {
        let result = test_shell_tool()
            .execute(
                "tool-1",
                serde_json::json!({ "command": "pwd", "cwd": "" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("platform shell should run");

        assert!(!result.is_error);
        let text = result.content[0].extract_text().expect("text content");
        assert!(text.contains("cwd: platform://"));
        let details = result.details.expect("platform shell protocol details");
        assert_eq!(details["original_command"], "pwd");
        assert_eq!(details["cwd"], "platform://");
        assert_eq!(details["exit_code"], 0);
        assert!(details["aggregated_output"].as_str().is_some());
    }

    #[tokio::test]
    async fn shell_exec_rejects_local_relative_cwd() {
        let error = test_shell_tool()
            .execute(
                "tool-1",
                serde_json::json!({ "command": "pwd", "cwd": "." }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect_err("relative cwd should be rejected");

        assert!(error.to_string().contains("mount_id://relative/path"));
    }

    #[tokio::test]
    async fn shell_exec_rejects_pseudo_mount_cwd_without_uri_separator() {
        let error = test_shell_tool()
            .execute(
                "tool-1",
                serde_json::json!({ "command": "pwd", "cwd": "main:" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect_err("pseudo mount cwd should be rejected");

        assert!(error.to_string().contains("mount_id://relative/path"));
    }

    #[tokio::test]
    async fn direct_shell_executor_honors_pre_cancelled_execution() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let error = test_shell_tool()
            .execute(
                "tool-cancelled",
                serde_json::json!({ "command": "pwd", "cwd": "" }),
                cancel,
                None,
            )
            .await
            .expect_err("pre-cancelled direct execution should stop before dispatch");

        assert_eq!(error, VfsToolExecutionError::Cancelled);
    }

    #[test]
    fn direct_update_sink_preserves_typed_rewrite_payload() {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let captured = updates.clone();
        let sink: VfsToolUpdateSink = Arc::new(move |update| {
            captured.lock().expect("updates lock").push(update);
        });

        sink(vfs_uri_rewrite_notice(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py",
            "python C:\\materialized\\lookup.py",
            &[rewrite()],
        ));

        let updates = updates.lock().expect("updates lock");
        assert_eq!(updates.len(), 1);
        assert!(!updates[0].is_error);
        assert_eq!(
            updates[0].details.as_ref().unwrap()["type"],
            "vfs_uri_rewrite"
        );
    }

    #[tokio::test]
    async fn shell_exec_running_handle_continues_and_projects_retained_output() {
        let owner = ShellTerminalOwner {
            run_id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
            agent_id: uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                .expect("agent id"),
            runtime_thread_id: "runtime-thread-shell".parse().expect("runtime thread"),
        };
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: "shell_lifecycle".to_string(),
                backend_id: "backend-local".to_string(),
                root_ref: "D:\\workspace".to_string(),
                capabilities: vec![agentdash_platform_spi::MountCapability::Exec],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let service = Arc::new(VfsService::new(Arc::new(
            MountProviderRegistryBuilder::new()
                .register(Arc::new(ShellLifecycleProvider))
                .build(),
        )));
        let registry = Arc::new(RecordingShellTerminalRegistry::default());
        let shared_vfs = SharedRuntimeVfs::new(vfs);
        let tool = ShellExecExecutor::new(service.clone(), shared_vfs.clone())
            .with_terminal_owner(owner.clone())
            .with_terminal_registry(registry.clone());

        let started = tool
            .execute(
                "call-start",
                serde_json::json!({
                    "operation": "start",
                    "cwd": "main://",
                    "command": "long-running"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("start shell");
        let terminal_id = started
            .details
            .as_ref()
            .and_then(|details| details["terminal_id"].as_str())
            .expect("terminal id")
            .to_string();
        let registration = registry
            .registration
            .lock()
            .expect("registration lock")
            .clone()
            .expect("registration");
        assert_eq!(registration.owner, owner);
        assert_eq!(registration.terminal_id, terminal_id);

        tool.execute(
            "call-write",
            serde_json::json!({
                "operation": "write",
                "terminal_id": terminal_id,
                "data": "continue\n"
            }),
            CancellationToken::new(),
            None,
        )
        .await
        .expect("write continued shell");
        let completed = tool
            .execute(
                "call-read",
                serde_json::json!({
                    "operation": "read",
                    "terminal_id": terminal_id,
                    "after_seq": 1
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("read retained completed output");

        assert_eq!(completed.details.as_ref().unwrap()["state"], "completed");
        assert_eq!(
            completed.details.as_ref().unwrap()["aggregated_output"],
            "retained completion\n"
        );
        let snapshots = registry.snapshots.lock().expect("snapshot lock");
        assert_eq!(snapshots.len(), 3, "start/write/read all project snapshots");
        assert_eq!(snapshots[0].state, "running");
        assert_eq!(snapshots[1].pty, "input accepted\n");
        assert_eq!(snapshots[2].stdout, "retained completion\n");
        assert_eq!(snapshots[2].next_seq, Some(3));
        drop(snapshots);

        let other_owner_tool = ShellExecExecutor::new(service, shared_vfs)
            .with_terminal_owner(ShellTerminalOwner {
                run_id: uuid::Uuid::parse_str("33333333-3333-3333-3333-333333333333")
                    .expect("other run id"),
                agent_id: owner.agent_id,
                runtime_thread_id: owner.runtime_thread_id,
            })
            .with_terminal_registry(registry);
        let error = other_owner_tool
            .execute(
                "call-cross-owner-read",
                serde_json::json!({
                    "operation": "read",
                    "terminal_id": terminal_id,
                    "after_seq": 1
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect_err("cross-owner continuation must be rejected");
        assert!(
            error
                .to_string()
                .contains("does not belong to the current AgentRun")
        );
    }

    #[test]
    fn shell_exec_result_details_preserve_plain_terminal_fields() {
        let plain = shell_exec_result_details(
            "echo ok",
            "echo ok",
            &[],
            &exec_result_fixture(),
            "main://",
            "ok",
            false,
            0,
        )
        .expect("plain command details");
        assert_eq!(plain["cwd"], "main://");
        assert_eq!(plain["aggregated_output"], "ok");

        let rewrites = vec![rewrite()];
        let details = shell_exec_result_details(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao",
            &rewrites,
            &exec_result_fixture(),
            "main://",
            "ok",
            false,
            0,
        )
        .expect("rewrite details");
        assert_eq!(details["type"], "shell_exec");
        assert_eq!(
            details["executed_command"],
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
        );
        assert_eq!(details["rewrite"]["type"], "vfs_uri_rewrite");
    }

    #[test]
    fn shell_exec_result_details_are_present_for_truncation() {
        let details = shell_exec_result_details(
            "echo lots",
            "echo lots",
            &[],
            &exec_result_fixture(),
            "main://",
            "lots",
            true,
            1234,
        )
        .expect("truncation details");

        assert_eq!(details["type"], "shell_exec");
        assert_eq!(details["truncated"], true);
        assert_eq!(details["omitted_bytes"], 1234);
    }

    #[test]
    fn unresolved_vfs_uri_is_rejected_before_shell_execution() {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: crate::PROVIDER_RELAY_FS.to_string(),
                backend_id: "local-dev-1".to_string(),
                root_ref: "D:\\workspace".to_string(),
                capabilities: vec![agentdash_platform_spi::MountCapability::Exec],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let message = unresolved_vfs_uri_message(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            &vfs,
        )
        .expect("unresolved VFS URI should be rejected");

        assert!(message.contains("未物化的 VFS URI"));
        assert!(message.contains("skill-assets://skills/abc-user-lookup/scripts/lookup.py"));
    }
}
