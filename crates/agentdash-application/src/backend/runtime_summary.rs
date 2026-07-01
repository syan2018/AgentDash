use std::collections::{HashMap, HashSet};

use agentdash_domain::backend::{
    BackendConfig, BackendExecutionLease, BackendExecutionLeaseRepository, BackendShareScopeKind,
    BackendType, BackendVisibility, RuntimeHealth, RuntimeHealthRepository,
};
use agentdash_relay::CapabilitiesPayload;

use crate::ApplicationError;

#[derive(Debug, Clone)]
pub struct BackendRuntimeOnlineSnapshot {
    pub backend_id: String,
    pub name: String,
    pub capabilities: CapabilitiesPayload,
    pub executors: Vec<BackendRuntimeExecutorSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendRuntimeExecutorSnapshot {
    pub executor_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct BackendRuntimeSummary {
    pub backend: BackendConfig,
    pub backend_id: String,
    pub name: String,
    pub enabled: bool,
    pub online: bool,
    pub capabilities: Option<CapabilitiesPayload>,
    pub runtime_health: Option<RuntimeHealth>,
    pub executors: Vec<BackendRuntimeExecutorSummary>,
    pub active_session_count: usize,
    pub active_sessions: Vec<BackendExecutionLease>,
    pub allocatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendRuntimeExecutorSummary {
    pub executor_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
    pub active_session_count: usize,
    pub allocatable: bool,
}

pub async fn list_backend_runtime_summaries(
    runtime_health_repo: &dyn RuntimeHealthRepository,
    backend_execution_lease_repo: &dyn BackendExecutionLeaseRepository,
    visible_backends: Vec<BackendConfig>,
    online_snapshots: Vec<BackendRuntimeOnlineSnapshot>,
    include_online_unregistered: bool,
) -> Result<Vec<BackendRuntimeSummary>, ApplicationError> {
    let runtime_health_by_backend = runtime_health_repo
        .list_runtime_health()
        .await?
        .into_iter()
        .map(|health| (health.backend_id.clone(), health))
        .collect::<HashMap<_, _>>();
    let active_leases = backend_execution_lease_repo.list_active().await?;
    let active_leases_by_backend = active_leases.into_iter().fold(
        HashMap::<String, Vec<BackendExecutionLease>>::new(),
        |mut acc, lease| {
            acc.entry(lease.backend_id.clone()).or_default().push(lease);
            acc
        },
    );

    let mut backends = visible_backends;
    let mut seen_ids = backends
        .iter()
        .map(|backend| backend.id.clone())
        .collect::<HashSet<_>>();
    if include_online_unregistered {
        for online in &online_snapshots {
            if seen_ids.insert(online.backend_id.clone()) {
                backends.push(online_backend_config(online));
            }
        }
    }

    Ok(project_backend_runtime_summaries(
        backends,
        online_snapshots,
        runtime_health_by_backend,
        active_leases_by_backend,
    ))
}

pub fn project_backend_runtime_summaries(
    backends: Vec<BackendConfig>,
    online_snapshots: Vec<BackendRuntimeOnlineSnapshot>,
    runtime_health_by_backend: HashMap<String, RuntimeHealth>,
    active_leases_by_backend: HashMap<String, Vec<BackendExecutionLease>>,
) -> Vec<BackendRuntimeSummary> {
    backends
        .into_iter()
        .map(|backend| {
            let online_info = online_snapshots
                .iter()
                .find(|online| online.backend_id == backend.id);
            let online = online_info.is_some();
            let runtime_health = runtime_health_by_backend.get(&backend.id).cloned();
            let active_sessions = active_leases_by_backend
                .get(&backend.id)
                .cloned()
                .unwrap_or_default();
            let executors = backend_runtime_executors(online_info, &active_sessions);
            let allocatable =
                backend.enabled && online && executors.iter().any(|executor| executor.allocatable);
            let backend_id = backend.id.clone();
            let name = backend.name.clone();
            let enabled = backend.enabled;
            BackendRuntimeSummary {
                backend,
                backend_id,
                name,
                enabled,
                online,
                capabilities: online_info.map(|online| online.capabilities.clone()),
                runtime_health,
                active_session_count: active_sessions.len(),
                active_sessions,
                executors,
                allocatable,
            }
        })
        .collect()
}

fn backend_runtime_executors(
    online_info: Option<&BackendRuntimeOnlineSnapshot>,
    active_sessions: &[BackendExecutionLease],
) -> Vec<BackendRuntimeExecutorSummary> {
    let Some(online_info) = online_info else {
        return Vec::new();
    };
    online_info
        .executors
        .iter()
        .map(|executor| {
            let active_session_count = active_sessions
                .iter()
                .filter(|lease| {
                    lease
                        .executor_id
                        .eq_ignore_ascii_case(&executor.executor_id)
                })
                .count();
            BackendRuntimeExecutorSummary {
                executor_id: executor.executor_id.clone(),
                name: executor.name.clone(),
                variants: executor.variants.clone(),
                available: executor.available,
                active_session_count,
                allocatable: executor.available,
            }
        })
        .collect()
}

fn online_backend_config(online: &BackendRuntimeOnlineSnapshot) -> BackendConfig {
    BackendConfig {
        id: online.backend_id.clone(),
        name: online.name.clone(),
        endpoint: String::new(),
        auth_token: None,
        enabled: true,
        backend_type: BackendType::Remote,
        owner_user_id: None,
        profile_id: None,
        device_id: None,
        machine_id: None,
        machine_label: None,
        visibility: BackendVisibility::Private,
        share_scope_kind: BackendShareScopeKind::User,
        share_scope_id: None,
        capability_slot: "default".to_string(),
        device: serde_json::json!({}),
        last_claimed_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::{BackendExecutionLease, BackendExecutionSelectionMode};
    use uuid::Uuid;

    fn backend(id: &str, enabled: bool) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: format!("{id} backend"),
            endpoint: String::new(),
            auth_token: None,
            enabled,
            backend_type: BackendType::Local,
            owner_user_id: None,
            profile_id: None,
            device_id: None,
            machine_id: None,
            machine_label: None,
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: None,
            capability_slot: "default".to_string(),
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }

    fn online(
        backend_id: &str,
        executor_id: &str,
        available: bool,
    ) -> BackendRuntimeOnlineSnapshot {
        BackendRuntimeOnlineSnapshot {
            backend_id: backend_id.to_string(),
            name: format!("{backend_id} online"),
            capabilities: agentdash_relay::CapabilitiesPayload {
                executors: vec![agentdash_relay::AgentInfoRelay {
                    id: executor_id.to_string(),
                    name: executor_id.to_string(),
                    variants: vec!["default".to_string()],
                    available,
                }],
                supports_cancel: true,
                supports_discover_options: true,
                ..Default::default()
            },
            executors: vec![BackendRuntimeExecutorSnapshot {
                executor_id: executor_id.to_string(),
                name: executor_id.to_string(),
                variants: vec!["default".to_string()],
                available,
            }],
        }
    }

    fn lease(backend_id: &str, executor_id: &str) -> BackendExecutionLease {
        BackendExecutionLease::claimed(
            backend_id.to_string(),
            format!("session-{}", Uuid::new_v4()),
            "turn-1".to_string(),
            executor_id.to_string(),
            BackendExecutionSelectionMode::AutoIdle,
            None,
        )
    }

    #[test]
    fn runtime_summary_uses_executor_availability_and_active_leases() {
        let summaries = project_backend_runtime_summaries(
            vec![backend("backend-a", true)],
            vec![online("backend-a", "CODEX", true)],
            HashMap::new(),
            HashMap::from([("backend-a".to_string(), vec![lease("backend-a", "codex")])]),
        );

        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].online);
        assert!(summaries[0].allocatable);
        assert_eq!(summaries[0].active_session_count, 1);
        assert_eq!(summaries[0].executors[0].active_session_count, 1);
        assert!(summaries[0].executors[0].allocatable);
        assert_eq!(
            summaries[0]
                .capabilities
                .as_ref()
                .map(|capabilities| capabilities.supports_cancel),
            Some(true)
        );
    }

    #[test]
    fn runtime_summary_requires_enabled_online_available_executor() {
        let disabled = project_backend_runtime_summaries(
            vec![backend("backend-a", false)],
            vec![online("backend-a", "CODEX", true)],
            HashMap::new(),
            HashMap::new(),
        );
        assert!(!disabled[0].allocatable);

        let unavailable_executor = project_backend_runtime_summaries(
            vec![backend("backend-a", true)],
            vec![online("backend-a", "CODEX", false)],
            HashMap::new(),
            HashMap::new(),
        );
        assert!(!unavailable_executor[0].allocatable);
        assert!(!unavailable_executor[0].executors[0].allocatable);
    }
}
