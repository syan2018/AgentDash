use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::workspace::{
    WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
};

use crate::backend_transport::{BackendTransport, P4WorkspaceInfo, TransportError, WorkspaceProbeInfo};

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

    let probe = transport.detect_workspace(backend_id, root_ref).await?;
    let mut warnings = probe.warnings.clone();
    let (identity_kind, identity_payload, confidence) = select_identity(root_ref, &probe, &mut warnings);

    let mut binding = WorkspaceBinding::new(
        Uuid::nil(),
        backend_id.to_string(),
        root_ref.to_string(),
        build_detected_facts(&probe),
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

fn select_identity(
    root_ref: &str,
    probe: &WorkspaceProbeInfo,
    warnings: &mut Vec<String>,
) -> (WorkspaceIdentityKind, Value, String) {
    if let Some(git) = &probe.git {
        if probe.p4.is_some() {
            warnings.push("同一路径同时探测到 Git 与 P4 信息，当前默认按 git_repo 处理。".to_string());
        }
        return (
            WorkspaceIdentityKind::GitRepo,
            build_git_identity_payload(root_ref, git),
            "high".to_string(),
        );
    }

    if let Some(p4) = &probe.p4 {
        return (
            WorkspaceIdentityKind::P4Workspace,
            build_p4_identity_payload(root_ref, p4),
            "high".to_string(),
        );
    }

    (
        WorkspaceIdentityKind::LocalDir,
        json!({
            "path_key": normalize_path_key(root_ref),
            "root_hint": root_ref,
        }),
        "medium".to_string(),
    )
}

fn build_git_identity_payload(root_ref: &str, git: &crate::backend_transport::GitRepoInfo) -> Value {
    let repo_root = git.repo_root.as_deref().unwrap_or(root_ref);
    let repo_key = git
        .source_repo
        .as_deref()
        .and_then(normalize_git_remote)
        .unwrap_or_else(|| format!("git-local:{}", normalize_path_key(repo_root)));

    json!({
        "repo_key": repo_key,
        "remote_url": git.source_repo,
        "repo_root": git.repo_root,
        "default_branch": git.default_branch,
        "current_branch": git.branch,
        "root_hint": root_ref,
    })
}

fn build_p4_identity_payload(root_ref: &str, p4: &P4WorkspaceInfo) -> Value {
    let workspace_root = p4.workspace_root.as_deref().unwrap_or(root_ref);
    let workspace_key = p4
        .server_address
        .as_deref()
        .zip(p4.client_name.as_deref())
        .map(|(server, client)| format!("p4:{}:{}", server.trim(), client.trim()))
        .or_else(|| {
            p4.server_address
                .as_deref()
                .zip(p4.stream.as_deref())
                .map(|(server, stream)| format!("p4-stream:{}:{}", server.trim(), stream.trim()))
        })
        .unwrap_or_else(|| format!("p4-local:{}", normalize_path_key(workspace_root)));

    json!({
        "workspace_key": workspace_key,
        "server_address": p4.server_address,
        "client_name": p4.client_name,
        "user_name": p4.user_name,
        "stream": p4.stream,
        "workspace_root": p4.workspace_root,
        "root_hint": root_ref,
    })
}

fn build_detected_facts(probe: &WorkspaceProbeInfo) -> Value {
    json!({
        "git": probe.git.as_ref().map(|git| json!({
            "is_repo": git.is_git_repo,
            "repo_root": git.repo_root,
            "source_repo": git.source_repo,
            "default_branch": git.default_branch,
            "branch": git.branch,
            "commit_hash": git.commit_hash,
        })),
        "p4": probe.p4.as_ref().map(|p4| json!({
            "is_workspace": p4.is_p4_workspace,
            "workspace_root": p4.workspace_root,
            "client_name": p4.client_name,
            "server_address": p4.server_address,
            "user_name": p4.user_name,
            "stream": p4.stream,
        })),
        "warnings": probe.warnings,
    })
}

fn normalize_git_remote(remote: &str) -> Option<String> {
    let trimmed = remote.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("git@") {
        let mut parts = rest.splitn(2, ':');
        let host = parts.next()?.trim().to_ascii_lowercase();
        let path = normalize_repo_path(parts.next()?);
        return Some(format!("{host}/{path}"));
    }

    if let Some((_, rest)) = trimmed.split_once("://") {
        let without_auth = rest.rsplit_once('@').map(|(_, tail)| tail).unwrap_or(rest);
        let mut parts = without_auth.splitn(2, '/');
        let host = parts.next()?.trim().to_ascii_lowercase();
        let path = normalize_repo_path(parts.next().unwrap_or_default());
        if path.is_empty() {
            return Some(host);
        }
        return Some(format!("{host}/{path}"));
    }

    Some(normalize_repo_path(trimmed))
}

fn normalize_repo_path(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .trim_end_matches(".git")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn normalize_path_key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        build_git_identity_payload, build_p4_identity_payload, normalize_git_remote, select_identity,
    };
    use crate::backend_transport::{GitRepoInfo, P4WorkspaceInfo, WorkspaceProbeInfo};
    use agentdash_domain::workspace::WorkspaceIdentityKind;

    #[test]
    fn git_identity_prefers_normalized_remote_key() {
        let payload = build_git_identity_payload(
            "C:/repo",
            &GitRepoInfo {
                is_git_repo: true,
                repo_root: Some("C:/repo".to_string()),
                source_repo: Some("git@GitHub.com:OpenAI/AgentDashboard.git".to_string()),
                default_branch: Some("main".to_string()),
                branch: Some("feature/demo".to_string()),
                commit_hash: Some("abc".to_string()),
            },
        );

        assert_eq!(
            payload.get("repo_key").and_then(|v| v.as_str()),
            Some("github.com/openai/agentdashboard")
        );
    }

    #[test]
    fn p4_identity_prefers_server_and_client_key() {
        let payload = build_p4_identity_payload(
            "C:/ws/demo",
            &P4WorkspaceInfo {
                is_p4_workspace: true,
                workspace_root: Some("C:/ws/demo".to_string()),
                client_name: Some("demo-client".to_string()),
                server_address: Some("perforce:1666".to_string()),
                user_name: Some("alice".to_string()),
                stream: Some("//Streams/Main".to_string()),
            },
        );

        assert_eq!(
            payload.get("workspace_key").and_then(|v| v.as_str()),
            Some("p4:perforce:1666:demo-client")
        );
    }

    #[test]
    fn selection_prefers_git_when_both_git_and_p4_exist() {
        let mut warnings = Vec::new();
        let (kind, _, _) = select_identity(
            "C:/repo",
            &WorkspaceProbeInfo {
                git: Some(GitRepoInfo {
                    is_git_repo: true,
                    repo_root: Some("C:/repo".to_string()),
                    source_repo: Some("https://github.com/openai/agentdash.git".to_string()),
                    default_branch: Some("main".to_string()),
                    branch: Some("main".to_string()),
                    commit_hash: None,
                }),
                p4: Some(P4WorkspaceInfo {
                    is_p4_workspace: true,
                    workspace_root: Some("C:/repo".to_string()),
                    client_name: Some("demo".to_string()),
                    server_address: Some("perforce:1666".to_string()),
                    user_name: None,
                    stream: None,
                }),
                warnings: Vec::new(),
            },
            &mut warnings,
        );

        assert_eq!(kind, WorkspaceIdentityKind::GitRepo);
        assert!(!warnings.is_empty());
    }

    #[test]
    fn normalize_git_remote_supports_scp_like_urls() {
        assert_eq!(
            normalize_git_remote("git@github.com:OpenAI/AgentDashboard.git"),
            Some("github.com/openai/agentdashboard".to_string())
        );
    }
}
