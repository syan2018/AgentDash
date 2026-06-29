# 桌面本机 Runner 启动链路收束 - Design

## Problem Statement

桌面 App 自包含了本机执行能力，但当前启动链路分散在 Tauri shell、Web app effect、profile 命令、托盘命令和 local runtime manager 之间。结果是桌面进程多开没有被阻止，local/runner 自动启动依赖前端页面登录态 effect，失败后缺少重试，导致“打开桌面包后本机 runner 有时没有起来”。

## Architecture Principles

- 桌面包启动链路以 Tauri/native lifecycle 为宿主。Web app 可以提供 token、展示状态、触发用户命令，但不应是唯一的本机执行面启动所有者。
- Desktop local/runner 与 Standalone runner 共享 relay 执行语义：领取 server-issued relay credentials 后连接 `/ws/backend` 并承载 executor、MCP、workspace 工具和 extension host。
- Desktop 与 Standalone runner 保留不同授权入口。Desktop 用用户 access token，Standalone runner 用 runner registration token。
- 单实例是桌面壳契约，不是 local runtime manager 契约。进程级互斥先成立，进程内 manager 再防重复 start。
- 状态事实源优先在 native 层形成，再投影到前端设置/诊断 UI。

## Proposed Shape

### 0. Final Process Decision

首版固定采用 **embedded local-owned runner host**：

- runner host 代码落在 `agentdash-local`，复用 standalone runner 的 claim、runtime config、relay connection、status reporter 和 token redaction 能力。
- Tauri 不直接拼装 runner 细节，只通过 `ensure_started/stop/snapshot` 一类 library API 管理桌面 runner 生命周期。
- runner 仍运行在 `agentdash-local-tauri.exe` 进程内；本任务不引入独立 `agentdash-local.exe` sidecar。
- sidecar 进程模型保留为后续增强，不进入本轮实现和验收范围。

### 1. Desktop Single Instance

在 `agentdash-local-tauri` 引入 Tauri v2 single instance 能力或等价 Windows 全局 mutex：

- 第一实例正常启动并持有生命周期。
- 第二实例启动时向第一实例发送聚焦/恢复请求。
- 第一实例执行 `restore_main_window`，必要时从托盘恢复。
- 第二实例不继续初始化 Desktop API、local runner 或 WebView。

推荐优先使用官方 single instance 插件，因为它与 Tauri 应用生命周期、事件派发和跨平台行为更贴合；如果插件带来构建约束，再评估 Windows mutex + IPC。

### 2. Desktop Runner Supervisor

在 Tauri 层收束一个 `DesktopRunnerSupervisor`，作为 `DesktopState` 的一部分：

- 读取 `DesktopAppSettings` 与 `LocalRuntimeProfile`。
- 接收 Web app 提供的当前 access token，或记录 `waiting_for_auth` 状态。
- 当 Dashboard API snapshot 为 running 且 token 可用、auto-connect 允许时，调用同一个 `start_runtime_from_request` 或其重构后的 service。
- 维护 retry/backoff、last_error、last_attempt_at、next_retry_at。
- 对托盘启动、设置页启动、自动启动提供同一入口。

短期实现可以继续复用 `LocalRuntimeManager` 的 in-process 模型；关键是把“什么时候启动”和“失败后如何重试”的责任从前端 effect 挪到 native supervisor。

### 3. Process Model Decision

存在两个可行形态：

| 形态 | 描述 | 优点 | 代价 |
| --- | --- | --- | --- |
| Embedded library runner | Tauri 进程内继续使用 `agentdash-local` library 与 `LocalRuntimeManager` | 代码路径最短，当前结构接近，停止/托盘生命周期容易统一 | 崩溃隔离弱；与 standalone binary/service 的日志、status 文件天然不同 |
| Bundled sidecar runner | 安装包携带 `agentdash-local.exe`，Tauri 启动并管理 child process | 与 standalone runner 的进程模型、status/log/doctor 更一致；崩溃隔离更好 | 需要 Tauri externalBin 或 bundle 产物配置；需要 child health、重启、退出清理、权限与路径验证 |

本任务选择 embedded library runner，但要求 runner 状态机归 `agentdash-local` 所有。后续若切 sidecar，`LocalRuntimeManager` 可逐步退为 sidecar 管理客户端；本轮需补齐 status/logs projection 来达到与 runner 等价的可观测性。

### 3.1 Recommended Direction: Local-Owned Runner Host

推荐把“runner 自身如何启动、claim、连接 relay、维护 status/logs”的责任放回 `agentdash-local`，而不是让 Web app 或 Tauri 壳直接拼装 runner 细节：

- `agentdash-local` 提供一个可嵌入的 `RunnerHost` / `DesktopRunnerHost` service，复用 standalone runner 的 claim、runtime config、relay connection、status reporter 和 token redaction。
- Tauri 只负责调用 host 的 `ensure_started(token/profile/settings)`、`stop(reason)`、`snapshot()`，并处理桌面单实例、托盘、窗口、退出。
- Web app 只负责把登录态/token availability 通知 Tauri，并展示 native snapshot，不直接决定 runner 生命周期。

这比“Web effect 启动 runtime”更合适，因为 runner 生命周期属于本机执行面，不属于页面生命周期；也比“Tauri main.rs 里继续堆业务逻辑”更合适，因为 Tauri 壳应保持为桌面入口层，避免复制 standalone runner 的 claim/status/logging 逻辑。

实现上可以分两步：

1. **Embedded Local-Owned Host（推荐首版）**：`agentdash-local` 以 Rust library 形式被 Tauri 调用，进程仍在 `agentdash-local-tauri.exe` 内，但 runner 状态机和 claim/relay 逻辑由 `agentdash-local` service 拥有。
2. **Sidecar Local-Owned Host（后续可选）**：安装包实际携带 `agentdash-local.exe`，Tauri 启动 sidecar 并通过 IPC/CLI/status file 控制它。

首版固定采用 1，原因是它能最小化打包和开发调试复杂度，同时把职责边界先拉正。等 embedded host 的状态契约稳定后，再评估是否需要 sidecar 的崩溃隔离和日志一致性。

### 3.2 Side Effects of Local-Owned Runner Host

Local-owned host 的副作用取决于进程形态：

**Embedded local-owned host 的副作用较小：**

- Rust 改动仍需要重启 `pnpm dev:desktop`；这是当前 Rust/Tauri 既有限制，不是新增成本。
- `pnpm dev:desktop` 可以继续先 `cargo build -p agentdash-api -p agentdash-local -p agentdash-local-tauri`，再启动 server、renderer、Tauri shell，不需要额外 runner 进程。
- 调试时 stack/log 会集中在 Tauri 进程里，进程少，快速迭代更顺。
- 代价是崩溃隔离弱：runner 内部 panic 或资源泄漏更可能影响桌面壳；status/logs 需要主动投影，不能天然复用 standalone runner 的文件。

**Sidecar local-owned host 的副作用更明显：**

- `pnpm dev:desktop` 会多一个长驻 `agentdash-local` child process，清理逻辑要同时杀 Tauri shell、sidecar runner、可能的 embedded PostgreSQL。
- 开发期二进制路径要更严格：Tauri dev、debug build、release bundle、NSIS 安装目录下都要能找到正确的 `agentdash-local.exe`。
- Rust 改动后可能被 sidecar 进程锁住 debug binary，Windows 上 build 前清理会更频繁，失败模式也更绕。
- 日志会分散到 Tauri、sidecar runner、cloud backend 三处；排查要有统一入口，否则“runner 没起来”会变得更难看。
- child process supervision 要处理启动超时、崩溃重启、退出顺序、隐藏窗口、stdout/stderr 脱敏、显式退出清理。
- 安装包要声明和验证 external binary/sidecar bundle，发布验收面会增加。

因此，若目标是快速修复桌面包“没有可靠拉起 runner + 可能多开”，首版不建议直接切 sidecar；若目标是长期让桌面 runner 与服务器 runner 在进程、status file、doctor、日志上完全一致，则 sidecar 值得作为第二阶段。

### 4. Auto-Start Flow

目标流：

1. Tauri 第一实例启动。
2. 初始化 Desktop API mode。
3. 初始化 DesktopRunnerSupervisor，读取 settings/profile。
4. Web app bridge 注册后，将 access token/current user availability 通知 Tauri。
5. Supervisor 判断：
   - settings 禁用 auto-connect -> `disabled_by_settings`
   - 未登录 -> `waiting_for_auth`
   - API 未就绪 -> `waiting_for_api`
   - 已 running/starting -> 复用
   - 可启动 -> claim credentials -> start runner/runtime -> relay connect
6. 失败进入 `error_retrying` 或 `error_terminal`，可由设置页/托盘手动重试。

前端 `ensureDesktopLocalRuntimeStarted` 可降级为“通知 native 当前 token 并请求 ensure auto-connect”，不再自己持有一次性启动标记。

### 5. Diagnostics Contract

新增或扩展 desktop runtime snapshot：

- `state`: `idle | disabled | waiting_for_auth | waiting_for_api | claiming | starting | running | retrying | error | stopping | stopped`
- `owner`: `desktop_embedded_runner` 或 `desktop_sidecar_runner`
- `registration_source`: `desktop_access_token`
- `backend_id`, `relay_connection`, `executor_enabled`, `workspace_roots`
- `last_error`, `last_attempt_at`, `next_retry_at`
- `process`: embedded/sidecar PID 信息（sidecar 形态才有）

UI 不从云端 `backend.online` 推断本地 runner 是否启动，而是展示 native snapshot + cloud diagnostics 的组合。

## Boundaries

- 不改变 `/api/local-runtime/ensure` 与 `/api/local-runtime/runner/claim` 的认证边界。
- 不把 Desktop access token 写入 standalone runner config。
- 不在本任务引入 sidecar/externalBin/Windows Service 作为桌面 runner 启动方式；本轮只做 embedded local-owned host。
- 不做多 project runner 授权管理。

## Validation Strategy

- Rust unit tests 覆盖 desktop API config、settings normalization、single-instance decision helpers、supervisor state transitions。
- Frontend tests 覆盖 bridge 从“自己启动”转为“通知 native/展示状态”的行为。
- `pnpm run desktop:check` 覆盖 Tauri + frontend 类型与 Rust check。
- Windows 手工验收覆盖安装包多开、托盘、登录后自动启动、断网重连、显式退出。

## Risks

- Tauri single-instance 插件引入后可能需要调整 capabilities 或 builder 初始化顺序。
- Sidecar 形态会引入打包路径、child process 清理、Windows 隐藏窗口、日志位置等额外验收面。
- Embedded 形态如果继续保留，需要刻意补齐与 standalone runner 一致的 status/log/doctor 投影，否则“等价”只停留在执行层。
- Local-owned host 如果抽象边界没有收紧，可能只是把 Tauri main.rs 的复杂度搬到 `agentdash-local`，需要用清晰的 `ensure_started/stop/snapshot` API 和状态机测试约束。
