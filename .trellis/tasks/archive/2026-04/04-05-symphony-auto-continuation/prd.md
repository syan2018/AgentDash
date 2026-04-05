# Symphony Milestone: Hook 驱动的 Turn Continuation

## 目标

Turn 完成后的行为（continuation / retry / stop）由 Hook 层定义驱动，
而不是 TurnMonitor 硬编码。实现 Symphony 中 worker 内循环的等价能力，
同时清理 TurnMonitor 技术债。

## 架构决策（设计对账 #2 结论）

> **TurnMonitor 是典型的冗余设计。Turn 完成后的逻辑应由 Agent 层绑定的 Hook 实现。**

当前 TurnMonitor 硬编码了 turn 完成后的状态转换逻辑：
- 成功 → AwaitingVerification
- 失败 + AutoRetry → RestartTracker 决策

这些逻辑应迁移到 Hook 体系（`AfterTurn` / `SessionTerminal`），
让 workflow/lifecycle 定义来控制 turn 完成后的行为。

## 实现方向

### Step 1: 识别 TurnMonitor 中的硬编码逻辑

当前 `run_turn_monitor` 中硬编码的行为：
- turn 成功 → task 状态推到 AwaitingVerification
- turn 失败 → 根据 TaskExecutionMode 决定 retry
- tool_call / tool_call_update → artifact 记录

### Step 2: 将决策逻辑迁移到 Hook 层

利用已有的 Hook 触发点：
- `AfterTurn`: turn 完成后决定 continuation / stop / retry
- `SessionTerminal`: session 终态处理
- `BeforeStop`: stop 前的 gate check

### Step 3: 通过 Workflow 定义 continuation 策略

在 `WorkflowContract` 或 `LifecycleStepDefinition` 中表达：
- 是否允许自动 continuation
- max_turns 限制
- continuation prompt 策略（复用 / 注入 guidance / agent 决定）

## 现有基础

- `WorkflowHookTrigger::AfterTurn` hook 触发点已存在
- `WorkflowHookTrigger::BeforeStop` hook 触发点已存在
- Hook preset 系统 + Rhai script engine 已就绪
- `RestartTracker` 已有指数退避策略（可复用于 Hook 实现）

## 前置依赖（技术债清理）

- [ ] 审计 TurnMonitor 中所有硬编码的 task 状态转换逻辑
- [ ] 明确哪些逻辑可以直接迁移到 Hook，哪些需要保留在平台层
- [ ] TaskExecutionMode (Standard/AutoRetry/OneShot) 是否需要同步重构

## 待讨论

- [ ] TurnMonitor 完全移除还是保留为"事件转发层"（只负责接收事件、触发 Hook、不做决策）？
- [ ] continuation turn 的 prompt 策略：复用原始 prompt？注入 continuation guidance？
- [ ] `max_turns` 的归属：ProjectConfig？WorkflowContract？还是 Hook params？

## 依赖

- symphony-orchestrator-config（max_turns 配置）

## 参考

- Symphony spec §7.1 (worker 内循环: turn 完成 → 检查状态 → 继续)
- Symphony spec §16.5 (Worker Attempt)
- `crates/agentdash-application/src/task/gateway/turn_monitor.rs`
- `crates/agentdash-application/src/task/execution.rs`
