use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub const SERVICE_NAME: &str = "AgentDashLocalRunner";
#[cfg(not(windows))]
pub const SERVICE_NAME: &str = "agentdash-local-runner";

#[cfg(windows)]
pub const SERVICE_DISPLAY_NAME: &str = "AgentDash Local Runner";
#[cfg(not(windows))]
pub const SERVICE_DISPLAY_NAME: &str = "AgentDash Local Runner";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServiceCommandResult {
    pub service_name: String,
    pub supported: bool,
    pub state: String,
    pub message: String,
    pub unit: Option<String>,
    pub commands: Vec<String>,
}

pub fn service_status() -> ServiceCommandResult {
    ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: false,
        state: "unknown".to_string(),
        message: "当前切片仅提供 service CLI 形状和状态占位，尚未执行 OS service 查询".to_string(),
        unit: None,
        commands: service_status_commands(),
    }
}

pub fn service_install_plan(config_path: &Path, exe_path: &Path) -> ServiceCommandResult {
    if cfg!(windows) {
        windows_service_plan("install", config_path, exe_path)
    } else {
        linux_service_plan("install", config_path, exe_path)
    }
}

pub fn service_action_plan(
    action: &str,
    config_path: &Path,
    exe_path: &Path,
) -> ServiceCommandResult {
    match action {
        "install" => service_install_plan(config_path, exe_path),
        "uninstall" | "start" | "stop" => {
            let commands = if cfg!(windows) {
                windows_action_commands(action)
            } else {
                linux_action_commands(action)
            };
            ServiceCommandResult {
                service_name: SERVICE_NAME.to_string(),
                supported: false,
                state: "not_executed".to_string(),
                message: format!("service {action} 当前切片仅生成命令计划，尚未执行系统服务管理"),
                unit: None,
                commands,
            }
        }
        "status" => service_status(),
        _ => ServiceCommandResult {
            service_name: SERVICE_NAME.to_string(),
            supported: false,
            state: "invalid_action".to_string(),
            message: format!("未知 service action: {action}"),
            unit: None,
            commands: Vec::new(),
        },
    }
}

fn linux_service_plan(action: &str, config_path: &Path, exe_path: &Path) -> ServiceCommandResult {
    let unit = systemd_unit(config_path, exe_path);
    ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: false,
        state: "not_installed".to_string(),
        message: format!(
            "service {action} 当前切片仅生成 systemd unit，不写入 /etc/systemd/system"
        ),
        unit: Some(unit),
        commands: linux_action_commands(action),
    }
}

fn windows_service_plan(action: &str, config_path: &Path, exe_path: &Path) -> ServiceCommandResult {
    ServiceCommandResult {
        service_name: SERVICE_NAME.to_string(),
        supported: false,
        state: "not_installed".to_string(),
        message: format!(
            "service {action} 当前切片仅生成 Windows Service 命令计划；native service dispatcher 尚未实现"
        ),
        unit: None,
        commands: vec![format!(
            "sc.exe create {SERVICE_NAME} binPath= \"{} service run --config {}\" DisplayName= \"{SERVICE_DISPLAY_NAME}\" start= auto",
            display_path(exe_path),
            display_path(config_path)
        )],
    }
}

fn systemd_unit(config_path: &Path, exe_path: &Path) -> String {
    format!(
        "[Unit]\nDescription=AgentDash Local Runner\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} run --config {}\nRestart=always\nRestartSec=5s\n\n[Install]\nWantedBy=multi-user.target\n",
        display_path(exe_path),
        display_path(config_path)
    )
}

fn linux_action_commands(action: &str) -> Vec<String> {
    match action {
        "install" => vec![
            format!("install generated unit to /etc/systemd/system/{SERVICE_NAME}.service"),
            "systemctl daemon-reload".to_string(),
            format!("systemctl enable {SERVICE_NAME}"),
        ],
        "uninstall" => vec![
            format!("systemctl stop {SERVICE_NAME}"),
            format!("systemctl disable {SERVICE_NAME}"),
            format!("rm /etc/systemd/system/{SERVICE_NAME}.service"),
            "systemctl daemon-reload".to_string(),
        ],
        "start" => vec![format!("systemctl start {SERVICE_NAME}")],
        "stop" => vec![format!("systemctl stop {SERVICE_NAME}")],
        _ => Vec::new(),
    }
}

fn windows_action_commands(action: &str) -> Vec<String> {
    match action {
        "uninstall" => vec![
            format!("sc.exe stop {SERVICE_NAME}"),
            format!("sc.exe delete {SERVICE_NAME}"),
        ],
        "start" => vec![format!("sc.exe start {SERVICE_NAME}")],
        "stop" => vec![format!("sc.exe stop {SERVICE_NAME}")],
        _ => Vec::new(),
    }
}

fn service_status_commands() -> Vec<String> {
    if cfg!(windows) {
        vec![format!("sc.exe query {SERVICE_NAME}")]
    } else {
        vec![format!("systemctl status {SERVICE_NAME}")]
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn current_exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agentdash-local"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_unit_uses_run_command_and_config_path() {
        let unit = systemd_unit(
            Path::new("/etc/agentdash/runner.toml"),
            Path::new("/usr/bin/agentdash-local"),
        );

        assert!(unit.contains(
            "ExecStart=/usr/bin/agentdash-local run --config /etc/agentdash/runner.toml"
        ));
        assert!(unit.contains("Restart=always"));
    }

    #[test]
    fn service_status_is_explicit_stub() {
        let status = service_status();

        assert!(!status.supported);
        assert_eq!(status.state, "unknown");
    }
}
