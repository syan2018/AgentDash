# references/multica 本地端参考

## Desktop daemon manager

`references/multica/apps/desktop/src/main/daemon-manager.ts` 提供了很清楚的本机端职责边界：

- 按目标 API URL 派生 desktop-owned profile，例如 `desktop-<host>`。
- profile config 写入 `server_url`，避免污染用户手工 CLI profile。
- 桌面主进程负责 CLI/daemon 二进制定位、启动、停止、重启、版本匹配、日志 tail。
- renderer 只通过 IPC 调用 `daemon:start/stop/restart/get-status/sync-token`。
- `syncToken` 会按当前登录用户 mint PAT，并在用户切换时重启 daemon。
- `auto-start` 只在 prefs 允许且 CLI 可用时启动。

对 AgentDash 的启发：

- Tauri 应按 server origin 隔离 profile。
- Renderer 不直接拼 token 或启动参数；通过 Tauri command 管理。
- 用户切换/server 切换必须触发 profile/token 同步，必要时重启 runtime。

## Desktop UI

`references/multica/apps/desktop/src/renderer/src/components/daemon-settings-tab.tsx`：

- daemon 配置在 settings tab，不是主应用顶级导航。
- 展示 auto-start、auto-stop、CLI status 和 diagnostics。

`daemon-panel.tsx`：

- 日志作为诊断 modal，支持搜索、level filter、copy、clear、jump to latest。

`desktop-runtimes-page.tsx`：

- web/shared `RuntimesPage` 仍是主体，desktop 只注入 topSlot 与 bootstrapping 状态。

`platform/daemon-ipc-bridge.ts`：

- IPC 状态只用于加速本机 runtime UI 反馈。
- server runtime row 仍保留 name/provider/last_seen 等权威字段。

对 AgentDash 的启发：

- Local Runtime 应在 Settings 面板中呈现。
- 本地 IPC 状态可以加速 UI，但不能替代 server runtime_health。
- Desktop-only UI 通过 adapter/slot 注入，Web 环境隐藏。

## Server runtime model

`references/multica/server/internal/handler/daemon.go`：

- `/api/daemon/register` 按 workspace/daemon/provider upsert runtime。
- 注册时写 runtime version、cli version、device info、owner、timezone。
- 心跳更新 last_seen，并支持 runtime gone 恢复。

`references/multica/server/pkg/db/queries/runtime.sql`：

- `UpsertAgentRuntime` 使用唯一键 `(workspace_id, daemon_id, provider)`。
- 心跳热路径只 bump `last_seen_at`，offline/online 状态变更才写 `updated_at`。
- sweeper 会将 stale runtime 标记 offline，并清理 orphan task。

`references/multica/server/cmd/server/runtime_sweeper.go`：

- 周期性标记 stale runtime offline。
- 失败运行中任务，清理长期 offline runtime。
- 使用 liveness store 防止 DB heartbeat lag 导致误判。

对 AgentDash 的启发：

- 本机 runtime 的 server row 应由 server upsert，且有唯一身份。
- last_seen/status/sweeper 需要保持 server 权威。
- runtime row 被删除或 token 失效时，本机端应能重新 ensure，而不是永久卡死。

## 不应照搬的点

- multica 的 daemon 轮询 workspace/task，AgentDash local runtime 是 relay backend，已有 `/ws/backend` 协议。
- multica 的 runtime 维度是 workspace + provider，AgentDash 当前维度是 backend + runtime_health。
- multica Electron 用 CLI sidecar，AgentDash Tauri 已嵌入 Rust local runtime manager。

因此 AgentDash 应学习架构边界与状态融合方式，不复制 multica 的数据模型。
