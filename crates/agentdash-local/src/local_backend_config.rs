use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const LOCAL_BACKEND_CONFIG_FILENAME: &str = "local-backend.json";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LocalBackendConfigFile {
    #[serde(default)]
    pub mcp_servers: Vec<McpLocalServerEntry>,
    #[serde(default)]
    pub workspace_contract: WorkspaceContractRuntimeConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorkspaceContractRuntimeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub prepare_on_first_prompt: bool,
    #[serde(default)]
    pub git: GitWorkspaceRuntimeConfig,
    #[serde(default)]
    pub p4: P4WorkspaceRuntimeConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GitWorkspaceRuntimeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allow_branch_sync: bool,
    #[serde(default)]
    pub allow_commit_reset: bool,
    #[serde(default)]
    pub default_remote: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct P4WorkspaceRuntimeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub force_sync: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpLocalServerEntry {
    pub name: String,
    /// "stdio" | "http" | "sse"
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<Vec<McpEnvEntry>>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpEnvEntry {
    pub name: String,
    pub value: String,
}

pub fn load_local_backend_config(accessible_roots: &[PathBuf]) -> LocalBackendConfigFile {
    let Some(root) = accessible_roots.first() else {
        return LocalBackendConfigFile::default();
    };

    load_local_backend_config_for_root(root)
}

pub fn load_local_backend_config_for_root(root: &std::path::Path) -> LocalBackendConfigFile {
    let config_path = local_backend_config_path(root);
    if !config_path.exists() {
        tracing::debug!(
            path = %config_path.display(),
            "Local backend 配置文件不存在，使用默认配置"
        );
        return LocalBackendConfigFile::default();
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<LocalBackendConfigFile>(&content) {
            Ok(config) => {
                tracing::info!(
                    path = %config_path.display(),
                    mcp_server_count = config.mcp_servers.len(),
                    contract_enabled = config.workspace_contract.enabled,
                    prepare_on_first_prompt = config.workspace_contract.prepare_on_first_prompt,
                    "已加载 local backend 配置"
                );
                config
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    path = %config_path.display(),
                    "Local backend 配置解析失败，使用默认配置"
                );
                LocalBackendConfigFile::default()
            }
        },
        Err(error) => {
            tracing::warn!(
                error = %error,
                path = %config_path.display(),
                "读取 local backend 配置失败，使用默认配置"
            );
            LocalBackendConfigFile::default()
        }
    }
}

pub fn save_local_backend_config_for_root(
    root: &std::path::Path,
    config: &LocalBackendConfigFile,
) -> Result<(), anyhow::Error> {
    let config_path = local_backend_config_path(root);
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&config_path, content)?;
    tracing::info!(
        path = %config_path.display(),
        mcp_server_count = config.mcp_servers.len(),
        "已保存 local backend 配置"
    );
    Ok(())
}

pub fn local_backend_config_path(root: &std::path::Path) -> PathBuf {
    root.join(".agentdash").join(LOCAL_BACKEND_CONFIG_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_local_backend_config_is_disabled() {
        let config = LocalBackendConfigFile::default();
        assert!(config.mcp_servers.is_empty());
        assert!(!config.workspace_contract.enabled);
        assert!(!config.workspace_contract.prepare_on_first_prompt);
        assert!(!config.workspace_contract.git.enabled);
        assert!(!config.workspace_contract.p4.enabled);
    }

    #[test]
    fn save_and_load_local_backend_config_round_trips_mcp_servers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = LocalBackendConfigFile {
            mcp_servers: vec![McpLocalServerEntry {
                name: "filesystem".to_string(),
                transport: "stdio".to_string(),
                command: Some("npx".to_string()),
                args: Some(vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    temp.path().to_string_lossy().to_string(),
                ]),
                env: Some(vec![McpEnvEntry {
                    name: "NODE_ENV".to_string(),
                    value: "test".to_string(),
                }]),
                url: None,
            }],
            workspace_contract: WorkspaceContractRuntimeConfig {
                enabled: true,
                prepare_on_first_prompt: true,
                git: GitWorkspaceRuntimeConfig {
                    enabled: true,
                    allow_branch_sync: true,
                    allow_commit_reset: false,
                    default_remote: Some("origin".to_string()),
                },
                p4: P4WorkspaceRuntimeConfig::default(),
            },
        };

        save_local_backend_config_for_root(temp.path(), &config).expect("save config");

        let loaded = load_local_backend_config_for_root(temp.path());
        assert_eq!(loaded.mcp_servers.len(), 1);
        assert_eq!(loaded.mcp_servers[0].name, "filesystem");
        assert_eq!(loaded.mcp_servers[0].transport, "stdio");
        assert_eq!(loaded.mcp_servers[0].command.as_deref(), Some("npx"));
        assert!(loaded.workspace_contract.enabled);
        assert!(loaded.workspace_contract.git.allow_branch_sync);
        assert_eq!(
            loaded.workspace_contract.git.default_remote.as_deref(),
            Some("origin")
        );
    }
}
