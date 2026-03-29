use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::workspace::{
    WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
};

use crate::backend_transport::{BackendTransport, TransportError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceDetectionResult {
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub binding: WorkspaceBinding,
    pub confidence: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceDetectionError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    BackendOffline(String),
    #[error("{0}")]
    TransportFailed(String),
}

impl From<TransportError> for WorkspaceDetectionError {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::BackendOffline(msg) => Self::BackendOffline(msg),
            other => Self::TransportFailed(other.to_string()),
        }
    }
}

/// 通过 BackendTransport 探测远程目录，推断 workspace 类型和 binding。
pub async fn detect_workspace_from_backend(
    transport: &dyn BackendTransport,
    backend_id: &str,
    root_ref: &str,
) -> Result<WorkspaceDetectionResult, WorkspaceDetectionError> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return Err(WorkspaceDetectionError::BadRequest(
            "backend_id 不能为空".into(),
        ));
    }
    let root_ref = root_ref.trim();
    if root_ref.is_empty() {
        return Err(WorkspaceDetectionError::BadRequest(
            "root_ref 不能为空".into(),
        ));
    }
    if !transport.is_online(backend_id).await {
        return Err(WorkspaceDetectionError::BackendOffline(format!(
            "目标 Backend 当前不在线: {backend_id}"
        )));
    }

    let git = transport.detect_git_repo(backend_id, root_ref).await?;
    let mut warnings = Vec::new();
    let (identity_kind, identity_payload, confidence) = if git.is_git_repo {
        (
            WorkspaceIdentityKind::GitRepo,
            json!({
                "remote_url": git.source_repo.clone(),
                "branch": git.branch.clone(),
                "root_hint": root_ref,
            }),
            "high".to_string(),
        )
    } else {
        warnings
            .push("当前未识别为 Git 仓库，已按 local_dir 处理。P4 自动识别尚未接入。".to_string());
        (
            WorkspaceIdentityKind::LocalDir,
            json!({ "root_hint": root_ref }),
            "medium".to_string(),
        )
    };

    let mut binding = WorkspaceBinding::new(
        Uuid::nil(),
        backend_id.to_string(),
        root_ref.to_string(),
        json!({
            "git": {
                "is_repo": git.is_git_repo,
                "source_repo": git.source_repo,
                "branch": git.branch,
                "commit_hash": git.commit_hash,
            },
        }),
    );
    binding.status = WorkspaceBindingStatus::Ready;
    binding.last_verified_at = Some(Utc::now());

    Ok(WorkspaceDetectionResult {
        identity_kind,
        identity_payload,
        binding,
        confidence,
        warnings,
    })
}
