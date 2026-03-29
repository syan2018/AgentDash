use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::workspace::{Workspace, WorkspaceBinding, WorkspaceResolutionPolicy};

use crate::backend_transport::BackendTransport;

/// 后端在线探测能力 — workspace resolution 的最小依赖。
///
/// 所有 `BackendTransport` 实现自动满足此 trait（blanket impl）。
#[async_trait]
pub trait BackendAvailability: Send + Sync {
    async fn is_online(&self, backend_id: &str) -> bool;
}

#[async_trait]
impl<T: BackendTransport + ?Sized> BackendAvailability for T {
    async fn is_online(&self, backend_id: &str) -> bool {
        BackendTransport::is_online(self, backend_id).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolvedWorkspaceBinding {
    pub workspace_id: Uuid,
    pub binding_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub resolution_reason: String,
    pub warnings: Vec<String>,
    pub detected_facts: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceResolutionError {
    #[error("{0}")]
    NoBindings(String),
    #[error("{0}")]
    NoAvailable(String),
}

pub async fn resolve_workspace_binding(
    availability: &dyn BackendAvailability,
    workspace: &Workspace,
) -> Result<ResolvedWorkspaceBinding, WorkspaceResolutionError> {
    if workspace.bindings.is_empty() {
        return Err(WorkspaceResolutionError::NoBindings(format!(
            "Workspace `{}` 当前还没有任何可解析 binding",
            workspace.name
        )));
    }

    let mut warnings = Vec::new();
    let mut online_candidates = Vec::new();
    for binding in &workspace.bindings {
        let backend_id = binding.backend_id.trim();
        if backend_id.is_empty() {
            warnings.push(format!("binding `{}` 缺少 backend_id", binding.id));
            continue;
        }
        let is_online = availability.is_online(backend_id).await;
        if !is_online {
            warnings.push(format!("backend `{backend_id}` 当前离线"));
        }
        online_candidates.push((binding, is_online));
    }

    let selected = match workspace.resolution_policy {
        WorkspaceResolutionPolicy::PreferDefaultBinding => {
            select_default_binding(workspace, &online_candidates)
                .or_else(|| select_first_online(&online_candidates))
                .or_else(|| online_candidates.first().map(|(binding, _)| *binding))
        }
        WorkspaceResolutionPolicy::PreferOnline => select_first_online(&online_candidates)
            .or_else(|| select_default_binding(workspace, &online_candidates))
            .or_else(|| online_candidates.first().map(|(binding, _)| *binding)),
    };

    let Some(binding) = selected else {
        return Err(WorkspaceResolutionError::NoAvailable(format!(
            "Workspace `{}` 没有可用 binding",
            workspace.name
        )));
    };

    Ok(ResolvedWorkspaceBinding {
        workspace_id: workspace.id,
        binding_id: binding.id,
        backend_id: binding.backend_id.trim().to_string(),
        root_ref: binding.root_ref.trim().to_string(),
        resolution_reason: build_resolution_reason(workspace, binding),
        warnings,
        detected_facts: binding.detected_facts.clone(),
    })
}

fn select_default_binding<'a>(
    workspace: &Workspace,
    bindings: &'a [(&'a WorkspaceBinding, bool)],
) -> Option<&'a WorkspaceBinding> {
    let default_binding_id = workspace.default_binding_id?;
    bindings
        .iter()
        .find(|(binding, _)| binding.id == default_binding_id)
        .map(|(binding, _)| *binding)
}

fn select_first_online<'a>(
    bindings: &'a [(&'a WorkspaceBinding, bool)],
) -> Option<&'a WorkspaceBinding> {
    bindings
        .iter()
        .filter(|(_, online)| *online)
        .map(|(binding, _)| *binding)
        .max_by_key(|binding| binding.priority)
}

fn build_resolution_reason(workspace: &Workspace, binding: &WorkspaceBinding) -> String {
    if workspace.default_binding_id == Some(binding.id) {
        return "命中默认 binding".to_string();
    }
    match workspace.resolution_policy {
        WorkspaceResolutionPolicy::PreferDefaultBinding => {
            "默认 binding 不可用，回退到候选 binding".to_string()
        }
        WorkspaceResolutionPolicy::PreferOnline => "根据在线 backend 选择候选 binding".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workspace::{
        WorkspaceBindingStatus, WorkspaceIdentityKind, WorkspaceStatus,
    };

    struct MockAvailability {
        online_backends: Vec<String>,
    }

    #[async_trait]
    impl BackendAvailability for MockAvailability {
        async fn is_online(&self, backend_id: &str) -> bool {
            self.online_backends.iter().any(|id| id == backend_id)
        }
    }

    fn workspace_with_bindings(bindings: Vec<WorkspaceBinding>) -> Workspace {
        let mut ws = Workspace::new(
            Uuid::new_v4(),
            "test-ws".to_string(),
            WorkspaceIdentityKind::LocalDir,
            serde_json::json!({}),
            WorkspaceResolutionPolicy::PreferOnline,
        );
        ws.status = WorkspaceStatus::Ready;
        ws.set_bindings(bindings);
        ws.refresh_default_binding();
        ws
    }

    fn make_binding(backend_id: &str) -> WorkspaceBinding {
        let mut b = WorkspaceBinding::new(
            Uuid::new_v4(),
            backend_id.to_string(),
            "/workspace".to_string(),
            serde_json::json!({}),
        );
        b.status = WorkspaceBindingStatus::Ready;
        b
    }

    #[tokio::test]
    async fn resolves_online_binding() {
        let avail = MockAvailability {
            online_backends: vec!["backend-a".to_string()],
        };
        let ws = workspace_with_bindings(vec![make_binding("backend-a")]);
        let result = resolve_workspace_binding(&avail, &ws)
            .await
            .expect("should resolve");
        assert_eq!(result.backend_id, "backend-a");
        assert!(result.warnings.is_empty());
    }

    #[tokio::test]
    async fn falls_back_to_offline_binding() {
        let avail = MockAvailability {
            online_backends: vec![],
        };
        let ws = workspace_with_bindings(vec![make_binding("backend-a")]);
        let result = resolve_workspace_binding(&avail, &ws)
            .await
            .expect("should still resolve with offline binding");
        assert_eq!(result.backend_id, "backend-a");
        assert!(!result.warnings.is_empty());
    }

    #[tokio::test]
    async fn rejects_empty_bindings() {
        let avail = MockAvailability {
            online_backends: vec![],
        };
        let ws = workspace_with_bindings(vec![]);
        let err = resolve_workspace_binding(&avail, &ws)
            .await
            .expect_err("should fail");
        assert!(matches!(err, WorkspaceResolutionError::NoBindings(_)));
    }
}
