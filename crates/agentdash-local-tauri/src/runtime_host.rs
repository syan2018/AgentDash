use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_local::{
    DesktopEnsureRetryEvent, DesktopEnsureRetryPolicy, DesktopRunnerHost,
    DesktopRuntimeStartRequest as RuntimeStartRequest, LocalRuntimeSnapshot,
    ensure_desktop_runtime_config, load_desktop_app_settings,
    load_desktop_runtime_profile_with_server_origin,
    normalize_desktop_runtime_start_request_with_server_origin, redact_secret,
};

use crate::desktop_api::desktop_runtime_server_origin;
use crate::desktop_update::{ensure_desktop_update_allows_runtime, refresh_desktop_update_policy};
use crate::state::DesktopState;

pub(crate) async fn start_runtime_from_request(
    state: &DesktopState,
    request: RuntimeStartRequest,
    retry_until_server_ready: bool,
) -> anyhow::Result<LocalRuntimeSnapshot> {
    ensure_desktop_update_allows_runtime(state).await?;
    let request = normalize_desktop_runtime_start_request_with_server_origin(
        request,
        &desktop_runtime_server_origin(),
    )?;
    let runtime_for_claim = state.runtime.clone();
    state
        .runtime
        .ensure_started_with(|| async move {
            let policy = if retry_until_server_ready {
                DesktopEnsureRetryPolicy::wait_for_server_ready()
            } else {
                DesktopEnsureRetryPolicy::single_attempt()
            };
            ensure_desktop_runtime_config(request, policy, |event| {
                let runtime = runtime_for_claim.clone();
                async move {
                    record_desktop_ensure_retry(&runtime, event).await;
                }
            })
            .await
            .map_err(anyhow::Error::from)
        })
        .await
}

async fn record_desktop_ensure_retry(runtime: &DesktopRunnerHost, event: DesktopEnsureRetryEvent) {
    runtime
        .mark_waiting_for_api(
            "Dashboard API 暂不可用，等待后继续领取本机 runtime",
            Some(event.error.clone()),
            Some(event.attempt),
            Some(event.next_retry_at.clone()),
        )
        .await;
    let error = redact_secret(&event.error);
    let context = DiagnosticErrorContext::new("desktop.runtime.ensure", "wait_for_api_ready");
    diag_error!(
        Warn,
        Subsystem::Api,
        context = &context,
        error = &error,
        attempt = event.attempt,
        retry_count = event.attempt.saturating_sub(1),
        "领取本机 runtime 失败，等待 server 就绪后重试"
    );
}

pub(crate) fn initialize_desktop_runner_host(state: DesktopState) {
    tauri::async_runtime::spawn(async move {
        let update_policy = refresh_desktop_update_policy(&state).await;
        if update_policy.force_update_required() {
            return;
        }

        let settings = match load_desktop_app_settings() {
            Ok(settings) => settings,
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("desktop.runtime.initialize", "load_settings");
                diag_error!(
                    Error,
                    Subsystem::Infra,
                    context = &context,
                    error = &error,
                    "读取桌面设置失败，无法判断 runtime 自动连接策略"
                );
                state
                    .runtime
                    .mark_error(
                        "读取桌面设置失败，无法判断 runtime 自动连接策略",
                        error.to_string(),
                    )
                    .await;
                return;
            }
        };

        if !settings.auto_connect_local_runtime {
            state
                .runtime
                .mark_disabled("桌面设置已关闭启动后自动连接 runtime")
                .await;
            return;
        }

        let profile =
            match load_desktop_runtime_profile_with_server_origin(&desktop_runtime_server_origin())
            {
                Ok(Some(profile)) => profile,
                Ok(None) => {
                    state
                        .runtime
                        .mark_idle("等待登录后创建桌面本机 runtime profile")
                        .await;
                    return;
                }
                Err(error) => {
                    let context =
                        DiagnosticErrorContext::new("desktop.runtime.initialize", "load_profile");
                    diag_error!(
                        Error,
                        Subsystem::Infra,
                        context = &context,
                        error = &error,
                        "读取桌面本机 runtime profile 失败"
                    );
                    state
                        .runtime
                        .mark_error("读取桌面本机 runtime profile 失败", error.to_string())
                        .await;
                    return;
                }
            };

        if !profile.auto_start {
            state
                .runtime
                .mark_idle("profile 未开启自动启动，等待登录桥接或手动启动")
                .await;
            return;
        }

        if let Err(error) =
            start_runtime_from_request(&state, RuntimeStartRequest::from(profile), true).await
        {
            let context = DiagnosticErrorContext::new("desktop.runtime.initialize", "auto_start");
            diag_error!(
                Warn,
                Subsystem::Infra,
                context = &context,
                error = &error,
                "桌面本机 runtime 自动启动未完成"
            );
        }
    });
}

pub(crate) async fn start_runtime_from_profile(state: DesktopState) {
    let profile =
        match load_desktop_runtime_profile_with_server_origin(&desktop_runtime_server_origin()) {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                state
                    .runtime
                    .record_log(
                        "warn",
                        "profile",
                        "未配置本机 runtime profile，无法从托盘启动 runtime",
                    )
                    .await;
                return;
            }
            Err(error) => {
                state
                    .runtime
                    .record_log(
                        "error",
                        "profile",
                        format!("托盘加载本机 runtime profile 失败: {error}"),
                    )
                    .await;
                return;
            }
        };

    match start_runtime_from_request(&state, RuntimeStartRequest::from(profile), false).await {
        Ok(snapshot) => {
            state
                .runtime
                .record_log(
                    "info",
                    "runtime",
                    format!("托盘已启动本机 runtime: backend={}", snapshot.backend_id),
                )
                .await;
        }
        Err(error) => {
            state
                .runtime
                .record_log(
                    "error",
                    "runtime",
                    format!("托盘启动本机 runtime 失败: {error}"),
                )
                .await;
        }
    }
}
