# 安装与本机运行形态产品化 - Design

## Architecture

本任务将本机能力拆成两个产品边界，并用父任务维护跨子任务执行图：

- Windows Desktop App：Tauri 壳负责窗口、托盘、安装包、自启动与用户设置；桌面壳可启动 `Desktop API` 与本机 Local Runtime。`Desktop API` 是桌面内置前端的 Dashboard API 宿主，默认只监听 `127.0.0.1:17301`。Desktop API 与普通 cloud/backend dev server 的 `3001` 分开，原因是桌面安装包需要稳定且不易撞到用户本机调试应用的 loopback 端口，而普通 Web 开发入口仍保留既有 dev server 约定。
- Local Runner：`agentdash-local` 的 headless 产品形态，面向服务器托管。Runner 不承载桌面 UI，不启动 Dashboard API，不对外暴露业务 HTTP API；它通过 WebSocket 出站连接云端 relay，并执行云端派发的本机任务。

父任务不直接实现大块代码，它维护依赖关系、集成契约、review gates 与最终验收。每个子任务的 `design.md` / `implement.md` 必须写清自己的产出如何交给下游任务消费。

## Dependency Map

核心依赖链：

```text
runner-enrollment-token
  -> local-runner-daemon
  -> runtime-diagnostics-settings
  -> distribution-release-validation
```

桌面并行链：

```text
windows-desktop-installer-background
  -> runtime-diagnostics-settings
  -> distribution-release-validation
```

解释：

- `runner-enrollment-token` 是 runner 可托管的身份地基。它必须先冻结 claim API、token 权限、backend access 与 DTO，`local-runner-daemon` 才能避免临时凭据方案。
- `local-runner-daemon` 可以在 claim 契约稳定后启动，不必等待 token 管理 UI 完成。
- `windows-desktop-installer-background` 与 runner 身份链基本独立，可以与 `runner-enrollment-token` / `local-runner-daemon` 并行推进。
- `runtime-diagnostics-settings` 可以先做 UI/DTO 设计，但实现完成依赖 runner 与 desktop 两条线输出稳定状态 snapshot 和命令。
- `distribution-release-validation` 是最终 release gate，不应作为第一批实现任务；但验收矩阵可以从第一天维护。

## Parallel Windows

第一批并行空间：

- 主线 A：`runner-enrollment-token` 实现云端 token/claim 契约。
- 主线 B：`windows-desktop-installer-background` 研究并实现 Tauri 托盘、关闭到后台、自启动、NSIS 安装器边界。
- 准备线 C：`distribution-release-validation` 维护验收矩阵草案，但不阻塞 A/B。

第二批并行空间：

- `local-runner-daemon` 内部可拆成配置/claim/runtime、Linux systemd、Windows Service、日志/status 四块。
- `runtime-diagnostics-settings` 可在 runner 与 desktop 状态字段稳定后，与 release checklist 收口并行。

不能并行的点：

- `local-runner-daemon` 的 registration-token 首次领取逻辑不能早于 claim DTO 冻结。
- `runtime-diagnostics-settings` 的实现不能早于状态 snapshot 字段冻结。
- `distribution-release-validation` 的完成不能早于三类产物真实可构建。

## Data Flow

- Desktop App 启动后加载桌面设置，按设置启动 `Desktop API`、恢复窗口/托盘状态，并可自动启动 Local Runtime。
- Desktop Local Runtime 使用用户 access token 调用云端 `/api/local-runtime/ensure`，领取 `backend_id`、`relay_ws_url`、`auth_token` 后建立 WebSocket。
- Local Runner 使用 runner registration token 调用云端 runner claim endpoint，领取后将 `backend_id`、`relay_ws_url`、`auth_token` 写入本地 runner 配置，再建立 WebSocket。
- 云端以 backend/runner 在线状态作为任务派发依据；前端展示云端 API、Desktop API、Local Runtime/Runner、relay 连接四类状态。
- Release validation 消费实际构建产物、服务安装命令和状态验证命令，不从源码结构推断发布是否成功。

## Integration Contracts

`runner-enrollment-token` 必须输出：

- Registration token 管理 API 与 runner claim API 的最终路径、DTO、错误码与权限要求。
- Claim 响应字段：`backend_id`、`relay_ws_url`、`auth_token`、machine identity、capability slot、registration source。
- 数据库迁移、repository、token hash/expiry/revoke/last_used_at 语义。
- ProjectBackendAccess 建立规则。

`local-runner-daemon` 必须输出：

- Runner 配置文件格式、默认路径、CLI/env/file 优先级、凭据写回规则。
- Linux systemd unit 与 Windows Service 的服务名、安装命令、卸载命令、status 命令。
- Runner 状态输出字段、日志路径、脱敏规则、断线重连语义。
- 不引入入站业务 HTTP API 的验证结果。

`windows-desktop-installer-background` 必须输出：

- Setup exe 与安装后 app exe 的产物边界。
- 托盘菜单、关闭到后台、显式退出、自启动、启动到托盘、自动连接 runtime 的最终行为。
- Desktop API localhost 绑定验证方式。
- 安装/卸载创建和清理的系统项清单。

`runtime-diagnostics-settings` 必须输出：

- 结构化状态 snapshot/DTO，区分 `cloud_api`、`desktop_api`、`local_runtime`、`runner_registration`、`relay_connection`。
- 日志 tail、清空日志、restart command 的前后端/桌面调用契约。
- 注册来源、backend id、连接目标、最近错误的 UI 展示规则。
- token 脱敏测试结果。

`distribution-release-validation` 必须输出：

- Windows Desktop Installer、Linux Runner、Windows Runner 三类产物矩阵。
- 版本一致性检查命令。
- 安装、启动、后台运行、自启动、service install/status/stop/uninstall、断网重连、卸载清理的手工验收 checklist。

## Review Gates

完整 design review 必须覆盖：

- `runner-enrollment-token`：token scope、hash 存储、claim 授权、ProjectBackendAccess、DTO、迁移、错误响应。
- `local-runner-daemon`：长期凭据落盘、配置优先级、systemd/Windows Service、权限用户、日志脱敏、断线重连。
- `windows-desktop-installer-background`：关闭到后台语义、显式退出语义、自启动清理、Desktop API localhost 绑定、安装器产物边界。

Focused review 覆盖：

- `runtime-diagnostics-settings`：状态事实源、DTO/mapper、日志脱敏、错误文案、设置入口。
- `distribution-release-validation`：验收矩阵是否可执行、版本一致性、卸载清理和真实产物路径。

每个子任务进入 `task.py start` 前，主会话必须确认：

- 子任务 `design.md` / `implement.md` 已包含上游依赖与下游 handoff。
- `implement.jsonl` / `check.jsonl` 已指向必要 spec。
- 当前并行子任务没有重叠写入同一批核心文件，或已写清协调顺序。

## Tradeoffs

- 第一阶段不把 runner 做成 HTTP API 服务，避免服务器托管场景暴露入站攻击面，也保持与现有 relay 架构一致。
- 第一阶段 Windows 桌面与独立 runner 分开交付，避免桌面生命周期假设污染服务器守护进程。
- 服务安装能力直接进入 runner 第一版，保证服务器托管场景不是“二进制 + 用户自己写服务脚本”的半成品体验。
- 父任务只维护跨任务集成契约，不替代子任务自己的 design/implement；否则父任务会变成不可执行的大泥球。
