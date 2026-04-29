use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkspaceRuntimeConfigFile {
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

pub fn load_workspace_runtime_config(
    accessible_roots: &[PathBuf],
) -> WorkspaceRuntimeConfigFile {
    let Some(root) = accessible_roots.first() else {
        return WorkspaceRuntimeConfigFile::default();
    };

    let config_path = root.join(".agentdash").join("workspace-runtime.json");
    if !config_path.exists() {
        tracing::debug!(
            path = %config_path.display(),
            "Workspace runtime 配置文件不存在，使用默认关闭策略"
        );
        return WorkspaceRuntimeConfigFile::default();
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<WorkspaceRuntimeConfigFile>(&content) {
            Ok(config) => {
                tracing::info!(
                    path = %config_path.display(),
                    contract_enabled = config.workspace_contract.enabled,
                    prepare_on_first_prompt = config.workspace_contract.prepare_on_first_prompt,
                    "已加载 workspace runtime 配置"
                );
                config
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    path = %config_path.display(),
                    "Workspace runtime 配置解析失败，使用默认关闭策略"
                );
                WorkspaceRuntimeConfigFile::default()
            }
        },
        Err(error) => {
            tracing::warn!(
                error = %error,
                path = %config_path.display(),
                "读取 workspace runtime 配置失败，使用默认关闭策略"
            );
            WorkspaceRuntimeConfigFile::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runtime_config_is_disabled() {
        let config = WorkspaceRuntimeConfigFile::default();
        assert!(!config.workspace_contract.enabled);
        assert!(!config.workspace_contract.prepare_on_first_prompt);
        assert!(!config.workspace_contract.git.enabled);
        assert!(!config.workspace_contract.p4.enabled);
    }
}
