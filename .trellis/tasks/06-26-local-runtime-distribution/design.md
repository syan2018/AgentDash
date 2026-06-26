# 安装与本机运行形态产品化 - Design

## Architecture

本任务将本机能力拆成两个产品边界：

- Windows Desktop App：Tauri 壳负责窗口、托盘、安装包、自启动与用户设置；桌面壳可启动 `Desktop API` 与本机 Local Runtime。`Desktop API` 是桌面内置前端的 Dashboard API 宿主，默认只监听 `127.0.0.1`。
- Local Runner：`agentdash-local` 的 headless 产品形态，面向服务器托管。Runner 不承载桌面 UI，不启动 Dashboard API，不对外暴露业务 HTTP API；它通过 WebSocket 出站连接云端 relay，并执行云端派发的本机任务。

## Data Flow

- Desktop App 启动后加载桌面设置，按设置启动 `Desktop API`、恢复窗口/托盘状态，并可自动启动 Local Runtime。
- Desktop Local Runtime 使用用户 access token 调用云端 `/api/local-runtime/ensure`，领取 `backend_id`、`relay_ws_url`、`auth_token` 后建立 WebSocket。
- Local Runner 使用 runner registration token 调用云端领取流程，领取后将运行凭据写入本地 runner 配置，再建立 WebSocket。
- 云端以 backend/runner 在线状态作为任务派发依据；前端展示云端 API、Desktop API、Local Runtime/Runner、relay 连接四类状态。

## Contracts

- `Desktop API` 与 `Local Runner` 是不同能力，不共享“本机 API”这个泛称。
- Runner registration token 是服务器托管入口的授权凭据，不要求保存用户 access token。
- Runner 领取后的运行凭据包含 `backend_id`、`relay_ws_url`、`auth_token`；运行时连接只使用 WebSocket 出站链路。
- 自启动/服务化配置属于本机运行设置；runner/backend 注册事实属于云端事实源；当前连接状态来自 relay/runtime heartbeat。

## Subtask Boundaries

- `06-26-local-runner-daemon`：headless runner 配置、日志、状态、systemd、Windows Service。
- `06-26-runner-enrollment-token`：云端 registration token 模型、API、领取授权、轮换/撤销。
- `06-26-windows-desktop-installer-background`：Windows 安装包、托盘、后台运行、自启动、显式退出。
- `06-26-runtime-diagnostics-settings`：前端/桌面设置、状态展示、日志和错误恢复入口。
- `06-26-distribution-release-validation`：发布产物、版本一致性、安装/服务化手工验收。

## Tradeoffs

- 第一阶段不把 runner 做成 HTTP API 服务，避免服务器托管场景暴露入站攻击面，也保持与现有 relay 架构一致。
- 第一阶段 Windows 桌面与独立 runner 分开交付，避免桌面生命周期假设污染服务器守护进程。
- 服务安装能力直接进入 runner 第一版，保证服务器托管场景不是“二进制 + 用户自己写服务脚本”的半成品体验。
