# Tauri 桌面端统一架构：Web Dashboard + Local Runtime 控制台

## 背景

`04-13-local-dashboard-ui` 原本只规划一个仅监听 localhost 的轻量本机管理界面，覆盖 MCP 配置、backend 连接状态、stdio 进程与日志。`04-28-tauri-desktop-unified-architecture` 将该方向升级为完整桌面端架构：Web Dashboard 复用、本机后端内嵌、local 管理设置页统一在一个 Tauri 应用中。

`05-13-multica-local-runtime-concept-alignment` 的研究结论进一步明确：multica 最值得学习的不是 Electron 或 Go daemon 形态，而是 runtime 可运营化、执行 attempt/message log、desktop 作为本机能力控制台、前端 core/views/ui/app 分层、server state 与 client state 分离、local profile/token/version 收敛策略。

## 目标

构建 AgentDash 桌面端的长期正确形态：

- Web Dashboard 与 Desktop Dashboard 共享业务视图和无头状态逻辑，避免复制页面。
- `agentdash-local` 从纯 CLI 进程重构为可嵌入 library，同时保留开发期 standalone bin。
- Tauri 桌面端负责拉起、监督和管理本机 runtime，并通过类型化 command 调用本机能力。
- Local 设置页成为桌面端一等入口，覆盖 backend health、MCP servers、accessible roots、进程/会话、日志与诊断。
- 云端 relay 状态、本机 IPC 状态、SessionHub 执行状态保持来源清晰，避免 cloud/server/local 三套状态分裂。
- 按 AgentDash 现有 VFS、Runtime Gateway、SessionHub、Story/Task runtime spec 演进，不照搬 multica 的 Issue/task queue 或 physical worktree 设计。

## 用户价值

- 用户打开桌面端即可使用 Dashboard 与本机执行能力，不需要手工启动 `agentdash-local`。
- 用户可以在图形界面中管理 MCP、可访问目录、backend 状态、日志和重启，而不是手改配置或翻终端日志。
- 本机 runtime 断连、异常、版本不匹配、MCP 配置错误等问题能被 UI 明确呈现并给出可执行操作。
- 后续 Web 与 Desktop 功能演进共享同一套业务页面和类型契约，降低桌面端长期维护成本。

## 已确认事实

- 当前根 `pnpm-workspace.yaml` 只包含 `frontend`，尚未拆分 web/desktop/shared packages。
- 当前仓库未发现正式 `src-tauri`、`tauri.conf.json` 或 Electron/Tauri desktop app。
- `agentdash-local` 当前只有 bin 入口，`src/main.rs` 直接组装 CLI、SessionHub、MCP manager、ToolExecutor 与 WebSocket 主循环。
- `agentdash-local` 通过 `ws_client.rs` 注册 WebSocket backend，首包上报 `backend_id/name/version/capabilities/accessible_roots`。
- 云端 `BackendRegistry` 当前是内存在线连接表，记录 `ConnectedBackend`、pending relay request、per-session sink；没有持久化 runtime health、last_seen、profile、version 或 sweeper。
- `04-13-local-dashboard-ui` 的首要诉求是 MCP 配置管理，其次是连接状态、进程监控和日志。
- 项目规范要求保留 AgentDash 的 Rust 分层、Runtime Gateway、SessionHub、VFS materialization 与 Story/Task runtime 边界。

## 需求

### R1. 本机 runtime 可嵌入化

- 将 `agentdash-local` 核心能力抽成 library API：配置加载、runtime 启停、WebSocket 连接、SessionHub、MCP manager、ToolExecutor、日志事件。
- 保留 standalone bin 作为开发期与手动调试入口，但业务逻辑不得留在 `main.rs`。
- library API 必须能被 Tauri command 层直接调用，并返回结构化状态。

### R2. Desktop runtime manager

- Tauri sidecar/main 侧提供 runtime manager，负责启动、停止、重启、健康检查、日志订阅、版本信息、active work 判断。
- Desktop 管理的 local profile 与用户手动 CLI profile 分离。
- 用户切换、登出、token 更新、backend token 变化时，runtime manager 必须收敛本机状态，不能留下旧账号连接。

### R3. Runtime health 与可观测性

- 后端引入 runtime/backend health 的持久化模型，至少覆盖 status、last_seen、version、capabilities、accessible_roots、owner/profile、disconnect reason、updated_at。
- `BackendRegistry` 继续承担在线 transport，但 runtime resource 成为可查询、可恢复、可展示的业务资源。
- 前端与 desktop local 面板展示 cloud 权威状态与本机即时状态，并明确状态来源。

### R4. Local 设置页

- 设置页作为 Desktop 内独立路由/页面，不再额外启动 localhost-only Web UI 作为主方案。
- 首批功能覆盖：
  - MCP servers 列表、编辑、连接测试、错误展示。
  - Backend/runtime 状态、capabilities、accessible roots、uptime、last_seen。
  - 本机日志 tail、过滤、复制、清空，并进行 token/path/prompt 脱敏。
  - active sessions / terminals / MCP calls 的只读状态与安全重启判断。
- 进程停止/重启必须基于 active work 判断；有活跃执行时默认阻止或延后。

### R5. 前端共享架构

- 从当前 `frontend` 渐进拆出可复用边界，而不是一次性搬空：
  - app shell：web/desktop 各自入口与宿主适配。
  - views：无宿主依赖的业务页面。
  - core：API/query keys/realtime sync/types/stores。
  - ui：通用组件。
- server state 查询与实时 invalidation 逐步规范化；Zustand 主要承载 client/UI state。
- 保持前端类型 snake_case 与后端 Rust DTO 对齐。

### R6. 执行 attempt/message log 投影

- 不改变 Story/Task/SessionHub 真相源；Task 仍是 Story aggregate 下 child entity，执行事实在 session event stream / LifecycleRun / SessionHub。
- 为用户可读的执行历史建立轻量投影：execution attempt、message summary、failure reason、usage、executor session id、workdir/materialization root。
- 该投影服务于 Task/Story/Runtime UI，不复制 Backbone/session events 全量内容。

### R7. VFS 与物化边界

- Desktop/local 不绕过 VFS 直接管理项目文件。
- 对 provider 原生文件注入、skills、物化 workdir、GC 的设计必须遵守 `.trellis/spec/backend/vfs/vfs-materialization.md`。
- 物化 root、manifest、dirty 状态、last_used、active root 防护作为后续 local 诊断 UI 的数据来源。

## 非目标

- 不照搬 multica 的 Issue / agent_task_queue 作为 AgentDash 的业务模型。
- 不使用 Electron 作为本轮目标技术栈；只借鉴 multica desktop 的职责边界。
- 不把 session/backbone stream 替换成普通 query invalidation。
- 不做兼容旧桌面端的方案；当前没有上线桌面端。
- 不把 physical worktree/repo cache 绕过 VFS 作为默认执行模型。
- 不一次性重构全部前端目录，拆包应由 desktop 复用需求牵引。

## 验收标准

- [ ] `agentdash-local` 暴露可嵌入 library API，standalone bin 仅负责 CLI 参数解析和调用 library。
- [ ] 新增 Tauri desktop app，可启动并展示复用 Dashboard 页面与 Local 设置页。
- [ ] Desktop 能启动/停止/重启本机 runtime，并显示结构化 health/log/version/capabilities。
- [ ] MCP servers 可在 Local 设置页中查看、编辑、测试连接，并与 runtime capability 更新联动。
- [ ] 后端具备持久化 runtime health 表/仓储/API，并与在线 `BackendRegistry` 状态合并展示。
- [ ] 前端完成最小共享边界拆分，Web 与 Desktop 共用至少 Dashboard 主业务视图、核心 API/types、基础 UI 组件。
- [ ] 本机日志 UI 有脱敏、限量、过滤和复制能力。
- [ ] Active work 存在时，desktop 重启 local runtime 会被阻止或进入延后队列。
- [ ] 执行 attempt/message log 投影能在 Task/Story 或 Runtime 详情中展示一次执行的状态、失败原因和摘要。
- [ ] 所有数据库变更都有 migration；不保留兼容性双写。
- [ ] 验证命令覆盖后端 check/test、前端 typecheck/test、Tauri build 或 dev smoke test。

## 待确认决策

唯一仍需产品侧确认的是首个交付切片的重心：

- 推荐：先交付“Desktop-managed local runtime + Local 设置页 MVP”，同时只做最小 runtime health schema。
- 取舍：如果先做完整 cloud runtime health 与 execution attempt，会更利于长期可观测性，但用户短期仍需要手工启动 local，无法尽快验证 desktop 体验。
