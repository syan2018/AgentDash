# 自主调度能力里程碑

## 背景

AgentDash 的 Project Agent 已具备伴随式项目管理能力，但缺少支撑其**自主、周期性调度**的基础设施。
本里程碑以 Symphony service specification 为参考蓝本，补齐这些通用能力，使 Project Agent 能独立完成：
- 定时评估项目状态并调度待执行任务
- 安全启动和管理多个并发 session
- 检测和处理异常 session（挂死、孤儿）
- 在 turn 完成后智能决定 continuation / retry / stop

**核心原则**：所有 task 都是 AgentDash 的通用基础设施建设，Symphony 只是验证用例。
GSD 等其他工作流同样受益于这些能力，但它们是独立的工作流定义，与本里程碑无关。

## 架构指导原则

### 1. Agent-as-Orchestrator
平台不硬编码 poll-dispatch daemon。调度逻辑由 Project Agent 自身定义和执行，
平台提供使能基础设施（定时触发、并发约束、状态查询）。

### 2. Session 通用基建优先
并发治理、stall 检测、启动对账、运行时对账——这些都是 session 级别的通用能力，
覆盖所有 session 类型，不绑定 task 特殊逻辑。

### 3. Hook 驱动的生命周期控制
Turn 完成后的行为由 Hook 层定义（AfterTurn / BeforeStop / SessionTerminal），
不由 TurnMonitor 硬编码。TurnMonitor 是已识别的技术债。

### 4. 事件驱动优于轮询
运行时对账优先采用事件驱动（状态变更 → 联动 session）而非周期性轮询。

## 任务拆分

### Wave 1 — Session 通用基建

| Task | 范围 | 目标 |
|------|------|------|
| `symphony-stall-detection` | session-infra | session 无活动超时检测，per-session timer |
| `symphony-startup-reconcile` | session-infra | 服务重启后的通用对账管线 |
| `symphony-runtime-reconcile` | session-infra | 事件驱动的运行时状态联动 |

### Wave 2 — Project Agent 调度能力 + 债务清理

| Task | 范围 | 目标 |
|------|------|------|
| `symphony-orchestrator-config` | orchestrator | Project 级调度配置字段 |
| `symphony-tick-loop` | orchestrator | Project Agent 定时触发基础设施 |
| `symphony-auto-continuation` | orchestrator | Hook 驱动的 turn continuation + TurnMonitor 债务清理 |

### 已排除

| 原 Task | 处置 | 原因 |
|---------|------|------|
| `symphony-priority-dispatch` | 砍掉 | Agent 自身即可做优先级判断，无需平台硬编码 |
| `symphony-concurrency-governor` | 砍掉 | 企业级多人协作平台，per-project 并发上限与产品定位冲突 |
| `symphony-state-snapshot-api` | 移出 | UI 优化项，不绑定本里程碑，后续按需推进 |

## 现有能力盘点

- Workflow / Lifecycle 定义体系（比 Symphony 的 WORKFLOW.md 更完善）
- Hook Runtime (Rhai scripting) + 内置触发点（AfterTurn, BeforeStop, SessionTerminal）
- SessionHub 事件广播 + 订阅
- TaskLifecycleService + TaskStateReconciler（boot-time）
- RestartTracker（指数退避重试）
- StateChange 事件流
- Companion tool 体系

## 验证标准

里程碑完成时，Project Agent 应能在 Hook+Config 驱动下自动执行以下流程：

1. 被定时触发器唤醒
2. 查询项目待执行 task
3. 自主选择 task 并发起 session
4. Session 挂死时被平台 stall timer 自动处理
5. Turn 完成后由 Hook 决定 continuation/retry/stop
6. 外部取消 task/story 时，关联 session 被事件驱动停止
7. 服务重启后，通用对账管线恢复一致状态

## 参考文档

- `docs/symphony-spec.md` — 完整 Symphony 规格参考
