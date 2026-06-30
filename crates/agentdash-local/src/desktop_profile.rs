use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::machine_identity::{LocalMachineIdentity, load_or_create_machine_identity};
use crate::runtime_paths::local_runtime_profile_path;

const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopRuntimeStartRequest {
    pub server_url: String,
    #[serde(default)]
    pub access_token: String,
    pub profile_id: String,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_label: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalRuntimeProfile {
    pub server_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_label: Option<String>,
    #[serde(default)]
    pub backend_id: Option<String>,
    #[serde(default)]
    pub relay_ws_url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    #[serde(default = "default_executor_enabled")]
    pub executor_enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
}

impl From<LocalRuntimeProfile> for DesktopRuntimeStartRequest {
    fn from(profile: LocalRuntimeProfile) -> Self {
        Self {
            server_url: profile.server_url,
            access_token: String::new(),
            profile_id: profile.profile_id,
            machine_id: profile.machine_id,
            machine_label: profile.machine_label,
            name: profile.name,
            workspace_roots: profile.workspace_roots,
            executor_enabled: profile.executor_enabled,
        }
    }
}

pub fn load_desktop_runtime_profile() -> anyhow::Result<Option<LocalRuntimeProfile>> {
    load_desktop_runtime_profile_at(local_runtime_profile_path()?, None)
}

pub fn load_desktop_runtime_profile_with_server_origin(
    server_origin: &str,
) -> anyhow::Result<Option<LocalRuntimeProfile>> {
    load_desktop_runtime_profile_at(local_runtime_profile_path()?, Some(server_origin))
}

pub fn save_desktop_runtime_profile(
    profile: LocalRuntimeProfile,
) -> anyhow::Result<LocalRuntimeProfile> {
    save_desktop_runtime_profile_at(local_runtime_profile_path()?, profile, None)
}

pub fn save_desktop_runtime_profile_with_server_origin(
    profile: LocalRuntimeProfile,
    server_origin: &str,
) -> anyhow::Result<LocalRuntimeProfile> {
    save_desktop_runtime_profile_at(local_runtime_profile_path()?, profile, Some(server_origin))
}

pub fn delete_desktop_runtime_profile() -> anyhow::Result<()> {
    delete_desktop_runtime_profile_at(local_runtime_profile_path()?)
}

pub fn normalize_desktop_runtime_profile(
    profile: LocalRuntimeProfile,
) -> anyhow::Result<LocalRuntimeProfile> {
    let identity = load_or_create_machine_identity()?;
    Ok(normalize_desktop_runtime_profile_with_identity(
        profile, identity, None,
    ))
}

pub fn normalize_desktop_runtime_profile_with_server_origin(
    profile: LocalRuntimeProfile,
    server_origin: &str,
) -> anyhow::Result<LocalRuntimeProfile> {
    let identity = load_or_create_machine_identity()?;
    Ok(normalize_desktop_runtime_profile_with_identity(
        profile,
        identity,
        Some(server_origin),
    ))
}

pub fn normalize_desktop_runtime_start_request(
    request: DesktopRuntimeStartRequest,
) -> anyhow::Result<DesktopRuntimeStartRequest> {
    let identity = load_or_create_machine_identity()?;
    Ok(normalize_desktop_runtime_start_request_with_identity(
        request, identity, None,
    ))
}

pub fn normalize_desktop_runtime_start_request_with_server_origin(
    request: DesktopRuntimeStartRequest,
    server_origin: &str,
) -> anyhow::Result<DesktopRuntimeStartRequest> {
    let identity = load_or_create_machine_identity()?;
    Ok(normalize_desktop_runtime_start_request_with_identity(
        request,
        identity,
        Some(server_origin),
    ))
}

fn load_desktop_runtime_profile_at(
    path: PathBuf,
    server_origin: Option<&str>,
) -> anyhow::Result<Option<LocalRuntimeProfile>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let profile = serde_json::from_str(&content)
        .map_err(|error| anyhow::anyhow!("读取桌面端 profile 失败: {error}"))?;
    let identity = load_or_create_machine_identity()?;
    Ok(Some(normalize_desktop_runtime_profile_with_identity(
        profile,
        identity,
        server_origin,
    )))
}

fn save_desktop_runtime_profile_at(
    path: PathBuf,
    profile: LocalRuntimeProfile,
    server_origin: Option<&str>,
) -> anyhow::Result<LocalRuntimeProfile> {
    let identity = load_or_create_machine_identity()?;
    save_desktop_runtime_profile_at_with_identity(path, profile, identity, server_origin)
}

fn save_desktop_runtime_profile_at_with_identity(
    path: PathBuf,
    profile: LocalRuntimeProfile,
    identity: LocalMachineIdentity,
    server_origin: Option<&str>,
) -> anyhow::Result<LocalRuntimeProfile> {
    let profile = normalize_desktop_runtime_profile_with_identity(profile, identity, server_origin);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&profile)?;
    std::fs::write(&path, content)?;
    Ok(profile)
}

fn delete_desktop_runtime_profile_at(path: PathBuf) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path)?;
    Ok(())
}

fn normalize_desktop_runtime_profile_with_identity(
    profile: LocalRuntimeProfile,
    identity: LocalMachineIdentity,
    server_origin: Option<&str>,
) -> LocalRuntimeProfile {
    let machine_label = profile
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);

    LocalRuntimeProfile {
        server_url: normalize_server_origin(&profile.server_url, server_origin),
        access_token: String::new(),
        profile_id: normalize_profile_id(profile.profile_id),
        machine_id: identity.machine_id,
        machine_label: Some(machine_label),
        backend_id: profile.backend_id.and_then(normalize_optional_text),
        relay_ws_url: profile.relay_ws_url.and_then(normalize_optional_text),
        name: profile.name.and_then(normalize_optional_text),
        workspace_roots: profile.workspace_roots,
        executor_enabled: profile.executor_enabled,
        auto_start: profile.auto_start,
    }
}

fn normalize_desktop_runtime_start_request_with_identity(
    request: DesktopRuntimeStartRequest,
    identity: LocalMachineIdentity,
    server_origin: Option<&str>,
) -> DesktopRuntimeStartRequest {
    let machine_label = request
        .machine_label
        .and_then(normalize_optional_text)
        .unwrap_or(identity.machine_label);

    DesktopRuntimeStartRequest {
        server_url: normalize_server_origin(&request.server_url, server_origin),
        access_token: request.access_token.trim().to_string(),
        profile_id: normalize_profile_id(request.profile_id),
        machine_id: identity.machine_id,
        machine_label: Some(machine_label),
        name: request.name.and_then(normalize_optional_text),
        workspace_roots: request.workspace_roots,
        executor_enabled: request.executor_enabled,
    }
}

fn normalize_server_origin(value: &str, server_origin: Option<&str>) -> String {
    let source = server_origin.unwrap_or(value);
    source.trim().trim_end_matches('/').to_string()
}

fn normalize_profile_id(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_PROFILE_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn default_profile_id() -> String {
    DEFAULT_PROFILE_ID.to_string()
}

fn default_executor_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> LocalMachineIdentity {
        LocalMachineIdentity {
            machine_id: "machine-local".to_string(),
            machine_label: "Desktop Host".to_string(),
        }
    }

    fn profile() -> LocalRuntimeProfile {
        LocalRuntimeProfile {
            server_url: " http://old.example:3001/ ".to_string(),
            access_token: "old-access-token".to_string(),
            profile_id: " default ".to_string(),
            machine_id: "stale-machine".to_string(),
            machine_label: Some(" Saved Host ".to_string()),
            backend_id: Some(" backend-1 ".to_string()),
            relay_ws_url: Some(" wss://relay.example/ws/backend ".to_string()),
            name: Some(" Desktop Runtime ".to_string()),
            workspace_roots: vec![PathBuf::from("C:/work")],
            executor_enabled: true,
            auto_start: true,
        }
    }

    #[test]
    fn desktop_profile_missing_returns_none() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("profile.json");

        let loaded = load_desktop_runtime_profile_at(path, Some("http://127.0.0.1:17301"))
            .expect("缺失 profile 应成功返回 None");

        assert!(loaded.is_none());
    }

    #[test]
    fn desktop_profile_save_load_roundtrip_normalizes_identity_and_token() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("nested").join("profile.json");

        let saved = save_desktop_runtime_profile_at_with_identity(
            path.clone(),
            profile(),
            identity(),
            Some(" http://127.0.0.1:17301/ "),
        )
        .expect("profile 应能保存");

        assert_eq!(saved.server_url, "http://127.0.0.1:17301");
        assert_eq!(saved.access_token, "");
        assert_eq!(saved.profile_id, "default");
        assert_eq!(saved.machine_id, "machine-local");
        assert_eq!(saved.machine_label.as_deref(), Some("Saved Host"));
        assert_eq!(saved.backend_id.as_deref(), Some("backend-1"));

        let content = std::fs::read_to_string(path).expect("应能读取 profile 文件");
        let persisted: LocalRuntimeProfile = serde_json::from_str(&content).expect("JSON 应有效");
        assert_eq!(persisted.access_token, "");
    }

    #[test]
    fn desktop_profile_delete_removes_file() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("profile.json");
        save_desktop_runtime_profile_at_with_identity(
            path.clone(),
            profile(),
            identity(),
            Some("http://127.0.0.1:17301"),
        )
        .expect("profile 应能保存");

        delete_desktop_runtime_profile_at(path.clone()).expect("profile 应能删除");

        assert!(
            load_desktop_runtime_profile_at(path, Some("http://127.0.0.1:17301"))
                .expect("删除后读取应成功")
                .is_none()
        );
    }

    #[test]
    fn desktop_start_request_normalization_keeps_one_time_token() {
        let request = normalize_desktop_runtime_start_request_with_identity(
            DesktopRuntimeStartRequest {
                server_url: "http://old.example:3001/".to_string(),
                access_token: " bearer-token ".to_string(),
                profile_id: " ".to_string(),
                machine_id: "stale".to_string(),
                machine_label: None,
                name: Some(" Runtime ".to_string()),
                workspace_roots: Vec::new(),
                executor_enabled: true,
            },
            identity(),
            Some(" http://127.0.0.1:17301/ "),
        );

        assert_eq!(request.server_url, "http://127.0.0.1:17301");
        assert_eq!(request.access_token, "bearer-token");
        assert_eq!(request.profile_id, "default");
        assert_eq!(request.machine_id, "machine-local");
        assert_eq!(request.machine_label.as_deref(), Some("Desktop Host"));
        assert_eq!(request.name.as_deref(), Some("Runtime"));
    }

    #[test]
    fn desktop_start_request_from_profile_does_not_reuse_persisted_access_token() {
        let request = DesktopRuntimeStartRequest::from(profile());

        assert_eq!(request.access_token, "");
    }

    #[test]
    fn desktop_profile_origin_uses_shell_supplied_origin() {
        let normalized = normalize_desktop_runtime_profile_with_identity(
            profile(),
            identity(),
            Some(" http://10.22.71.7:8080/ "),
        );

        assert_eq!(normalized.server_url, "http://10.22.71.7:8080");
    }

    #[test]
    fn desktop_profile_origin_trims_input_when_no_shell_origin_is_supplied() {
        let normalized =
            normalize_desktop_runtime_profile_with_identity(profile(), identity(), None);

        assert_eq!(normalized.server_url, "http://old.example:3001");
    }
}
