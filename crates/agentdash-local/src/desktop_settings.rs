use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runtime_paths::desktop_app_settings_path;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DesktopAppSettings {
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub start_minimized_to_tray: bool,
    #[serde(default = "default_auto_connect_local_runtime")]
    pub auto_connect_local_runtime: bool,
}

impl Default for DesktopAppSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized_to_tray: false,
            auto_connect_local_runtime: default_auto_connect_local_runtime(),
        }
    }
}

pub fn load_desktop_app_settings() -> anyhow::Result<DesktopAppSettings> {
    load_desktop_app_settings_at(desktop_app_settings_path()?)
}

pub fn save_desktop_app_settings(
    settings: DesktopAppSettings,
) -> anyhow::Result<DesktopAppSettings> {
    save_desktop_app_settings_at(desktop_app_settings_path()?, settings)
}

pub fn normalize_desktop_app_settings(settings: DesktopAppSettings) -> DesktopAppSettings {
    DesktopAppSettings {
        launch_at_login: settings.launch_at_login,
        start_minimized_to_tray: settings.start_minimized_to_tray,
        auto_connect_local_runtime: settings.auto_connect_local_runtime,
    }
}

fn load_desktop_app_settings_at(path: PathBuf) -> anyhow::Result<DesktopAppSettings> {
    if !path.exists() {
        return Ok(DesktopAppSettings::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let settings = serde_json::from_str(&content)
        .map_err(|error| anyhow::anyhow!("读取桌面端设置失败: {error}"))?;
    Ok(normalize_desktop_app_settings(settings))
}

fn save_desktop_app_settings_at(
    path: PathBuf,
    settings: DesktopAppSettings,
) -> anyhow::Result<DesktopAppSettings> {
    let settings = normalize_desktop_app_settings(settings);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&path, content)?;
    Ok(settings)
}

fn default_auto_connect_local_runtime() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_settings_missing_returns_default() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("desktop-app-settings.json");

        let settings = load_desktop_app_settings_at(path).expect("缺失设置应返回默认值");

        assert!(!settings.launch_at_login);
        assert!(!settings.start_minimized_to_tray);
        assert!(settings.auto_connect_local_runtime);
    }

    #[test]
    fn desktop_settings_save_load_roundtrip() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("nested").join("desktop-app-settings.json");

        let saved = save_desktop_app_settings_at(
            path.clone(),
            DesktopAppSettings {
                launch_at_login: true,
                start_minimized_to_tray: true,
                auto_connect_local_runtime: false,
            },
        )
        .expect("设置应能保存");
        let loaded = load_desktop_app_settings_at(path).expect("设置应能读取");

        assert_eq!(loaded, saved);
        assert!(loaded.launch_at_login);
        assert!(loaded.start_minimized_to_tray);
        assert!(!loaded.auto_connect_local_runtime);
    }

    #[test]
    fn desktop_settings_malformed_file_returns_error() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp.path().join("desktop-app-settings.json");
        std::fs::write(&path, "{").expect("应能写入损坏 JSON");

        let error = load_desktop_app_settings_at(path).expect_err("损坏设置应返回错误");

        assert!(error.to_string().contains("读取桌面端设置失败"));
    }

    #[test]
    fn desktop_settings_save_creates_parent_dir() {
        let temp = tempfile::tempdir().expect("应能创建临时目录");
        let path = temp
            .path()
            .join("new-parent")
            .join("desktop-app-settings.json");

        save_desktop_app_settings_at(path.clone(), DesktopAppSettings::default())
            .expect("设置保存应创建父目录");

        assert!(path.exists());
    }
}
