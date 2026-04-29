use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use agentdash_relay::{
    ResponseWorkspaceDetectPayload, WorkspaceGitProbePayload, WorkspaceP4ProbePayload,
};
use gix::bstr::ByteSlice;

pub fn detect_workspace(path: &Path) -> ResponseWorkspaceDetectPayload {
    let mut warnings = Vec::new();
    let git = detect_git_workspace(path, &mut warnings);
    let p4 = detect_p4_workspace(path, &mut warnings);

    ResponseWorkspaceDetectPayload { git, p4, warnings }
}

fn detect_git_workspace(
    path: &Path,
    warnings: &mut Vec<String>,
) -> Option<WorkspaceGitProbePayload> {
    let repo = match gix::discover(path) {
        Ok(repo) => repo,
        Err(_) => return None,
    };

    let repo_root = repo
        .workdir()
        .map(PathBuf::from)
        .or_else(|| repo.git_dir().parent().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf());

    let remote_url = preferred_remote_url(&repo);
    if remote_url.is_none() {
        warnings
            .push("Git 仓库未配置可读 remote，将退化为基于 repo_root 的本机身份键。".to_string());
    }

    Some(WorkspaceGitProbePayload {
        repo_root: normalize_display_path(&repo_root),
        default_branch: detect_default_branch(&repo),
        current_branch: detect_current_branch(&repo),
        remote_url,
        commit_hash: detect_head_commit(&repo),
    })
}

fn preferred_remote_url(repo: &gix::Repository) -> Option<String> {
    if let Ok(remote) = repo.find_remote("origin")
        && let Some(url) = remote_fetch_url(&remote)
    {
        return Some(url);
    }

    if let Ok(Some(head_ref)) = repo.head_ref()
        && let Some(Ok(remote)) = head_ref.remote(gix::remote::Direction::Fetch)
        && let Some(url) = remote_fetch_url(&remote)
    {
        return Some(url);
    }

    if let Some(Ok(remote)) = repo.find_default_remote(gix::remote::Direction::Fetch)
        && let Some(url) = remote_fetch_url(&remote)
    {
        return Some(url);
    }

    for remote_name in repo.remote_names() {
        if let Ok(remote) = repo.find_remote(remote_name.as_ref())
            && let Some(url) = remote_fetch_url(&remote)
        {
            return Some(url);
        }
    }

    None
}

fn remote_fetch_url(remote: &gix::Remote<'_>) -> Option<String> {
    remote
        .url(gix::remote::Direction::Fetch)
        .map(ToString::to_string)
        .filter(|url| !url.trim().is_empty())
}

fn detect_default_branch(repo: &gix::Repository) -> Option<String> {
    let reference = repo.find_reference("refs/remotes/origin/HEAD").ok()?;
    let target = match reference.follow() {
        Some(Ok(target)) => target,
        _ => return None,
    };

    target
        .name()
        .shorten()
        .as_bstr()
        .to_str()
        .ok()
        .map(strip_remote_name)
        .map(str::to_string)
}

fn detect_current_branch(repo: &gix::Repository) -> Option<String> {
    repo.head_name()
        .ok()
        .flatten()
        .and_then(|name| name.shorten().as_bstr().to_str().ok().map(str::to_string))
        .or_else(|| detect_head_commit(repo))
}

fn detect_head_commit(repo: &gix::Repository) -> Option<String> {
    repo.head_id().ok().map(|oid| oid.to_string())
}

fn strip_remote_name(short_name: &str) -> &str {
    short_name
        .split_once('/')
        .map(|(_, branch_name)| branch_name)
        .unwrap_or(short_name)
}

fn detect_p4_workspace(path: &Path, warnings: &mut Vec<String>) -> Option<WorkspaceP4ProbePayload> {
    let Some(p4_executable) = detect_p4_executable() else {
        return None;
    };

    let workspace_probe = build_p4_path_probe(path);
    let where_result = run_p4_tagged(&p4_executable, path, &["where", &workspace_probe]);
    let where_fields = match where_result {
        Ok(fields) if has_p4_mapping(&fields) => fields,
        Ok(_) => return None,
        Err(err) => {
            if should_surface_p4_warning(path) {
                warnings.push(format!("P4 workspace 探测失败: {err}"));
            }
            return None;
        }
    };

    let info_fields = match run_p4_tagged(&p4_executable, path, &["info"]) {
        Ok(fields) => fields,
        Err(err) => {
            warnings.push(format!("P4 info 读取失败: {err}"));
            return None;
        }
    };

    let client_name = pick_tagged(
        &info_fields,
        &["clientName", "clientname", "Client name", "ClientName"],
    );
    let workspace_root = pick_tagged(
        &info_fields,
        &["clientRoot", "clientroot", "Client root", "ClientRoot"],
    )
    .or_else(|| pick_tagged(&where_fields, &["path", "clientFile", "clientfile"]))?;
    let server_address = pick_tagged(
        &info_fields,
        &[
            "serverAddress",
            "serveraddress",
            "serverAddressIPv6",
            "Server address",
            "ServerAddress",
        ],
    );
    let user_name = pick_tagged(
        &info_fields,
        &["userName", "username", "User name", "UserName"],
    );

    let stream = client_name.as_deref().and_then(|client| {
        run_p4_tagged(&p4_executable, path, &["client", "-o", client])
            .ok()
            .and_then(|fields| pick_tagged(&fields, &["Stream", "stream"]))
    });

    Some(WorkspaceP4ProbePayload {
        workspace_root: normalize_display_path(Path::new(&workspace_root)),
        client_name,
        server_address,
        user_name,
        stream,
    })
}

fn detect_p4_executable() -> Option<String> {
    let lookup = if cfg!(windows) { "where" } else { "which" };
    let candidate = if cfg!(windows) { "p4.exe" } else { "p4" };
    let output = Command::new(lookup).arg(candidate).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn build_p4_path_probe(path: &Path) -> String {
    let mut probe = normalize_display_path(path);
    if !probe.ends_with('\\') && !probe.ends_with('/') {
        probe.push(std::path::MAIN_SEPARATOR);
    }
    probe.push_str("...");
    probe
}

fn has_p4_mapping(fields: &HashMap<String, String>) -> bool {
    let Some(path) = pick_tagged(fields, &["path", "depotFile", "depotfile", "clientFile"]) else {
        return false;
    };
    !path.starts_with('-')
}

fn run_p4_tagged(
    executable: &str,
    cwd: &Path,
    args: &[&str],
) -> Result<HashMap<String, String>, String> {
    let output = Command::new(executable)
        .current_dir(cwd)
        .arg("-ztag")
        .args(args)
        .output()
        .map_err(|err| format!("启动 p4 失败: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if message.is_empty() {
            format!("p4 返回非零退出码: {}", output.status)
        } else {
            message
        });
    }

    parse_p4_tagged_output(&String::from_utf8_lossy(&output.stdout))
}

fn parse_p4_tagged_output(raw: &str) -> Result<HashMap<String, String>, String> {
    let mut fields = HashMap::new();

    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some(rest) = line.strip_prefix("... ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(key) = parts.next() else {
            continue;
        };
        let value = parts.next().unwrap_or("").trim().to_string();
        fields.insert(key.to_string(), value);
    }

    if fields.is_empty() {
        return Err("p4 未返回可解析的 -ztag 输出".to_string());
    }

    Ok(fields)
}

fn pick_tagged(fields: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn should_surface_p4_warning(path: &Path) -> bool {
    path.join(".p4config").exists()
        || path.join("p4config.txt").exists()
        || std::env::var_os("P4CONFIG").is_some()
        || std::env::var_os("P4CLIENT").is_some()
}

fn normalize_display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_p4_path_probe, parse_p4_tagged_output, strip_remote_name};
    use std::path::Path;

    #[test]
    fn parse_p4_tagged_output_extracts_fields() {
        let fields = parse_p4_tagged_output(
            "... clientName demo-client\n... clientRoot C:\\ws\\demo\n... serverAddress perforce:1666\n",
        )
        .expect("tagged output should parse");

        assert_eq!(
            fields.get("clientName").map(String::as_str),
            Some("demo-client")
        );
        assert_eq!(
            fields.get("clientRoot").map(String::as_str),
            Some("C:\\ws\\demo")
        );
        assert_eq!(
            fields.get("serverAddress").map(String::as_str),
            Some("perforce:1666")
        );
    }

    #[test]
    fn build_p4_path_probe_appends_recursive_suffix() {
        let probe = build_p4_path_probe(Path::new("C:\\ws\\demo"));
        assert!(probe.ends_with("..."));
    }

    #[test]
    fn strip_remote_name_keeps_nested_branch_segments() {
        assert_eq!(strip_remote_name("origin/main"), "main");
        assert_eq!(strip_remote_name("origin/feature/demo"), "feature/demo");
    }
}
