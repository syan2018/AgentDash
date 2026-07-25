use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use agentdash_domain::shared_library::{ExtensionBundleKind, ExtensionTemplatePayload};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::process::{ActiveExtension, ExtensionHostProcess};
use super::schema::validate_json_schema_subset;
use super::{
    LocalExtensionHostActivation, LocalExtensionHostError, LocalExtensionHostHealth,
    LocalExtensionHostProfile, LocalExtensionHostWorkspaceRoot,
};
use crate::extensions::artifact_cache::ExtensionArtifactCacheEntry;
use crate::tool_executor::ToolExecutor;

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

#[derive(Clone)]
pub struct LocalExtensionHostManager {
    config: LocalTsExtensionHostConfig,
    process: Arc<Mutex<Option<ExtensionHostProcess>>>,
}

#[derive(Debug)]
struct LoadedExtensionPackage {
    manifest: ExtensionTemplatePayload,
    bundle_path: PathBuf,
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
            process.active_extensions.clear();
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
        let output_schema = action_output_schema(process, action_key)?;
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
        if let Ok(output) = &result {
            validate_json_schema_subset(
                &output_schema,
                output,
                &format!("extension action `{action_key}` output"),
            )?;
        }
        result
    }

    pub async fn invoke_protocol(
        &self,
        protocol_key: &str,
        method: &str,
        input: Value,
    ) -> Result<Value, LocalExtensionHostError> {
        let mut guard = self.process.lock().await;
        let process = guard
            .as_mut()
            .ok_or_else(|| LocalExtensionHostError::Process("extension host 尚未启动".into()))?;
        let output_schema = protocol_output_schema(process, protocol_key, method)?;
        let result = process
            .call(
                "invoke_protocol",
                json!({
                    "protocol_key": protocol_key,
                    "method": method,
                    "input": input,
                }),
            )
            .await;
        reset_after_process_exit(&mut guard, &result);
        if let Ok(output) = &result {
            validate_json_schema_subset(
                &output_schema,
                output,
                &format!("extension protocol `{protocol_key}.{method}` output"),
            )?;
        }
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
        let tool_executor = tool_executor_for_activation(&activation);
        let active = ActiveExtension {
            extension_key: activation.extension_key,
            manifest: loaded.manifest.clone(),
            profile,
            default_workspace_root: activation.default_workspace_root.clone(),
            tool_executor,
        };
        process
            .active_extensions
            .insert(active.extension_key.clone(), active.clone());
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
            process.active_extensions.remove(&active.extension_key);
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
        let tool_executor = tool_executor_for_activation(&activation);
        let active = ActiveExtension {
            extension_key: activation.extension_key,
            manifest: loaded.manifest.clone(),
            profile,
            default_workspace_root: activation.default_workspace_root.clone(),
            tool_executor,
        };
        process
            .active_extensions
            .insert(active.extension_key.clone(), active.clone());
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
            process.active_extensions.remove(&active.extension_key);
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
        execution_id: activation.execution_id.clone(),
        workspace_roots: activation
            .workspace_roots
            .iter()
            .enumerate()
            .map(|(index, root)| workspace_root_summary(index, root))
            .collect(),
    }
}

fn tool_executor_for_activation(activation: &LocalExtensionHostActivation) -> ToolExecutor {
    let mut roots = activation.workspace_roots.clone();
    if let Some(default_root) = activation.default_workspace_root.as_ref()
        && !roots.iter().any(|root| root == default_root)
    {
        roots.push(default_root.clone());
    }
    ToolExecutor::new(roots)
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

fn action_output_schema(
    process: &ExtensionHostProcess,
    action_key: &str,
) -> Result<Value, LocalExtensionHostError> {
    process
        .active_extensions
        .values()
        .find_map(|active| {
            active
                .manifest
                .runtime_actions
                .iter()
                .find(|action| action.action_key == action_key)
                .map(|action| action.output_schema.clone())
        })
        .ok_or_else(|| {
            LocalExtensionHostError::Host(format!(
                "extension action 未声明 output schema: {action_key}"
            ))
        })
}

fn protocol_output_schema(
    process: &ExtensionHostProcess,
    protocol_key: &str,
    method: &str,
) -> Result<Value, LocalExtensionHostError> {
    process
        .active_extensions
        .values()
        .find_map(|active| {
            let canonical_protocol_key = if protocol_key.contains('.') {
                protocol_key.to_string()
            } else {
                format!("{}.{}", active.extension_key, protocol_key)
            };
            active
                .manifest
                .protocols
                .iter()
                .find(|channel| channel.protocol_key == canonical_protocol_key)
                .and_then(|channel| {
                    channel
                        .methods
                        .iter()
                        .find(|candidate| candidate.name == method)
                })
                .map(|method| method.output_schema.clone())
        })
        .ok_or_else(|| {
            LocalExtensionHostError::Host(format!(
                "extension protocol 未声明 output schema: {protocol_key}.{method}"
            ))
        })
}

fn local_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-user".to_string())
}

fn reset_after_process_exit<T>(
    guard: &mut Option<ExtensionHostProcess>,
    result: &Result<T, LocalExtensionHostError>,
) {
    if matches!(result, Err(LocalExtensionHostError::Process(_))) {
        *guard = None;
    }
}

pub(super) fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
