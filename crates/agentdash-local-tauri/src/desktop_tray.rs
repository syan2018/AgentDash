use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_local::{DesktopAppSettings, StopReason, load_desktop_app_settings};
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::runtime_host::start_runtime_from_profile;
use crate::state::DesktopState;

pub(crate) const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_MENU_OPEN: &str = "open_agentdash";
const TRAY_MENU_RUNTIME_START: &str = "start_local_runtime";
const TRAY_MENU_RUNTIME_STOP: &str = "stop_local_runtime";
const TRAY_MENU_STATUS: &str = "show_status";
const TRAY_MENU_QUIT: &str = "quit_agentdash";

pub(crate) fn configure_tray(app: &AppHandle, state: DesktopState) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text(TRAY_MENU_OPEN, "打开 AgentDash")
        .separator()
        .text(TRAY_MENU_RUNTIME_START, "启动本机 runtime")
        .text(TRAY_MENU_RUNTIME_STOP, "停止本机 runtime")
        .text(TRAY_MENU_STATUS, "查看状态")
        .separator()
        .text(TRAY_MENU_QUIT, "退出 AgentDash")
        .build()?;

    let mut tray = TrayIconBuilder::with_id("agentdash-main")
        .tooltip("AgentDash")
        .menu(&menu)
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    let state_for_menu = state.clone();
    tray.on_menu_event(move |app, event| {
        handle_tray_menu_event(app.clone(), state_for_menu.clone(), event.id().as_ref());
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            restore_main_window(tray.app_handle());
        }
    })
    .build(app)?;

    Ok(())
}

fn handle_tray_menu_event(app: AppHandle, state: DesktopState, id: &str) {
    match id {
        TRAY_MENU_OPEN => restore_main_window(&app),
        TRAY_MENU_RUNTIME_START => {
            tauri::async_runtime::spawn(async move {
                start_runtime_from_profile(state).await;
            });
        }
        TRAY_MENU_RUNTIME_STOP => {
            tauri::async_runtime::spawn(async move {
                if let Err(error) = state.runtime.stop(StopReason::UserRequested).await {
                    state
                        .runtime
                        .record_log(
                            "error",
                            "runtime",
                            format!("托盘停止本机 runtime 失败: {error}"),
                        )
                        .await;
                }
            });
        }
        TRAY_MENU_STATUS => {
            restore_main_window(&app);
            tauri::async_runtime::spawn(async move {
                record_tray_status(state).await;
            });
        }
        TRAY_MENU_QUIT => {
            tauri::async_runtime::spawn(async move {
                request_desktop_quit(app, state).await;
            });
        }
        _ => {}
    }
}

async fn record_tray_status(state: DesktopState) {
    let api = state.api.snapshot().await;
    let runtime = state.runtime.snapshot().await;
    let runtime_message = runtime
        .map(|snapshot| format!("{:?}", snapshot.state))
        .unwrap_or_else(|| "未启动".to_string());
    state
        .runtime
        .record_log(
            "info",
            "desktop",
            format!(
                "托盘状态查看: desktop_api={}, runtime={}",
                api.state_label(),
                runtime_message
            ),
        )
        .await;
}

pub(crate) async fn request_desktop_quit(app: AppHandle, state: DesktopState) {
    state.request_explicit_quit();
    if let Err(error) = state.runtime.stop(StopReason::UserRequested).await {
        let context = DiagnosticErrorContext::new("desktop.lifecycle.quit", "stop_runtime");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            stop_reason = "user_requested",
            "显式退出前停止本机 runtime 失败"
        );
        state
            .runtime
            .record_log(
                "warn",
                "runtime",
                format!("显式退出前停止本机 runtime 失败: {error}"),
            )
            .await;
    }
    state.api.stop_sidecar();
    app.exit(0);
}

pub(crate) fn apply_startup_window_visibility(app: &AppHandle) {
    let settings = match load_desktop_app_settings() {
        Ok(settings) => settings,
        Err(error) => {
            let context =
                DiagnosticErrorContext::new("desktop.window.startup_visibility", "load_settings");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                "读取桌面端启动窗口设置失败，使用默认显示行为"
            );
            DesktopAppSettings::default()
        }
    };

    if settings.start_minimized_to_tray {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL)
            && let Err(error) = window.hide()
        {
            let context =
                DiagnosticErrorContext::new("desktop.window.startup_visibility", "hide_window");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                window_label = MAIN_WINDOW_LABEL,
                "按启动到托盘设置隐藏主窗口失败"
            );
        }
    } else {
        restore_main_window(app);
    }
}

pub(crate) fn restore_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    if let Err(error) = window.show() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "show_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "显示 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.unminimize() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "unminimize_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "还原 AgentDash 主窗口失败"
        );
    }
    if let Err(error) = window.set_focus() {
        let context = DiagnosticErrorContext::new("desktop.window.restore", "focus_window");
        diag_error!(
            Warn,
            Subsystem::Infra,
            context = &context,
            error = &error,
            window_label = MAIN_WINDOW_LABEL,
            "聚焦 AgentDash 主窗口失败"
        );
    }
}
