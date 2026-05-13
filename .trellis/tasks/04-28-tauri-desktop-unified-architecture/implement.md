# 实施计划：Tauri 桌面端统一架构与 Local Runtime 控制台

## 推荐执行顺序

### M0. 任务切片与基线检查

- 确认首个交付切片采用“Desktop-managed local runtime + Local 设置页 MVP”。
- 记录当前 `agentdash-local` bin 入口、`BackendRegistry`、frontend store/service 现状。
- 建立 desktop 分支后先跑基线：
  - `cargo check -p agentdash-local`
  - `cargo check -p agentdash-api`
  - `pnpm run frontend:check`

验收：只产生规划/基线记录，不改业务行为。

### M1. `agentdash-local` bin -> lib + optional bin

- 新增 `crates/agentdash-local/src/lib.rs`，导出 runtime config、runtime manager、status/log event 类型。
- 将 `main.rs` 中 CLI parsing 之外的组装逻辑迁入 library。
- 将 `ws_client::run` 改为可取消 runtime task，暴露 start/stop/restart/snapshot。
- MCP manager、SessionHub、ToolExecutor 初始化由 library 统一负责。
- standalone bin 保留，用于 `pnpm dev` 和手动调试。

验证：

- `cargo check -p agentdash-local`
- `cargo test -p agentdash-local`
- `pnpm run dev:local` 能继续启动本机 backend。

风险文件：

- `crates/agentdash-local/src/main.rs`
- `crates/agentdash-local/src/ws_client.rs`
- `crates/agentdash-local/src/mcp_client_manager.rs`
- `crates/agentdash-local/Cargo.toml`

### M2. Tauri desktop skeleton + runtime commands

- 新增 `crates/agentdash-local-tauri` 或约定的 desktop crate/package。
- 新增 `packages/app-tauri` renderer shell 与 Tauri 配置。
- 建立 Tauri command：
  - runtime snapshot/start/stop/restart
  - log tail/clear
  - MCP list/probe/update
  - accessible roots list/update
- desktop dev 命令接入根 `package.json`，但 `pnpm dev` 仍保持现有联合调试语义。

验证：

- Tauri dev 可打开窗口。
- Local 设置页能显示 runtime snapshot 和日志。
- `cargo check -p agentdash-local-tauri`
- `pnpm --filter app-tauri check`

风险文件：

- `pnpm-workspace.yaml`
- `package.json`
- `crates/agentdash-local-tauri/**`
- `packages/app-tauri/**`

### M3. 前端最小共享包拆分

- 新增 `packages/ui`，迁移 Local 设置页需要的纯 UI 组件。
- 新增 `packages/core`，迁移 API client/types/runtime query key/local adapter types。
- 新增 `packages/views`，先迁移 Dashboard 主框架可复用部分与 Local Settings view。
- `packages/app-web` 可先包装当前 `frontend`，避免一次性大搬迁；当 views/core 稳定后再替换入口。
- 保持 snake_case 类型，不做字段名转换。

验证：

- `pnpm run frontend:check`
- `pnpm --filter app-tauri check`
- Web Dashboard 与 Desktop Dashboard 都能渲染同一业务 view。

风险文件：

- `frontend/src/App.tsx`
- `frontend/src/pages/**`
- `frontend/src/features/**`
- `frontend/src/stores/**`
- `frontend/src/services/**`

### M4. Runtime health 持久化与 API

- 新增 domain entity/repository：runtime/backend health。
- 新增 migration：runtime health 表、必要索引、status 约束。
- `BackendRegistry` 注册、断开、capability 更新时同步 health repository。
- 新增 API：
  - list runtime health
  - runtime detail
  - merged online/offline status
- 前端 runtime query 使用该 API；desktop local snapshot 只补充本机即时状态。

验证：

- repository 单测覆盖 upsert online、mark offline、last_seen update、list by status。
- API 测试覆盖在线 registry 与持久化 health 合并。
- `cargo check -p agentdash-domain`
- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-api`

风险文件：

- `crates/agentdash-domain/src/**`
- `crates/agentdash-infrastructure/migrations/**`
- `crates/agentdash-infrastructure/src/persistence/postgres/**`
- `crates/agentdash-api/src/relay/**`
- `crates/agentdash-api/src/routes/**`

### M5. Local 设置页 MVP

- 页面结构：
  - Runtime status header
  - MCP servers tab
  - Accessible roots tab
  - Logs tab
  - Diagnostics tab
- MCP servers 支持编辑、保存、probe、错误展示。
- Logs 支持 tail、level/filter、copy、clear、脱敏。
- Runtime restart 使用 active work gate；存在活跃 session/terminal/MCP call 时提示并阻止或延后。

验证：

- `pnpm --filter app-tauri test`
- Playwright 或 Tauri smoke：编辑 MCP 配置、probe 失败/成功、查看日志、restart gate。
- 文本不溢出，状态不重叠，桌面窗口和窄 viewport 都可用。

风险文件：

- `packages/views/src/local-settings/**`
- `packages/core/src/local-runtime/**`
- `packages/app-tauri/**`
- `crates/agentdash-local/src/**`

### M6. Execution attempt/message log 投影

- 新增 execution attempt/message log domain 与 migration。
- 从 SessionHub/LifecycleRun/relay session event 投影：
  - attempt started/progress/completed/failed/cancelled
  - executor session id
  - failure reason
  - usage summary
  - message summary
- Task/Story/Runtime UI 读取投影，不直接遍历完整 session events。

验证：

- projector 单测：session started -> attempt created；terminal failed -> failure reason 写入。
- API 测试：按 story/task/runtime 查询 attempts。
- 前端详情页展示一次执行历史。

风险文件：

- `crates/agentdash-application/src/session/**`
- `crates/agentdash-application/src/task/**`
- `crates/agentdash-infrastructure/migrations/**`
- `frontend/packages/views` 或 `packages/views`

### M7. Runtime recovery 与 version/profile 收敛

- 引入 desktop-managed local profile，分离手动 CLI profile。
- token/user 切换时停止或重启 desktop-managed runtime。
- 版本不匹配时，如果没有 active work，执行安全重启；有 active work 则 defer。
- cloud runtime health 引入 last_seen sweeper，处理 stale online。
- 本机收到 runtime gone/unauthorized 类错误时收敛状态并重新注册。

验证：

- runtime manager 单测或集成测试覆盖 token 切换、active work defer、stale offline。
- `pnpm dev` 开发路径仍可杀旧进程后重新调试。

风险文件：

- `crates/agentdash-local/src/**`
- `crates/agentdash-local-tauri/src/**`
- `crates/agentdash-api/src/relay/**`
- `crates/agentdash-infrastructure/migrations/**`

### M8. VFS materialization 诊断与 GC

- Local Diagnostics 显示 materialization roots、dirty、last_used、owner、active 标记。
- 引入 active root 防护与安全 GC 操作。
- provider-native instruction/skill materialization 另开任务处理，当前只做可观测与清理入口。

验证：

- materialization manifest 读取测试。
- GC 不删除 active session/materialization root。
- Windows 路径显示与复制正常。

风险文件：

- `crates/agentdash-local/src/materialization/**`
- `crates/agentdash-api/src/vfs_materialization.rs`
- `packages/views/src/local-settings/**`

## 里程碑拆分

### MVP A：桌面端能管本机 runtime

包含 M1、M2、M3 的最小子集、M5 的 status/logs。完成后用户不需要手工启动 `agentdash-local`，能打开 desktop 查看 runtime 状态和日志。

### MVP B：Local 设置页可用

包含 MCP servers、accessible roots、probe、restart gate、日志脱敏。完成后覆盖 `04-13-local-dashboard-ui` 的核心诉求。

### MVP C：Runtime 可运营化

包含 M4、M7。完成后 runtime 从内存在线表升级为可查询、可恢复、可展示的业务资源。

### MVP D：执行历史可读

包含 M6。完成后 Story/Task 能展示 execution attempt 与 message summary。

### MVP E：物化与本机资源治理

包含 M8。完成后 local 诊断页能解释本机资源占用，并安全清理。

## 建议任务拆分

1. `refactor(local): agentdash-local 抽出可嵌入 runtime library`
2. `feat(desktop): 新增 Tauri shell 与 local runtime commands`
3. `refactor(frontend): 建立 desktop 复用所需 core/views/ui 最小边界`
4. `feat(runtime): runtime health 持久化模型与 API`
5. `feat(desktop): local 设置页 MCP roots logs MVP`
6. `feat(task): execution attempt 与 message log 投影`
7. `feat(desktop): local profile token version 安全收敛`
8. `feat(local): materialization 诊断与安全 GC`

## 最小验证清单

- `cargo check -p agentdash-local`
- `cargo test -p agentdash-local`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-infrastructure`
- `pnpm run frontend:check`
- `pnpm --filter app-tauri check`
- `pnpm dev` 联合调试仍可启动云端后端、本机后端、前端。
- Tauri dev smoke：窗口打开、Dashboard 渲染、Local 设置页显示 runtime snapshot、MCP probe 可执行、日志可查看。

## 开始实现前必须确认

- 首个实施切片是否采用推荐的 MVP A + MVP B：先让 desktop 管起本机 runtime，再补 runtime health 完整可运营化。
- Tauri app 目录命名最终采用 `packages/app-tauri` + `crates/agentdash-local-tauri`，还是将 Tauri 配置放在单一 `apps/desktop` 包内。

推荐答案：采用 `packages/app-tauri` + `crates/agentdash-local-tauri`。它更贴合当前 Rust workspace 与未来 web/desktop 前端共享包的边界，也能避免把 Tauri command 与 renderer build 混在同一个目录里。
