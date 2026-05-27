use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use agentdash_domain::shared_library::{ExtensionBundleKind, ExtensionTemplatePayload};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::artifact_cache::ExtensionArtifactCacheEntry;

#[derive(Debug, Clone)]
pub struct LocalTsExtensionHostConfig {
    pub node_command: String,
    pub runner_dir: PathBuf,
}

impl Default for LocalTsExtensionHostConfig {
    fn default() -> Self {
        Self {
            node_command: "node".to_string(),
            runner_dir: std::env::temp_dir().join("agentdash-extension-host"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalExtensionHostActivation {
    pub extension_key: String,
    pub backend_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostProfile {
    pub username: String,
    pub platform: String,
    pub arch: String,
    pub backend_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_roots: Vec<LocalExtensionHostWorkspaceRoot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostWorkspaceRoot {
    pub index: usize,
    pub name: String,
    pub display_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LocalExtensionHostHealth {
    pub active: bool,
    pub extension_id: Option<String>,
    pub action_keys: Vec<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Error)]
pub enum LocalExtensionHostError {
    #[error("extension host package 非法: {0}")]
    InvalidPackage(String),
    #[error("extension host 进程失败: {0}")]
    Process(String),
    #[error("extension host protocol 非法: {0}")]
    Protocol(String),
    #[error("extension host 权限拒绝: {0}")]
    PermissionDenied(String),
    #[error("extension host 执行失败: {0}")]
    Host(String),
    #[error("extension host I/O 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("extension host JSON 失败: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct LocalExtensionHostManager {
    config: LocalTsExtensionHostConfig,
    process: Arc<Mutex<Option<ExtensionHostProcess>>>,
}

struct ExtensionHostProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    active: Option<ActiveExtension>,
}

#[derive(Debug, Clone)]
struct ActiveExtension {
    extension_key: String,
    manifest: ExtensionTemplatePayload,
    profile: LocalExtensionHostProfile,
}

#[derive(Debug)]
struct LoadedExtensionPackage {
    manifest: ExtensionTemplatePayload,
    bundle_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct RunnerRequest<'a> {
    kind: &'static str,
    id: &'a str,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct RunnerMessage {
    kind: String,
    id: Option<String>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<String>,
    level: Option<String>,
    message: Option<String>,
}

impl LocalExtensionHostManager {
    pub fn new(config: LocalTsExtensionHostConfig) -> Self {
        Self {
            config,
            process: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(LocalTsExtensionHostConfig::default())
    }

    pub async fn start(&self) -> Result<(), LocalExtensionHostError> {
        let mut guard = self.process.lock().await;
        if guard.is_none() {
            *guard = Some(ExtensionHostProcess::spawn(&self.config).await?);
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), LocalExtensionHostError> {
        let process = {
            let mut guard = self.process.lock().await;
            guard.take()
        };
        if let Some(mut process) = process {
            let _ = process.call("deactivate", json!({})).await;
            let _ = process.child.kill().await;
            let _ = process.child.wait().await;
        }
        Ok(())
    }

    pub async fn activate_dev_directory(
        &self,
        package_dir: impl AsRef<Path>,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        let loaded = load_extension_package(package_dir.as_ref(), false).await?;
        self.activate_loaded(loaded, activation).await
    }

    pub async fn activate_packaged_dir(
        &self,
        package_dir: impl AsRef<Path>,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        let loaded = load_extension_package(package_dir.as_ref(), true).await?;
        self.activate_loaded(loaded, activation).await
    }

    pub async fn activate_cached_artifact(
        &self,
        cache_entry: &ExtensionArtifactCacheEntry,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        self.activate_packaged_dir(&cache_entry.unpacked_dir, activation)
            .await
    }

    pub async fn reload_dev_directory(
        &self,
        package_dir: impl AsRef<Path>,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        let loaded = load_extension_package(package_dir.as_ref(), false).await?;
        self.reload_loaded(loaded, activation).await
    }

    pub async fn invoke_action(
        &self,
        action_key: &str,
        input: Value,
    ) -> Result<Value, LocalExtensionHostError> {
        let mut guard = self.process.lock().await;
        let process = guard
            .as_mut()
            .ok_or_else(|| LocalExtensionHostError::Process("extension host 尚未启动".into()))?;
        let result = process
            .call(
                "invoke_action",
                json!({
                    "action_key": action_key,
                    "input": input,
                }),
            )
            .await;
        reset_after_process_exit(&mut guard, &result);
        result
    }

    pub async fn health(&self) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        let mut guard = self.process.lock().await;
        let process = guard
            .as_mut()
            .ok_or_else(|| LocalExtensionHostError::Process("extension host 尚未启动".into()))?;
        let result = process.call("health", json!({})).await;
        reset_after_process_exit(&mut guard, &result);
        serde_json::from_value(result?).map_err(LocalExtensionHostError::from)
    }

    async fn activate_loaded(
        &self,
        loaded: LoadedExtensionPackage,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        self.start().await?;
        let mut guard = self.process.lock().await;
        let process = guard
            .as_mut()
            .ok_or_else(|| LocalExtensionHostError::Process("extension host 尚未启动".into()))?;
        let profile = profile_from_activation(&activation);
        let active = ActiveExtension {
            extension_key: activation.extension_key,
            manifest: loaded.manifest.clone(),
            profile,
        };
        process.active = Some(active.clone());
        let result = process
            .call(
                "activate",
                json!({
                    "extension_key": active.extension_key,
                    "bundle_path": loaded.bundle_path,
                    "manifest": loaded.manifest,
                }),
            )
            .await;
        if result.is_err() {
            process.active = None;
        }
        reset_after_process_exit(&mut guard, &result);
        serde_json::from_value(result?).map_err(LocalExtensionHostError::from)
    }

    async fn reload_loaded(
        &self,
        loaded: LoadedExtensionPackage,
        activation: LocalExtensionHostActivation,
    ) -> Result<LocalExtensionHostHealth, LocalExtensionHostError> {
        self.start().await?;
        let mut guard = self.process.lock().await;
        let process = guard
            .as_mut()
            .ok_or_else(|| LocalExtensionHostError::Process("extension host 尚未启动".into()))?;
        let profile = profile_from_activation(&activation);
        let active = ActiveExtension {
            extension_key: activation.extension_key,
            manifest: loaded.manifest.clone(),
            profile,
        };
        process.active = Some(active.clone());
        let result = process
            .call(
                "reload",
                json!({
                    "extension_key": active.extension_key,
                    "bundle_path": loaded.bundle_path,
                    "manifest": loaded.manifest,
                }),
            )
            .await;
        if result.is_err() {
            process.active = None;
        }
        reset_after_process_exit(&mut guard, &result);
        serde_json::from_value(result?).map_err(LocalExtensionHostError::from)
    }
}

impl Default for LocalExtensionHostManager {
    fn default() -> Self {
        Self::with_default_config()
    }
}

impl ExtensionHostProcess {
    async fn spawn(config: &LocalTsExtensionHostConfig) -> Result<Self, LocalExtensionHostError> {
        tokio::fs::create_dir_all(&config.runner_dir).await?;
        let runner_path = config
            .runner_dir
            .join("agentdash-extension-host-runner.mjs");
        tokio::fs::write(&runner_path, EXTENSION_HOST_RUNNER).await?;
        let mut child = Command::new(&config.node_command)
            .arg("--experimental-vm-modules")
            .arg(&runner_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                LocalExtensionHostError::Process(format!("启动 node extension host 失败: {error}"))
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LocalExtensionHostError::Process("无法打开 host stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LocalExtensionHostError::Process("无法打开 host stdout".into()))?;
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_drain(stderr);
        }
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            next_id: 1,
            active: None,
        })
    }

    async fn call(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Value, LocalExtensionHostError> {
        let id = format!("local-{}", self.next_id);
        self.next_id += 1;
        let request = RunnerRequest {
            kind: "request",
            id: &id,
            method,
            params,
        };
        self.write_json(&request).await?;
        loop {
            let Some(line) = self.stdout.next_line().await? else {
                return Err(self.exit_error()?);
            };
            let message: RunnerMessage = serde_json::from_str(&line)?;
            match message.kind.as_str() {
                "response" => {
                    if message.id.as_deref() != Some(id.as_str()) {
                        return Err(LocalExtensionHostError::Protocol(format!(
                            "收到不匹配响应 id: {:?}",
                            message.id
                        )));
                    }
                    if let Some(error) = message.error {
                        return Err(LocalExtensionHostError::Host(error));
                    }
                    return Ok(message.result.unwrap_or(Value::Null));
                }
                "host_api_request" => {
                    self.handle_host_api_request(message).await?;
                }
                "log" => {
                    let level = message.level.unwrap_or_else(|| "info".to_string());
                    let text = message.message.unwrap_or_default();
                    tracing::debug!(level = %level, message = %text, "extension host log");
                }
                other => {
                    return Err(LocalExtensionHostError::Protocol(format!(
                        "未知 host 消息类型: {other}"
                    )));
                }
            }
        }
    }

    async fn handle_host_api_request(
        &mut self,
        message: RunnerMessage,
    ) -> Result<(), LocalExtensionHostError> {
        let id = message
            .id
            .ok_or_else(|| LocalExtensionHostError::Protocol("host api request 缺少 id".into()))?;
        let method = message.method.unwrap_or_default();
        let params = message.params.unwrap_or(Value::Null);
        let response = match self.resolve_host_api(&method, &params) {
            Ok(result) => json!({ "kind": "host_api_response", "id": id, "result": result }),
            Err(error) => {
                json!({ "kind": "host_api_response", "id": id, "error": error.to_string() })
            }
        };
        self.write_json(&response).await
    }

    fn resolve_host_api(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<Value, LocalExtensionHostError> {
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| LocalExtensionHostError::Host("extension 尚未激活".into()))?;
        match method {
            "local.get_profile" => {
                let action_key = params
                    .get("action_key")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty());
                if !allows_local_profile(&active.manifest, action_key) {
                    return Err(LocalExtensionHostError::PermissionDenied(
                        "local.profile.read 未声明".to_string(),
                    ));
                }
                serde_json::to_value(&active.profile).map_err(LocalExtensionHostError::from)
            }
            other => Err(LocalExtensionHostError::PermissionDenied(format!(
                "未知 host api: {other}"
            ))),
        }
    }

    async fn write_json<T: Serialize>(
        &mut self,
        message: &T,
    ) -> Result<(), LocalExtensionHostError> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    fn exit_error(&mut self) -> Result<LocalExtensionHostError, LocalExtensionHostError> {
        let status = self.child.try_wait()?;
        Ok(match status {
            Some(status) => {
                LocalExtensionHostError::Process(format!("extension host 已退出: {status}"))
            }
            None => LocalExtensionHostError::Process("extension host stdout 已关闭".into()),
        })
    }
}

async fn load_extension_package(
    package_dir: &Path,
    verify_digest: bool,
) -> Result<LoadedExtensionPackage, LocalExtensionHostError> {
    let package_dir = tokio::fs::canonicalize(package_dir)
        .await
        .map_err(|error| {
            LocalExtensionHostError::InvalidPackage(format!(
                "package dir 不存在: {} ({error})",
                package_dir.display()
            ))
        })?;
    let manifest_path = package_dir.join("agentdash.extension.json");
    let manifest_bytes = tokio::fs::read(&manifest_path).await.map_err(|error| {
        LocalExtensionHostError::InvalidPackage(format!(
            "读取 agentdash.extension.json 失败: {error}"
        ))
    })?;
    let manifest: ExtensionTemplatePayload = serde_json::from_slice(&manifest_bytes)?;
    manifest
        .validate()
        .map_err(|error| LocalExtensionHostError::InvalidPackage(error.to_string()))?;
    let bundle_ref = manifest
        .bundles
        .iter()
        .find(|bundle| matches!(bundle.kind, ExtensionBundleKind::ExtensionHost))
        .ok_or_else(|| {
            LocalExtensionHostError::InvalidPackage("缺少 extension_host bundle".into())
        })?;
    let bundle_path = safe_package_path(&package_dir, &bundle_ref.entry)?;
    if !tokio::fs::try_exists(&bundle_path).await? {
        return Err(LocalExtensionHostError::InvalidPackage(format!(
            "extension bundle 不存在: {}",
            bundle_ref.entry
        )));
    }
    if verify_digest {
        let actual = digest_bytes(&tokio::fs::read(&bundle_path).await?);
        if actual != bundle_ref.digest {
            return Err(LocalExtensionHostError::InvalidPackage(format!(
                "extension bundle digest 不匹配: expected {}, actual {}",
                bundle_ref.digest, actual
            )));
        }
    }
    Ok(LoadedExtensionPackage {
        manifest,
        bundle_path,
    })
}

fn safe_package_path(root: &Path, relative: &str) -> Result<PathBuf, LocalExtensionHostError> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(LocalExtensionHostError::InvalidPackage(format!(
            "bundle entry 必须是相对路径: {relative}"
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(LocalExtensionHostError::InvalidPackage(format!(
                    "bundle entry 包含不安全路径: {relative}"
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(LocalExtensionHostError::InvalidPackage(
            "bundle entry 不能为空".to_string(),
        ));
    }
    Ok(root.join(normalized))
}

fn profile_from_activation(activation: &LocalExtensionHostActivation) -> LocalExtensionHostProfile {
    LocalExtensionHostProfile {
        username: local_username(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        backend_id: activation.backend_id.clone(),
        project_id: activation.project_id.clone(),
        session_id: activation.session_id.clone(),
        workspace_roots: activation
            .workspace_roots
            .iter()
            .enumerate()
            .map(|(index, root)| workspace_root_summary(index, root))
            .collect(),
    }
}

fn workspace_root_summary(index: usize, root: &Path) -> LocalExtensionHostWorkspaceRoot {
    let name = root
        .file_name()
        .or_else(|| root.as_os_str().to_str().map(std::ffi::OsStr::new))
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("workspace-{index}"));
    LocalExtensionHostWorkspaceRoot {
        index,
        name: name.clone(),
        display_path: name,
    }
}

fn local_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-user".to_string())
}

fn allows_local_profile(manifest: &ExtensionTemplatePayload, action_key: Option<&str>) -> bool {
    action_key
        .map(|key| manifest.allows_local_profile_read_for_action(key))
        .unwrap_or(false)
}

fn reset_after_process_exit<T>(
    guard: &mut Option<ExtensionHostProcess>,
    result: &Result<T, LocalExtensionHostError>,
) {
    if matches!(result, Err(LocalExtensionHostError::Process(_))) {
        *guard = None;
    }
}

fn spawn_stderr_drain(stderr: ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(message = %line, "extension host stderr");
        }
    });
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

const EXTENSION_HOST_RUNNER: &str = r#"
import fs from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import vm from "node:vm";
import { pathToFileURL } from "node:url";

let active = null;
let currentActionKey = null;
let nextHostApiId = 1;
const pendingHostApi = new Map();

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function log(level, message) {
  send({ kind: "log", level, message: String(message) });
}

function toJsonValue(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (typeof value === "object") {
    const result = {};
    for (const [key, item] of Object.entries(value)) {
      if (typeof item !== "function" && typeof item !== "symbol" && typeof item !== "undefined") {
        result[key] = toJsonValue(item);
      }
    }
    return result;
  }
  return null;
}

function createExtensionContext() {
  const actions = new Map();
  const contributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    workspace_panels: [],
    permissions: [],
  };
  const ctx = {
    api: {
      runtime: {
        async invoke(actionKey, input) {
          return await requestHostApi("runtime.invoke", { action_key: actionKey, input: toJsonValue(input) });
        },
      },
      local: {
        async getProfile() {
          return await requestHostApi("local.get_profile", { action_key: currentActionKey });
        },
      },
    },
    commands: {
      registerCommand(definition) {
        contributions.commands.push(toJsonValue(definition));
      },
    },
    flags: {
      registerFlag(definition) {
        contributions.flags.push(toJsonValue(definition));
      },
    },
    runtime: {
      registerAction(definition) {
        if (!definition || typeof definition.action_key !== "string" || typeof definition.invoke !== "function") {
          throw new Error("runtime action must include action_key and invoke");
        }
        actions.set(definition.action_key, definition);
        const { invoke, ...serializable } = definition;
        contributions.runtime_actions.push(toJsonValue(serializable));
      },
    },
    workspace: {
      registerPanel(definition) {
        contributions.workspace_panels.push(toJsonValue(definition));
      },
    },
    permissions: {
      require(permission) {
        contributions.permissions.push(toJsonValue(permission));
      },
    },
    contributions,
  };
  return { ctx, actions, contributions };
}

async function requestHostApi(method, params) {
  const id = `host-api-${nextHostApiId++}`;
  send({ kind: "host_api_request", id, method, params: toJsonValue(params) });
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingHostApi.delete(id);
      reject(new Error(`host api timeout: ${method}`));
    }, 30000);
    pendingHostApi.set(id, {
      resolve(value) {
        clearTimeout(timeout);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timeout);
        reject(error);
      },
    });
  });
}

async function loadExtension(bundlePath) {
  const source = await fs.readFile(bundlePath, "utf8");
  const moduleUrl = pathToFileURL(path.resolve(bundlePath)).href;
  const context = vm.createContext({
    console: {
      log: (...args) => log("info", args.join(" ")),
      warn: (...args) => log("warn", args.join(" ")),
      error: (...args) => log("error", args.join(" ")),
    },
    setTimeout,
    clearTimeout,
    structuredClone,
    TextDecoder,
    TextEncoder,
  });
  const module = new vm.SourceTextModule(source, {
    context,
    identifier: `${moduleUrl}?t=${Date.now()}`,
    initializeImportMeta(meta) {
      meta.url = moduleUrl;
    },
    importModuleDynamically(specifier) {
      throw new Error(`extension bundle must be self-contained; dynamic import blocked: ${specifier}`);
    },
  });
  await module.link((specifier) => {
    throw new Error(`extension bundle must be self-contained; import blocked: ${specifier}`);
  });
  await module.evaluate();
  const exported = module.namespace.default ?? module.namespace.extension;
  if (!exported || typeof exported !== "object") {
    throw new Error("extension bundle must export a default extension object");
  }
  return exported;
}

async function activate(params) {
  const extension = await loadExtension(params.bundle_path);
  const { ctx, actions, contributions } = createExtensionContext();
  active = {
    extension,
    manifest: params.manifest,
    extensionKey: params.extension_key,
    actions,
    contributions,
  };
  if (typeof extension.activate === "function") {
    await extension.activate(ctx);
  }
  return healthPayload();
}

async function deactivate() {
  if (active?.extension && typeof active.extension.deactivate === "function") {
    await active.extension.deactivate();
  }
  active = null;
  return healthPayload();
}

async function invokeAction(params) {
  if (!active) throw new Error("extension is not active");
  const actionKey = params.action_key;
  const action = active.actions.get(actionKey);
  if (!action) throw new Error(`extension action is not registered: ${actionKey}`);
  const previous = currentActionKey;
  currentActionKey = actionKey;
  try {
    return toJsonValue(await action.invoke(toJsonValue(params.input)));
  } finally {
    currentActionKey = previous;
  }
}

function healthPayload() {
  return {
    active: Boolean(active),
    extension_id: active?.manifest?.extension_id ?? null,
    action_keys: active ? [...active.actions.keys()].sort() : [],
    pid: process.pid,
  };
}

async function handleRequest(message) {
  switch (message.method) {
    case "activate":
      return await activate(message.params ?? {});
    case "reload":
      await deactivate();
      return await activate(message.params ?? {});
    case "deactivate":
      return await deactivate();
    case "invoke_action":
      return await invokeAction(message.params ?? {});
    case "health":
      return healthPayload();
    default:
      throw new Error(`unknown extension host method: ${message.method}`);
  }
}

rl.on("line", (line) => {
  void (async () => {
    const message = JSON.parse(line);
    if (message.kind === "host_api_response") {
      const pending = pendingHostApi.get(message.id);
      if (!pending) return;
      pendingHostApi.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.result ?? null);
      return;
    }
    if (message.kind !== "request") return;
    try {
      const result = await handleRequest(message);
      send({ kind: "response", id: message.id, result: toJsonValue(result) });
    } catch (error) {
      send({ kind: "response", id: message.id, error: error instanceof Error ? error.message : String(error) });
    }
  })().catch((error) => {
    send({ kind: "log", level: "error", message: error instanceof Error ? error.message : String(error) });
  });
});
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_hello_profile_executes_in_host() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), profile_bundle(), true, true)
            .await
            .expect("package");
        let manager = test_manager(temp.path());

        let health = manager
            .activate_dev_directory(&package_dir, activation())
            .await
            .expect("activate");
        assert_eq!(health.action_keys, vec!["local-hello.profile".to_string()]);

        let result = manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect("invoke");
        assert_eq!(result["backend_id"], "backend-1");
        assert_eq!(result["project_id"], "project-1");
        manager.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn reload_updates_action_handler() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), version_bundle(1), true, true)
            .await
            .expect("package");
        let manager = test_manager(temp.path());
        manager
            .activate_dev_directory(&package_dir, activation())
            .await
            .expect("activate");
        assert_eq!(
            manager
                .invoke_action("local-hello.profile", Value::Null)
                .await
                .expect("invoke")["version"],
            1
        );

        write_bundle(&package_dir, version_bundle(2))
            .await
            .expect("rewrite bundle");
        manager
            .reload_dev_directory(&package_dir, activation())
            .await
            .expect("reload");
        assert_eq!(
            manager
                .invoke_action("local-hello.profile", Value::Null)
                .await
                .expect("invoke")["version"],
            2
        );
        manager.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn permission_denied_when_local_profile_is_not_declared() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), profile_bundle(), false, false)
            .await
            .expect("package");
        let manager = test_manager(temp.path());
        manager
            .activate_dev_directory(&package_dir, activation())
            .await
            .expect("activate");
        let err = manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect_err("permission denied");
        assert!(err.to_string().contains("local.profile.read"));
        manager.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn permission_denied_when_action_local_profile_is_not_declared() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), profile_bundle(), true, false)
            .await
            .expect("package");
        let manager = test_manager(temp.path());
        manager
            .activate_dev_directory(&package_dir, activation())
            .await
            .expect("activate");
        let err = manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect_err("permission denied");
        assert!(err.to_string().contains("local.profile.read"));
        manager.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn packaged_directory_verifies_bundle_digest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), version_bundle(7), true, true)
            .await
            .expect("package");
        let cache_entry = ExtensionArtifactCacheEntry {
            cache_key: "cache-key".to_string(),
            archive_path: temp.path().join("archive.agentdash-extension.tgz"),
            unpacked_dir: package_dir,
        };
        let manager = test_manager(temp.path());
        manager
            .activate_cached_artifact(&cache_entry, activation())
            .await
            .expect("activate packaged");
        assert_eq!(
            manager
                .invoke_action("local-hello.profile", Value::Null)
                .await
                .expect("invoke")["version"],
            7
        );
        manager.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn action_exception_does_not_stop_host_process() {
        let temp = tempfile::tempdir().expect("tempdir");
        let package_dir = write_package(temp.path(), throwing_bundle(), true, true)
            .await
            .expect("package");
        let manager = test_manager(temp.path());
        manager
            .activate_dev_directory(&package_dir, activation())
            .await
            .expect("activate");
        let err = manager
            .invoke_action("local-hello.profile", Value::Null)
            .await
            .expect_err("action error");
        assert!(err.to_string().contains("boom"));
        let health = manager.health().await.expect("health");
        assert!(health.active);
        manager.stop().await.expect("stop");
    }

    fn test_manager(root: &Path) -> LocalExtensionHostManager {
        LocalExtensionHostManager::new(LocalTsExtensionHostConfig {
            node_command: "node".to_string(),
            runner_dir: root.join("runner"),
        })
    }

    fn activation() -> LocalExtensionHostActivation {
        LocalExtensionHostActivation {
            extension_key: "local-hello".to_string(),
            backend_id: "backend-1".to_string(),
            project_id: Some("project-1".to_string()),
            session_id: Some("session-1".to_string()),
            workspace_roots: vec![PathBuf::from("C:/secret/workspace")],
        }
    }

    async fn write_package(
        root: &Path,
        bundle: String,
        include_top_level_permission: bool,
        include_action_permission: bool,
    ) -> anyhow::Result<PathBuf> {
        let package_dir = root.join("package");
        tokio::fs::create_dir_all(package_dir.join("dist")).await?;
        write_bundle(&package_dir, bundle.clone()).await?;
        let digest = digest_bytes(bundle.as_bytes());
        let permissions = if include_top_level_permission {
            json!([{ "kind": "local_profile", "access": "read" }])
        } else {
            json!([])
        };
        let action_permissions = if include_action_permission {
            json!(["local.profile.read"])
        } else {
            json!([])
        };
        let manifest = json!({
            "manifest_version": "2",
            "extension_id": "local-hello",
            "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
            "asset_version": "0.1.0",
            "runtime_actions": [{
                "action_key": "local-hello.profile",
                "kind": "session_runtime",
                "description": "Read local profile",
                "input_schema": {},
                "output_schema": {},
                "permissions": action_permissions,
            }],
            "permissions": permissions,
            "bundles": [{
                "kind": "extension_host",
                "entry": "dist/extension.js",
                "digest": digest,
            }],
        });
        tokio::fs::write(
            package_dir.join("agentdash.extension.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )
        .await?;
        Ok(package_dir)
    }

    async fn write_bundle(package_dir: &Path, bundle: String) -> anyhow::Result<()> {
        tokio::fs::write(package_dir.join("dist").join("extension.js"), bundle).await?;
        Ok(())
    }

    fn profile_bundle() -> String {
        r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Read local profile",
      async invoke() {
        return await ctx.api.local.getProfile();
      },
    });
  },
};
"#
        .to_string()
    }

    fn version_bundle(version: i32) -> String {
        format!(
            r#"
export default {{
  activate(ctx) {{
    ctx.runtime.registerAction({{
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Version",
      invoke() {{
        return {{ version: {version} }};
      }},
    }});
  }},
}};
"#
        )
    }

    fn throwing_bundle() -> String {
        r#"
export default {
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "local-hello.profile",
      kind: "session_runtime",
      description: "Throw",
      invoke() {
        throw new Error("boom");
      },
    });
  },
};
"#
        .to_string()
    }
}
