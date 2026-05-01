# CodexBridgeConnector 实现补齐

## Goal

将当前 PoC 级别的 CodexBridgeConnector 补齐为生产可用的完整 connector 实现。

## 需要解决的四个问题

### 1. 进程模型优化（高优先级）

**现状：** 每次 `prompt()` 调用都 spawn 一个新的 `npx -y @openai/codex@0.121.0 app-server` 进程。

**问题：**
- npm 下载和 Node.js 启动的冷启动延迟
- 无法利用 Codex 的 thread resume 能力（进程死后 thread 状态丢失）
- 资源浪费

**方案选项：**
- **A: 长驻进程 + 多 thread 复用** — 启动一个 app-server 进程，多个 session 共享
  - 优势：改动最小，仅需把进程生命周期提升到 connector 级
  - 劣势：仍有 Node.js/npx 依赖，首次启动仍需下载
- **B: 预编译 codex-rs 二进制** — 跳过 npx，直接用编译好的 Codex binary
  - 优势：启动快，无 npm 依赖
  - 劣势：需要分发和版本管理二进制
- **C: in-process 引入 codex-rs（推荐）** — 作为 Rust library 直接调用
  - 优势：零启动延迟、类型安全、可复用 `InProcessClientHandle` 的 typed API
  - 劣势：编译依赖重，需要引入 codex-core 和相关基础设施
  - 参考：`codex-rs/app-server/src/in_process.rs` 已提供完整的 in-process runtime
  - `InProcessClientHandle` 提供 `request()` / `notify()` / `next_event()` / `shutdown()` API
  - `InProcessServerEvent` 已区分 `ServerRequest` / `ServerNotification` / `LegacyNotification`

**建议：推迟到链路统一替换后再决定；A（长驻进程）和 C（in-process）都可行**

决策依据（需要评估后决定）：
- C 是理想终态：零启动延迟、类型安全，但需评估 codex-core 的编译时间增量和发布体积
- 如果 codex-rs 引入的额外成本过大，A 的改动量最小（把 process spawn 提到 connector 初始化时而非每次 prompt），足以满足预研需求
- 本任务建议在 backbone 事件模型和链路替换基本完成后再推进，届时集成路径更清晰

### 2. 事件映射补齐

**现状：** `handle_server_notification` 只处理 5 种通知。

**需补齐（依赖 P0 backbone-event-model 完成后）：**
- `item/started` + `item/completed` — item 生命周期
- `item/commandExecution/outputDelta` — 命令执行过程
- `item/fileChange/outputDelta` — 文件变更过程
- `turn/started` — turn 生命周期开始
- `turn/diff/updated` — turn 级 diff

### 3. 审批链路正式化

**现状：** `handle_server_request` 对所有审批直接返回 `acceptForSession`。

**目标：**
- `commandExecution/requestApproval` → 通过 BackboneEvent 上报给平台
- `fileChange/requestApproval` → 通过 BackboneEvent 上报给平台
- 平台决策后通过 pending response channel 回传
- `approve_tool_call` / `reject_tool_call` 方法连接到实际的审批链路

### 4. 标识域修复

**现状：** `follow_up_session_id` 被直接当作 Codex `ThreadForkParams.thread_id`。

**问题：** AgentDash session ID（由平台生成）和 Codex thread ID（由 Codex 生成）是不同标识域。

**方案：** 维护 `session_id → thread_id` 的映射表，在 `prompt()` 中正确查找。

## Acceptance Criteria

* [ ] `prompt()` 不再每次 spawn 新进程
* [ ] 所有 P0 事件类型都有对应的映射处理
* [ ] 审批请求能上报到平台并等待决策
* [ ] session_id ↔ thread_id 映射正确
* [ ] `approve_tool_call` / `reject_tool_call` 能正确回传到 Codex

## Dependencies

* 依赖 `backbone-event-model` 任务的类型定义

## Out of Scope

* ACP 链路替换
* 前端审批 UI
