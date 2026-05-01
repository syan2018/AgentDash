use std::path::Path;
use std::process::Command;

use agentdash_domain::workspace::{
    GitWorkspaceIdentityContract, GitWorkspaceMatchMode, P4WorkspaceIdentityContract,
    P4WorkspaceMatchMode, WorkspaceIdentityKind, normalize_identity_payload,
};

use crate::local_backend_config::WorkspaceContractRuntimeConfig;

pub fn prepare_workspace(
    path: &Path,
    identity_kind: Option<WorkspaceIdentityKind>,
    identity_payload: Option<&serde_json::Value>,
    runtime_config: &WorkspaceContractRuntimeConfig,
) -> Result<(), String> {
    if !runtime_config.enabled
        || !runtime_config.prepare_on_first_prompt
    {
        return Ok(());
    }

    let (Some(identity_kind), Some(identity_payload)) = (identity_kind, identity_payload) else {
        return Ok(());
    };

    let normalized = normalize_identity_payload(identity_kind.clone(), identity_payload)
        .map_err(|error| format!("workspace identity 归一化失败: {error}"))?;

    match identity_kind {
        WorkspaceIdentityKind::GitRepo => {
            if !runtime_config.git.enabled {
                return Ok(());
            }
            let contract = serde_json::from_value::<GitWorkspaceIdentityContract>(normalized)
                .map_err(|error| format!("解析 Git workspace contract 失败: {error}"))?;
            prepare_git_workspace(path, &contract, runtime_config)
        }
        WorkspaceIdentityKind::P4Workspace => {
            if !runtime_config.p4.enabled {
                return Ok(());
            }
            let contract = serde_json::from_value::<P4WorkspaceIdentityContract>(normalized)
                .map_err(|error| format!("解析 P4 workspace contract 失败: {error}"))?;
            prepare_p4_workspace(path, &contract, runtime_config)
        }
        WorkspaceIdentityKind::LocalDir => Ok(()),
    }
}

fn prepare_git_workspace(
    path: &Path,
    contract: &GitWorkspaceIdentityContract,
    runtime_config: &WorkspaceContractRuntimeConfig,
) -> Result<(), String> {
    let branch = contract
        .prepare_profile
        .as_ref()
        .and_then(|profile| profile.get("branch"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| contract.branch.clone())
        .or_else(|| contract.hints.current_branch.clone())
        .or_else(|| contract.hints.default_branch.clone());

    let remote = contract
        .prepare_profile
        .as_ref()
        .and_then(|profile| profile.get("remote"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| runtime_config.git.default_remote.as_deref())
        .unwrap_or("origin");

    match contract.match_mode {
        GitWorkspaceMatchMode::RepoOnly => Ok(()),
        GitWorkspaceMatchMode::RepoBranch => {
            if !runtime_config.git.allow_branch_sync {
                return Ok(());
            }
            let Some(branch) = branch else {
                return Err("Git prepare 需要 branch，但 contract 中未提供".to_string());
            };
            run_git(path, &["fetch", remote, "--prune"])?;
            run_git(path, &["checkout", &branch])?;
            if git_ref_exists(path, &format!("refs/remotes/{remote}/{branch}"))? {
                run_git(path, &["reset", "--hard", &format!("{remote}/{branch}")])?;
            }
            Ok(())
        }
        GitWorkspaceMatchMode::RepoCommit => {
            if !runtime_config.git.allow_commit_reset {
                return Ok(());
            }
            let Some(commit_hash) = contract.commit_hash.as_deref() else {
                return Err("Git prepare 需要 commit_hash，但 contract 中未提供".to_string());
            };
            run_git(path, &["fetch", "--all", "--prune"])?;
            run_git(path, &["reset", "--hard", commit_hash])?;
            Ok(())
        }
    }
}

fn prepare_p4_workspace(
    path: &Path,
    contract: &P4WorkspaceIdentityContract,
    runtime_config: &WorkspaceContractRuntimeConfig,
) -> Result<(), String> {
    let info_fields = run_p4_tagged(path, &["info"])?;
    let server_address = pick_tagged(
        &info_fields,
        &["serverAddress", "serveraddress", "Server address", "ServerAddress"],
    )
    .map(|value| value.trim().to_ascii_lowercase());
    let client_name = pick_tagged(&info_fields, &["clientName", "clientname", "Client", "ClientName"]);
    let actual_stream = client_name.as_deref().and_then(|client| {
        run_p4_tagged(path, &["client", "-o", client])
            .ok()
            .and_then(|fields| pick_tagged(&fields, &["Stream", "stream"]))
    });

    match contract.match_mode {
        P4WorkspaceMatchMode::ServerStream | P4WorkspaceMatchMode::ServerStreamClient => {
            if let Some(expected_server) = contract.server_address.as_deref()
                && server_address.as_deref() != Some(expected_server)
            {
                return Err(format!(
                    "P4 server 不匹配，期望 `{expected_server}`，实际 `{:?}`",
                    server_address
                ));
            }
            if let Some(expected_stream) = contract.stream.as_deref()
                && actual_stream.as_deref() != Some(expected_stream)
            {
                return Err(format!(
                    "P4 stream 不匹配，期望 `{expected_stream}`，实际 `{:?}`",
                    actual_stream
                ));
            }
            if matches!(contract.match_mode, P4WorkspaceMatchMode::ServerStreamClient)
                && let Some(expected_client) = contract.client_name.as_deref()
                && client_name.as_deref() != Some(expected_client)
            {
                return Err(format!(
                    "P4 client 不匹配，期望 `{expected_client}`，实际 `{:?}`",
                    client_name
                ));
            }
        }
        P4WorkspaceMatchMode::ServerClient => {
            if let Some(expected_server) = contract.server_address.as_deref()
                && server_address.as_deref() != Some(expected_server)
            {
                return Err(format!(
                    "P4 server 不匹配，期望 `{expected_server}`，实际 `{:?}`",
                    server_address
                ));
            }
            if let Some(expected_client) = contract.client_name.as_deref()
                && client_name.as_deref() != Some(expected_client)
            {
                return Err(format!(
                    "P4 client 不匹配，期望 `{expected_client}`，实际 `{:?}`",
                    client_name
                ));
            }
        }
        P4WorkspaceMatchMode::PathKey => {}
    }

    let mut args = vec!["sync"];
    let force = contract
        .prepare_profile
        .as_ref()
        .and_then(|profile| profile.get("force"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(runtime_config.p4.force_sync);
    if force {
        args.push("-f");
    }
    run_p4(path, &args)
}

fn run_git(path: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .map_err(|error| format!("启动 git 失败: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(render_process_error("git", args, &output.stdout, &output.stderr, output.status))
}

fn git_ref_exists(path: &Path, reference: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--verify", "--quiet", reference])
        .output()
        .map_err(|error| format!("检查 Git ref 失败: {error}"))?;
    Ok(output.status.success())
}

fn run_p4(path: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("p4")
        .current_dir(path)
        .args(args)
        .output()
        .map_err(|error| format!("启动 p4 失败: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(render_process_error("p4", args, &output.stdout, &output.stderr, output.status))
}

fn run_p4_tagged(path: &Path, args: &[&str]) -> Result<std::collections::HashMap<String, String>, String> {
    let output = Command::new("p4")
        .current_dir(path)
        .arg("-ztag")
        .args(args)
        .output()
        .map_err(|error| format!("启动 p4 失败: {error}"))?;

    if !output.status.success() {
        return Err(render_process_error(
            "p4",
            args,
            &output.stdout,
            &output.stderr,
            output.status,
        ));
    }

    let mut fields = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some(rest) = line.strip_prefix("... ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(key) = parts.next() else {
            continue;
        };
        fields.insert(
            key.to_string(),
            parts.next().unwrap_or("").trim().to_string(),
        );
    }
    if fields.is_empty() {
        return Err("p4 未返回可解析的 -ztag 输出".to_string());
    }
    Ok(fields)
}

fn pick_tagged(
    fields: &std::collections::HashMap<String, String>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn render_process_error(
    program: &str,
    args: &[&str],
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    if detail.is_empty() {
        format!("{program} {:?} 失败: {status}", args)
    } else {
        format!("{program} {:?} 失败: {detail}", args)
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_workspace;
    use agentdash_domain::workspace::WorkspaceIdentityKind;
    use crate::local_backend_config::{
        GitWorkspaceRuntimeConfig, P4WorkspaceRuntimeConfig, WorkspaceContractRuntimeConfig,
    };

    #[test]
    fn prepare_workspace_skips_when_contract_missing() {
        let runtime_config = WorkspaceContractRuntimeConfig::default();
        let result = prepare_workspace(std::path::Path::new("."), None, None, &runtime_config);
        assert!(result.is_ok());
    }

    #[test]
    fn prepare_workspace_rejects_invalid_git_contract() {
        let runtime_config = WorkspaceContractRuntimeConfig {
            enabled: true,
            prepare_on_first_prompt: true,
            git: GitWorkspaceRuntimeConfig {
                enabled: true,
                allow_branch_sync: true,
                allow_commit_reset: true,
                default_remote: None,
            },
            p4: P4WorkspaceRuntimeConfig::default(),
        };
        let result = prepare_workspace(
            std::path::Path::new("."),
            Some(WorkspaceIdentityKind::GitRepo),
            Some(&serde_json::json!({
                "match_mode": "repo_branch",
                "remote_url": "git@example.com:repo/demo.git"
            })),
            &runtime_config,
        );
        assert!(result.is_err());
    }
}
