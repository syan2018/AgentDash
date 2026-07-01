use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use agentdash_domain::workspace::{
    P4WorkspaceIdentityContract, P4WorkspaceMatchMode, WorkspaceIdentityKind,
    identity_payload_from_detected_facts, identity_payload_matches_detected_facts,
    normalize_identity_payload,
};
use agentdash_relay::{
    CommandWorkspaceDiscoverByIdentityPayload, ResponseWorkspaceDetectPayload,
    ResponseWorkspaceDiscoverByIdentityPayload, WorkspaceIdentityDiscoveryCandidateRelay,
    WorkspaceIdentityDiscoverySkippedRelay, WorkspaceIdentityDiscoveryWorkspaceRelay,
    WorkspaceIdentityKindRelay,
};
use serde_json::{Value, json};

use crate::process_window::hide_window_for_std_command;
use crate::tool_executor::resolve_detect_workspace_root;
use crate::workspace_probe::{
    P4ProbeContext, detect_p4_executable, detect_workspace_with_p4_context, normalize_display_path,
};

pub fn discover_workspaces_by_identity(
    payload: CommandWorkspaceDiscoverByIdentityPayload,
) -> ResponseWorkspaceDiscoverByIdentityPayload {
    WorkspaceIdentityDiscoveryRegistry::default().discover(payload.workspaces)
}

struct WorkspaceIdentityDiscoveryRegistry {
    strategies: Vec<Box<dyn WorkspaceIdentityDiscoveryStrategy>>,
}

impl Default for WorkspaceIdentityDiscoveryRegistry {
    fn default() -> Self {
        Self {
            strategies: vec![Box::new(P4WorkspaceDiscoveryStrategy)],
        }
    }
}

impl WorkspaceIdentityDiscoveryRegistry {
    fn discover(
        &self,
        workspaces: Vec<WorkspaceIdentityDiscoveryWorkspaceRelay>,
    ) -> ResponseWorkspaceDiscoverByIdentityPayload {
        let mut output = ResponseWorkspaceDiscoverByIdentityPayload {
            candidates: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
        };

        for relay_workspace in workspaces {
            let workspace = DiscoveryWorkspace::from(relay_workspace);
            let Some(strategy) = self
                .strategies
                .iter()
                .find(|strategy| strategy.identity_kind() == workspace.identity_kind)
            else {
                output.skipped.push(skipped(
                    &workspace,
                    "unsupported_identity_kind",
                    "当前版本尚未支持该 Workspace identity 类型的本机发现",
                ));
                continue;
            };

            let result = strategy.discover(&workspace);
            output.candidates.extend(result.candidates);
            output.skipped.extend(result.skipped);
            output.warnings.extend(result.warnings);
        }

        output
    }
}

trait WorkspaceIdentityDiscoveryStrategy {
    fn identity_kind(&self) -> WorkspaceIdentityKind;
    fn discover(&self, workspace: &DiscoveryWorkspace) -> DiscoveryResult;
}

struct DiscoveryWorkspace {
    workspace_id: String,
    identity_kind: WorkspaceIdentityKind,
    relay_identity_kind: WorkspaceIdentityKindRelay,
    identity_payload: Value,
}

impl From<WorkspaceIdentityDiscoveryWorkspaceRelay> for DiscoveryWorkspace {
    fn from(value: WorkspaceIdentityDiscoveryWorkspaceRelay) -> Self {
        let identity_kind = identity_kind_from_relay(value.identity_kind.clone());
        Self {
            workspace_id: value.workspace_id,
            identity_kind,
            relay_identity_kind: value.identity_kind,
            identity_payload: value.identity_payload,
        }
    }
}

#[derive(Default)]
struct DiscoveryResult {
    candidates: Vec<WorkspaceIdentityDiscoveryCandidateRelay>,
    skipped: Vec<WorkspaceIdentityDiscoverySkippedRelay>,
    warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct P4WorkspaceDiscoveryStrategy;

impl WorkspaceIdentityDiscoveryStrategy for P4WorkspaceDiscoveryStrategy {
    fn identity_kind(&self) -> WorkspaceIdentityKind {
        WorkspaceIdentityKind::P4Workspace
    }

    fn discover(&self, workspace: &DiscoveryWorkspace) -> DiscoveryResult {
        let mut result = DiscoveryResult::default();
        let normalized = match normalize_identity_payload(
            WorkspaceIdentityKind::P4Workspace,
            &workspace.identity_payload,
        ) {
            Ok(value) => value,
            Err(error) => {
                result.skipped.push(skipped(
                    workspace,
                    "invalid_identity_payload",
                    &format!("P4 Workspace identity 无法归一化: {error}"),
                ));
                return result;
            }
        };
        let contract =
            match serde_json::from_value::<P4WorkspaceIdentityContract>(normalized.clone()) {
                Ok(contract) => contract,
                Err(error) => {
                    result.skipped.push(skipped(
                        workspace,
                        "invalid_identity_payload",
                        &format!("P4 Workspace identity 无法解析: {error}"),
                    ));
                    return result;
                }
            };

        if !matches!(
            contract.match_mode,
            P4WorkspaceMatchMode::ServerStream | P4WorkspaceMatchMode::ServerStreamClient
        ) {
            result.skipped.push(skipped(
                workspace,
                "unsupported_p4_match_mode",
                "当前版本仅支持 server_stream 与 server_stream_client 反向发现",
            ));
            return result;
        }

        let Some(server_address) = contract.server_address.as_deref() else {
            result.skipped.push(skipped(
                workspace,
                "invalid_identity_payload",
                "P4 Workspace identity 缺少 server_address",
            ));
            return result;
        };
        let Some(stream) = contract.stream.as_deref() else {
            result.skipped.push(skipped(
                workspace,
                "invalid_identity_payload",
                "P4 Workspace identity 缺少 stream",
            ));
            return result;
        };

        let p4 = match P4Cli::detect() {
            Some(cli) => cli,
            None => {
                result.skipped.push(skipped(
                    workspace,
                    "p4_cli_unavailable",
                    "本机未找到 p4 CLI，无法执行 P4 Workspace discovery",
                ));
                return result;
            }
        };
        let clients = match p4.clients_for_stream(server_address, stream) {
            Ok(clients) => clients,
            Err(error) => {
                result.skipped.push(skipped(
                    workspace,
                    "p4_clients_failed",
                    &format!("读取 P4 client 列表失败: {error}"),
                ));
                return result;
            }
        };

        let mut seen_roots = Vec::new();
        for client in clients {
            let spec = match p4.client_spec(server_address, &client.name) {
                Ok(spec) => spec,
                Err(error) => {
                    result.warnings.push(format!(
                        "读取 P4 client `{}` spec 失败: {error}",
                        client.name
                    ));
                    continue;
                }
            };
            if spec.stream.as_deref() != Some(stream) {
                result.warnings.push(format!(
                    "P4 client `{}` 的 stream 与 Workspace identity 不一致",
                    client.name
                ));
                continue;
            }

            for root in spec.roots() {
                let Ok(root_path) = resolve_detect_workspace_root(&root) else {
                    result.warnings.push(format!(
                        "P4 client `{}` 的 root 不可读: {root}",
                        client.name
                    ));
                    continue;
                };
                let root_ref = normalize_display_path(&root_path);
                if seen_roots.iter().any(|seen| seen == &root_ref) {
                    continue;
                }
                seen_roots.push(root_ref.clone());

                let p4_context = P4ProbeContext {
                    server_address: Some(server_address.to_string()),
                    client_name: Some(client.name.clone()),
                };
                let detected = detect_workspace_with_p4_context(&root_path, Some(&p4_context));
                let detected_facts = detected_facts_from_probe(&detected);
                if !discovery_identity_payload_matches_detected_facts(
                    WorkspaceIdentityKind::P4Workspace,
                    &normalized,
                    &detected_facts,
                    &root_ref,
                ) {
                    result.warnings.push(format!(
                        "P4 client `{}` 的 root 与 Workspace identity 不匹配: {root_ref}",
                        client.name
                    ));
                    continue;
                }

                let identity_payload = identity_payload_from_detected_facts(
                    WorkspaceIdentityKind::P4Workspace,
                    &detected_facts,
                    &root_ref,
                )
                .unwrap_or_else(|| normalized.clone());
                result
                    .candidates
                    .push(WorkspaceIdentityDiscoveryCandidateRelay {
                        workspace_id: workspace.workspace_id.clone(),
                        root_ref,
                        identity_kind: workspace.relay_identity_kind.clone(),
                        identity_payload,
                        detected_facts,
                        confidence: "high".to_string(),
                        display_name: Some(format!("{} · {}", client.name, stream)),
                        client_name: Some(client.name.clone()),
                        server_address: Some(server_address.to_string()),
                        stream: Some(stream.to_string()),
                        warnings: detected.warnings,
                    });
            }
        }

        if result.candidates.is_empty() && result.skipped.is_empty() {
            result.skipped.push(skipped(
                workspace,
                "no_candidates",
                "未发现可读且匹配该 Workspace identity 的本机 P4 client root",
            ));
        }

        result
    }
}

struct P4Cli {
    executable: String,
}

impl P4Cli {
    fn detect() -> Option<Self> {
        detect_p4_executable().map(|executable| Self { executable })
    }

    fn clients_for_stream(
        &self,
        server_address: &str,
        stream: &str,
    ) -> Result<Vec<P4Client>, String> {
        let mut args = vec![
            "-ztag".to_string(),
            "-p".to_string(),
            server_address.to_string(),
            "clients".to_string(),
            "-S".to_string(),
            stream.to_string(),
            "--me".to_string(),
        ];
        let output = match self.run(&args, None) {
            Ok(output) => output,
            Err(first_error) => {
                let user_name = self.current_user(server_address).ok();
                let Some(user_name) = user_name else {
                    return Err(first_error);
                };
                args = vec![
                    "-ztag".to_string(),
                    "-p".to_string(),
                    server_address.to_string(),
                    "clients".to_string(),
                    "-S".to_string(),
                    stream.to_string(),
                    "-u".to_string(),
                    user_name,
                ];
                self.run(&args, None)?
            }
        };
        Ok(parse_p4_clients(&output))
    }

    fn current_user(&self, server_address: &str) -> Result<String, String> {
        let output = self.run(
            &[
                "-ztag".to_string(),
                "-p".to_string(),
                server_address.to_string(),
                "info".to_string(),
            ],
            None,
        )?;
        let records = parse_tagged_records(&output);
        records
            .first()
            .and_then(|record| pick_tagged(record, &["userName", "username", "UserName"]))
            .ok_or_else(|| "p4 info 未返回 userName".to_string())
    }

    fn client_spec(&self, server_address: &str, client_name: &str) -> Result<P4ClientSpec, String> {
        let output = self.run(
            &[
                "-p".to_string(),
                server_address.to_string(),
                "client".to_string(),
                "-o".to_string(),
                client_name.to_string(),
            ],
            None,
        )?;
        parse_p4_client_spec(&output)
    }

    fn run(&self, args: &[String], cwd: Option<&Path>) -> Result<String, String> {
        let mut command = Command::new(&self.executable);
        command.args(args);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        hide_window_for_std_command(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("启动 p4 失败: {error}"))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() { stderr } else { stdout };
        Err(if message.is_empty() {
            format!("p4 返回非零退出码: {}", output.status)
        } else {
            message
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct P4Client {
    name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct P4ClientSpec {
    root: Option<String>,
    alt_roots: Vec<String>,
    stream: Option<String>,
}

impl P4ClientSpec {
    fn roots(&self) -> Vec<String> {
        let mut roots = Vec::new();
        if let Some(root) = self.root.clone() {
            roots.push(root);
        }
        for root in &self.alt_roots {
            if !roots.iter().any(|seen| seen == root) {
                roots.push(root.clone());
            }
        }
        roots
    }
}

fn parse_p4_clients(raw: &str) -> Vec<P4Client> {
    parse_tagged_records(raw)
        .into_iter()
        .filter_map(|record| {
            pick_tagged(&record, &["client", "Client"]).map(|name| P4Client { name })
        })
        .collect()
}

fn parse_tagged_records(raw: &str) -> Vec<HashMap<String, String>> {
    let mut records = Vec::new();
    let mut current = HashMap::new();

    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some(rest) = line.strip_prefix("... ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(key) = parts.next() else {
            continue;
        };
        let value = parts.next().unwrap_or("").trim().to_string();
        if key == "client" && !current.is_empty() {
            records.push(std::mem::take(&mut current));
        }
        current.insert(key.to_string(), value);
    }

    if !current.is_empty() {
        records.push(current);
    }
    records
}

fn parse_p4_client_spec(raw: &str) -> Result<P4ClientSpec, String> {
    let mut spec = P4ClientSpec::default();
    let mut in_alt_roots = false;

    for line in raw.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if line.chars().next().is_some_and(char::is_whitespace) {
            if in_alt_roots && let Some(root) = normalize_spec_value(line) {
                spec.alt_roots.push(root);
            }
            continue;
        }
        in_alt_roots = false;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "Root" => spec.root = normalize_spec_value(value),
            "Stream" => spec.stream = normalize_spec_value(value),
            "AltRoots" => {
                in_alt_roots = true;
                if let Some(root) = normalize_spec_value(value) {
                    spec.alt_roots.push(root);
                }
            }
            _ => {}
        }
    }

    if spec.root.is_none() && spec.alt_roots.is_empty() {
        return Err("P4 client spec 缺少 Root/AltRoots".to_string());
    }
    Ok(spec)
}

fn normalize_spec_value(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"').trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(value.to_string())
}

fn pick_tagged(fields: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn detected_facts_from_probe(payload: &ResponseWorkspaceDetectPayload) -> Value {
    json!({
        "git": payload.git.as_ref().map(|git| json!({
            "is_repo": true,
            "repo_root": git.repo_root,
            "source_repo": git.remote_url,
            "default_branch": git.default_branch,
            "branch": git.current_branch,
            "commit_hash": git.commit_hash,
        })),
        "p4": payload.p4.as_ref().map(|p4| json!({
            "is_workspace": true,
            "workspace_root": p4.workspace_root,
            "client_name": p4.client_name,
            "server_address": p4.server_address,
            "user_name": p4.user_name,
            "stream": p4.stream,
        })),
        "warnings": payload.warnings,
    })
}

fn discovery_identity_payload_matches_detected_facts(
    kind: WorkspaceIdentityKind,
    expected_payload: &Value,
    detected_facts: &Value,
    root_ref: &str,
) -> bool {
    if identity_payload_matches_detected_facts(
        kind.clone(),
        expected_payload,
        detected_facts,
        root_ref,
    ) {
        return true;
    }

    if kind != WorkspaceIdentityKind::P4Workspace {
        return false;
    }
    let Ok(normalized) =
        normalize_identity_payload(WorkspaceIdentityKind::P4Workspace, expected_payload)
    else {
        return false;
    };
    let Ok(mut contract) = serde_json::from_value::<P4WorkspaceIdentityContract>(normalized) else {
        return false;
    };
    if contract.match_mode != P4WorkspaceMatchMode::ServerStreamClient {
        return false;
    }
    contract.match_mode = P4WorkspaceMatchMode::ServerStream;
    contract.client_name = None;
    let Ok(relaxed_payload) = serde_json::to_value(contract) else {
        return false;
    };

    identity_payload_matches_detected_facts(
        WorkspaceIdentityKind::P4Workspace,
        &relaxed_payload,
        detected_facts,
        root_ref,
    )
}

fn skipped(
    workspace: &DiscoveryWorkspace,
    reason: &str,
    message: &str,
) -> WorkspaceIdentityDiscoverySkippedRelay {
    WorkspaceIdentityDiscoverySkippedRelay {
        workspace_id: workspace.workspace_id.clone(),
        identity_kind: workspace.relay_identity_kind.clone(),
        reason: reason.to_string(),
        message: message.to_string(),
    }
}

fn identity_kind_from_relay(kind: WorkspaceIdentityKindRelay) -> WorkspaceIdentityKind {
    match kind {
        WorkspaceIdentityKindRelay::GitRepo => WorkspaceIdentityKind::GitRepo,
        WorkspaceIdentityKindRelay::P4Workspace => WorkspaceIdentityKind::P4Workspace,
        WorkspaceIdentityKindRelay::LocalDir => WorkspaceIdentityKind::LocalDir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_skips_unsupported_identity_kind() {
        let output = discover_workspaces_by_identity(CommandWorkspaceDiscoverByIdentityPayload {
            workspaces: vec![WorkspaceIdentityDiscoveryWorkspaceRelay {
                workspace_id: "00000000-0000-0000-0000-000000000001".to_string(),
                identity_kind: WorkspaceIdentityKindRelay::GitRepo,
                identity_payload: json!({ "repo_key": "example/repo" }),
            }],
        });

        assert!(output.candidates.is_empty());
        assert_eq!(output.skipped[0].reason, "unsupported_identity_kind");
    }

    #[test]
    fn parse_tagged_clients_keeps_multiple_records() {
        let clients = parse_p4_clients(
            "... client demo-main\n... Root C:\\ws\\main\n... client demo-alt\n... Root D:\\ws\\alt\n",
        );

        assert_eq!(
            clients,
            vec![
                P4Client {
                    name: "demo-main".to_string()
                },
                P4Client {
                    name: "demo-alt".to_string()
                }
            ]
        );
    }

    #[test]
    fn parse_client_spec_extracts_root_alt_roots_and_stream() {
        let spec = parse_p4_client_spec(
            "Client: demo\nRoot: C:\\ws\\demo\nAltRoots:\n\tD:\\ws\\demo\n\t//server/share/demo\nStream: //Depot/main\n",
        )
        .expect("client spec should parse");

        assert_eq!(spec.root.as_deref(), Some("C:\\ws\\demo"));
        assert_eq!(
            spec.alt_roots,
            vec![
                "D:\\ws\\demo".to_string(),
                "//server/share/demo".to_string()
            ]
        );
        assert_eq!(spec.stream.as_deref(), Some("//Depot/main"));
    }

    #[test]
    fn server_stream_client_discovery_match_ignores_project_client_name() {
        let expected_payload = json!({
            "match_mode": "server_stream_client",
            "server_address": "ssl:p4.example.com:1666",
            "stream": "//Depot/main",
            "client_name": "project-template-client",
        });
        let detected_facts = json!({
            "p4": {
                "is_workspace": true,
                "workspace_root": "C:/ws/main",
                "client_name": "local-user-client",
                "server_address": "ssl:p4.example.com:1666",
                "user_name": "local-user",
                "stream": "//Depot/main",
            },
            "warnings": [],
        });

        assert!(discovery_identity_payload_matches_detected_facts(
            WorkspaceIdentityKind::P4Workspace,
            &expected_payload,
            &detected_facts,
            "C:/ws/main",
        ));
    }
}
