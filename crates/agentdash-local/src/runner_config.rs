use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::LocalRuntimeConfig;
use crate::runtime::canonicalize_workspace_roots;

const DEFAULT_RUNNER_NAME: &str = "agentdash-runner";

pub const ENV_CONFIG: &str = "AGENTDASH_RUNNER_CONFIG";
pub const ENV_SERVER_URL: &str = "AGENTDASH_RUNNER_SERVER_URL";
pub const ENV_REGISTRATION_TOKEN: &str = "AGENTDASH_RUNNER_REGISTRATION_TOKEN";
pub const ENV_BACKEND_ID: &str = "AGENTDASH_RUNNER_BACKEND_ID";
pub const ENV_RELAY_WS_URL: &str = "AGENTDASH_RUNNER_RELAY_WS_URL";
pub const ENV_AUTH_TOKEN: &str = "AGENTDASH_RUNNER_AUTH_TOKEN";
pub const ENV_NAME: &str = "AGENTDASH_RUNNER_NAME";
pub const ENV_WORKSPACE_ROOTS: &str = "AGENTDASH_RUNNER_WORKSPACE_ROOTS";
pub const ENV_EXECUTOR_ENABLED: &str = "AGENTDASH_RUNNER_EXECUTOR_ENABLED";
pub const ENV_LOG_PATH: &str = "AGENTDASH_RUNNER_LOG_PATH";
pub const ENV_STATE_DIR: &str = "AGENTDASH_RUNNER_STATE_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Cli,
    Env,
    File,
    Default,
    Missing,
}

impl ConfigSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::File => "file",
            Self::Default => "default",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnerCliOverrides {
    pub config_path: Option<PathBuf>,
    pub server_url: Option<String>,
    pub registration_token: Option<String>,
    pub backend_id: Option<String>,
    pub relay_ws_url: Option<String>,
    pub auth_token: Option<String>,
    pub runner_name: Option<String>,
    pub workspace_roots: Option<Vec<PathBuf>>,
    pub executor_enabled: Option<bool>,
    pub log_path: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RunnerConfigFile {
    #[serde(default)]
    pub runner: RunnerSection,
    #[serde(default)]
    pub registration: RegistrationSection,
    #[serde(default)]
    pub credentials: RunnerCredentials,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RunnerSection {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub server_url: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    #[serde(default)]
    pub executor_enabled: Option<bool>,
    #[serde(default)]
    pub log_path: Option<PathBuf>,
    #[serde(default)]
    pub state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RegistrationSection {
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct RunnerCredentials {
    #[serde(default)]
    pub backend_id: Option<String>,
    #[serde(default)]
    pub relay_ws_url: Option<String>,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub claimed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub token_source: Option<String>,
}

impl RunnerCredentials {
    pub fn is_complete(&self) -> bool {
        self.backend_id.as_deref().is_some_and(non_empty)
            && self.relay_ws_url.as_deref().is_some_and(non_empty)
            && self.auth_token.as_deref().is_some_and(non_empty)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedRunnerConfig {
    pub config_path: PathBuf,
    pub server_url: Option<String>,
    pub registration_token: Option<String>,
    pub credentials: RunnerCredentials,
    pub runner_name: String,
    pub workspace_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
    pub log_path: PathBuf,
    pub state_dir: PathBuf,
    pub sources: BTreeMap<String, String>,
}

impl ResolvedRunnerConfig {
    pub fn credential_state(&self) -> &'static str {
        if self.credentials.is_complete() {
            "ready"
        } else if self.registration_token.as_deref().is_some_and(non_empty) {
            "needs_claim"
        } else {
            "missing"
        }
    }

    pub fn registration_source(&self) -> Option<String> {
        self.credentials.token_source.clone().or_else(|| {
            self.registration_token
                .as_ref()
                .map(|_| "config_or_env".to_string())
        })
    }

    pub fn runtime_config(&self) -> anyhow::Result<LocalRuntimeConfig> {
        if !self.credentials.is_complete() {
            anyhow::bail!("runner credentials 不完整，无法启动 relay");
        }

        Ok(LocalRuntimeConfig::new(
            self.credentials
                .relay_ws_url
                .clone()
                .expect("checked by is_complete"),
            self.credentials
                .auth_token
                .clone()
                .expect("checked by is_complete"),
            self.credentials
                .backend_id
                .clone()
                .expect("checked by is_complete"),
            self.runner_name.clone(),
            self.workspace_roots.clone(),
            self.executor_enabled,
        ))
    }

    pub fn apply_credentials(&mut self, credentials: RunnerCredentials) {
        self.runner_name = self.runner_name.trim().to_string();
        self.credentials = credentials;
        self.sources.insert(
            "credentials".to_string(),
            ConfigSource::File.as_str().to_string(),
        );
    }
}

pub fn resolve_runner_config(
    overrides: RunnerCliOverrides,
) -> anyhow::Result<ResolvedRunnerConfig> {
    let default_paths = RunnerDefaultPaths::current();
    let env = RunnerEnv::read();
    let config_path = first_path(
        overrides.config_path.clone(),
        env.config_path.clone(),
        None,
        Some(default_paths.config_path.clone()),
    )
    .expect("default config path is always present")
    .0;

    let file_config = read_config_file(&config_path)?;
    let mut sources = BTreeMap::new();
    sources.insert(
        "config_path".to_string(),
        source_for_path(
            overrides.config_path.as_ref(),
            env.config_path.as_ref(),
            Some(&default_paths.config_path),
        )
        .as_str()
        .to_string(),
    );

    let server_url = first_string(
        overrides.server_url,
        env.server_url,
        file_config.runner.server_url,
        None,
    );
    sources.insert("server_url".to_string(), server_url.1.as_str().to_string());

    let registration_token = first_string(
        overrides.registration_token,
        env.registration_token,
        file_config.registration.token,
        None,
    );
    sources.insert(
        "registration_token".to_string(),
        registration_token.1.as_str().to_string(),
    );

    let backend_id = first_string(
        overrides.backend_id,
        env.backend_id,
        file_config.credentials.backend_id,
        None,
    );
    sources.insert("backend_id".to_string(), backend_id.1.as_str().to_string());

    let relay_ws_url = first_string(
        overrides.relay_ws_url,
        env.relay_ws_url,
        file_config.credentials.relay_ws_url,
        None,
    );
    sources.insert(
        "relay_ws_url".to_string(),
        relay_ws_url.1.as_str().to_string(),
    );

    let auth_token = first_string(
        overrides.auth_token,
        env.auth_token,
        file_config.credentials.auth_token,
        None,
    );
    sources.insert("auth_token".to_string(), auth_token.1.as_str().to_string());

    let runner_name = first_string(
        overrides.runner_name,
        env.runner_name,
        file_config.runner.name,
        Some(DEFAULT_RUNNER_NAME.to_string()),
    );
    sources.insert(
        "runner_name".to_string(),
        runner_name.1.as_str().to_string(),
    );

    let workspace_roots = first_vec(
        overrides.workspace_roots,
        env.workspace_roots,
        Some(file_config.runner.workspace_roots),
        Some(Vec::new()),
    );
    sources.insert(
        "workspace_roots".to_string(),
        workspace_roots.1.as_str().to_string(),
    );

    let executor_enabled = first_bool(
        overrides.executor_enabled,
        env.executor_enabled,
        file_config.runner.executor_enabled,
        Some(true),
    );
    sources.insert(
        "executor_enabled".to_string(),
        executor_enabled.1.as_str().to_string(),
    );

    let log_path = first_path(
        overrides.log_path,
        env.log_path,
        file_config.runner.log_path,
        Some(default_paths.log_path),
    )
    .expect("default log path is always present");
    sources.insert("log_path".to_string(), log_path.1.as_str().to_string());

    let state_dir = first_path(
        overrides.state_dir,
        env.state_dir,
        file_config.runner.state_dir,
        Some(default_paths.state_dir),
    )
    .expect("default state dir is always present");
    sources.insert("state_dir".to_string(), state_dir.1.as_str().to_string());

    Ok(ResolvedRunnerConfig {
        config_path,
        server_url: server_url.0,
        registration_token: registration_token.0,
        credentials: RunnerCredentials {
            backend_id: backend_id.0,
            relay_ws_url: relay_ws_url.0,
            auth_token: auth_token.0,
            claimed_at: file_config.credentials.claimed_at,
            token_source: file_config.credentials.token_source,
        },
        runner_name: runner_name
            .0
            .unwrap_or_else(|| DEFAULT_RUNNER_NAME.to_string()),
        workspace_roots: canonicalize_workspace_roots(workspace_roots.0.unwrap_or_default()),
        executor_enabled: executor_enabled.0.unwrap_or(true),
        log_path: log_path.0,
        state_dir: state_dir.0,
        sources,
    })
}

pub fn read_config_file(path: &Path) -> anyhow::Result<RunnerConfigFile> {
    if !path.exists() {
        return Ok(RunnerConfigFile::default());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("读取 runner 配置失败: {}", path.display()))?;
    toml::from_str(&content)
        .with_context(|| format!("解析 runner TOML 配置失败: {}", path.display()))
}

pub fn persist_credentials(
    config_path: &Path,
    credentials: RunnerCredentials,
) -> anyhow::Result<()> {
    let mut config = read_config_file(config_path)?;
    config.credentials = credentials;
    atomic_write_config(config_path, &config)
}

fn atomic_write_config(path: &Path, config: &RunnerConfigFile) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 runner 配置目录失败: {}", parent.display()))?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("runner.toml"),
        uuid::Uuid::new_v4()
    ));
    let content = toml::to_string_pretty(config)?;
    {
        let mut file = std::fs::File::create(&temp_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    match std::fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(error) if cfg!(windows) && path.exists() => {
            std::fs::remove_file(path)?;
            std::fs::rename(&temp_path, path).map_err(|_| error.into())
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerDefaultPaths {
    pub config_path: PathBuf,
    pub state_dir: PathBuf,
    pub log_path: PathBuf,
}

impl RunnerDefaultPaths {
    pub fn current() -> Self {
        if cfg!(windows) {
            let base = std::env::var("PROGRAMDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(r"C:\ProgramData"))
                .join("AgentDash")
                .join("runner");
            Self {
                config_path: base.join("config.toml"),
                state_dir: base.clone(),
                log_path: base.join("runner.log"),
            }
        } else {
            Self {
                config_path: PathBuf::from("/etc/agentdash/runner.toml"),
                state_dir: PathBuf::from("/var/lib/agentdash/runner"),
                log_path: PathBuf::from("/var/log/agentdash/runner.log"),
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RunnerEnv {
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    registration_token: Option<String>,
    backend_id: Option<String>,
    relay_ws_url: Option<String>,
    auth_token: Option<String>,
    runner_name: Option<String>,
    workspace_roots: Option<Vec<PathBuf>>,
    executor_enabled: Option<bool>,
    log_path: Option<PathBuf>,
    state_dir: Option<PathBuf>,
}

impl RunnerEnv {
    fn read() -> Self {
        Self {
            config_path: env_path(ENV_CONFIG),
            server_url: env_string(ENV_SERVER_URL),
            registration_token: env_string(ENV_REGISTRATION_TOKEN),
            backend_id: env_string(ENV_BACKEND_ID),
            relay_ws_url: env_string(ENV_RELAY_WS_URL),
            auth_token: env_string(ENV_AUTH_TOKEN),
            runner_name: env_string(ENV_NAME),
            workspace_roots: env_string(ENV_WORKSPACE_ROOTS).map(parse_workspace_roots),
            executor_enabled: env_string(ENV_EXECUTOR_ENABLED).and_then(|value| parse_bool(&value)),
            log_path: env_path(ENV_LOG_PATH),
            state_dir: env_path(ENV_STATE_DIR),
        }
    }
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_path(key: &str) -> Option<PathBuf> {
    env_string(key).map(PathBuf::from)
}

fn parse_workspace_roots(value: String) -> Vec<PathBuf> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn first_string(
    cli: Option<String>,
    env: Option<String>,
    file: Option<String>,
    default: Option<String>,
) -> (Option<String>, ConfigSource) {
    if let Some(value) = normalize_string(cli) {
        return (Some(value), ConfigSource::Cli);
    }
    if let Some(value) = normalize_string(env) {
        return (Some(value), ConfigSource::Env);
    }
    if let Some(value) = normalize_string(file) {
        return (Some(value), ConfigSource::File);
    }
    if let Some(value) = normalize_string(default) {
        return (Some(value), ConfigSource::Default);
    }
    (None, ConfigSource::Missing)
}

fn first_bool(
    cli: Option<bool>,
    env: Option<bool>,
    file: Option<bool>,
    default: Option<bool>,
) -> (Option<bool>, ConfigSource) {
    if cli.is_some() {
        return (cli, ConfigSource::Cli);
    }
    if env.is_some() {
        return (env, ConfigSource::Env);
    }
    if file.is_some() {
        return (file, ConfigSource::File);
    }
    if default.is_some() {
        return (default, ConfigSource::Default);
    }
    (None, ConfigSource::Missing)
}

fn first_vec<T>(
    cli: Option<Vec<T>>,
    env: Option<Vec<T>>,
    file: Option<Vec<T>>,
    default: Option<Vec<T>>,
) -> (Option<Vec<T>>, ConfigSource) {
    if let Some(value) = cli {
        return (Some(value), ConfigSource::Cli);
    }
    if let Some(value) = env {
        return (Some(value), ConfigSource::Env);
    }
    if let Some(value) = file {
        if !value.is_empty() {
            return (Some(value), ConfigSource::File);
        }
    }
    if let Some(value) = default {
        return (Some(value), ConfigSource::Default);
    }
    (None, ConfigSource::Missing)
}

fn first_path(
    cli: Option<PathBuf>,
    env: Option<PathBuf>,
    file: Option<PathBuf>,
    default: Option<PathBuf>,
) -> Option<(PathBuf, ConfigSource)> {
    if let Some(value) = cli {
        return Some((value, ConfigSource::Cli));
    }
    if let Some(value) = env {
        return Some((value, ConfigSource::Env));
    }
    if let Some(value) = file {
        return Some((value, ConfigSource::File));
    }
    default.map(|value| (value, ConfigSource::Default))
}

fn source_for_path(
    cli: Option<&PathBuf>,
    env: Option<&PathBuf>,
    default: Option<&PathBuf>,
) -> ConfigSource {
    if cli.is_some() {
        ConfigSource::Cli
    } else if env.is_some() {
        ConfigSource::Env
    } else if default.is_some() {
        ConfigSource::Default
    } else {
        ConfigSource::Missing
    }
}

fn normalize_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_merge_prefers_cli_over_file_defaults() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("runner.toml");
        std::fs::write(
            &config_path,
            r#"
[runner]
name = "file-runner"
server_url = "https://file.example"
workspace_roots = []
executor_enabled = false

[registration]
token = "file-token"

[credentials]
backend_id = "file-backend"
relay_ws_url = "wss://file.example/ws/backend"
auth_token = "file-auth"
"#,
        )
        .expect("write config");

        let resolved = resolve_runner_config(RunnerCliOverrides {
            config_path: Some(config_path),
            server_url: Some("https://cli.example".to_string()),
            runner_name: Some("cli-runner".to_string()),
            executor_enabled: Some(true),
            ..Default::default()
        })
        .expect("resolve");

        assert_eq!(resolved.server_url.as_deref(), Some("https://cli.example"));
        assert_eq!(resolved.runner_name, "cli-runner");
        assert!(resolved.executor_enabled);
        assert_eq!(
            resolved.credentials.backend_id.as_deref(),
            Some("file-backend")
        );
        assert_eq!(resolved.sources["server_url"], "cli");
        assert_eq!(resolved.sources["backend_id"], "file");
    }

    #[test]
    fn credentials_detect_incomplete_state() {
        let credentials = RunnerCredentials {
            backend_id: Some("backend".to_string()),
            relay_ws_url: Some("wss://example/ws/backend".to_string()),
            auth_token: None,
            ..Default::default()
        };

        assert!(!credentials.is_complete());
    }

    #[test]
    fn credential_write_round_trips_toml() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("runner.toml");
        persist_credentials(
            &config_path,
            RunnerCredentials {
                backend_id: Some("backend-1".to_string()),
                relay_ws_url: Some("wss://example/ws/backend".to_string()),
                auth_token: Some("relay-secret".to_string()),
                claimed_at: Some(Utc::now()),
                token_source: Some("runner_registration_token".to_string()),
            },
        )
        .expect("persist credentials");

        let loaded = read_config_file(&config_path).expect("read back");
        assert_eq!(loaded.credentials.backend_id.as_deref(), Some("backend-1"));
        assert!(loaded.credentials.is_complete());
    }
}
