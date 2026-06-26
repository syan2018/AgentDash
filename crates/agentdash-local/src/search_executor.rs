use agentdash_diagnostics::{diag, Subsystem};
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentdash_relay::SearchHit;

use crate::file_discovery_policy::FileDiscoveryPolicy;
use crate::tool_executor::{ToolError, resolve_existing_path_with_root, workspace_relative_path};

const SEARCH_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchParams<'a> {
    pub query: &'a str,
    pub path: Option<&'a str>,
    pub is_regex: bool,
    pub include_glob: Option<&'a str>,
    pub max_results: usize,
    pub context_lines: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SearchExecutor;

impl SearchExecutor {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn search(
        &self,
        workspace_root: &Path,
        params: &SearchParams<'_>,
    ) -> Result<(Vec<SearchHit>, bool), ToolError> {
        self.search_with_ripgrep_path(workspace_root, params, detect_ripgrep().await)
            .await
    }

    async fn search_with_ripgrep_path(
        &self,
        workspace_root: &Path,
        params: &SearchParams<'_>,
        rg_path: Option<PathBuf>,
    ) -> Result<(Vec<SearchHit>, bool), ToolError> {
        let search_dir = match params.path {
            Some(p) if !p.trim().is_empty() && p.trim() != "." => {
                resolve_existing_path_with_root(workspace_root, p)?
            }
            _ => workspace_root.to_path_buf(),
        };
        let policy = FileDiscoveryPolicy::from_base(&search_dir, workspace_root);
        if !policy.allows_path(&search_dir, workspace_root) {
            return Ok((Vec::new(), false));
        }

        let rg = rg_path.ok_or_else(ripgrep_unavailable_error)?;
        run_ripgrep(&rg, &search_dir, workspace_root, params, policy).await
    }
}

fn ripgrep_unavailable_error() -> ToolError {
    ToolError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "未找到 rg/ripgrep",
    ))
}

async fn detect_ripgrep() -> Option<PathBuf> {
    let candidates = if cfg!(windows) {
        vec!["rg.exe", "rg"]
    } else {
        vec!["rg"]
    };
    for name in candidates {
        if let Ok(output) =
            tokio::process::Command::new(if cfg!(windows) { "where" } else { "which" })
                .arg(name)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            && output.status.success()
        {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let first_line = path_str.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Some(PathBuf::from(first_line));
            }
        }
    }
    None
}

async fn run_ripgrep(
    rg_path: &Path,
    search_dir: &Path,
    workspace_root: &Path,
    params: &SearchParams<'_>,
    policy: FileDiscoveryPolicy,
) -> Result<(Vec<SearchHit>, bool), ToolError> {
    let mut cmd = tokio::process::Command::new(rg_path);
    cmd.arg("--json")
        .arg("--max-count")
        .arg(params.max_results.to_string());
    for arg in ripgrep_policy_args(policy) {
        cmd.arg(arg);
    }

    if params.context_lines > 0 {
        cmd.arg("-C").arg(params.context_lines.to_string());
    }
    if !params.is_regex {
        cmd.arg("--fixed-strings");
    }
    if let Some(glob) = params.include_glob {
        cmd.arg("--glob").arg(glob);
    }

    cmd.arg("--").arg(params.query).arg(search_dir);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(Duration::from_millis(SEARCH_TIMEOUT_MS), cmd.output())
        .await
        .map_err(|_| ToolError::Timeout(SEARCH_TIMEOUT_MS))?
        .map_err(ToolError::Io)?;

    let mut hits = Vec::new();
    let mut truncated = false;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if json.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }

        let data = match json.get("data") {
            Some(d) => d,
            None => continue,
        };

        let abs_path = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let Ok(rel_path) = workspace_relative_path(Path::new(abs_path), workspace_root) else {
            diag!(Warn, Subsystem::AgentRun,
        
                path = abs_path,
                workspace_root = %workspace_root.display(),
                "ripgrep returned path outside workspace root"
            );
            continue;
        };

        let line_number = data
            .get("line_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;

        let content = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string();

        hits.push(SearchHit {
            path: rel_path,
            line_number,
            content,
            context_before: Vec::new(),
            context_after: Vec::new(),
        });

        if hits.len() >= params.max_results {
            truncated = true;
            break;
        }
    }

    Ok((hits, truncated))
}

fn ripgrep_policy_args(policy: FileDiscoveryPolicy) -> Vec<String> {
    let mut args = vec!["--hidden".to_string()];
    if policy.respects_workspace_ignores() {
        args.push("--no-require-git".to_string());
    }
    if !policy.respects_workspace_ignores() {
        args.push("--no-ignore".to_string());
    }
    push_ripgrep_exclude_globs(&mut args, policy.exclude_dir_names());
    args
}

fn push_ripgrep_exclude_globs(args: &mut Vec<String>, dirs: Vec<&'static str>) {
    for dir in dirs {
        args.push("--glob".to_string());
        args.push(format!("!{dir}/**"));
        args.push("--glob".to_string());
        args.push(format!("!**/{dir}/**"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(path, content).expect("write file");
    }

    #[tokio::test]
    async fn search_default_skips_ignored_subtree_but_explicit_path_finds_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join(".gitignore"), "ignored/\n");
        write_file(&temp.path().join("src/main.rs"), "needle in source\n");
        write_file(
            &temp.path().join("ignored/generated.rs"),
            "needle in generated\n",
        );
        let executor = SearchExecutor::new();
        let root = std::fs::canonicalize(temp.path()).expect("canonical workspace");

        let params = SearchParams {
            query: "needle",
            path: None,
            is_regex: false,
            include_glob: None,
            max_results: 20,
            context_lines: 0,
        };
        let (default_hits, _) = executor.search(&root, &params).await.expect("search");
        assert!(default_hits.iter().any(|hit| hit.path == "src/main.rs"));
        assert!(
            !default_hits
                .iter()
                .any(|hit| hit.path.starts_with("ignored/")),
            "default search should skip ignored subtree: {default_hits:?}"
        );

        let explicit_params = SearchParams {
            path: Some("ignored"),
            ..params
        };
        let (explicit_hits, _) = executor
            .search(&root, &explicit_params)
            .await
            .expect("explicit search");
        assert!(
            explicit_hits
                .iter()
                .any(|hit| hit.path == "ignored/generated.rs"),
            "explicit ignored subtree should be searchable: {explicit_hits:?}"
        );
    }

    #[tokio::test]
    async fn search_requires_ripgrep_when_unavailable() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(&temp.path().join("src/main.rs"), "needle in source\n");
        let executor = SearchExecutor::new();
        let root = std::fs::canonicalize(temp.path()).expect("canonical workspace");
        let params = SearchParams {
            query: "needle",
            path: None,
            is_regex: false,
            include_glob: None,
            max_results: 20,
            context_lines: 0,
        };

        let error = executor
            .search_with_ripgrep_path(&root, &params, None)
            .await
            .expect_err("search should require ripgrep");
        assert!(matches!(
            error,
            ToolError::Io(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn ripgrep_policy_args_enter_explicit_ignored_subtree_without_vcs_metadata() {
        let implicit = ripgrep_policy_args(FileDiscoveryPolicy::implicit_workspace_scan());
        assert!(implicit.contains(&"--hidden".to_string()));
        assert!(implicit.contains(&"--no-require-git".to_string()));
        assert!(!implicit.contains(&"--no-ignore".to_string()));
        assert!(implicit.contains(&"!**/node_modules/**".to_string()));
        assert!(implicit.contains(&"!**/.git/**".to_string()));

        let explicit = ripgrep_policy_args(FileDiscoveryPolicy::explicit_subtree_scan());
        assert!(explicit.contains(&"--no-ignore".to_string()));
        assert!(!explicit.contains(&"--no-require-git".to_string()));
        assert!(!explicit.contains(&"!**/node_modules/**".to_string()));
        assert!(explicit.contains(&"!**/.git/**".to_string()));
    }
}
