use std::sync::Arc;

use agentdash_spi::platform::auth::AuthIdentity;
use agentdash_spi::platform::tool_capability::{CAP_FILE_READ, CAP_FILE_WRITE};
use agentdash_spi::{CapabilityState, ToolCluster, Vfs};
use serde_json::json;

use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::service::VfsService;
use crate::vfs::{ListOptions, ResourceRef, normalize_mount_relative_path, parse_mount_uri};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PlatformShellCwd {
    Root,
    Mount(ResourceRef),
}

impl PlatformShellCwd {
    pub(super) fn from_param(cwd: Option<&str>) -> Result<Option<Self>, String> {
        let Some(raw) = cwd else {
            return Ok(Some(Self::Root));
        };
        let trimmed = raw.trim();
        if !trimmed.starts_with("platform://") {
            return Ok(None);
        }
        let rest = trimmed["platform://".len()..].trim_start_matches('/');
        if rest.is_empty() || rest == "." {
            return Ok(Some(Self::Root));
        }
        let (mount_id, path) = rest.split_once('/').unwrap_or((rest, ""));
        if mount_id.trim().is_empty() {
            return Err("platform cwd 缺少 mount ID".to_string());
        }
        Ok(Some(Self::Mount(ResourceRef {
            mount_id: mount_id.to_string(),
            path: normalize_mount_relative_path(path, true)?,
        })))
    }

    fn display(&self) -> String {
        match self {
            Self::Root => "platform://".to_string(),
            Self::Mount(target) if target.path.is_empty() => {
                format!("platform://{}", target.mount_id)
            }
            Self::Mount(target) => format!("platform://{}/{}", target.mount_id, target.path),
        }
    }
}

pub(super) struct PlatformShell<'a> {
    service: Arc<VfsService>,
    vfs: &'a Vfs,
    cwd: PlatformShellCwd,
    overlay: Option<&'a InlineContentOverlay>,
    identity: Option<&'a AuthIdentity>,
    capability_state: &'a CapabilityState,
}

pub(super) struct PlatformShellResult {
    pub cwd: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub details: serde_json::Value,
}

impl PlatformShellResult {
    fn ok(cwd: String, stdout: String, operations: Vec<serde_json::Value>) -> Self {
        Self {
            cwd,
            exit_code: 0,
            stdout,
            stderr: String::new(),
            details: json!({
                "type": "platform_shell_exec",
                "operations": operations,
            }),
        }
    }

    fn error(cwd: String, message: impl Into<String>) -> Self {
        Self {
            cwd,
            exit_code: 2,
            stdout: String::new(),
            stderr: message.into(),
            details: json!({
                "type": "platform_shell_exec",
                "operations": [],
            }),
        }
    }
}

impl<'a> PlatformShell<'a> {
    pub(super) fn new(
        service: Arc<VfsService>,
        vfs: &'a Vfs,
        cwd: PlatformShellCwd,
        overlay: Option<&'a InlineContentOverlay>,
        identity: Option<&'a AuthIdentity>,
        capability_state: &'a CapabilityState,
    ) -> Self {
        Self {
            service,
            vfs,
            cwd,
            overlay,
            identity,
            capability_state,
        }
    }

    pub(super) async fn execute(&self, command: &str) -> PlatformShellResult {
        match self.execute_inner(command).await {
            Ok(result) => result,
            Err(message) => PlatformShellResult::error(self.cwd.display(), message),
        }
    }

    async fn execute_inner(&self, command: &str) -> Result<PlatformShellResult, String> {
        let argv = parse_argv(command)?;
        if argv.is_empty() {
            return Ok(PlatformShellResult::ok(
                self.cwd.display(),
                String::new(),
                Vec::new(),
            ));
        }

        let (argv, redirect) = split_redirect(argv)?;
        let command_name = argv
            .first()
            .ok_or_else(|| "platform shell 命令不能为空".to_string())?;
        let output = match command_name.as_str() {
            "pwd" => self.cmd_pwd(&argv, redirect).await,
            "ls" => self.cmd_ls(&argv, redirect).await,
            "cat" => self.cmd_cat(&argv, redirect).await,
            "cp" => self.cmd_cp(&argv, redirect).await,
            "mv" => self.cmd_mv(&argv, redirect).await,
            "rm" => self.cmd_rm(&argv, redirect).await,
            "echo" => self.cmd_echo(&argv, redirect).await,
            other => Err(format!(
                "platform shell 不支持命令 `{other}`；支持 pwd, ls, cat, cp, mv, rm, echo"
            )),
        }?;

        Ok(PlatformShellResult::ok(
            self.cwd.display(),
            output.stdout,
            output.operations,
        ))
    }

    async fn cmd_pwd(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        reject_redirect("pwd", &redirect)?;
        expect_arg_count(argv, 1)?;
        Ok(CommandOutput::text(self.cwd.display()))
    }

    async fn cmd_ls(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        reject_redirect("ls", &redirect)?;
        expect_arg_range(argv, 1, 2)?;
        self.ensure_read("fs_glob")?;
        if argv.len() == 1 && matches!(self.cwd, PlatformShellCwd::Root) {
            let mut mounts = self
                .vfs
                .mounts
                .iter()
                .map(|mount| format!("{}://", mount.id))
                .collect::<Vec<_>>();
            mounts.sort();
            return Ok(CommandOutput::with_operation(
                mounts.join("\n"),
                json!({"kind": "ls", "path": "platform://"}),
            ));
        }
        let target = self.resolve_arg(argv.get(1).map(String::as_str).unwrap_or("."))?;
        let result = self
            .service
            .list(
                self.vfs,
                &target.mount_id,
                ListOptions {
                    path: if target.path.is_empty() {
                        ".".to_string()
                    } else {
                        target.path.clone()
                    },
                    pattern: None,
                    recursive: false,
                },
                self.overlay,
                self.identity,
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut entries = result
            .entries
            .into_iter()
            .map(|entry| {
                if entry.is_dir {
                    format!("{}/", entry.path)
                } else {
                    entry.path
                }
            })
            .collect::<Vec<_>>();
        entries.sort();
        Ok(CommandOutput::with_operation(
            entries.join("\n"),
            json!({"kind": "ls", "path": format_resource_ref(&target)}),
        ))
    }

    async fn cmd_cat(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        expect_arg_count(argv, 2)?;
        self.ensure_read("fs_read")?;
        let source = self.resolve_arg(&argv[1])?;
        let content = self.read_text(&source).await?;
        if let Some(destination) = redirect {
            self.ensure_write()?;
            let destination = self.resolve_arg(&destination)?;
            self.write_text(&destination, &content).await?;
            return Ok(CommandOutput::with_operation(
                String::new(),
                json!({
                    "kind": "redirect",
                    "source": format_resource_ref(&source),
                    "destination": format_resource_ref(&destination),
                }),
            ));
        }
        Ok(CommandOutput::with_operation(
            content,
            json!({"kind": "cat", "path": format_resource_ref(&source)}),
        ))
    }

    async fn cmd_cp(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        reject_redirect("cp", &redirect)?;
        expect_arg_count(argv, 3)?;
        self.ensure_read("fs_read")?;
        self.ensure_write()?;
        let source = self.resolve_arg(&argv[1])?;
        let destination = self.resolve_arg(&argv[2])?;
        let content = self.read_text(&source).await?;
        self.write_text(&destination, &content).await?;
        Ok(CommandOutput::with_operation(
            format!(
                "copied {} -> {}",
                format_resource_ref(&source),
                format_resource_ref(&destination)
            ),
            json!({
                "kind": "copy",
                "source": format_resource_ref(&source),
                "destination": format_resource_ref(&destination),
            }),
        ))
    }

    async fn cmd_mv(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        reject_redirect("mv", &redirect)?;
        expect_arg_count(argv, 3)?;
        self.ensure_read("fs_read")?;
        self.ensure_write()?;
        let source = self.resolve_arg(&argv[1])?;
        let destination = self.resolve_arg(&argv[2])?;
        if source.mount_id == destination.mount_id {
            self.service
                .rename_text(
                    self.vfs,
                    &source.mount_id,
                    &source.path,
                    &destination.path,
                    self.overlay,
                    self.identity,
                )
                .await
                .map_err(|error| error.to_string())?;
        } else {
            let content = self.read_text(&source).await?;
            self.write_text(&destination, &content).await?;
            self.delete_text(&source).await?;
        }
        Ok(CommandOutput::with_operation(
            format!(
                "moved {} -> {}",
                format_resource_ref(&source),
                format_resource_ref(&destination)
            ),
            json!({
                "kind": "move",
                "source": format_resource_ref(&source),
                "destination": format_resource_ref(&destination),
            }),
        ))
    }

    async fn cmd_rm(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        reject_redirect("rm", &redirect)?;
        expect_arg_count(argv, 2)?;
        self.ensure_write()?;
        let target = self.resolve_arg(&argv[1])?;
        self.delete_text(&target).await?;
        Ok(CommandOutput::with_operation(
            format!("removed {}", format_resource_ref(&target)),
            json!({"kind": "remove", "path": format_resource_ref(&target)}),
        ))
    }

    async fn cmd_echo(
        &self,
        argv: &[String],
        redirect: Option<String>,
    ) -> Result<CommandOutput, String> {
        let text = if argv.len() <= 1 {
            String::new()
        } else {
            argv[1..].join(" ")
        };
        let text = format!("{text}\n");
        if let Some(destination) = redirect {
            self.ensure_write()?;
            let destination = self.resolve_arg(&destination)?;
            self.write_text(&destination, &text).await?;
            return Ok(CommandOutput::with_operation(
                String::new(),
                json!({
                    "kind": "redirect",
                    "source": "echo",
                    "destination": format_resource_ref(&destination),
                }),
            ));
        }
        Ok(CommandOutput::with_operation(text, json!({"kind": "echo"})))
    }

    fn resolve_arg(&self, raw: &str) -> Result<ResourceRef, String> {
        if raw.contains("://") {
            return parse_mount_uri(raw, self.vfs);
        }
        let PlatformShellCwd::Mount(cwd) = &self.cwd else {
            return Err(format!(
                "path `{raw}` 缺少 mount 前缀；platform shell 根目录下请使用 mount_id://relative/path"
            ));
        };
        let joined = if cwd.path.is_empty() {
            raw.to_string()
        } else {
            format!("{}/{}", cwd.path, raw)
        };
        Ok(ResourceRef {
            mount_id: cwd.mount_id.clone(),
            path: normalize_mount_relative_path(&joined, true)?,
        })
    }

    async fn read_text(&self, target: &ResourceRef) -> Result<String, String> {
        self.service
            .read_text(self.vfs, target, self.overlay, self.identity)
            .await
            .map(|result| result.content)
            .map_err(|error| error.to_string())
    }

    async fn write_text(&self, target: &ResourceRef, content: &str) -> Result<(), String> {
        self.service
            .write_text(self.vfs, target, content, self.overlay, self.identity)
            .await
            .map_err(|error| error.to_string())
    }

    async fn delete_text(&self, target: &ResourceRef) -> Result<(), String> {
        self.service
            .delete_text(self.vfs, target, self.overlay, self.identity)
            .await
            .map_err(|error| error.to_string())
    }

    fn ensure_read(&self, tool_name: &str) -> Result<(), String> {
        if self.capability_state.is_capability_tool_enabled(
            CAP_FILE_READ,
            tool_name,
            Some(ToolCluster::Read),
        ) {
            Ok(())
        } else {
            Err(format!("platform shell 缺少 file_read::{tool_name} 权限"))
        }
    }

    fn ensure_write(&self) -> Result<(), String> {
        if self.capability_state.is_capability_tool_enabled(
            CAP_FILE_WRITE,
            "fs_apply_patch",
            Some(ToolCluster::Write),
        ) {
            Ok(())
        } else {
            Err("platform shell 缺少 file_write::fs_apply_patch 权限".to_string())
        }
    }
}

struct CommandOutput {
    stdout: String,
    operations: Vec<serde_json::Value>,
}

impl CommandOutput {
    fn text(stdout: String) -> Self {
        Self {
            stdout,
            operations: Vec::new(),
        }
    }

    fn with_operation(stdout: String, operation: serde_json::Value) -> Self {
        Self {
            stdout,
            operations: vec![operation],
        }
    }
}

fn parse_argv(command: &str) -> Result<Vec<String>, String> {
    shell_words::split(command).map_err(|error| format!("命令解析失败: {error}"))
}

fn split_redirect(mut argv: Vec<String>) -> Result<(Vec<String>, Option<String>), String> {
    let redirects = argv
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (token == ">").then_some(index))
        .collect::<Vec<_>>();
    match redirects.as_slice() {
        [] => Ok((argv, None)),
        [index] => {
            if *index + 2 != argv.len() {
                return Err("platform shell 的 `>` 后必须且只能跟一个目标路径".to_string());
            }
            let target = argv.remove(*index + 1);
            argv.remove(*index);
            Ok((argv, Some(target)))
        }
        _ => Err("platform shell 每条命令最多支持一个 `>` 重定向".to_string()),
    }
}

fn reject_redirect(command: &str, redirect: &Option<String>) -> Result<(), String> {
    if redirect.is_some() {
        Err(format!("platform shell 命令 `{command}` 不支持重定向"))
    } else {
        Ok(())
    }
}

fn expect_arg_count(argv: &[String], expected: usize) -> Result<(), String> {
    if argv.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "`{}` 需要 {} 个参数，实际收到 {} 个",
            argv.first().map(String::as_str).unwrap_or("<empty>"),
            expected.saturating_sub(1),
            argv.len().saturating_sub(1)
        ))
    }
}

fn expect_arg_range(argv: &[String], min: usize, max: usize) -> Result<(), String> {
    if (min..=max).contains(&argv.len()) {
        Ok(())
    } else {
        Err(format!(
            "`{}` 需要 {}-{} 个参数，实际收到 {} 个",
            argv.first().map(String::as_str).unwrap_or("<empty>"),
            min.saturating_sub(1),
            max.saturating_sub(1),
            argv.len().saturating_sub(1)
        ))
    }
}

fn format_resource_ref(target: &ResourceRef) -> String {
    if target.path.is_empty() {
        format!("{}://", target.mount_id)
    } else {
        format!("{}://{}", target.mount_id, target.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{
        MountError, MountOperationContext, MountProvider, MountProviderRegistryBuilder, ReadResult,
        RuntimeFileEntry,
    };
    use agentdash_spi::{Mount, MountCapability, ToolCapability};
    use async_trait::async_trait;
    use std::collections::BTreeMap;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct MemoryProvider {
        files: Mutex<BTreeMap<String, String>>,
    }

    impl MemoryProvider {
        async fn insert(&self, path: &str, content: &str) {
            self.files
                .lock()
                .await
                .insert(path.to_string(), content.to_string());
        }

        async fn get(&self, path: &str) -> Option<String> {
            self.files.lock().await.get(path).cloned()
        }
    }

    #[async_trait]
    impl MountProvider for MemoryProvider {
        fn provider_id(&self) -> &str {
            "memory"
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "write", "list"]
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            self.get(path)
                .await
                .map(|content| ReadResult::new(path, content))
                .ok_or_else(|| MountError::NotFound(path.to_string()))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            path: &str,
            content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            self.files
                .lock()
                .await
                .insert(path.to_string(), content.to_string());
            Ok(())
        }

        async fn delete_text(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            self.files.lock().await.remove(path);
            Ok(())
        }

        async fn rename_text(
            &self,
            _mount: &Mount,
            from_path: &str,
            to_path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            let mut files = self.files.lock().await;
            let content = files
                .remove(from_path)
                .ok_or_else(|| MountError::NotFound(from_path.to_string()))?;
            files.insert(to_path.to_string(), content);
            Ok(())
        }

        async fn list(
            &self,
            _mount: &Mount,
            options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<crate::vfs::ListResult, MountError> {
            let prefix = options.path.trim_matches('/');
            let entries = self
                .files
                .lock()
                .await
                .keys()
                .filter(|path| prefix == "." || prefix.is_empty() || path.starts_with(prefix))
                .map(|path| RuntimeFileEntry::file(path.clone()))
                .collect();
            Ok(crate::vfs::ListResult { entries })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &crate::vfs::SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<crate::vfs::SearchResult, MountError> {
            Ok(crate::vfs::SearchResult::default())
        }
    }

    fn mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "memory".to_string(),
            backend_id: String::new(),
            root_ref: format!("memory://{id}"),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
            ],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn capability_state(read: bool, write: bool) -> CapabilityState {
        let mut state = CapabilityState::default();
        state.tool.enabled_clusters.insert(ToolCluster::Execute);
        if read {
            state.tool.enabled_clusters.insert(ToolCluster::Read);
            state
                .tool
                .capabilities
                .insert(ToolCapability::new(CAP_FILE_READ));
        }
        if write {
            state.tool.enabled_clusters.insert(ToolCluster::Write);
            state
                .tool
                .capabilities
                .insert(ToolCapability::new(CAP_FILE_WRITE));
        }
        state
    }

    async fn shell(
        provider: Arc<MemoryProvider>,
        state: CapabilityState,
    ) -> PlatformShell<'static> {
        let service = Arc::new(VfsService::new(Arc::new(
            MountProviderRegistryBuilder::new()
                .register(provider)
                .build(),
        )));
        let vfs = Box::new(Vfs {
            mounts: vec![mount("main"), mount("out")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        });
        let vfs_ref: &'static Vfs = Box::leak(vfs);
        let state_ref: &'static CapabilityState = Box::leak(Box::new(state));
        PlatformShell::new(
            service,
            vfs_ref,
            PlatformShellCwd::Root,
            None,
            None,
            state_ref,
        )
    }

    #[tokio::test]
    async fn cat_reads_text_with_shell_words_quotes() {
        let provider = Arc::new(MemoryProvider::default());
        provider.insert("hello world.txt", "hello").await;
        let shell = shell(provider, capability_state(true, false)).await;

        let result = shell.execute("cat 'main://hello world.txt'").await;

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn cp_copies_across_mounts() {
        let provider = Arc::new(MemoryProvider::default());
        provider.insert("source.txt", "copied").await;
        let shell = shell(provider.clone(), capability_state(true, true)).await;

        let result = shell.execute("cp main://source.txt out://dest.txt").await;

        assert_eq!(result.exit_code, 0);
        assert_eq!(provider.get("dest.txt").await.as_deref(), Some("copied"));
    }

    #[tokio::test]
    async fn missing_write_capability_rejects_cp() {
        let provider = Arc::new(MemoryProvider::default());
        provider.insert("source.txt", "copied").await;
        let shell = shell(provider, capability_state(true, false)).await;

        let result = shell.execute("cp main://source.txt out://dest.txt").await;

        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("file_write"));
    }

    #[tokio::test]
    async fn parseable_shell_tokens_are_literal_arguments() {
        let provider = Arc::new(MemoryProvider::default());
        let shell = shell(provider, capability_state(true, true)).await;

        let result = shell.execute("echo ok | cat").await;

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "ok | cat\n");
    }
}
