# Session 业务耦合收束：迁移到 AgentRun Scope

## Background

当前多个业务模块以 `session_id` 为主键索引和路由，这是早期实现残留。
Runtime session 是内部实现细节（模型会话连接），不应暴露给业务层。
业务特性的生命周期绑定的是 AgentRun（用户可见执行单元：`run_id + agent_id`），一个 AgentRun 可能跨越多个 session（context compact、reconnect、failover）。

**核心风险**：session 重建后，绑定在旧 session_id 下的业务数据变为孤岛，对用户不可见。

## 审计发现

### HIGH — 直接影响用户功能

| # | 模块 | 位置 | 问题 |
|---|---|---|---|
| 1 | **Terminal Cache** | `crates/.../terminal_cache.rs` | `HashMap<session_id, HashMap<terminal_id, State>>`，session 切换后旧终端不可见 |
| 2 | **Terminal Store (前端)** | `packages/app-web/.../useTerminalStore.ts` | `Map<sessionId, Map<terminalId, Info>>`，同上 |
| 3 | **Context Audit Bus** | `crates/.../context/audit.rs` | `HashMap<session_id, VecDeque<event>>`，session 切换后审计时间线断裂 |
| 4 | **Wait Activity (exec)** | `crates/.../wait_activity/sources/exec.rs` | `list_terminals(session_id)` 查询，session 切换后 agent 丢失对运行进程的感知 |
| 5 | **Session Title** | `crates/.../session_persistence.rs` SessionMeta | title/title_source 存在 SessionMeta，workspace display_title 只是读穿投影，无独立存储 |

### MEDIUM — 功能正确但架构脆弱

| # | 模块 | 位置 | 问题 |
|---|---|---|---|
| 6 | **Canvas Runtime (Legacy)** | `crates/.../routes/canvases.rs` | Legacy endpoint 接受裸 session_id 调用，已有 AgentRun scope 替代 |
| 7 | **Terminal Control Callback** | `crates/.../agent_run_terminal_control.rs` | 业务操作以 session_id 为入口 resolve AgentRun，anchor 过期时静默失败 |
| 8 | **Hook Script Engine** | `crates/.../script_engine.rs:129` | 向 hook 脚本暴露 `ctx.session_id`，泄漏实现细节到用户扩展面 |

### 已正确处理（不动）

- Permission Grant Service — `source_runtime_session_id` 仅审计溯源
- Companion Gate Control — session_id 仅传输路由
- 前端 AgentRun runtime service — 全部走 `runId/agentId` path
- Session Summary — 不存在这个概念，是 compaction record，正确归属 projection 层

## Goal

1. 消除业务模块对 session_id 的一级索引依赖，老路径直接移除不做兼容
2. 改为以 AgentRun scope（`run_id + agent_id`）或 terminal_id 直接索引
3. Session 重建后业务连续性不中断
4. Title 提升为 AgentRun workspace 一级属性，不再穿透 SessionMeta 读取

## Requirements

### Phase 1 — Terminal 收束
- 后端 `SessionTerminalCache` → `AgentRunTerminalCache`，以 `(run_id, agent_id)` 为 scope
- `ws_handler.rs` 终端事件注入：terminal_id 反查 AgentRun scope 路由
- 前端 `useTerminalStore.terminals` 扁平化为 `Map<terminalId, TerminalInfo>`（terminal_id 全局唯一）
- 新增终端输出回查 API：`GET .../runtime/terminals/:terminal_id/output`
- 前端 TerminalView mount 时若 store 为空，从 API 加载

### Phase 2 — Title 收束
- AgentRun workspace 新增独立 `title` + `title_source` 字段，持久化到 workspace 层
- Title 写入路径改为直接写 workspace，不再写 SessionMeta
- `source_session_title_updated` → `workspace_title_updated`
- 前端标题更新走 workspace state 而非 session_meta_updated 事件

### Phase 3 — Context Audit 收束
- `InMemoryContextAuditBus` 索引改为 `(run_id, agent_id)`
- Session 切换后审计时间线连续

### Phase 4 — Wait Activity + 清理
- `collect_scope_exec_items` 通过 AgentRun scope 查询终端
- Legacy canvas session endpoint 移除
- Hook script engine: `ctx.session_id` → `ctx.run_id` + `ctx.agent_id`
- Terminal control callback 入口从 session_id → AgentRun identity

## Constraints

- 不做向下兼容，老路径直接丢干净
- terminal_id 全局唯一（`term-{timestamp}-{random}`），可安全扁平索引
- `terminal_output` journal 持久化语义不变

## Acceptance Criteria

- [ ] Terminal 注册和查找不再依赖 session_id
- [ ] Session 重建后终端输出事件正确路由
- [ ] 侧栏任何时刻打开都能展示终端历史输出
- [ ] AgentRun workspace 拥有独立 title 字段，不再读穿 SessionMeta
- [ ] Context Audit 在 session 切换后时间线不断裂
- [ ] Wait Activity 在 session 切换后不丢失运行中进程感知
- [ ] Hook 脚本不再暴露 session_id
- [ ] 代码中不存在 SessionTerminalCache / session_meta.title 业务读写路径
- [ ] 现有终端交互功能（spawn、input、resize、kill）不回归
