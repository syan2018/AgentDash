use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::WorkspaceIdentityKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitWorkspaceMatchMode {
    RepoOnly,
    RepoBranch,
    RepoCommit,
}

impl Default for GitWorkspaceMatchMode {
    fn default() -> Self {
        Self::RepoOnly
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum P4WorkspaceMatchMode {
    ServerStream,
    ServerClient,
    ServerStreamClient,
    PathKey,
}

impl Default for P4WorkspaceMatchMode {
    fn default() -> Self {
        Self::ServerStream
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalDirMatchMode {
    PathKey,
}

impl Default for LocalDirMatchMode {
    fn default() -> Self {
        Self::PathKey
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitWorkspaceIdentityHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hint: Option<String>,
}

impl GitWorkspaceIdentityHints {
    pub fn is_empty(&self) -> bool {
        self.remote_url.is_none()
            && self.repo_root.is_none()
            && self.default_branch.is_none()
            && self.current_branch.is_none()
            && self.root_hint.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4WorkspaceIdentityHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hint: Option<String>,
}

impl P4WorkspaceIdentityHints {
    pub fn is_empty(&self) -> bool {
        self.workspace_root.is_none() && self.user_name.is_none() && self.root_hint.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalDirIdentityHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hint: Option<String>,
}

impl LocalDirIdentityHints {
    pub fn is_empty(&self) -> bool {
        self.root_hint.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitWorkspaceIdentityContract {
    #[serde(default)]
    pub match_mode: GitWorkspaceMatchMode,
    pub repo_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binding_label_selectors: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "GitWorkspaceIdentityHints::is_empty")]
    pub hints: GitWorkspaceIdentityHints,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4WorkspaceIdentityContract {
    #[serde(default)]
    pub match_mode: P4WorkspaceMatchMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_key: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binding_label_selectors: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "P4WorkspaceIdentityHints::is_empty")]
    pub hints: P4WorkspaceIdentityHints,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalDirIdentityContract {
    #[serde(default)]
    pub match_mode: LocalDirMatchMode,
    pub path_key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binding_label_selectors: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "LocalDirIdentityHints::is_empty")]
    pub hints: LocalDirIdentityHints,
}

pub fn normalize_identity_payload(
    kind: WorkspaceIdentityKind,
    payload: &Value,
) -> Result<Value, String> {
    match kind {
        WorkspaceIdentityKind::GitRepo => serde_json::to_value(parse_git_contract(payload)?)
            .map_err(|error| format!("序列化 Git workspace identity 失败: {error}")),
        WorkspaceIdentityKind::P4Workspace => serde_json::to_value(parse_p4_contract(payload)?)
            .map_err(|error| format!("序列化 P4 workspace identity 失败: {error}")),
        WorkspaceIdentityKind::LocalDir => serde_json::to_value(parse_local_contract(payload)?)
            .map_err(|error| format!("序列化 Local workspace identity 失败: {error}")),
    }
}

pub fn identity_payload_matches(
    kind: WorkspaceIdentityKind,
    expected_payload: &Value,
    actual_payload: &Value,
    actual_binding_facts: Option<&Value>,
) -> bool {
    match kind {
        WorkspaceIdentityKind::GitRepo => {
            let Ok(expected) = parse_git_contract(expected_payload) else {
                return false;
            };
            let Ok(actual) = parse_git_contract(actual_payload) else {
                return false;
            };
            binding_labels_match(&expected.binding_label_selectors, actual_binding_facts)
                && expected.repo_key == actual.repo_key
                && match expected.match_mode {
                    GitWorkspaceMatchMode::RepoOnly => true,
                    GitWorkspaceMatchMode::RepoBranch => {
                        expected.branch.is_some() && expected.branch == actual.branch
                    }
                    GitWorkspaceMatchMode::RepoCommit => {
                        expected.commit_hash.is_some() && expected.commit_hash == actual.commit_hash
                    }
                }
        }
        WorkspaceIdentityKind::P4Workspace => {
            let Ok(expected) = parse_p4_contract(expected_payload) else {
                return false;
            };
            let Ok(actual) = parse_p4_contract(actual_payload) else {
                return false;
            };
            binding_labels_match(&expected.binding_label_selectors, actual_binding_facts)
                && match expected.match_mode {
                    P4WorkspaceMatchMode::ServerStream => {
                        expected.server_address.is_some()
                            && expected.server_address == actual.server_address
                            && expected.stream.is_some()
                            && expected.stream == actual.stream
                    }
                    P4WorkspaceMatchMode::ServerClient => {
                        expected.server_address.is_some()
                            && expected.server_address == actual.server_address
                            && expected.client_name.is_some()
                            && expected.client_name == actual.client_name
                    }
                    P4WorkspaceMatchMode::ServerStreamClient => {
                        expected.server_address.is_some()
                            && expected.server_address == actual.server_address
                            && expected.stream.is_some()
                            && expected.stream == actual.stream
                            && expected.client_name.is_some()
                            && expected.client_name == actual.client_name
                    }
                    P4WorkspaceMatchMode::PathKey => {
                        expected.path_key.is_some() && expected.path_key == actual.path_key
                    }
                }
        }
        WorkspaceIdentityKind::LocalDir => {
            let Ok(expected) = parse_local_contract(expected_payload) else {
                return false;
            };
            let Ok(actual) = parse_local_contract(actual_payload) else {
                return false;
            };
            binding_labels_match(&expected.binding_label_selectors, actual_binding_facts)
                && expected.path_key == actual.path_key
        }
    }
}

pub fn identity_payload_matches_detected_facts(
    kind: WorkspaceIdentityKind,
    expected_payload: &Value,
    detected_facts: &Value,
    root_ref: &str,
) -> bool {
    let Some(actual_payload) =
        identity_payload_from_detected_facts(kind.clone(), detected_facts, root_ref)
    else {
        return false;
    };

    identity_payload_matches(
        kind,
        expected_payload,
        &actual_payload,
        Some(detected_facts),
    )
}

pub fn identity_payload_supports_local_prepare(
    kind: WorkspaceIdentityKind,
    payload: &Value,
) -> bool {
    match kind {
        WorkspaceIdentityKind::GitRepo => parse_git_contract(payload)
            .map(|contract| {
                matches!(
                    contract.match_mode,
                    GitWorkspaceMatchMode::RepoBranch | GitWorkspaceMatchMode::RepoCommit
                )
            })
            .unwrap_or(false),
        WorkspaceIdentityKind::P4Workspace => parse_p4_contract(payload).is_ok(),
        WorkspaceIdentityKind::LocalDir => false,
    }
}

pub fn identity_payload_from_detected_facts(
    kind: WorkspaceIdentityKind,
    detected_facts: &Value,
    root_ref: &str,
) -> Option<Value> {
    match kind {
        WorkspaceIdentityKind::GitRepo => {
            let git = detected_facts.get("git")?;
            if git
                .get("is_repo")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                == false
            {
                return None;
            }

            let payload = serde_json::to_value(GitWorkspaceIdentityContract {
                match_mode: parse_git_match_mode(git).unwrap_or_else(|| {
                    if string_field(git, "branch").is_some() {
                        GitWorkspaceMatchMode::RepoBranch
                    } else {
                        GitWorkspaceMatchMode::RepoOnly
                    }
                }),
                repo_key: string_field(git, "source_repo")
                    .and_then(|value| normalize_git_remote(&value))
                    .or_else(|| {
                        string_field(git, "repo_root")
                            .map(|path| format!("git-local:{}", normalize_path_key(&path)))
                    })
                    .unwrap_or_else(|| format!("git-local:{}", normalize_path_key(root_ref))),
                branch: string_field(git, "branch"),
                commit_hash: string_field(git, "commit_hash"),
                binding_label_selectors: Default::default(),
                prepare_profile: None,
                hints: GitWorkspaceIdentityHints {
                    remote_url: string_field(git, "source_repo"),
                    repo_root: string_field(git, "repo_root"),
                    default_branch: string_field(git, "default_branch"),
                    current_branch: string_field(git, "branch"),
                    root_hint: Some(root_ref.to_string()),
                },
            })
            .ok()?;
            Some(payload)
        }
        WorkspaceIdentityKind::P4Workspace => {
            let p4 = detected_facts.get("p4")?;
            if p4
                .get("is_workspace")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                == false
            {
                return None;
            }

            let server_address =
                string_field(p4, "server_address").map(|value| value.trim().to_ascii_lowercase());
            let stream = string_field(p4, "stream");
            let client_name = string_field(p4, "client_name");
            let workspace_root = string_field(p4, "workspace_root");
            let payload = serde_json::to_value(P4WorkspaceIdentityContract {
                match_mode: parse_p4_match_mode(p4).unwrap_or_else(|| {
                    if server_address.is_some() && stream.is_some() && client_name.is_some() {
                        P4WorkspaceMatchMode::ServerStreamClient
                    } else if server_address.is_some() && stream.is_some() {
                        P4WorkspaceMatchMode::ServerStream
                    } else if server_address.is_some() && client_name.is_some() {
                        P4WorkspaceMatchMode::ServerClient
                    } else {
                        P4WorkspaceMatchMode::PathKey
                    }
                }),
                server_address,
                stream,
                client_name,
                path_key: workspace_root
                    .as_deref()
                    .map(normalize_path_key)
                    .or_else(|| Some(normalize_path_key(root_ref))),
                binding_label_selectors: Default::default(),
                prepare_profile: None,
                hints: P4WorkspaceIdentityHints {
                    workspace_root,
                    user_name: string_field(p4, "user_name"),
                    root_hint: Some(root_ref.to_string()),
                },
            })
            .ok()?;
            Some(payload)
        }
        WorkspaceIdentityKind::LocalDir => serde_json::to_value(LocalDirIdentityContract {
            match_mode: LocalDirMatchMode::PathKey,
            path_key: normalize_path_key(root_ref),
            binding_label_selectors: Default::default(),
            prepare_profile: None,
            hints: LocalDirIdentityHints {
                root_hint: Some(root_ref.to_string()),
            },
        })
        .ok(),
    }
}

pub fn normalize_git_remote(remote: &str) -> Option<String> {
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

pub fn normalize_path_key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn parse_git_contract(payload: &Value) -> Result<GitWorkspaceIdentityContract, String> {
    let repo_key = string_field(payload, "repo_key")
        .or_else(|| {
            string_field(payload, "remote_url").and_then(|value| normalize_git_remote(&value))
        })
        .or_else(|| {
            string_field(payload, "repo_root")
                .or_else(|| string_field(payload, "root_hint"))
                .map(|path| format!("git-local:{}", normalize_path_key(&path)))
        })
        .ok_or_else(|| "Git workspace identity 缺少可归一化 repo_key".to_string())?;
    let branch = string_field(payload, "branch")
        .or_else(|| string_field(payload, "current_branch"))
        .or_else(|| {
            payload
                .get("hints")
                .and_then(|v| string_field(v, "current_branch"))
        });
    let commit_hash = string_field(payload, "commit_hash");
    let match_mode = parse_git_match_mode(payload).unwrap_or_else(|| {
        if branch.is_some() {
            GitWorkspaceMatchMode::RepoBranch
        } else if commit_hash.is_some() {
            GitWorkspaceMatchMode::RepoCommit
        } else {
            GitWorkspaceMatchMode::RepoOnly
        }
    });
    let contract = GitWorkspaceIdentityContract {
        match_mode,
        repo_key,
        branch,
        commit_hash,
        binding_label_selectors: parse_string_map(payload, "binding_label_selectors"),
        prepare_profile: payload.get("prepare_profile").cloned(),
        hints: GitWorkspaceIdentityHints {
            remote_url: string_field(payload, "remote_url").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "remote_url"))
            }),
            repo_root: string_field(payload, "repo_root").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "repo_root"))
            }),
            default_branch: string_field(payload, "default_branch").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "default_branch"))
            }),
            current_branch: string_field(payload, "current_branch").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "current_branch"))
            }),
            root_hint: string_field(payload, "root_hint").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "root_hint"))
            }),
        },
    };
    validate_git_contract(&contract)?;
    Ok(contract)
}

fn parse_p4_contract(payload: &Value) -> Result<P4WorkspaceIdentityContract, String> {
    let server_address = string_field(payload, "server_address");
    let stream = string_field(payload, "stream");
    let client_name = string_field(payload, "client_name");
    let path_key = string_field(payload, "path_key").or_else(|| {
        string_field(payload, "workspace_root")
            .or_else(|| string_field(payload, "root_hint"))
            .map(|path| normalize_path_key(&path))
    });
    let match_mode = parse_p4_match_mode(payload).unwrap_or_else(|| {
        if server_address.is_some() && stream.is_some() {
            P4WorkspaceMatchMode::ServerStream
        } else if server_address.is_some() && client_name.is_some() {
            P4WorkspaceMatchMode::ServerClient
        } else {
            P4WorkspaceMatchMode::PathKey
        }
    });
    let contract = P4WorkspaceIdentityContract {
        match_mode,
        server_address: server_address.map(|value| value.trim().to_ascii_lowercase()),
        stream,
        client_name,
        path_key,
        binding_label_selectors: parse_string_map(payload, "binding_label_selectors"),
        prepare_profile: payload.get("prepare_profile").cloned(),
        hints: P4WorkspaceIdentityHints {
            workspace_root: string_field(payload, "workspace_root").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "workspace_root"))
            }),
            user_name: string_field(payload, "user_name").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "user_name"))
            }),
            root_hint: string_field(payload, "root_hint").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "root_hint"))
            }),
        },
    };
    validate_p4_contract(&contract)?;
    Ok(contract)
}

fn parse_local_contract(payload: &Value) -> Result<LocalDirIdentityContract, String> {
    let path_key = string_field(payload, "path_key")
        .or_else(|| string_field(payload, "root_hint").map(|path| normalize_path_key(&path)))
        .ok_or_else(|| "Local workspace identity 缺少 path_key/root_hint".to_string())?;
    let contract = LocalDirIdentityContract {
        match_mode: LocalDirMatchMode::PathKey,
        path_key,
        binding_label_selectors: parse_string_map(payload, "binding_label_selectors"),
        prepare_profile: payload.get("prepare_profile").cloned(),
        hints: LocalDirIdentityHints {
            root_hint: string_field(payload, "root_hint").or_else(|| {
                payload
                    .get("hints")
                    .and_then(|v| string_field(v, "root_hint"))
            }),
        },
    };
    Ok(contract)
}

fn validate_git_contract(contract: &GitWorkspaceIdentityContract) -> Result<(), String> {
    match contract.match_mode {
        GitWorkspaceMatchMode::RepoOnly => Ok(()),
        GitWorkspaceMatchMode::RepoBranch => {
            if contract.branch.is_some() {
                Ok(())
            } else {
                Err("Git workspace identity 采用 repo_branch，但缺少 branch".to_string())
            }
        }
        GitWorkspaceMatchMode::RepoCommit => {
            if contract.commit_hash.is_some() {
                Ok(())
            } else {
                Err("Git workspace identity 采用 repo_commit，但缺少 commit_hash".to_string())
            }
        }
    }
}

fn validate_p4_contract(contract: &P4WorkspaceIdentityContract) -> Result<(), String> {
    match contract.match_mode {
        P4WorkspaceMatchMode::ServerStream => {
            if contract.server_address.is_some() && contract.stream.is_some() {
                Ok(())
            } else {
                Err(
                    "P4 workspace identity 采用 server_stream，但缺少 server_address 或 stream"
                        .to_string(),
                )
            }
        }
        P4WorkspaceMatchMode::ServerClient => {
            if contract.server_address.is_some() && contract.client_name.is_some() {
                Ok(())
            } else {
                Err(
                    "P4 workspace identity 采用 server_client，但缺少 server_address 或 client_name"
                        .to_string(),
                )
            }
        }
        P4WorkspaceMatchMode::ServerStreamClient => {
            if contract.server_address.is_some()
                && contract.stream.is_some()
                && contract.client_name.is_some()
            {
                Ok(())
            } else {
                Err("P4 workspace identity 采用 server_stream_client，但缺少必需字段".to_string())
            }
        }
        P4WorkspaceMatchMode::PathKey => {
            if contract.path_key.is_some() {
                Ok(())
            } else {
                Err("P4 workspace identity 采用 path_key，但缺少 path_key".to_string())
            }
        }
    }
}

fn parse_git_match_mode(payload: &Value) -> Option<GitWorkspaceMatchMode> {
    match payload.get("match_mode").and_then(Value::as_str)? {
        "repo_only" => Some(GitWorkspaceMatchMode::RepoOnly),
        "repo_branch" => Some(GitWorkspaceMatchMode::RepoBranch),
        "repo_commit" => Some(GitWorkspaceMatchMode::RepoCommit),
        _ => None,
    }
}

fn parse_p4_match_mode(payload: &Value) -> Option<P4WorkspaceMatchMode> {
    match payload.get("match_mode").and_then(Value::as_str)? {
        "server_stream" => Some(P4WorkspaceMatchMode::ServerStream),
        "server_client" => Some(P4WorkspaceMatchMode::ServerClient),
        "server_stream_client" => Some(P4WorkspaceMatchMode::ServerStreamClient),
        "path_key" => Some(P4WorkspaceMatchMode::PathKey),
        _ => None,
    }
}

fn binding_labels_match(
    selectors: &BTreeMap<String, String>,
    binding_facts: Option<&Value>,
) -> bool {
    if selectors.is_empty() {
        return true;
    }

    let Some(facts) = binding_facts else {
        return false;
    };
    let Some(labels) = facts.get("binding_labels").and_then(Value::as_object) else {
        return false;
    };

    selectors
        .iter()
        .all(|(key, expected)| labels.get(key).and_then(Value::as_str) == Some(expected.as_str()))
}

fn string_field(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_string_map(payload: &Value, key: &str) -> BTreeMap<String, String> {
    payload
        .get(key)
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(label, value)| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(|value| (label.trim().to_string(), value.to_string()))
                })
                .filter(|(label, _)| !label.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_repo_path(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .trim_end_matches(".git")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_git_payload_promotes_old_shape_to_contract() {
        let payload = normalize_identity_payload(
            WorkspaceIdentityKind::GitRepo,
            &serde_json::json!({
                "remote_url": "git@GitHub.com:OpenAI/AgentDashboard.git",
                "current_branch": "main",
                "root_hint": "C:/repo"
            }),
        )
        .expect("payload should normalize");

        assert_eq!(payload["match_mode"], "repo_branch");
        assert_eq!(payload["repo_key"], "github.com/openai/agentdashboard");
        assert_eq!(payload["branch"], "main");
    }

    #[test]
    fn git_identity_matcher_uses_expected_branch_policy() {
        let expected = serde_json::json!({
            "match_mode": "repo_branch",
            "repo_key": "github.com/openai/agentdashboard",
            "branch": "main"
        });
        let actual = serde_json::json!({
            "match_mode": "repo_only",
            "repo_key": "github.com/openai/agentdashboard",
            "branch": "main"
        });

        assert!(identity_payload_matches(
            WorkspaceIdentityKind::GitRepo,
            &expected,
            &actual,
            None
        ));
    }

    #[test]
    fn p4_identity_defaults_to_server_stream_when_available() {
        let payload = normalize_identity_payload(
            WorkspaceIdentityKind::P4Workspace,
            &serde_json::json!({
                "server_address": "P4.EXAMPLE.COM:1666",
                "stream": "//ExampleProject/main",
                "client_name": "example-client"
            }),
        )
        .expect("payload should normalize");

        assert_eq!(payload["match_mode"], "server_stream");
        assert_eq!(payload["server_address"], "p4.example.com:1666");
        assert_eq!(payload["stream"], "//ExampleProject/main");
    }

    #[test]
    fn label_selectors_must_match_detected_binding_facts() {
        let expected = serde_json::json!({
            "match_mode": "path_key",
            "path_key": "d:/repo",
            "binding_label_selectors": {
                "owner": "example-user"
            }
        });
        let actual = serde_json::json!({
            "match_mode": "path_key",
            "path_key": "d:/repo"
        });

        assert!(identity_payload_matches(
            WorkspaceIdentityKind::LocalDir,
            &expected,
            &actual,
            Some(&serde_json::json!({
                "binding_labels": {
                    "owner": "example-user"
                }
            }))
        ));
        assert!(!identity_payload_matches(
            WorkspaceIdentityKind::LocalDir,
            &expected,
            &actual,
            Some(&serde_json::json!({
                "binding_labels": {
                    "owner": "someone-else"
                }
            }))
        ));
    }

    #[test]
    fn detected_p4_facts_can_be_rebuilt_into_identity_payload() {
        let payload = identity_payload_from_detected_facts(
            WorkspaceIdentityKind::P4Workspace,
            &serde_json::json!({
                "p4": {
                    "is_workspace": true,
                    "workspace_root": "D:/ExampleWorkspace",
                    "client_name": "example-client",
                    "server_address": "p4.example.com:1666",
                    "user_name": "example-user",
                    "stream": "//ExampleProject/main"
                }
            }),
            "D:/ExampleWorkspace",
        )
        .expect("payload should be derived from detected facts");

        assert_eq!(payload["match_mode"], "server_stream_client");
        assert_eq!(payload["stream"], "//ExampleProject/main");
        assert_eq!(payload["server_address"], "p4.example.com:1666");
    }
}
