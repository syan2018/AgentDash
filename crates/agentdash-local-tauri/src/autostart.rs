use std::path::{Path, PathBuf};

use serde::Serialize;

const DESKTOP_AUTOSTART_ENTRY_NAME: &str = "AgentDash";
#[cfg(target_os = "windows")]
const WINDOWS_AUTOSTART_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DesktopAutostartStatus {
    supported: bool,
    enabled: bool,
    message: Option<String>,
}

impl DesktopAutostartStatus {
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }
}

pub(crate) fn desktop_autostart_is_enabled_internal() -> Result<DesktopAutostartStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let app_exe = current_app_exe_path()?;
        let expected = build_windows_autostart_command(&app_exe)?;
        let stored = windows_autostart_read_value()?;
        let enabled = stored.as_deref() == Some(expected.as_str());
        let message = match stored {
            Some(value) if value != expected => Some(
                "检测到 AgentDash 登录项，但它不指向当前应用可执行文件；请重新启用登录自启动"
                    .to_string(),
            ),
            Some(_) => Some("Windows 登录自启动已启用".to_string()),
            None => Some("Windows 登录自启动未启用".to_string()),
        };
        Ok(DesktopAutostartStatus {
            supported: true,
            enabled,
            message,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(DesktopAutostartStatus {
            supported: false,
            enabled: false,
            message: Some("当前平台不支持 AgentDash 桌面登录自启动".to_string()),
        })
    }
}

pub(crate) fn desktop_autostart_set_enabled_internal(
    enabled: bool,
) -> Result<DesktopAutostartStatus, String> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let app_exe = current_app_exe_path()?;
            let command = build_windows_autostart_command(&app_exe)?;
            windows_autostart_write_value(&command)?;
            Ok(DesktopAutostartStatus {
                supported: true,
                enabled: true,
                message: Some(format!("Windows 登录自启动已指向 {}", app_exe.display())),
            })
        } else {
            windows_autostart_delete_value()?;
            Ok(DesktopAutostartStatus {
                supported: true,
                enabled: false,
                message: Some("Windows 登录自启动已关闭".to_string()),
            })
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(DesktopAutostartStatus {
            supported: false,
            enabled: false,
            message: Some("当前平台不支持 AgentDash 桌面登录自启动".to_string()),
        })
    }
}

fn current_app_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("读取当前应用可执行文件路径失败: {error}"))
}

fn build_windows_autostart_command(app_exe: &Path) -> Result<String, String> {
    let file_name = app_exe
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "应用可执行文件路径缺少文件名".to_string())?;
    if is_setup_exe_name(file_name) {
        return Err(format!(
            "拒绝将安装器写入登录自启动项: {}",
            app_exe.display()
        ));
    }
    let raw = app_exe
        .to_str()
        .ok_or_else(|| "应用可执行文件路径不是有效 Unicode".to_string())?;
    if raw.contains('"') {
        return Err("应用可执行文件路径不能包含双引号".to_string());
    }
    Ok(format!("\"{raw}\""))
}

fn is_setup_exe_name(file_name: &str) -> bool {
    let normalized = file_name.to_ascii_lowercase();
    normalized.ends_with(".exe")
        && (normalized.contains("setup") || normalized.contains("installer"))
}

#[cfg(target_os = "windows")]
fn windows_autostart_read_value() -> Result<Option<String>, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu.open_subkey(WINDOWS_AUTOSTART_RUN_KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 Windows 登录自启动注册表失败: {error}")),
    };
    match run_key.get_value(DESKTOP_AUTOSTART_ENTRY_NAME) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取 AgentDash 登录自启动项失败: {error}")),
    }
}

#[cfg(target_os = "windows")]
fn windows_autostart_write_value(command: &str) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey(WINDOWS_AUTOSTART_RUN_KEY)
        .map_err(|error| format!("打开 Windows 登录自启动注册表失败: {error}"))?;
    run_key
        .set_value(DESKTOP_AUTOSTART_ENTRY_NAME, &command)
        .map_err(|error| format!("写入 AgentDash 登录自启动项失败: {error}"))
}

#[cfg(target_os = "windows")]
fn windows_autostart_delete_value() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu
        .open_subkey_with_flags(WINDOWS_AUTOSTART_RUN_KEY, winreg::enums::KEY_SET_VALUE)
    {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("打开 Windows 登录自启动注册表失败: {error}")),
    };
    match run_key.delete_value(DESKTOP_AUTOSTART_ENTRY_NAME) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除 AgentDash 登录自启动项失败: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_autostart_command_quotes_app_exe_path() {
        let command =
            build_windows_autostart_command(Path::new(r"C:\Program Files\AgentDash\AgentDash.exe"))
                .expect("installed app exe path should form a Run key command");

        assert_eq!(command, r#""C:\Program Files\AgentDash\AgentDash.exe""#);
    }

    #[test]
    fn windows_autostart_command_rejects_setup_exe() {
        let error = build_windows_autostart_command(Path::new(
            r"C:\Users\me\Downloads\AgentDash_0.1.0_x64-setup.exe",
        ))
        .expect_err("login autostart must not point at the installer");

        assert!(error.contains("安装器"));
    }
}
