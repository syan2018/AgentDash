use std::path::PathBuf;

use serde::Deserialize;

pub const LOCAL_BACKEND_CONFIG_FILENAME: &str = "local-backend.json";
const LEGACY_MCP_CONFIG_FILENAME: &str = "mcp-servers.json";
const LEGACY_WORKSPACE_CONFIG_FILENAME: &str = "workspace-runtime.json";

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocalBackendConfigFile {
    #[serde(default)]
    pub mcp_servers: Vec<McpLocalServerEntry>,
    #[serde(default)]
    pub workspace_contract: WorkspaceContractRuntimeConfig,
}

#[derive(Debug, Clone, Deserialize)]
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

impl Default for WorkspaceContractRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prepare_on_first_prompt: false,
            git: Default::default(),
            p4: Default::default(),
        }
    }
}

impl WorkspaceContractRuntimeConfig {
    pub fn is_disabled(&self) -> bool {
        !self.enabled
            && !self.prepare_on_first_prompt
            && self.git.is_disabled()
            && self.p4.is_disabled()
    }
}

#[derive(Debug, Clone, Deserialize)]
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

impl Default for GitWorkspaceRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_branch_sync: false,
            allow_commit_reset: false,
            default_remote: None,
        }
    }
}

impl GitWorkspaceRuntimeConfig {
    pub fn is_disabled(&self) -> bool {
        !self.enabled
            && !self.allow_branch_sync
            && !self.allow_commit_reset
            && self.default_remote.is_none()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct P4WorkspaceRuntimeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub force_sync: bool,
}

impl Default for P4WorkspaceRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            force_sync: false,
        }
    }
}

impl P4WorkspaceRuntimeConfig {
    pub fn is_disabled(&self) -> bool {
        !self.enabled && !self.force_sync
    }
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct McpEnvEntry {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LegacyMcpConfigFile {
    #[serde(default)]
    pub servers: Vec<McpLocalServerEntry>,
}

pub fn load_local_backend_config(accessible_roots: &[PathBuf]) -> LocalBackendConfigFile {
    let Some(root) = accessible_roots.first() else {
        return LocalBackendConfigFile::default();
    };

    let config_path = root.join(".agentdash").join(LOCAL_BACKEND_CONFIG_FILENAME);
    let mut config = if !config_path.exists() {
        tracing::debug!(
            path = %config_path.display(),
            "Local backend 配置文件不存在，先使用默认配置，再尝试兼容旧配置文件"
        );
        LocalBackendConfigFile::default()
    } else {
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
                        "Local backend 配置解析失败，使用默认配置并继续尝试兼容旧配置文件"
                    );
                    LocalBackendConfigFile::default()
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    path = %config_path.display(),
                    "读取 local backend 配置失败，使用默认配置并继续尝试兼容旧配置文件"
                );
                LocalBackendConfigFile::default()
            }
        }
    };

    apply_legacy_fallbacks(root, &mut config);
    config
}

fn apply_legacy_fallbacks(root: &std::path::Path, config: &mut LocalBackendConfigFile) {
    if config.mcp_servers.is_empty() {
        let legacy_mcp_path = root.join(".agentdash").join(LEGACY_MCP_CONFIG_FILENAME);
        if let Some(legacy) = read_json_file::<LegacyMcpConfigFile>(&legacy_mcp_path) {
            if !legacy.servers.is_empty() {
                tracing::warn!(
                    path = %legacy_mcp_path.display(),
                    "检测到旧版 MCP 配置文件，已临时并入 local-backend.json 运行时配置；建议尽快迁移"
                );
                config.mcp_servers = legacy.servers;
            }
        }
    }

    if config.workspace_contract.is_disabled() {
        let legacy_workspace_path = root.join(".agentdash").join(LEGACY_WORKSPACE_CONFIG_FILENAME);
        if let Some(legacy) = read_json_file::<WorkspaceContractRuntimeConfig>(&legacy_workspace_path)
            && !legacy.is_disabled()
        {
            tracing::warn!(
                path = %legacy_workspace_path.display(),
                "检测到旧版 workspace runtime 配置文件，已临时并入 local-backend.json 运行时配置；建议尽快迁移"
            );
            config.workspace_contract = legacy;
        }
    }
}

fn read_json_file<T>(path: &std::path::Path) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<T>(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
    fn legacy_mcp_config_is_merged_when_unified_config_missing() {
        let dir = tempdir().expect("tempdir");
        let agentdash_dir = dir.path().join(".agentdash");
        std::fs::create_dir_all(&agentdash_dir).expect("create config dir");
        std::fs::write(
            agentdash_dir.join(LEGACY_MCP_CONFIG_FILENAME),
            r#"{
  "servers": [
    {
      "name": "demo-mcp",
      "transport": "stdio",
      "command": "node",
      "args": ["server.js"]
    }
  ]
}"#,
        )
        .expect("write legacy mcp config");

        let config = load_local_backend_config(&[dir.path().to_path_buf()]);
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers[0].name, "demo-mcp");
    }

    #[test]
    fn legacy_workspace_config_is_merged_when_unified_config_missing() {
        let dir = tempdir().expect("tempdir");
        let agentdash_dir = dir.path().join(".agentdash");
        std::fs::create_dir_all(&agentdash_dir).expect("create config dir");
        std::fs::write(
            agentdash_dir.join(LEGACY_WORKSPACE_CONFIG_FILENAME),
            r#"{
  "enabled": true,
  "prepare_on_first_prompt": true,
  "p4": {
    "enabled": true,
    "force_sync": true
  }
}"#,
        )
        .expect("write legacy workspace config");

        let config = load_local_backend_config(&[dir.path().to_path_buf()]);
        assert!(config.workspace_contract.enabled);
        assert!(config.workspace_contract.prepare_on_first_prompt);
        assert!(config.workspace_contract.p4.enabled);
        assert!(config.workspace_contract.p4.force_sync);
    }
}
