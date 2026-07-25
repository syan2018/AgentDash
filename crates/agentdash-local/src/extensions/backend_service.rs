use std::collections::{BTreeMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::shared_library::{
    ExtensionBackendServiceDefinition, ExtensionTemplatePayload,
};
use agentdash_process::{ProcessDomain, background_tokio_command_with_cwd};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

use super::artifact_cache::ExtensionArtifactCacheEntry;

#[derive(Debug, Clone)]
pub struct ExtensionBackendServiceArtifact {
    pub artifact_id: String,
    pub archive_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceInstanceIdentity {
    pub project_id: String,
    pub backend_id: String,
    pub extension_key: String,
    pub service_key: String,
    pub artifact_id: String,
    pub archive_digest: String,
}

impl ExtensionBackendServiceInstanceIdentity {
    fn instance_key(&self) -> String {
        [
            self.project_id.as_str(),
            self.backend_id.as_str(),
            self.extension_key.as_str(),
            self.service_key.as_str(),
            self.artifact_id.as_str(),
            self.archive_digest.as_str(),
        ]
        .join("\u{1f}")
    }

    fn same_service_without_artifact(&self, other: &Self) -> bool {
        self.project_id == other.project_id
            && self.backend_id == other.backend_id
            && self.extension_key == other.extension_key
            && self.service_key == other.service_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBackendServiceReadiness {
    MissingArtifact,
    MaterializeFailed,
    Starting,
    HealthFailed,
    Ready,
    ProcessExited,
    UnsupportedRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceMaterialization {
    pub artifact_id: String,
    pub archive_digest: String,
    pub extension_key: String,
    pub service_key: String,
    pub service_root: PathBuf,
    pub entry_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceLogLine {
    pub stream: String,
    pub line: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceStatus {
    pub identity: ExtensionBackendServiceInstanceIdentity,
    pub readiness: ExtensionBackendServiceReadiness,
    pub message: Option<String>,
    pub endpoint: Option<String>,
    pub pid: Option<u32>,
    pub materialization: Option<ExtensionBackendServiceMaterialization>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ExtensionBackendServiceMaterializeRequest {
    pub identity: ExtensionBackendServiceInstanceIdentity,
    pub cache_entry: ExtensionArtifactCacheEntry,
}

#[derive(Debug, Clone)]
pub struct ExtensionBackendServiceStartRequest {
    pub project_id: String,
    pub backend_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub artifact: Option<ExtensionBackendServiceArtifact>,
    pub cache_entry: Option<ExtensionArtifactCacheEntry>,
}

#[derive(Debug, Clone)]
pub struct ExtensionBackendServiceInvokeRequest {
    pub identity: ExtensionBackendServiceInstanceIdentity,
    pub extension_id: String,
    pub route: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceInvokeMetadata {
    pub project_id: String,
    pub backend_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub route: String,
    pub trace_id: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionBackendServiceInvokeResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub metadata: ExtensionBackendServiceInvokeMetadata,
}

#[derive(Debug, Clone)]
pub struct LocalExtensionBackendServiceManagerConfig {
    pub node_command: String,
    pub service_cache_root: PathBuf,
    pub startup_timeout: Duration,
    pub health_poll_interval: Duration,
    pub request_timeout: Duration,
    pub max_log_lines: usize,
}

impl Default for LocalExtensionBackendServiceManagerConfig {
    fn default() -> Self {
        Self {
            node_command: "node".to_string(),
            service_cache_root: std::env::temp_dir().join("agentdash-backend-services"),
            startup_timeout: Duration::from_secs(5),
            health_poll_interval: Duration::from_millis(100),
            request_timeout: Duration::from_secs(15),
            max_log_lines: 200,
        }
    }
}

#[derive(Debug, Error)]
pub enum ExtensionBackendServiceError {
    #[error("backend service artifact 缺失: {0}")]
    MissingArtifact(String),
    #[error("backend service materialize 失败: {0}")]
    Materialize(String),
    #[error("backend service runtime 不支持: {0}")]
    UnsupportedRuntime(String),
    #[error("backend service 进程失败: {0}")]
    Process(String),
    #[error("backend service health 失败: {0}")]
    Health(String),
    #[error("backend service 请求非法: {0}")]
    InvalidRequest(String),
    #[error("backend service HTTP 调用失败: {0}")]
    Http(String),
    #[error("backend service I/O 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend service manifest 解析失败: {0}")]
    Json(#[from] serde_json::Error),
}

impl ExtensionBackendServiceError {
    fn readiness(&self) -> ExtensionBackendServiceReadiness {
        match self {
            Self::MissingArtifact(_) => ExtensionBackendServiceReadiness::MissingArtifact,
            Self::Materialize(_) | Self::Io(_) | Self::Json(_) => {
                ExtensionBackendServiceReadiness::MaterializeFailed
            }
            Self::UnsupportedRuntime(_) => ExtensionBackendServiceReadiness::UnsupportedRuntime,
            Self::Health(_) | Self::Http(_) | Self::InvalidRequest(_) => {
                ExtensionBackendServiceReadiness::HealthFailed
            }
            Self::Process(_) => ExtensionBackendServiceReadiness::ProcessExited,
        }
    }
}

#[derive(Debug, Error)]
pub enum ExtensionBackendServiceInvokeError {
    #[error("backend service unavailable: {status:?}")]
    Unavailable {
        status: Box<ExtensionBackendServiceStatus>,
    },
    #[error("backend service invoke request 非法: {0}")]
    InvalidRequest(String),
    #[error("backend service HTTP 调用失败: {0}")]
    Http(String),
}

#[derive(Clone)]
pub struct LocalExtensionBackendServiceManager {
    config: LocalExtensionBackendServiceManagerConfig,
    instances: Arc<Mutex<BTreeMap<String, BackendServiceInstance>>>,
}

struct BackendServiceInstance {
    identity: ExtensionBackendServiceInstanceIdentity,
    service: ExtensionBackendServiceDefinition,
    materialization: ExtensionBackendServiceMaterialization,
    endpoint: String,
    child: Child,
    logs: SharedLogBuffer,
    readiness: ExtensionBackendServiceReadiness,
    message: Option<String>,
    updated_at: DateTime<Utc>,
}

type SharedLogBuffer = Arc<Mutex<VecDeque<ExtensionBackendServiceLogLine>>>;

impl LocalExtensionBackendServiceManager {
    pub fn new(config: LocalExtensionBackendServiceManagerConfig) -> Self {
        Self {
            config,
            instances: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(LocalExtensionBackendServiceManagerConfig::default())
    }

    pub async fn materialize_cached_artifact(
        &self,
        request: ExtensionBackendServiceMaterializeRequest,
    ) -> Result<ExtensionBackendServiceMaterialization, ExtensionBackendServiceError> {
        materialize_backend_service(&self.config.service_cache_root, request).await
    }

    pub async fn start(
        &self,
        request: ExtensionBackendServiceStartRequest,
    ) -> ExtensionBackendServiceStatus {
        let Some(artifact) = request.artifact.clone() else {
            let identity = missing_artifact_identity(&request);
            return status_from_parts(
                identity,
                ExtensionBackendServiceReadiness::MissingArtifact,
                Some("package_artifact 缺失".to_string()),
                None,
                None,
                None,
            );
        };
        let identity = ExtensionBackendServiceInstanceIdentity {
            project_id: request.project_id.clone(),
            backend_id: request.backend_id.clone(),
            extension_key: request.extension_key.clone(),
            service_key: request.service_key.clone(),
            artifact_id: artifact.artifact_id,
            archive_digest: artifact.archive_digest,
        };
        let Some(cache_entry) = request.cache_entry.clone() else {
            return status_from_parts(
                identity,
                ExtensionBackendServiceReadiness::MissingArtifact,
                Some("artifact cache entry 缺失".to_string()),
                None,
                None,
                None,
            );
        };

        let start_result = self
            .start_cached_artifact(identity.clone(), request.extension_id, cache_entry)
            .await;
        match start_result {
            Ok(status) => status,
            Err(error) => status_from_parts(
                identity,
                error.readiness(),
                Some(error.to_string()),
                None,
                None,
                None,
            ),
        }
    }

    pub async fn restart(
        &self,
        request: ExtensionBackendServiceStartRequest,
    ) -> ExtensionBackendServiceStatus {
        if let Some(artifact) = request.artifact.as_ref() {
            let identity = ExtensionBackendServiceInstanceIdentity {
                project_id: request.project_id.clone(),
                backend_id: request.backend_id.clone(),
                extension_key: request.extension_key.clone(),
                service_key: request.service_key.clone(),
                artifact_id: artifact.artifact_id.clone(),
                archive_digest: artifact.archive_digest.clone(),
            };
            let _ = self.stop(&identity).await;
        }
        self.start(request).await
    }

    pub async fn stop(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> ExtensionBackendServiceStatus {
        let instance = {
            let mut guard = self.instances.lock().await;
            guard.remove(&identity.instance_key())
        };
        if let Some(mut instance) = instance {
            let pid = instance.child.id();
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
            return status_from_parts(
                identity.clone(),
                ExtensionBackendServiceReadiness::ProcessExited,
                Some("backend service stopped".to_string()),
                Some(instance.endpoint),
                pid,
                Some(instance.materialization),
            );
        }
        status_from_parts(
            identity.clone(),
            ExtensionBackendServiceReadiness::MissingArtifact,
            Some("backend service instance 未找到".to_string()),
            None,
            None,
            None,
        )
    }

    pub async fn status(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> Option<ExtensionBackendServiceStatus> {
        let guard = self.instances.lock().await;
        guard
            .get(&identity.instance_key())
            .map(status_from_instance)
    }

    pub async fn health(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> ExtensionBackendServiceStatus {
        self.refresh_health(identity).await
    }

    pub async fn logs(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> Vec<ExtensionBackendServiceLogLine> {
        let logs = {
            let guard = self.instances.lock().await;
            guard
                .get(&identity.instance_key())
                .map(|instance| Arc::clone(&instance.logs))
        };
        let Some(logs) = logs else {
            return Vec::new();
        };
        logs.lock().await.iter().cloned().collect()
    }

    pub async fn invoke(
        &self,
        request: ExtensionBackendServiceInvokeRequest,
    ) -> Result<ExtensionBackendServiceInvokeResponse, ExtensionBackendServiceInvokeError> {
        let status = self.refresh_health(&request.identity).await;
        if status.readiness != ExtensionBackendServiceReadiness::Ready {
            return Err(ExtensionBackendServiceInvokeError::Unavailable {
                status: Box::new(status),
            });
        }

        let (endpoint, routes) = {
            let guard = self.instances.lock().await;
            let Some(instance) = guard.get(&request.identity.instance_key()) else {
                return Err(ExtensionBackendServiceInvokeError::Unavailable {
                    status: Box::new(status_from_parts(
                        request.identity,
                        ExtensionBackendServiceReadiness::MissingArtifact,
                        Some("backend service instance 未找到".to_string()),
                        None,
                        None,
                        None,
                    )),
                });
            };
            (instance.endpoint.clone(), instance.service.routes.clone())
        };

        if !route_matches_any(&request.route, &routes) {
            return Err(ExtensionBackendServiceInvokeError::InvalidRequest(format!(
                "route 未声明: {}",
                request.route
            )));
        }

        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|error| {
            ExtensionBackendServiceInvokeError::InvalidRequest(format!("method 非法: {error}"))
        })?;
        let headers = header_map_from_record(&request.headers)
            .map_err(ExtensionBackendServiceInvokeError::InvalidRequest)?;
        let url = service_url(&endpoint, &request.route)
            .map_err(ExtensionBackendServiceInvokeError::InvalidRequest)?;
        let client = reqwest::Client::builder()
            .timeout(self.config.request_timeout)
            .build()
            .map_err(|error| ExtensionBackendServiceInvokeError::Http(error.to_string()))?;
        let mut builder = client.request(method, url).headers(headers);
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| ExtensionBackendServiceInvokeError::Http(error.to_string()))?;
        let status_code = response.status().as_u16();
        let response_headers = headers_to_record(response.headers());
        let body = if no_body_status(status_code) {
            None
        } else {
            Some(
                response
                    .bytes()
                    .await
                    .map_err(|error| ExtensionBackendServiceInvokeError::Http(error.to_string()))?
                    .to_vec(),
            )
        };

        Ok(ExtensionBackendServiceInvokeResponse {
            status: status_code,
            headers: response_headers,
            body,
            metadata: ExtensionBackendServiceInvokeMetadata {
                project_id: request.identity.project_id,
                backend_id: request.identity.backend_id,
                extension_key: request.identity.extension_key,
                extension_id: request.extension_id,
                service_key: request.identity.service_key,
                route: request.route,
                trace_id: request.trace_id,
                endpoint,
            },
        })
    }

    async fn start_cached_artifact(
        &self,
        identity: ExtensionBackendServiceInstanceIdentity,
        extension_id: String,
        cache_entry: ExtensionArtifactCacheEntry,
    ) -> Result<ExtensionBackendServiceStatus, ExtensionBackendServiceError> {
        if self.status(&identity).await.is_some() {
            let status = self.refresh_health(&identity).await;
            if status.readiness == ExtensionBackendServiceReadiness::Ready {
                return Ok(status);
            }
        }

        let materialization = self
            .materialize_cached_artifact(ExtensionBackendServiceMaterializeRequest {
                identity: identity.clone(),
                cache_entry,
            })
            .await?;
        let service = load_backend_service_definition(
            &materialization.service_root.join("package"),
            &identity.service_key,
        )
        .await?;
        if service.runtime != "node" {
            return Err(ExtensionBackendServiceError::UnsupportedRuntime(format!(
                "{} 使用 runtime `{}`",
                service.service_key, service.runtime
            )));
        }

        self.stop_replaced_service_instances(&identity).await;

        let port = reserve_loopback_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let package_dir = materialization.service_root.join("package");
        let mut command = background_tokio_command_with_cwd(
            ProcessDomain::ExtensionHost,
            &self.config.node_command,
            &package_dir,
        );
        command
            .arg(&materialization.entry_path)
            .env("AGENTDASH_BACKEND_SERVICE_HOST", "127.0.0.1")
            .env("AGENTDASH_BACKEND_SERVICE_PORT", port.to_string())
            .env("AGENTDASH_BACKEND_SERVICE_URL", &endpoint)
            .env("HOST", "127.0.0.1")
            .env("PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            ExtensionBackendServiceError::Process(format!(
                "启动 Node backend service 失败: {error}"
            ))
        })?;
        if let Some(stdout) = child.stdout.take() {
            spawn_log_drain(
                Arc::clone(&logs),
                stdout,
                "stdout",
                self.config.max_log_lines,
            );
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_drain(
                Arc::clone(&logs),
                stderr,
                "stderr",
                self.config.max_log_lines,
            );
        }

        let pid = child.id();
        let starting = BackendServiceInstance {
            identity: identity.clone(),
            service,
            materialization: materialization.clone(),
            endpoint,
            child,
            logs,
            readiness: ExtensionBackendServiceReadiness::Starting,
            message: None,
            updated_at: Utc::now(),
        };
        {
            let mut guard = self.instances.lock().await;
            guard.insert(identity.instance_key(), starting);
        }

        diag!(
            Info,
            Subsystem::AgentRun,
            operation = "extension_backend_service.start",
            stage = "spawned",
            backend_id = %identity.backend_id,
            project_id = %identity.project_id,
            extension_key = %identity.extension_key,
            extension_id = %extension_id,
            service_key = %identity.service_key,
            pid = pid.unwrap_or_default(),
            "extension backend service spawned"
        );

        self.wait_until_ready(&identity).await
    }

    async fn wait_until_ready(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> Result<ExtensionBackendServiceStatus, ExtensionBackendServiceError> {
        let deadline = Instant::now() + self.config.startup_timeout;
        loop {
            let status = self.refresh_health(identity).await;
            match status.readiness {
                ExtensionBackendServiceReadiness::Ready
                | ExtensionBackendServiceReadiness::ProcessExited => return Ok(status),
                ExtensionBackendServiceReadiness::HealthFailed if Instant::now() >= deadline => {
                    return Ok(status);
                }
                ExtensionBackendServiceReadiness::HealthFailed
                | ExtensionBackendServiceReadiness::Starting => {
                    if Instant::now() >= deadline {
                        return Ok(status);
                    }
                    sleep(self.config.health_poll_interval).await;
                }
                _ => return Ok(status),
            }
        }
    }

    async fn refresh_health(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) -> ExtensionBackendServiceStatus {
        let health_target = {
            let mut guard = self.instances.lock().await;
            let Some(instance) = guard.get_mut(&identity.instance_key()) else {
                return status_from_parts(
                    identity.clone(),
                    ExtensionBackendServiceReadiness::MissingArtifact,
                    Some("backend service instance 未找到".to_string()),
                    None,
                    None,
                    None,
                );
            };
            if let Some(status) = process_exit_status(instance) {
                instance.readiness = ExtensionBackendServiceReadiness::ProcessExited;
                instance.message = Some(format!("backend service 已退出: {status}"));
                instance.updated_at = Utc::now();
                return status_from_instance(instance);
            }
            match instance.service.health_path.clone() {
                Some(path) => Some((instance.endpoint.clone(), path)),
                None => {
                    instance.readiness = ExtensionBackendServiceReadiness::Ready;
                    instance.message = None;
                    instance.updated_at = Utc::now();
                    return status_from_instance(instance);
                }
            }
        };

        let (endpoint, health_path) = health_target.expect("health target");
        let health_url = match service_url(&endpoint, &health_path) {
            Ok(url) => url,
            Err(error) => {
                return self
                    .update_health_failed(identity, format!("health_path 非法: {error}"))
                    .await;
            }
        };
        let client = match reqwest::Client::builder()
            .timeout(self.config.request_timeout)
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                return self
                    .update_health_failed(identity, format!("health client 初始化失败: {error}"))
                    .await;
            }
        };
        match client.get(health_url).send().await {
            Ok(response) if response.status().is_success() => {
                let mut guard = self.instances.lock().await;
                let Some(instance) = guard.get_mut(&identity.instance_key()) else {
                    return status_from_parts(
                        identity.clone(),
                        ExtensionBackendServiceReadiness::MissingArtifact,
                        Some("backend service instance 未找到".to_string()),
                        None,
                        None,
                        None,
                    );
                };
                if let Some(status) = process_exit_status(instance) {
                    instance.readiness = ExtensionBackendServiceReadiness::ProcessExited;
                    instance.message = Some(format!("backend service 已退出: {status}"));
                } else {
                    instance.readiness = ExtensionBackendServiceReadiness::Ready;
                    instance.message = None;
                }
                instance.updated_at = Utc::now();
                status_from_instance(instance)
            }
            Ok(response) => {
                self.update_health_failed(
                    identity,
                    format!("health 返回 HTTP {}", response.status()),
                )
                .await
            }
            Err(error) => {
                self.update_health_failed(identity, format!("health 请求失败: {error}"))
                    .await
            }
        }
    }

    async fn update_health_failed(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
        message: String,
    ) -> ExtensionBackendServiceStatus {
        let mut guard = self.instances.lock().await;
        let Some(instance) = guard.get_mut(&identity.instance_key()) else {
            return status_from_parts(
                identity.clone(),
                ExtensionBackendServiceReadiness::MissingArtifact,
                Some("backend service instance 未找到".to_string()),
                None,
                None,
                None,
            );
        };
        if let Some(status) = process_exit_status(instance) {
            instance.readiness = ExtensionBackendServiceReadiness::ProcessExited;
            instance.message = Some(format!("backend service 已退出: {status}"));
        } else {
            instance.readiness = ExtensionBackendServiceReadiness::HealthFailed;
            instance.message = Some(message);
        }
        instance.updated_at = Utc::now();
        status_from_instance(instance)
    }

    async fn stop_replaced_service_instances(
        &self,
        identity: &ExtensionBackendServiceInstanceIdentity,
    ) {
        let removed = {
            let mut guard = self.instances.lock().await;
            let keys = guard
                .iter()
                .filter_map(|(key, instance)| {
                    if identity.same_service_without_artifact(&instance.identity) {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| guard.remove(&key))
                .collect::<Vec<_>>()
        };
        for mut instance in removed {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
        }
    }
}

impl Default for LocalExtensionBackendServiceManager {
    fn default() -> Self {
        Self::with_default_config()
    }
}

async fn materialize_backend_service(
    service_cache_root: &Path,
    request: ExtensionBackendServiceMaterializeRequest,
) -> Result<ExtensionBackendServiceMaterialization, ExtensionBackendServiceError> {
    let package_dir = tokio::fs::canonicalize(&request.cache_entry.unpacked_dir)
        .await
        .map_err(|error| {
            ExtensionBackendServiceError::Materialize(format!(
                "artifact package 不存在: {} ({error})",
                request.cache_entry.unpacked_dir.display()
            ))
        })?;
    let service = load_backend_service_definition(&package_dir, &request.identity.service_key)
        .await
        .map_err(materialize_load_error)?;
    if service.runtime != "node" {
        return Err(ExtensionBackendServiceError::UnsupportedRuntime(format!(
            "{} 使用 runtime `{}`",
            service.service_key, service.runtime
        )));
    }
    let entry_path = safe_package_path(&package_dir, &service.entry)
        .map_err(|error| ExtensionBackendServiceError::Materialize(error.to_string()))?;
    if !tokio::fs::try_exists(&entry_path).await? {
        return Err(ExtensionBackendServiceError::Materialize(format!(
            "backend service entry 不存在: {}",
            service.entry
        )));
    }

    let service_root = service_root(service_cache_root, &request.identity)?;
    let materialized_package_dir = service_root.join("package");
    replace_dir_with_copy(&package_dir, &materialized_package_dir).await?;

    let materialized_entry = safe_package_path(&materialized_package_dir, &service.entry)
        .map_err(|error| ExtensionBackendServiceError::Materialize(error.to_string()))?;
    Ok(ExtensionBackendServiceMaterialization {
        artifact_id: request.identity.artifact_id,
        archive_digest: request.identity.archive_digest,
        extension_key: request.identity.extension_key,
        service_key: request.identity.service_key,
        service_root,
        entry_path: materialized_entry,
    })
}

fn materialize_load_error(error: ExtensionBackendServiceError) -> ExtensionBackendServiceError {
    match error {
        ExtensionBackendServiceError::UnsupportedRuntime(_) => error,
        other => ExtensionBackendServiceError::Materialize(other.to_string()),
    }
}

async fn load_backend_service_definition(
    package_dir: &Path,
    service_key: &str,
) -> Result<ExtensionBackendServiceDefinition, ExtensionBackendServiceError> {
    let manifest_path = package_dir.join("agentdash.extension.json");
    let bytes = tokio::fs::read(&manifest_path).await.map_err(|error| {
        ExtensionBackendServiceError::Materialize(format!(
            "读取 agentdash.extension.json 失败: {error}"
        ))
    })?;
    let manifest: ExtensionTemplatePayload = serde_json::from_slice(&bytes)?;
    let service = manifest
        .backend_services
        .iter()
        .find(|service| service.service_key == service_key)
        .cloned()
        .ok_or_else(|| {
            ExtensionBackendServiceError::Materialize(format!(
                "manifest 未声明 backend service: {service_key}"
            ))
        })?;
    if service.runtime != "node" {
        return Err(ExtensionBackendServiceError::UnsupportedRuntime(format!(
            "{} 使用 runtime `{}`",
            service.service_key, service.runtime
        )));
    }
    manifest
        .validate()
        .map_err(|error| ExtensionBackendServiceError::Materialize(error.to_string()))?;
    Ok(service)
}

async fn replace_dir_with_copy(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || {
        if destination.exists() {
            std::fs::remove_dir_all(&destination)?;
        }
        std::fs::create_dir_all(&destination)?;
        copy_dir_recursive(&source, &destination)
    })
    .await
    .map_err(|error| std::io::Error::other(format!("copy task join 失败: {error}")))?
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn service_root(
    root: &Path,
    identity: &ExtensionBackendServiceInstanceIdentity,
) -> Result<PathBuf, ExtensionBackendServiceError> {
    let digest = identity
        .archive_digest
        .strip_prefix("sha256:")
        .ok_or_else(|| {
            ExtensionBackendServiceError::Materialize("archive_digest 非法".to_string())
        })?;
    Ok(root
        .join("backend-services")
        .join(sanitize_path_segment(&identity.project_id))
        .join(sanitize_path_segment(&identity.backend_id))
        .join(sanitize_path_segment(&identity.extension_key))
        .join(sanitize_path_segment(&identity.service_key))
        .join(format!(
            "{}-{digest}",
            sanitize_path_segment(&identity.artifact_id)
        )))
}

fn safe_package_path(root: &Path, relative: &str) -> Result<PathBuf, ExtensionBackendServiceError> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(ExtensionBackendServiceError::Materialize(format!(
            "service entry 必须是相对路径: {relative}"
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ExtensionBackendServiceError::Materialize(format!(
                    "service entry 包含不安全路径: {relative}"
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(ExtensionBackendServiceError::Materialize(
            "service entry 不能为空".to_string(),
        ));
    }
    Ok(root.join(normalized))
}

fn reserve_loopback_port() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn process_exit_status(instance: &mut BackendServiceInstance) -> Option<String> {
    match instance.child.try_wait() {
        Ok(Some(status)) => Some(status.to_string()),
        Ok(None) => None,
        Err(error) => Some(format!("status unavailable: {error}")),
    }
}

fn status_from_instance(instance: &BackendServiceInstance) -> ExtensionBackendServiceStatus {
    ExtensionBackendServiceStatus {
        identity: instance.identity.clone(),
        readiness: instance.readiness.clone(),
        message: instance.message.clone(),
        endpoint: Some(instance.endpoint.clone()),
        pid: instance.child.id(),
        materialization: Some(instance.materialization.clone()),
        updated_at: instance.updated_at,
    }
}

fn status_from_parts(
    identity: ExtensionBackendServiceInstanceIdentity,
    readiness: ExtensionBackendServiceReadiness,
    message: Option<String>,
    endpoint: Option<String>,
    pid: Option<u32>,
    materialization: Option<ExtensionBackendServiceMaterialization>,
) -> ExtensionBackendServiceStatus {
    ExtensionBackendServiceStatus {
        identity,
        readiness,
        message,
        endpoint,
        pid,
        materialization,
        updated_at: Utc::now(),
    }
}

fn missing_artifact_identity(
    request: &ExtensionBackendServiceStartRequest,
) -> ExtensionBackendServiceInstanceIdentity {
    ExtensionBackendServiceInstanceIdentity {
        project_id: request.project_id.clone(),
        backend_id: request.backend_id.clone(),
        extension_key: request.extension_key.clone(),
        service_key: request.service_key.clone(),
        artifact_id: String::new(),
        archive_digest: String::new(),
    }
}

fn service_url(endpoint: &str, route: &str) -> Result<String, String> {
    if !route.starts_with('/') {
        return Err("route 必须以 / 开头".to_string());
    }
    Ok(format!("{}{}", endpoint.trim_end_matches('/'), route))
}

fn route_matches_any(route: &str, patterns: &[String]) -> bool {
    let candidate = route.split('?').next().unwrap_or(route);
    patterns.iter().any(|pattern| {
        let pattern = strip_pattern_query(pattern);
        if pattern.starts_with("http://") || pattern.starts_with("https://") {
            let Ok(parsed) = reqwest::Url::parse(pattern) else {
                return false;
            };
            return matches_path_pattern(candidate, parsed.path());
        }
        matches_path_pattern(candidate, pattern)
    })
}

fn matches_path_pattern(candidate: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return candidate == prefix || candidate.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return candidate.starts_with(prefix);
    }
    candidate == pattern
}

fn strip_pattern_query(pattern: &str) -> &str {
    pattern.split('?').next().unwrap_or(pattern)
}

fn header_map_from_record(headers: &BTreeMap<String, String>) -> Result<HeaderMap, String> {
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("header name 非法 `{name}`: {error}"))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|error| format!("header value 非法 `{name}`: {error}"))?;
        result.insert(header_name, header_value);
    }
    Ok(result)
}

fn headers_to_record(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

fn no_body_status(status: u16) -> bool {
    matches!(status, 204 | 205 | 304)
}

fn sanitize_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn spawn_log_drain<T>(logs: SharedLogBuffer, stream: T, stream_name: &'static str, max_lines: usize)
where
    T: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut guard = logs.lock().await;
            guard.push_back(ExtensionBackendServiceLogLine {
                stream: stream_name.to_string(),
                line,
                timestamp: Utc::now(),
            });
            while guard.len() > max_lines {
                guard.pop_front();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[tokio::test]
    async fn materializes_backend_service_entry_from_cached_package() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_entry = write_service_package(temp.path(), service_script()).await;
        let manager = test_manager(temp.path());
        let identity = identity();

        let materialization = manager
            .materialize_cached_artifact(ExtensionBackendServiceMaterializeRequest {
                identity: identity.clone(),
                cache_entry,
            })
            .await
            .expect("materialize");

        assert_eq!(materialization.artifact_id, identity.artifact_id);
        assert_eq!(materialization.extension_key, "local-hello");
        assert!(materialization.entry_path.ends_with("dist/server.mjs"));
        let copied = tokio::fs::read_to_string(&materialization.entry_path)
            .await
            .expect("read copied entry");
        assert!(copied.contains("createServer"));
    }

    #[tokio::test]
    async fn starts_node_service_invokes_http_and_stops() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_entry = write_service_package(temp.path(), service_script()).await;
        let manager = test_manager(temp.path());
        let status = manager.start(start_request(cache_entry)).await;

        assert_eq!(status.readiness, ExtensionBackendServiceReadiness::Ready);
        assert!(
            status
                .endpoint
                .as_deref()
                .unwrap_or("")
                .starts_with("http://127.0.0.1:")
        );

        let response = manager
            .invoke(ExtensionBackendServiceInvokeRequest {
                identity: identity(),
                extension_id: "local-hello".to_string(),
                route: "/api/echo?source=test".to_string(),
                method: "POST".to_string(),
                headers: BTreeMap::from([("content-type".to_string(), "text/plain".to_string())]),
                body: Some(b"ping".to_vec()),
                trace_id: "trace-1".to_string(),
            })
            .await
            .expect("invoke");

        assert_eq!(response.status, 200);
        assert_eq!(response.metadata.service_key, "local-hello.api");
        let body = String::from_utf8(response.body.expect("body")).expect("utf8");
        assert!(body.contains("ping"));

        let stopped = manager.stop(&identity()).await;
        assert_eq!(
            stopped.readiness,
            ExtensionBackendServiceReadiness::ProcessExited
        );
    }

    #[tokio::test]
    async fn health_failure_returns_diagnostic_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_entry = write_service_package(temp.path(), unhealthy_service_script()).await;
        let manager = test_manager(temp.path());

        let status = manager.start(start_request(cache_entry)).await;

        assert_eq!(
            status.readiness,
            ExtensionBackendServiceReadiness::HealthFailed
        );
        assert!(status.message.as_deref().unwrap_or("").contains("HTTP 500"));
        let _ = manager.stop(&identity()).await;
    }

    #[tokio::test]
    async fn process_exit_updates_diagnostic_and_keeps_logs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_entry =
            write_service_package_with_health(temp.path(), exiting_service_script(), None).await;
        let manager = test_manager(temp.path());
        let status = manager.start(start_request(cache_entry)).await;
        assert_eq!(status.readiness, ExtensionBackendServiceReadiness::Ready);

        tokio::time::sleep(Duration::from_millis(200)).await;
        let status = manager.health(&identity()).await;

        assert_eq!(
            status.readiness,
            ExtensionBackendServiceReadiness::ProcessExited
        );
        let logs = manager.logs(&identity()).await;
        assert!(logs.iter().any(|line| line.line.contains("service fatal")));
    }

    #[tokio::test]
    async fn unsupported_runtime_is_structured_diagnostic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_entry =
            write_service_package_with_runtime(temp.path(), service_script(), "python").await;
        let manager = test_manager(temp.path());

        let status = manager.start(start_request(cache_entry)).await;

        assert_eq!(
            status.readiness,
            ExtensionBackendServiceReadiness::UnsupportedRuntime
        );
    }

    fn test_manager(root: &Path) -> LocalExtensionBackendServiceManager {
        LocalExtensionBackendServiceManager::new(LocalExtensionBackendServiceManagerConfig {
            service_cache_root: root.join("runtime-cache"),
            startup_timeout: Duration::from_millis(600),
            health_poll_interval: Duration::from_millis(50),
            request_timeout: Duration::from_millis(500),
            ..LocalExtensionBackendServiceManagerConfig::default()
        })
    }

    fn identity() -> ExtensionBackendServiceInstanceIdentity {
        ExtensionBackendServiceInstanceIdentity {
            project_id: "project-1".to_string(),
            backend_id: "backend-1".to_string(),
            extension_key: "local-hello".to_string(),
            service_key: "local-hello.api".to_string(),
            artifact_id: "artifact-1".to_string(),
            archive_digest: DIGEST.to_string(),
        }
    }

    fn start_request(
        cache_entry: ExtensionArtifactCacheEntry,
    ) -> ExtensionBackendServiceStartRequest {
        ExtensionBackendServiceStartRequest {
            project_id: "project-1".to_string(),
            backend_id: "backend-1".to_string(),
            extension_key: "local-hello".to_string(),
            extension_id: "local-hello".to_string(),
            service_key: "local-hello.api".to_string(),
            artifact: Some(ExtensionBackendServiceArtifact {
                artifact_id: "artifact-1".to_string(),
                archive_digest: DIGEST.to_string(),
            }),
            cache_entry: Some(cache_entry),
        }
    }

    async fn write_service_package(root: &Path, script: &str) -> ExtensionArtifactCacheEntry {
        write_service_package_with_health(root, script, Some("/health")).await
    }

    async fn write_service_package_with_runtime(
        root: &Path,
        script: &str,
        runtime: &str,
    ) -> ExtensionArtifactCacheEntry {
        write_service_package_manifest(root, script, Some("/health"), runtime).await
    }

    async fn write_service_package_with_health(
        root: &Path,
        script: &str,
        health_path: Option<&str>,
    ) -> ExtensionArtifactCacheEntry {
        write_service_package_manifest(root, script, health_path, "node").await
    }

    async fn write_service_package_manifest(
        root: &Path,
        script: &str,
        health_path: Option<&str>,
        runtime: &str,
    ) -> ExtensionArtifactCacheEntry {
        let package_dir = root.join("artifact-package");
        tokio::fs::create_dir_all(package_dir.join("dist"))
            .await
            .expect("dist dir");
        tokio::fs::write(package_dir.join("dist/server.mjs"), script)
            .await
            .expect("script");
        let manifest = json!({
            "manifest_version": "2",
            "extension_id": "local-hello",
            "package": { "name": "@agentdash/local-hello", "version": "0.1.0" },
            "asset_version": "0.1.0",
            "backend_services": [{
                "service_key": "local-hello.api",
                "runtime": runtime,
                "entry": "dist/server.mjs",
                "routes": ["/api/**"],
                "health_path": health_path,
            }],
        });
        tokio::fs::write(
            package_dir.join("agentdash.extension.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest"),
        )
        .await
        .expect("write manifest");
        ExtensionArtifactCacheEntry {
            cache_key: "artifact-1-0123456789abcdef".to_string(),
            archive_path: root.join("archive.agentdash-extension.tgz"),
            unpacked_dir: package_dir,
        }
    }

    fn service_script() -> &'static str {
        r#"
import { createServer } from 'node:http';

const port = Number(process.env.AGENTDASH_BACKEND_SERVICE_PORT || process.env.PORT);
const host = process.env.AGENTDASH_BACKEND_SERVICE_HOST || process.env.HOST || '127.0.0.1';
const server = createServer((request, response) => {
  if (request.url === '/health') {
    response.writeHead(200, { 'content-type': 'text/plain' });
    response.end('ok');
    return;
  }
  if (request.url?.startsWith('/api/echo')) {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => { body += chunk; });
    request.on('end', () => {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ method: request.method, body }));
    });
    return;
  }
  response.writeHead(404);
  response.end('missing');
});
server.listen(port, host);
"#
    }

    fn unhealthy_service_script() -> &'static str {
        r#"
import { createServer } from 'node:http';
const port = Number(process.env.AGENTDASH_BACKEND_SERVICE_PORT || process.env.PORT);
const host = process.env.AGENTDASH_BACKEND_SERVICE_HOST || '127.0.0.1';
createServer((request, response) => {
  response.writeHead(request.url === '/health' ? 500 : 404);
  response.end('not-ready');
}).listen(port, host);
"#
    }

    fn exiting_service_script() -> &'static str {
        r#"
console.log('service fatal');
setTimeout(() => process.exit(17), 50);
"#
    }
}
