use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::path::PathBuf;

use agentdash_domain::mcp_preset::McpTransportConfig;
use serde::{Deserialize, Serialize};

pub const LOCAL_BACKEND_CONFIG_FILENAME: &str = "local-backend.json";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LocalBackendConfigFile {
    #[serde(default)]
    pub mcp_servers: Vec<McpLocalServerEntry>,
    #[serde(default)]
    pub mcp_protect_mode: bool,
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
    pub transport: McpTransportConfig,
}

pub fn load_local_backend_config(workspace_roots: &[PathBuf]) -> LocalBackendConfigFile {
    let Some(root) = workspace_roots.first() else {
        return LocalBackendConfigFile::default();
    };

    load_local_backend_config_for_root(root)
}

pub fn load_local_backend_config_for_root(root: &std::path::Path) -> LocalBackendConfigFile {
    let config_path = local_backend_config_path(root);
    if !config_path.exists() {
        diag!(Debug, Subsystem::Infra,

            path = %config_path.display(),
            "Local backend 配置文件不存在，使用默认配置"
        );
        return LocalBackendConfigFile::default();
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<LocalBackendConfigFile>(&content) {
            Ok(config) => {
                diag!(Info, Subsystem::Infra,

                    path = %config_path.display(),
                    mcp_server_count = config.mcp_servers.len(),
                    mcp_protect_mode = config.mcp_protect_mode,
                    contract_enabled = config.workspace_contract.enabled,
                    prepare_on_first_prompt = config.workspace_contract.prepare_on_first_prompt,
                    "已加载 local backend 配置"
                );
                config
            }
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("local_backend_config.load", "parse_json");
                diag_error!(
                    Warn,
                    Subsystem::Infra,
                    context = &context,
                    error = &error,
                    config_file = LOCAL_BACKEND_CONFIG_FILENAME,
                    "Local backend 配置解析失败，使用默认配置"
                );
                LocalBackendConfigFile::default()
            }
        },
        Err(error) => {
            let context = DiagnosticErrorContext::new("local_backend_config.load", "read_file");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                config_file = LOCAL_BACKEND_CONFIG_FILENAME,
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
    diag!(Info, Subsystem::Infra,

        path = %config_path.display(),
        mcp_server_count = config.mcp_servers.len(),
        mcp_protect_mode = config.mcp_protect_mode,
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
        assert!(!config.mcp_protect_mode);
        assert!(!config.workspace_contract.enabled);
        assert!(!config.workspace_contract.prepare_on_first_prompt);
        assert!(!config.workspace_contract.git.enabled);
        assert!(!config.workspace_contract.p4.enabled);
    }

    #[test]
    fn save_and_load_local_backend_config_round_trips_mcp_servers() {
        use agentdash_domain::mcp_preset::{McpEnvVar, McpTransportConfig};

        let temp = tempfile::tempdir().expect("tempdir");
        let config = LocalBackendConfigFile {
            mcp_servers: vec![McpLocalServerEntry {
                name: "filesystem".to_string(),
                transport: McpTransportConfig::Stdio {
                    command: "npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-filesystem".to_string(),
                        temp.path().to_string_lossy().to_string(),
                    ],
                    env: vec![McpEnvVar {
                        name: "NODE_ENV".to_string(),
                        value: "test".to_string(),
                    }],
                    cwd: Some(temp.path().to_string_lossy().to_string()),
                },
            }],
            mcp_protect_mode: true,
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
        assert!(loaded.mcp_protect_mode);
        assert_eq!(loaded.mcp_servers[0].name, "filesystem");
        assert_eq!(loaded.mcp_servers[0].transport.transport_kind(), "stdio");
        match &loaded.mcp_servers[0].transport {
            McpTransportConfig::Stdio { command, cwd, .. } => {
                assert_eq!(command, "npx");
                assert_eq!(cwd.as_deref(), Some(temp.path().to_string_lossy().as_ref()));
            }
            _ => panic!("expected stdio transport"),
        }
        assert!(loaded.workspace_contract.enabled);
        assert!(loaded.workspace_contract.git.allow_branch_sync);
        assert_eq!(
            loaded.workspace_contract.git.default_remote.as_deref(),
            Some("origin")
        );
    }
}
