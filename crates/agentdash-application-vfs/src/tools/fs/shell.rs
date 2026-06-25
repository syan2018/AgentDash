use std::sync::Arc;

use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{
    AgentTool, AgentToolError, AgentToolResult, CapabilityState, ContentPart, ToolUpdateCallback,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::platform_shell::{PlatformShell, PlatformShellCwd};
use crate::inline_persistence::InlineContentOverlay;
use crate::rewrite::find_mount_uri_candidates;
use crate::service::VfsService;
use crate::tools::common::{SharedRuntimeVfs, resolve_uri_path};
use crate::{
    ExecRequest, MaterializationRewrite, RewriteShellCommandOutput, VfsMaterializationService,
    format_mount_uri, resolve_mount,
};

const SHELL_EXEC_RESULT_OUTPUT_MAX_BYTES: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ShellExecTool {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    session_id: String,
    turn_id: Option<String>,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    capability_state: CapabilityState,
}
impl ShellExecTool {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self {
            service,
            vfs,
            shell_output_registry: None,
            materialization: None,
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

    pub fn with_materialization_context(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
        session_id: String,
        turn_id: Option<String>,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellExecParams {
    /// Working directory in `mount_id://relative/path` format for OS shell execution. Omit it to use the platform shell; use `platform://` explicitly to force the platform shell.
    pub cwd: Option<String>,
    /// The shell command to execute.
    pub command: String,
    /// Hard process timeout in seconds. If omitted, the process may continue as a background session after the initial yield.
    pub timeout_secs: Option<u64>,
}

#[async_trait]
impl AgentTool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }
    fn description(&self) -> &str {
        "Execute a shell command.\n\
         \n\
         Usage:\n\
         - Omit cwd to run the platform shell: a restricted VFS-backed command set that supports pwd, ls, cat, cp, mv, rm, and echo.\n\
         - Use cwd=`platform://` to explicitly run the same platform shell.\n\
         - Use cwd=`mount_id://relative/path` to run the command in the real OS shell environment of an exec-capable mount.\n\
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
    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        if let Some(platform_cwd) = PlatformShellCwd::from_param(params.cwd.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?
        {
            let result = PlatformShell::new(
                self.service.clone(),
                &vfs,
                platform_cwd,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
                &self.capability_state,
            )
            .execute(&params.command)
            .await;
            return Ok(AgentToolResult {
                content: vec![ContentPart::text(platform_shell_result_text(
                    &params.command,
                    &result.cwd,
                    Some(result.exit_code),
                    "completed",
                    None,
                    None,
                    &result.stdout,
                    &result.stderr,
                ))],
                is_error: result.exit_code != 0,
                details: Some(result.details),
            });
        }
        let cwd_param = params
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "shell_exec.cwd 留空时应进入 platform shell；真实 OS shell cwd 必须显式使用 mount_id://relative/path"
                        .to_string(),
                )
            })?;
        if !cwd_param.contains("://") {
            return Err(AgentToolError::ExecutionFailed(format!(
                "shell_exec.cwd 必须留空使用 platform shell，或显式使用 mount_id://relative/path 指向 exec mount；收到 `{cwd_param}`"
            )));
        }
        let target = resolve_uri_path(&vfs, cwd_param).map_err(AgentToolError::ExecutionFailed)?;
        let cwd = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let display_cwd = format_mount_uri(&target.mount_id, &cwd_for_display(&cwd));
        let exec_mount =
            resolve_mount(&vfs, &target.mount_id, agentdash_spi::MountCapability::Exec)
                .map_err(AgentToolError::ExecutionFailed)?;

        let rewrite_output = if let Some(materialization) = &self.materialization {
            materialization
                .rewrite_shell_command(crate::RewriteShellCommandInput {
                    vfs: &vfs,
                    exec_mount_id: &target.mount_id,
                    command: &params.command,
                    session_id: &self.session_id,
                    turn_id: self.turn_id.as_deref(),
                    tool_call_id: Some(_tool_call_id),
                    overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                    identity: self.identity.as_ref(),
                })
                .await
                .map_err(AgentToolError::ExecutionFailed)?
        } else {
            RewriteShellCommandOutput {
                command: params.command.clone(),
                rewrites: Vec::new(),
            }
        };
        if !rewrite_output.rewrites.is_empty() {
            tracing::info!(
                exec_mount_id = %exec_mount.id,
                rewrite_count = rewrite_output.rewrites.len(),
                "shell_exec command 中的 VFS URI 已物化并重写"
            );
            if let Some(on_update) = &on_update {
                on_update(vfs_uri_rewrite_notice(
                    &params.command,
                    &rewrite_output.command,
                    &rewrite_output.rewrites,
                ));
            }
        }
        let rewritten_command = rewrite_output.command.clone();
        if let Some(message) = unresolved_vfs_uri_message(&rewritten_command, &vfs) {
            return Err(AgentToolError::ExecutionFailed(message));
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
                    cb(AgentToolResult {
                        content: vec![ContentPart::text(chunk.delta)],
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

        let result = self
            .service
            .exec(
                &vfs,
                &ExecRequest {
                    mount_id: target.mount_id.clone(),
                    cwd: cwd.clone(),
                    command: rewritten_command.clone(),
                    timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)),
                    streaming_call_id: streaming_call_id.clone(),
                },
            )
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        // 清理通道
        if let Some(ref call_id) = streaming_call_id
            && let Some(registry) = &self.shell_output_registry
        {
            registry.unregister(call_id);
        }
        if let Some(handle) = forward_handle {
            handle.abort();
        }

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
        Ok(AgentToolResult {
            content: vec![ContentPart::text(shell_exec_result_text(
                &params.command,
                &rewritten_command,
                &display_cwd,
                result.exit_code,
                &result.state,
                result.session_id.as_deref(),
                result.next_seq,
                &merged,
                !rewrite_output.rewrites.is_empty(),
                result_truncated,
                result_omitted_bytes,
            ))],
            is_error: exit_code.is_some_and(|code| code != 0),
            details: shell_exec_result_details(
                &params.command,
                &rewritten_command,
                &rewrite_output.rewrites,
                &result,
                result_truncated,
                result_omitted_bytes,
            ),
        })
    }
}

fn vfs_uri_rewrite_notice(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(format_vfs_uri_rewrite_notice(
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

fn shell_exec_result_text(
    original_command: &str,
    rewritten_command: &str,
    display_cwd: &str,
    exit_code: Option<i32>,
    state: &str,
    session_id: Option<&str>,
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
    if let Some(session_id) = session_id {
        lines.push(format!("session_id: {session_id}"));
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
    truncated: bool,
    omitted_bytes: usize,
) -> Option<serde_json::Value> {
    (!rewrites.is_empty()
        || result.session_id.is_some()
        || truncated
        || omitted_bytes > 0)
        .then(|| {
        serde_json::json!({
            "type": "shell_exec",
            "original_command": original_command,
            "executed_command": rewritten_command,
            "state": result.state.as_str(),
            "exit_code": result.exit_code,
            "session_id": result.session_id.as_deref(),
            "terminal_id": result.terminal_id.as_deref(),
            "next_seq": result.next_seq,
            "truncated": truncated,
            "omitted_bytes": omitted_bytes,
            "rewrite": (!rewrites.is_empty()).then(|| vfs_uri_rewrite_details(original_command, rewritten_command, rewrites)),
        })
    })
}

fn unresolved_vfs_uri_message(command: &str, vfs: &agentdash_spi::Vfs) -> Option<String> {
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

fn unresolved_current_mount_uris(command: &str, vfs: &agentdash_spi::Vfs) -> Vec<String> {
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
    use crate::MountProviderRegistryBuilder;
    use agentdash_spi::{Mount, Vfs};

    fn test_shell_tool() -> ShellExecTool {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: crate::PROVIDER_RELAY_FS.to_string(),
                backend_id: "local-dev-1".to_string(),
                root_ref: "D:\\workspace".to_string(),
                capabilities: vec![
                    agentdash_spi::MountCapability::Read,
                    agentdash_spi::MountCapability::Exec,
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
        ShellExecTool::new(
            Arc::new(VfsService::new(Arc::new(
                MountProviderRegistryBuilder::new().build(),
            ))),
            SharedRuntimeVfs::new(vfs),
        )
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

    #[test]
    fn shell_exec_result_details_are_absent_without_rewrite() {
        assert!(
            shell_exec_result_details("echo ok", "echo ok", &[], &exec_result_fixture(), false, 0)
                .is_none()
        );

        let rewrites = vec![rewrite()];
        let details = shell_exec_result_details(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao",
            &rewrites,
            &exec_result_fixture(),
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
                capabilities: vec![agentdash_spi::MountCapability::Exec],
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
