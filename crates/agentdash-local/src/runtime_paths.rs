use std::path::PathBuf;

const APP_DIR_NAME: &str = "AgentDash";
const LOCAL_RUNTIME_DIR_NAME: &str = "local-runtime";
const LOCAL_RUNTIME_CONFIG_DIR_NAME: &str = "config";
const LOCAL_RUNTIME_PROFILE_FILE: &str = "local-runtime-profile.json";
const DESKTOP_APP_SETTINGS_FILE: &str = "desktop-app-settings.json";
const LOCAL_MCP_SERVERS_FILE: &str = "local-mcp-servers.json";
const MACHINE_IDENTITY_FILE: &str = "machine-identity.json";

pub fn local_runtime_data_dir() -> anyhow::Result<PathBuf> {
    if cfg!(windows)
        && let Some(value) = non_empty_env("APPDATA").or_else(|| non_empty_env("LOCALAPPDATA"))
    {
        return Ok(PathBuf::from(value)
            .join(APP_DIR_NAME)
            .join(LOCAL_RUNTIME_DIR_NAME));
    }

    if cfg!(target_os = "macos")
        && let Some(home) = non_empty_env("HOME")
    {
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(APP_DIR_NAME)
            .join(LOCAL_RUNTIME_DIR_NAME));
    }

    if let Some(value) = non_empty_env("XDG_DATA_HOME") {
        return Ok(PathBuf::from(value)
            .join("agentdash")
            .join(LOCAL_RUNTIME_DIR_NAME));
    }
    if let Some(home) = non_empty_env("HOME") {
        return Ok(PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("agentdash")
            .join(LOCAL_RUNTIME_DIR_NAME));
    }

    anyhow::bail!("无法定位本机 runtime 数据目录");
}

pub fn local_runtime_config_dir() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_data_dir()?.join(LOCAL_RUNTIME_CONFIG_DIR_NAME))
}

pub fn local_runtime_profile_path() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_config_dir()?.join(LOCAL_RUNTIME_PROFILE_FILE))
}

pub fn desktop_app_settings_path() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_config_dir()?.join(DESKTOP_APP_SETTINGS_FILE))
}

pub fn local_mcp_servers_path() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_config_dir()?.join(LOCAL_MCP_SERVERS_FILE))
}

pub fn machine_identity_path() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_data_dir()?.join(MACHINE_IDENTITY_FILE))
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
