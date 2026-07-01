use std::collections::BTreeSet;

use agentdash_application_ports::backend_transport::RelayPromptTransport;
use agentdash_domain::backend::{BackendExecutionLeaseRepository, BackendExecutionSelectionMode};
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

    let has_executor = transport.list_online_executors().iter().any(|executor| {
        executor.backend_id == backend_id
            && executor.executor_id.eq_ignore_ascii_case(executor_id)
            && executor.available
    });
    if !has_executor {
        return Err(ConnectorError::Runtime(format!(
            "backend `{backend_id}` 当前未提供可用执行器 `{executor_id}`"
        )));
    }

    Ok(ExecutionPlacementPlan::new(
        backend_id.to_string(),
        executor_id.to_string(),
        selection_mode,
        claim_reason,
    ))
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
        return Err(ConnectorError::Runtime(format!(
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
