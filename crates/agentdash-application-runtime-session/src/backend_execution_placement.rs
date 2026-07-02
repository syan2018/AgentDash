use std::collections::BTreeSet;

use agentdash_application_ports::backend_transport::RelayPromptTransport;
use agentdash_domain::backend::{BackendExecutionLeaseRepository, BackendExecutionSelectionMode};
use agentdash_domain::common::AgentConfig;
use agentdash_spi::connector::ConnectorError;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSelectionRequest {
    pub executor_id: String,
    pub intent: BackendSelectionIntent,
    pub reason: Option<String>,
    pub authorized_backend_ids: Vec<String>,
}

impl BackendSelectionRequest {
    pub fn auto_idle(executor_id: impl Into<String>, reason: Option<String>) -> Self {
        Self {
            executor_id: executor_id.into(),
            intent: BackendSelectionIntent::AutoIdle,
            reason,
            authorized_backend_ids: Vec::new(),
        }
    }

    pub fn explicit(
        executor_id: impl Into<String>,
        backend_id: impl Into<String>,
        reason: Option<String>,
    ) -> Self {
        Self {
            executor_id: executor_id.into(),
            intent: BackendSelectionIntent::Explicit {
                backend_id: backend_id.into(),
            },
            reason,
            authorized_backend_ids: Vec::new(),
        }
    }

    pub fn workspace_binding(
        executor_id: impl Into<String>,
        backend_id: impl Into<String>,
        reason: Option<String>,
    ) -> Self {
        Self {
            executor_id: executor_id.into(),
            intent: BackendSelectionIntent::WorkspaceBinding {
                backend_id: backend_id.into(),
            },
            reason,
            authorized_backend_ids: Vec::new(),
        }
    }

    pub fn with_authorized_backend_ids(mut self, backend_ids: Vec<String>) -> Self {
        self.authorized_backend_ids = backend_ids
            .into_iter()
            .map(|backend_id| backend_id.trim().to_string())
            .filter(|backend_id| !backend_id.is_empty())
            .collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendSelectionIntent {
    Explicit { backend_id: String },
    AutoIdle,
    WorkspaceBinding { backend_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlacementPlan {
    pub backend_id: String,
    pub executor_id: String,
    pub selection_mode: BackendExecutionSelectionMode,
    pub claim_reason: Option<String>,
    pub lease_id: Option<Uuid>,
}

impl ExecutionPlacementPlan {
    fn new(
        backend_id: String,
        executor_id: String,
        selection_mode: BackendExecutionSelectionMode,
        claim_reason: Option<String>,
    ) -> Self {
        Self {
            backend_id,
            executor_id,
            selection_mode,
            claim_reason,
            lease_id: None,
        }
    }

    pub fn with_lease_id(mut self, lease_id: Uuid) -> Self {
        self.lease_id = Some(lease_id);
        self
    }
}

pub fn has_available_relay_executor(
    transport: &dyn RelayPromptTransport,
    executor_id: &str,
) -> bool {
    transport.list_online_executors().iter().any(|executor| {
        executor.executor_id.eq_ignore_ascii_case(executor_id) && executor.available
    })
}

pub async fn resolve_backend_execution_placement(
    transport: &dyn RelayPromptTransport,
    lease_repo: &dyn BackendExecutionLeaseRepository,
    request: &BackendSelectionRequest,
) -> Result<ExecutionPlacementPlan, ConnectorError> {
    let executor_id = request.executor_id.trim();
    if executor_id.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "backend selection 缺少 executor_id".to_string(),
        ));
    }

    match &request.intent {
        BackendSelectionIntent::Explicit { backend_id } => {
            resolve_fixed_backend(
                transport,
                executor_id,
                backend_id,
                BackendExecutionSelectionMode::Explicit,
                request.reason.clone(),
                &request.authorized_backend_ids,
            )
            .await
        }
        BackendSelectionIntent::WorkspaceBinding { backend_id } => {
            resolve_fixed_backend(
                transport,
                executor_id,
                backend_id,
                BackendExecutionSelectionMode::WorkspaceBinding,
                request.reason.clone(),
                &request.authorized_backend_ids,
            )
            .await
        }
        BackendSelectionIntent::AutoIdle => {
            resolve_auto_idle_backend(
                transport,
                lease_repo,
                executor_id,
                request.reason.clone(),
                &request.authorized_backend_ids,
            )
            .await
        }
    }
}

async fn resolve_fixed_backend(
    transport: &dyn RelayPromptTransport,
    executor_id: &str,
    backend_id: &str,
    selection_mode: BackendExecutionSelectionMode,
    claim_reason: Option<String>,
    authorized_backend_ids: &[String],
) -> Result<ExecutionPlacementPlan, ConnectorError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "backend selection 缺少 backend_id".to_string(),
        ));
    }
    if !authorized_backend_ids.is_empty()
        && !authorized_backend_ids
            .iter()
            .any(|authorized| authorized == backend_id)
    {
        return Err(ConnectorError::InvalidConfig(format!(
            "backend `{backend_id}` 不在当前 Project 授权范围内"
        )));
    }
    if !transport.is_online(backend_id).await {
        return Err(ConnectorError::ConnectionFailed(format!(
            "backend `{backend_id}` 当前不在线"
        )));
    }

    if requires_backend_relay_executor(executor_id) {
        let has_executor = transport.list_online_executors().iter().any(|executor| {
            executor.backend_id == backend_id
                && executor.executor_id.eq_ignore_ascii_case(executor_id)
                && executor.available
        });
        if !has_executor {
            return Err(ConnectorError::ConnectionFailed(format!(
                "backend `{backend_id}` 当前未提供可用执行器 `{executor_id}`"
            )));
        }
    }

    Ok(ExecutionPlacementPlan::new(
        backend_id.to_string(),
        executor_id.to_string(),
        selection_mode,
        claim_reason,
    ))
}

fn requires_backend_relay_executor(executor_id: &str) -> bool {
    !AgentConfig::new(executor_id).is_cloud_native()
}

async fn resolve_auto_idle_backend(
    transport: &dyn RelayPromptTransport,
    lease_repo: &dyn BackendExecutionLeaseRepository,
    executor_id: &str,
    claim_reason: Option<String>,
    authorized_backend_ids: &[String],
) -> Result<ExecutionPlacementPlan, ConnectorError> {
    let authorized = authorized_backend_ids.iter().collect::<BTreeSet<_>>();
    let mut candidates = transport
        .list_online_executors()
        .iter()
        .filter(|executor| {
            executor.executor_id.eq_ignore_ascii_case(executor_id) && executor.available
        })
        .filter(|executor| authorized.is_empty() || authorized.contains(&executor.backend_id))
        .map(|executor| executor.backend_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        let scope = if authorized.is_empty() {
            String::new()
        } else {
            "（已按当前 Project 授权范围过滤）".to_string()
        };
        return Err(ConnectorError::ConnectionFailed(format!(
            "没有在线后端提供可用执行器 `{executor_id}`{scope}"
        )));
    }

    let counts = lease_repo
        .count_active_by_backend(&candidates)
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!("读取 backend active lease 失败: {error}"))
        })?;
    candidates.sort_by(|left, right| {
        let left_count = counts.get(left).copied().unwrap_or_default();
        let right_count = counts.get(right).copied().unwrap_or_default();
        left_count.cmp(&right_count).then_with(|| left.cmp(right))
    });

    Ok(ExecutionPlacementPlan::new(
        candidates.remove(0),
        executor_id.to_string(),
        BackendExecutionSelectionMode::AutoIdle,
        claim_reason,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use agentdash_application_ports::backend_transport::{
        BackendTransport, DirectoryBrowseInfo, RelayPromptRequest, RelayPromptTransport,
        RelaySessionRoute, RelaySessionRouteInfo, RelaySteerRequest, RemoteExecutorInfo,
        TransportError, WorkspaceIdentityDiscoveryInfo, WorkspaceIdentityDiscoveryRequest,
        WorkspaceProbeInfo,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::backend::{
        BackendExecutionLease, BackendExecutionLeaseRepository, BackendExecutionTerminalKind,
    };
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};

    use super::*;

    #[derive(Default)]
    struct TestTransport {
        executors: Vec<RemoteExecutorInfo>,
        offline_backend_ids: BTreeSet<String>,
    }

    #[async_trait]
    impl BackendTransport for TestTransport {
        async fn is_online(&self, backend_id: &str) -> bool {
            !self.offline_backend_ids.contains(backend_id)
        }

        async fn list_online_backend_ids(&self) -> Vec<String> {
            Vec::new()
        }

        async fn detect_workspace(
            &self,
            _backend_id: &str,
            _root: &str,
        ) -> Result<WorkspaceProbeInfo, TransportError> {
            Ok(WorkspaceProbeInfo::default())
        }

        async fn browse_directory(
            &self,
            _backend_id: &str,
            _path: Option<&str>,
        ) -> Result<DirectoryBrowseInfo, TransportError> {
            Ok(DirectoryBrowseInfo::default())
        }

        async fn discover_workspace_by_identity(
            &self,
            _backend_id: &str,
            _workspaces: Vec<WorkspaceIdentityDiscoveryRequest>,
        ) -> Result<WorkspaceIdentityDiscoveryInfo, TransportError> {
            Ok(WorkspaceIdentityDiscoveryInfo::default())
        }
    }

    #[async_trait]
    impl RelayPromptTransport for TestTransport {
        async fn relay_prompt(
            &self,
            _backend_id: &str,
            _payload: RelayPromptRequest,
        ) -> Result<String, TransportError> {
            Ok("turn".to_string())
        }

        async fn relay_cancel(
            &self,
            _backend_id: &str,
            _session_id: &str,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        async fn relay_steer(
            &self,
            _backend_id: &str,
            _payload: RelaySteerRequest,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        fn list_online_executors(&self) -> Vec<RemoteExecutorInfo> {
            self.executors.clone()
        }

        async fn resolve_backend(
            &self,
            _executor_id: &str,
            preferred_backend_id: Option<&str>,
        ) -> Result<String, TransportError> {
            preferred_backend_id
                .map(ToString::to_string)
                .ok_or_else(|| TransportError::OperationFailed("missing backend".to_string()))
        }

        fn register_session_sink(&self, _route: RelaySessionRoute) {}

        fn unregister_session_sink(&self, _session_id: &str) {}

        fn has_session_sink(&self, _session_id: &str) -> bool {
            false
        }

        fn session_route(&self, _session_id: &str) -> Option<RelaySessionRouteInfo> {
            None
        }
    }

    struct TestLeaseRepository;

    #[async_trait]
    impl BackendExecutionLeaseRepository for TestLeaseRepository {
        async fn claim(&self, _lease: &BackendExecutionLease) -> Result<(), DomainError> {
            Ok(())
        }

        async fn activate(
            &self,
            _lease_id: Uuid,
            _activated_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn release(
            &self,
            _lease_id: Uuid,
            _terminal_kind: Option<BackendExecutionTerminalKind>,
            _reason: Option<String>,
            _released_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn fail(
            &self,
            _lease_id: Uuid,
            _reason: Option<String>,
            _failed_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn mark_lost_by_backend(
            &self,
            _backend_id: &str,
            _reason: Option<String>,
            _lost_at: DateTime<Utc>,
        ) -> Result<u64, DomainError> {
            Ok(0)
        }

        async fn get_by_id(
            &self,
            _lease_id: Uuid,
        ) -> Result<Option<BackendExecutionLease>, DomainError> {
            Ok(None)
        }

        async fn list_active(&self) -> Result<Vec<BackendExecutionLease>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_active_by_backend(
            &self,
            backend_ids: &[String],
        ) -> Result<HashMap<String, i64>, DomainError> {
            Ok(backend_ids
                .iter()
                .map(|backend_id| (backend_id.clone(), 0))
                .collect())
        }
    }

    #[tokio::test]
    async fn explicit_cloud_native_executor_does_not_require_backend_relay_executor() {
        let transport = TestTransport::default();
        let lease_repo = TestLeaseRepository;

        let plan = resolve_backend_execution_placement(
            &transport,
            &lease_repo,
            &BackendSelectionRequest::explicit(
                "PI_AGENT",
                "local_4625439c028d045e69448a88",
                Some("test".to_string()),
            ),
        )
        .await
        .expect("cloud-native executor should not require backend relay executor");

        assert_eq!(plan.backend_id, "local_4625439c028d045e69448a88");
        assert_eq!(plan.executor_id, "PI_AGENT");
        assert_eq!(plan.selection_mode, BackendExecutionSelectionMode::Explicit);
    }

    #[tokio::test]
    async fn explicit_cloud_native_executor_rejects_offline_backend() {
        let transport = TestTransport {
            executors: Vec::new(),
            offline_backend_ids: BTreeSet::from(["local_offline".to_string()]),
        };
        let lease_repo = TestLeaseRepository;

        let error = resolve_backend_execution_placement(
            &transport,
            &lease_repo,
            &BackendSelectionRequest::explicit(
                "PI_AGENT",
                "local_offline",
                Some("test".to_string()),
            ),
        )
        .await
        .expect_err("offline backend should be rejected even for cloud-native executor");

        assert!(matches!(error, ConnectorError::ConnectionFailed(_)));
        assert!(
            error
                .to_string()
                .contains("backend `local_offline` 当前不在线")
        );
    }

    #[tokio::test]
    async fn explicit_relay_executor_still_requires_backend_executor() {
        let transport = TestTransport::default();
        let lease_repo = TestLeaseRepository;

        let error = resolve_backend_execution_placement(
            &transport,
            &lease_repo,
            &BackendSelectionRequest::explicit("CODEX", "local", Some("test".to_string())),
        )
        .await
        .expect_err("relay executor should require backend executor");

        assert!(error.to_string().contains("当前未提供可用执行器 `CODEX`"));
        assert!(matches!(error, ConnectorError::ConnectionFailed(_)));
    }

    #[tokio::test]
    async fn auto_idle_reports_no_available_backend_as_connection_failure() {
        let transport = TestTransport::default();
        let lease_repo = TestLeaseRepository;

        let error = resolve_backend_execution_placement(
            &transport,
            &lease_repo,
            &BackendSelectionRequest::auto_idle("CODEX", Some("test".to_string())),
        )
        .await
        .expect_err("auto idle should report no available backend");

        assert!(matches!(error, ConnectorError::ConnectionFailed(_)));
        assert!(
            error
                .to_string()
                .contains("没有在线后端提供可用执行器 `CODEX`")
        );
    }
}
