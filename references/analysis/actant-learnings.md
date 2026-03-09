# Actant 项目可借鉴设计分析

> 分析日期：2026-03-09
> 对比基线：AgentDashboard (Rust + React) vs Actant v0.2.6 (TypeScript monorepo)
> 参考源码：`references/Actant/` (commit at 2026-03-09)

---

## 目录

1. [项目定位差异](#1-项目定位差异)
2. [思路清单总览](#2-思路清单总览)
3. [高优先级：系统健壮性](#3-高优先级系统健壮性)
   - 3.1 [指数退避重启策略 (RestartTracker)](#31-指数退避重启策略-restarttracker)
   - 3.2 [Per-Task 异步操作锁 (withAgentLock)](#32-per-task-异步操作锁-withagentlock)
   - 3.3 [LaunchMode 策略模式](#33-launchmode-策略模式)
4. [中优先级：上下文编排](#4-中优先级上下文编排)
   - 4.1 [三阶段上下文流水线](#41-三阶段上下文流水线)
   - 4.2 [双层物化策略 (静态文件 + 动态注入)](#42-双层物化策略-静态文件--动态注入)
   - 4.3 [ComponentTypeHandler 可扩展架构](#43-componenttypehandler-可扩展架构)
   - 4.4 [Archetype Scope 分级](#44-archetype-scope-分级)
   - 4.5 [Session Token 安全模型](#45-session-token-安全模型)
5. [中优先级：工作流编排](#5-中优先级工作流编排)
   - 5.1 [EmployeeScheduler 自动化调度](#51-employeescheduler-自动化调度)
   - 5.2 [HookEventBus 通用事件总线](#52-hookeventbus-通用事件总线)
   - 5.3 [InitializationPipeline 事务性流水线](#53-initializationpipeline-事务性流水线)
6. [低优先级：可扩展性](#6-低优先级可扩展性)
   - 6.1 [PluginHost 插件系统](#61-pluginhost-插件系统)
   - 6.2 [DeclarativeBuilder 数据驱动后端适配](#62-declarativebuilder-数据驱动后端适配)
   - 6.3 [VFS 虚拟文件系统上下文](#63-vfs-虚拟文件系统上下文)
   - 6.4 [BaseComponentManager 通用组件管理](#64-basecomponentmanager-通用组件管理)
7. [AgentDashboard 现有优势（不应丢失）](#7-agentdashboard-现有优势不应丢失)
8. [落地路线图建议](#8-落地路线图建议)

---

## 1. 项目定位差异

| 维度 | AgentDashboard | Actant |
|------|---------------|--------|
| 核心隐喻 | 看板/项目管理（Story → Task） | Docker for AI Agents（Template → Instance） |
| Agent 地位 | Task 的执行工具（通过 AgentBinding 绑定） | 一等公民（完整生命周期管理） |
| Task 定义 | Story 拆解出的结构化执行单元 | 调度器中的轻量 prompt 指令 |
| 架构风格 | 六边形架构 (Rust) | 插件化 Daemon (TypeScript) |
| 多 Agent 编排 | 通过 Story 拆解多个 Task | 通过 SubAgent + Scheduler |

两个项目在 **Agent 上下文编排** 和 **执行健壮性** 方面各有特色，下面按优先级梳理可借鉴点。

---

## 2. 思路清单总览

| # | 思路 | 优先级 | 分类 | 状态 | 来源文件 |
|---|------|--------|------|------|---------|
| 3.1 | 指数退避重启策略 | 高 | 健壮性 | ✅ 已落地 | `core/src/manager/restart-tracker.ts` |
| 3.2 | Per-Task 异步操作锁 | 高 | 健壮性 | ✅ 已落地 | `core/src/manager/agent-manager.ts` |
| 3.3 | LaunchMode 策略模式 | 高 | 健壮性 | ✅ 已落地 | `core/src/manager/launch-mode-handler.ts` |
| 4.1 | 三阶段上下文流水线 | 中 | 上下文 | 🔲 待实现 | Builder + Injector + Scheduler |
| 4.2 | 双层物化策略 | 中 | 上下文 | 🔲 待实现 | `core/src/initializer/context/context-materializer.ts` |
| 4.3 | ComponentTypeHandler 架构 | 中 | 上下文 | 🔲 待实现 | `core/src/builder/component-type-handler.ts` |
| 4.4 | Archetype Scope 分级 | 中 | 上下文 | 🔲 待实现 | `core/src/context-injector/session-context-injector.ts` |
| 4.5 | Session Token 安全模型 | 中 | 上下文 | 🔲 待实现 | `core/src/context-injector/session-token-store.ts` |
| 5.1 | EmployeeScheduler 调度 | 中 | 工作流 | 🔲 待实现 | `core/src/scheduler/` 目录 |
| 5.2 | HookEventBus 事件总线 | 中 | 工作流 | 🔲 待实现 | `core/src/hooks/hook-event-bus.ts` |
| 5.3 | InitializationPipeline | 中 | 工作流 | 🔲 待实现 | `core/src/initializer/pipeline/initialization-pipeline.ts` |
| 6.1 | PluginHost 插件系统 | 低 | 扩展性 | 🔲 待实现 | `core/src/plugin/plugin-host.ts` |
| 6.2 | DeclarativeBuilder | 低 | 扩展性 | 🔲 待实现 | `core/src/builder/declarative-builder.ts` |
| 6.3 | VFS 虚拟文件系统 | 低 | 扩展性 | 🔲 待实现 | `core/src/vfs/vfs-context-provider.ts` |
| 6.4 | BaseComponentManager | 低 | 扩展性 | 🔲 待实现 | `core/src/domain/base-component-manager.ts` |

> 以下所有文件路径前缀均为 `references/Actant/packages/`，省略不写。

---

## 3. 高优先级：系统健壮性

### 3.1 指数退避重启策略 (RestartTracker)

**问题**：AgentDashboard 的 `TaskStateReconciler` 仅在进程启动时做一次性修复。如果 Task 执行中 Agent 崩溃，Task 会停留在 Running 直到下次重启。

**Actant 方案**：

> 源码：`core/src/manager/restart-tracker.ts` (全文 114 行)

```typescript
// restart-tracker.ts:5-14
export interface RestartPolicy {
  maxRestarts: number;       // 最大重启次数，默认 5
  backoffBaseMs: number;     // 退避基准，默认 1000ms
  backoffMaxMs: number;      // 退避上限，默认 60000ms
  resetAfterMs: number;      // 稳定运行后重置计数，默认 300000ms (5分钟)
}
```

核心逻辑：

```typescript
// restart-tracker.ts:58-79
shouldRestart(instanceName: string): RestartDecision {
  // 1. 如果稳定运行超过 resetAfterMs，重置计数
  if (stableMs >= this.policy.resetAfterMs) {
    state.count = 0;
  }
  // 2. 超过 maxRestarts 则拒绝
  if (state.count >= this.policy.maxRestarts) {
    return { allowed: false, ... };
  }
  // 3. 指数退避计算延迟
  const delayMs = Math.min(
    this.policy.backoffBaseMs * Math.pow(2, state.count),
    this.policy.backoffMaxMs,
  );
  return { allowed: true, delayMs, attempt: state.count + 1 };
}
```

Employee Agent 使用独立的更宽松策略：

```typescript
// agent-manager.ts:153-158
this.employeeRestartTracker = new RestartTracker({
  maxRestarts: 100,
  backoffBaseMs: 2_000,
  backoffMaxMs: 120_000,
  resetAfterMs: 60_000,
});
```

**适配建议**：

在 `agentdash-application` 中创建 `TaskRestartTracker`，为每个 Task 维护重启状态。在 `task_execution.rs` 的 turn monitor 检测到失败后，根据策略决定是否自动重试。Rust 实现可用 `HashMap<Uuid, RestartState>` + `tokio::time::sleep`。

**关联现有代码**：
- `crates/agentdash-application/src/task_state_reconciler.rs` — 现有的一次性修复逻辑
- `crates/agentdash-application/src/task_execution.rs` — start_task 中的失败回滚
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs` — spawn_task_turn_monitor

---

### 3.2 Per-Task 异步操作锁 (withAgentLock)

**问题**：AgentDashboard 的 `start_task` / `cancel_task` / `continue_task` 之间没有应用层锁，同一个 Task 可能被并发操作。

**Actant 方案**：

> 源码：`core/src/manager/agent-manager.ts:971-985`

```typescript
private async withAgentLock<T>(name: string, fn: () => Promise<T>): Promise<T> {
  const prev = this.agentLocks.get(name) ?? Promise.resolve();
  let releaseFn!: () => void;
  const next = new Promise<void>((resolve) => { releaseFn = resolve; });
  this.agentLocks.set(name, next);
  await prev;             // 等待前一个操作完成
  try {
    return await fn();
  } finally {
    releaseFn();          // 释放锁，允许下一个操作
    if (this.agentLocks.get(name) === next) {
      this.agentLocks.delete(name);  // 清理空锁
    }
  }
}
```

所有生命周期操作（start/stop/destroy）都通过此锁串行化：

```typescript
// agent-manager.ts:306-308
async startAgent(name: string, options?): Promise<void> {
  return this.withAgentLock(name, () => this._startAgent(name, options));
}
```

**适配建议**：

Rust 中使用 `DashMap<Uuid, Arc<Mutex<()>>>` 或 `tokio::sync::Mutex` per-task：

```rust
// 概念性 Rust 实现
struct TaskLockMap {
    locks: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
}

impl TaskLockMap {
    async fn with_lock<F, R>(&self, task_id: Uuid, f: F) -> R
    where F: Future<Output = R> {
        let lock = self.locks.entry(task_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        f.await
    }
}
```

**关联现有代码**：
- `crates/agentdash-application/src/task_execution.rs` — start_task / continue_task / cancel_task
- `crates/agentdash-api/src/handlers/task_handler.rs` — HTTP 入口

---

### 3.3 LaunchMode 策略模式

**问题**：AgentDashboard 所有 Task 失败后行为一致（标记 Failed），无法区分"应该重试"、"应该清理"、"应该标记完成"等场景。

**Actant 方案**：

> 源码：`core/src/manager/launch-mode-handler.ts` (全文 97 行)

```typescript
// launch-mode-handler.ts:12-15
export type ProcessExitAction =
  | { type: "mark-stopped" }
  | { type: "restart" }
  | { type: "destroy" };
```

四种模式及其行为：

| 模式 | 进程退出行为 | 启动修复行为 | 场景 |
|------|------------|------------|------|
| `direct` | mark-stopped | mark-stopped | 用户直接操作 |
| `acp-background` | 无条件重启 | 无条件重启 | 雇员 Agent |
| `acp-service` | 策略重启 | 策略重启 | 服务型 Agent |
| `one-shot` | mark-stopped 或 destroy | mark-stopped | 一次性执行 |

```typescript
// launch-mode-handler.ts:87-96
const handlers: Record<LaunchMode, LaunchModeHandler> = {
  "direct": new DirectModeHandler(),
  "acp-background": new AcpBackgroundModeHandler(),
  "acp-service": new AcpServiceModeHandler(),
  "one-shot": new OneShotModeHandler(),
};

export function getLaunchModeHandler(mode: LaunchMode): LaunchModeHandler {
  return handlers[mode];
}
```

在 AgentManager 中根据模式决定恢复行为：

```typescript
// agent-manager.ts:1019-1021
const handler = getLaunchModeHandler(meta.launchMode);
const action = handler.getProcessExitAction(instanceName, meta);
// ... switch (action.type) { case "restart": ... case "destroy": ... }
```

**适配建议**：

在 Task 的 `AgentBinding` 或 Story 层面增加 `execution_mode` 字段：

```rust
enum TaskExecutionMode {
    Standard,       // 失败后标记 Failed，等待人工
    AutoRetry,      // 失败后自动重试（配合 RestartTracker）
    OneShot,        // 完成或失败后自动清理 session
    Persistent,     // 永续运行，崩溃自动重启
}
```

**关联现有代码**：
- `crates/agentdash-domain/src/task/value_objects.rs` — TaskStatus / AgentBinding 定义
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs` — spawn_task_turn_monitor 中的状态决策

---

## 4. 中优先级：上下文编排

### 4.1 三阶段上下文流水线

**当前状态**：AgentDashboard 在每次 `start_task` / `continue_task` 时实时构建全部上下文（`build_task_agent_context`），单阶段完成。

**Actant 方案**：三个阶段将上下文构建分层解耦。

```
阶段 1: 创建时 (WorkspaceBuilder)        → 静态文件写入工作区
阶段 2: 启动时 (SessionContextInjector)  → 运行时 MCP/Tools/Context 注入
阶段 3: 运行时 (EmployeeScheduler/API)   → 动态 prompt 发送
```

**源码索引**：

| 阶段 | 核心类 | 文件 |
|------|-------|------|
| 1 | `WorkspaceBuilder` | `core/src/builder/workspace-builder.ts` |
| 1 | `ContextMaterializer` | `core/src/initializer/context/context-materializer.ts` |
| 1 | `BackendBuilder` | `core/src/builder/backend-builder.ts` (接口) |
| 1 | `DeclarativeBuilder` | `core/src/builder/declarative-builder.ts` |
| 2 | `SessionContextInjector` | `core/src/context-injector/session-context-injector.ts` |
| 2 | `CoreContextProvider` | `core/src/context-injector/core-context-provider.ts` |
| 2 | `CanvasContextProvider` | `core/src/context-injector/canvas-context-provider.ts` |
| 2 | `VfsContextProvider` | `core/src/vfs/vfs-context-provider.ts` |
| 3 | `EmployeeScheduler` | `core/src/scheduler/employee-scheduler.ts` |
| 3 | `AgentManager.promptAgent` | `core/src/manager/agent-manager.ts:868-961` |

**适配建议**：

在 Workspace 准备阶段（或 Story 进入 Decomposed 时），将 Story 的 PRD、spec_refs 等预物化到工作区目录。Task 启动时只需注入增量上下文。

**关联现有代码**：
- `crates/agentdash-api/src/task_agent_context.rs` — 当前单阶段构建
- `crates/agentdash-domain/src/story/value_objects.rs` — StoryContext 定义

---

### 4.2 双层物化策略 (静态文件 + 动态注入)

**Actant 方案**：同一份上下文通过两条路径到达 Agent。

> 源码：`core/src/initializer/context/context-materializer.ts:48-81`

| 组件 | 静态路径（文件写入工作区） | 动态路径（ACP 注入） |
|------|----------------------|-------------------|
| Skills | `AGENTS.md` | — (Agent 自动读取文件) |
| MCP Servers | `.cursor/mcp.json` | ACP `session/new` 的 `mcpServers` |
| Prompts | `prompts/system.md` | — |
| Workflow | `.trellis/workflow.md` | — |
| Identity | — | `systemContext` additions |
| Tools | — | ACP `tools` + CLI token auth |

关键洞察：Cursor/Claude Code 等 Agent 后端会**原生读取**工作区中的 `AGENTS.md` 和 `.cursor/mcp.json`，无需额外 prompt 注入。Actant 巧妙利用了这一点。

Claude Code 还需要额外的权限预批准：

```typescript
// context-materializer.ts:131-164
private async materializeClaudePermissions(...) {
  const allowedTools = ["Bash", "Read", "Write", "Edit", "MultiEdit", ...];
  for (const server of servers) {
    allowedTools.push(`mcp__${server.name}`);
  }
  const settings = { permissions: { allow: allowedTools, deny: [] } };
  await writeFile(join(configDir, "settings.local.json"), ...);
}
```

**适配建议**：

当 AgentDashboard 的 Connector 是 `claude-code` 或 `cursor` 时，可以在 `start_task_turn` 之前，将 Task 上下文物化为工作区文件，作为 prompt 注入的**备份通道**。

**关联现有代码**：
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs` — `start_task_turn` 流程
- `crates/agentdash-executor/src/connectors/` — 各 Connector 实现

---

### 4.3 ComponentTypeHandler 可扩展架构

**Actant 方案**：

> 源码：`core/src/builder/component-type-handler.ts` (全文 20 行)

```typescript
export interface ComponentTypeHandler<TDef = unknown> {
  readonly contextKey: string;  // DomainContext 中的字段名
  resolve(refs: unknown, manager?: BaseComponentManager<NamedComponent>): TDef[];
  materialize(workspaceDir: string, definitions: TDef[], ...): Promise<void>;
}
```

每种组件类型一个 Handler，统一的 `resolve → materialize` 二步法：

> 源码（示例）：`core/src/builder/handlers/skills-handler.ts`

```typescript
export const skillsHandler: ComponentTypeHandler<SkillDefinition> = {
  contextKey: "skills",
  resolve(refs, manager) {
    return manager?.resolve(refs as string[]) ?? refs.map(name => ({ name, content: `- ${name}` }));
  },
  async materialize(workspaceDir, definitions, _backendType, builder) {
    await (builder as BackendBuilder).materializeSkills(workspaceDir, definitions);
  },
};
```

WorkspaceBuilder 通过注册表遍历所有 Handler：

```typescript
// workspace-builder.ts:125-138
for (const handler of this.handlers) {
  const refs = domainContext[handler.contextKey as keyof DomainContextConfig];
  if (...) continue;
  const manager = this.getManager(handler.contextKey);
  const definitions = handler.resolve(refs, manager);
  if (definitions.length > 0) {
    await handler.materialize(workspaceDir, definitions, backendType, activeBuilder);
  }
}
```

**对比 AgentDashboard**：

AgentDashboard 的 `ContextContributor` trait 概念类似，但 Contributors 是在 `build_task_agent_context` 中硬编码组装的：

```rust
// task_agent_context.rs:385-394
let builtin_contributors: Vec<Box<dyn ContextContributor>> = vec![
    Box::new(CoreContextContributor),
    Box::new(BindingContextContributor),
    Box::new(DeclaredSourcesContributor),
    Box::new(InstructionContributor),
];
let all_contributors = builtin_contributors.into_iter().chain(input.extra_contributors);
```

**适配建议**：

将 `extra_contributors` 从函数参数改为注册表模式。在 `AppState` 中维护 `Vec<Box<dyn ContextContributor>>` 的注册表，新增上下文来源只需注册 Contributor 而不需要修改构建入口。

---

### 4.4 Archetype Scope 分级

**Actant 方案**：

> 源码：`core/src/context-injector/session-context-injector.ts:17-21`

```typescript
export type ToolScope = "employee" | "service" | "all";
const ARCHETYPE_LEVEL: Record<AgentArchetype, number> = { repo: 0, service: 1, employee: 2 };
const SCOPE_MIN_LEVEL: Record<ToolScope, number> = { all: 0, service: 1, employee: 2 };
```

过滤逻辑：

```typescript
// session-context-injector.ts:137-139
const level = ARCHETYPE_LEVEL[meta.archetype] ?? 0;
if (level < SCOPE_MIN_LEVEL[tool.scope]) continue;  // 低级别 Agent 跳过高级别工具
```

效果：`repo` 级 Agent 只能用基础工具，`service` 级可用 Canvas，`employee` 级拥有全部能力。

**适配建议**：

AgentDashboard 中的 MCP 工具目前有 `ToolScope`（Story/Task/Relay）但用于标识工具归属，不用于 Agent 权限过滤。可以扩展为双维度：`scope`（归属）+ `min_level`（最低 Agent 等级要求）。

**关联现有代码**：
- `crates/agentdash-mcp/src/scope.rs` — 现有 ToolScope 定义
- `crates/agentdash-mcp/src/servers/task.rs` — Task 级 MCP 工具定义

---

### 4.5 Session Token 安全模型

**Actant 方案**：

> 源码：`core/src/context-injector/session-token-store.ts` (全文 101 行)

```typescript
export class SessionTokenStore {
  private byToken = new Map<string, SessionToken>();    // token → 元数据
  private bySession = new Map<string, string>();        // sessionId → token
  private byAgent = new Map<string, Set<string>>();     // agentName → tokens

  generate(agentName, sessionId, pid?): string {
    this.revokeSession(sessionId);  // 旧 session token 自动失效
    const token = randomBytes(32).toString("hex");
    // 三重索引存储 ...
    return token;
  }

  validate(token): SessionToken | null { ... }
  revokeSession(sessionId): boolean { ... }
  revokeByAgent(agentName): number { ... }  // Agent 停止时批量清理
}
```

Token 通过 system context 传递给 Agent，Agent 调用内部工具时需携带：

```typescript
// session-context-injector.ts:178-194  (buildToolContextBlock)
// 生成类似："Usage: actant internal canvas_update --token $ACTANT_SESSION_TOKEN --html <string>"
```

**适配建议**：

AgentDashboard 当前 MCP 通过 URL 路径中的 task_id 做隐式鉴权。可以增加 per-session token 做显式鉴权，防止 Agent 越权调用其他 Task 的 MCP 工具。

**关联现有代码**：
- `crates/agentdash-mcp/src/injection.rs` — MCP URL 构建（含 task_id）
- `crates/agentdash-mcp/src/servers/task.rs` — 工具注册

---

## 5. 中优先级：工作流编排

### 5.1 EmployeeScheduler 自动化调度

**问题**：AgentDashboard 的 Task 执行完全由用户手动触发（`POST /tasks/{id}/start`），没有自动化调度能力。Story 下多个 Task 不能自动串联执行。

**Actant 方案**：三种输入源 + 优先级队列 + 串行分发。

> 源码目录：`core/src/scheduler/`

| 文件 | 职责 |
|------|------|
| `types.ts` | AgentTask / ExecutionRecord 类型定义 |
| `task-queue.ts` | 按 Agent 隔离的优先级队列 |
| `task-dispatcher.ts` | 轮询队列、串行执行、idle 事件发射 |
| `employee-scheduler.ts` | 调度器门面，整合 Queue + Dispatcher + InputRouter |
| `inputs/input-router.ts` | 输入源注册与统一路由 |
| `inputs/heartbeat-input.ts` | 心跳定时触发 |
| `inputs/cron-input.ts` | Cron 表达式触发 |
| `inputs/hook-input.ts` | 事件驱动触发 |

TaskQueue 关键设计——每个 Agent 独立队列 + 处理中标记：

```typescript
// task-queue.ts:10-29
export class TaskQueue {
  private queues = new Map<string, AgentTask[]>();
  private processing = new Set<string>();

  enqueue(task: AgentTask): void {
    // ... 入队后按优先级排序
    queue.sort((a, b) => PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority]);
  }

  dequeue(agentName: string): AgentTask | undefined {
    if (this.processing.has(agentName)) return undefined;  // 正在处理则不出队
    // ...
  }
}
```

TaskDispatcher 的 tick 循环——串行执行 + idle 事件：

```typescript
// task-dispatcher.ts:69-90
async tick(): Promise<void> {
  for (const agentName of this.agentNames) {
    if (this.queue.isProcessing(agentName)) continue;
    const task = this.queue.dequeue(agentName);
    if (!task) {
      if (this.busyAgents.has(agentName)) {
        this.busyAgents.delete(agentName);
        this.hookEventBus?.emit("idle", ...);  // 队列耗尽时发射 idle
      }
      continue;
    }
    this.busyAgents.add(agentName);
    void this.executeTask(task);  // fire-and-forget，markDone 在 finally 中
  }
}
```

**适配建议**：

AgentDashboard 可以在 Story 层面引入 Task 调度器：
- Story 进入 `Executing` 后，自动将 `Pending` Task 按优先级入队
- 前一个 Task 完成（`AwaitingVerification` → `Completed`）后，自动启动下一个
- 支持 Hook 触发（如"当 Task A 完成时启动 Task B"）

**关联现有代码**：
- `crates/agentdash-api/src/handlers/story_handler.rs` — Story 状态管理
- `crates/agentdash-application/src/task_execution.rs` — Task 执行入口

---

### 5.2 HookEventBus 通用事件总线

**Actant 方案**：

> 源码：`core/src/hooks/hook-event-bus.ts` (全文 175 行)

```typescript
export class HookEventBus {
  private readonly emitter = new EventEmitter();
  private emitGuard: EmitGuard | null = null;      // 事件发射权限守卫
  private journal: EventJournal | null = null;      // 事件持久化日志
  private readonly recentBuffer: HookEventPayload[] = [];  // 最近 N 条缓存
  private readonly maxRecent: number;  // 默认 500

  emit(event, context, agentName?, data?) {
    // 1. EmitGuard 权限校验
    if (this.emitGuard && !this.emitGuard(event, context)) return;
    // 2. 构建带 caller context 的 payload
    const payload: HookEventPayload = { event, agentName, data, timestamp, callerType, callerId };
    // 3. Ring buffer 缓存
    this.recentBuffer.push(payload);
    // 4. EventJournal 持久化
    this.journal?.append("hook", event, payload);
    // 5. 分发给监听器（异常隔离）
    for (const listener of listeners) {
      try { listener(payload); } catch { /* 单个监听器错误不影响其他 */ }
    }
  }
}
```

核心特性：
- **EmitGuard**：事件发射前的权限校验，可拒绝非法事件源
- **Caller Context**：每个事件携带 `callerType`（system/agent/plugin/user）+ `callerId`
- **EventJournal**：所有事件写入磁盘，支持审计回溯
- **Ring Buffer**：最近 500 条事件内存缓存，用于 Dashboard 实时展示
- **异常隔离**：单个监听器失败不影响其他

**对比 AgentDashboard**：

AgentDashboard 有 `StateChange` 事件系统，但：
- 仅记录实体状态变更，不支持通用事件
- 没有权限守卫
- 没有订阅/触发钩子能力

**适配建议**：

将现有 `StateChange` 系统扩展为通用事件总线，增加：
- 自定义事件类型（如 `task:retry`、`story:all_tasks_done`）
- 事件钩子注册（允许 Task 完成后自动触发下一个 Task）
- Caller Context（区分系统自动触发 vs 用户手动触发）

**关联现有代码**：
- `crates/agentdash-domain/src/state_change.rs` — 现有 StateChange 定义
- `crates/agentdash-api/src/events.rs` — SSE 事件推送

---

### 5.3 InitializationPipeline 事务性流水线

**Actant 方案**：

> 源码：`core/src/initializer/pipeline/initialization-pipeline.ts` (全文 163 行)

```typescript
// initialization-pipeline.ts:31-43
export class InitializationPipeline {
  // 单步超时 60s，总体超时 300s
  // run() 方法：顺序执行，任何步骤失败时反向回滚
}
```

关键设计：

```typescript
// initialization-pipeline.ts:62-123
async run(steps, context): Promise<PipelineResult> {
  for (let i = 0; i < steps.length; i++) {
    // 总体超时检查
    if (Date.now() - pipelineStart > this.totalTimeoutMs) {
      await this.rollback(executed, context, err);
      return { success: false, ... };
    }
    try {
      const result = await this.executeWithTimeout(executor, context, step.config);
      executed.push({ index: i, executor, config });
    } catch (err) {
      await this.rollback(executed, context, error);  // ← 反向回滚
      return { success: false, ... };
    }
  }
}

// 反向回滚
private async rollback(executed, context, triggerError) {
  for (let i = executed.length - 1; i >= 0; i--) {
    await executor.rollback?.(context, config, triggerError);  // best-effort
  }
}
```

支持 dry-run 预校验：

```typescript
dryRun(steps): StepValidationResult[] {
  return steps.map(step => executor.validate(step.config ?? {}));
}
```

内置步骤类型：

> 源码：`core/src/initializer/steps/` 目录

| 步骤 | 文件 | 用途 |
|------|------|------|
| `git-clone` | `git-clone-step.ts` | 克隆仓库 |
| `npm-install` | `npm-install-step.ts` | 安装依赖 |
| `file-copy` | `file-copy-step.ts` | 复制文件 |
| `mkdir` | `mkdir-step.ts` | 创建目录 |
| `exec` | `exec-step.ts` | 执行命令 |

**适配建议**：

AgentDashboard 如果需要支持复杂的 Workspace 准备（克隆仓库、安装依赖、准备环境），可以实现类似的 Pipeline。关键是**事务性回滚**——如果第 3 步失败，前 2 步的操作需要被清理。

**关联现有代码**：
- `crates/agentdash-domain/src/workspace/` — Workspace 模型
- `crates/agentdash-api/src/handlers/workspace_handler.rs` — Workspace CRUD

---

## 6. 低优先级：可扩展性

### 6.1 PluginHost 插件系统

> 源码：`core/src/plugin/plugin-host.ts` (全文 329 行)

核心设计：
- **Kahn 拓扑排序**：按依赖顺序初始化插件（`topoSort()` 方法，L285-327）
- **6-plug 接口**：`init → hooks → start → contextProviders → subsystems → sources`
- **异常隔离**：单个插件的 init/start 失败不影响其他（L100-108）
- **Tick 防重入**：`ticking` 标志防止慢插件堆积（L174-198）
- **反向停止**：`[...this.sortedNames].reverse()` 确保依赖方先停（L207-232）

内置插件示例：

> 源码：`core/src/plugin/builtins/heartbeat-plugin.ts` — HeartbeatPlugin

**适配建议**：当 AgentDashboard 需要支持第三方 Connector 或自定义上下文来源时考虑。

---

### 6.2 DeclarativeBuilder 数据驱动后端适配

> 源码：`core/src/builder/declarative-builder.ts` (全文 263 行)

新增 Agent 后端不需要写代码，只需提供 `MaterializationSpec` JSON：

```typescript
export class DeclarativeBuilder implements BackendBuilder {
  constructor(backendType, private readonly spec: MaterializationSpec) { ... }
  // 所有 materialize* 方法都从 spec 读取配置，通用逻辑一套搞定
}
```

**适配建议**：AgentDashboard 的 Connector 目前每种后端手写。如果后端类型增多（Codex、Gemini、Windsurf...），可以抽象出声明式配置，减少重复代码。

**关联现有代码**：
- `crates/agentdash-executor/src/connector.rs` — Connector trait
- `crates/agentdash-executor/src/connectors/` — 各实现

---

### 6.3 VFS 虚拟文件系统上下文

> 源码：`core/src/vfs/vfs-context-provider.ts` (全文 52 行)

通过 VFS 让 Agent 用标准 Read/Write 访问虚拟路径：

```typescript
// vfs-context-provider.ts:39-43
lines.push('  Read("/proc/<agent>/<pid>/stdout")       -- read process output');
lines.push('  Read("/memory/<agent>/notes.md")         -- read agent memory');
lines.push('  Write("/proc/<agent>/<pid>/cmd", "stop") -- send command to process');
lines.push('  Read("/vcs/status")                      -- git status');
```

**适配建议**：AgentDashboard 通过 MCP 工具暴露信息（`get_story_context`、`get_sibling_tasks`），可以补充一个虚拟文件接口，让 Agent 也能通过路径方式访问。

> 源码目录：`core/src/vfs/` — VfsRegistry, VfsPathResolver, sources/, storage/

---

### 6.4 BaseComponentManager 通用组件管理

> 源码：`core/src/domain/base-component-manager.ts` (全文 314 行)

泛型基类，为 Skill/Prompt/MCP/Workflow/Plugin 提供统一的：
- **CRUD** + 可选持久化（L80-108）
- **名称解析**（`resolve(names)` L54-62）
- **目录加载**（JSON 文件 + manifest.json 目录，L177-258）
- **Schema 校验**（Zod，子类实现 `validate`）
- **导入/导出**（L114-132）
- **搜索/过滤**（L138-150）

子类只需声明 `componentType` 和实现 `validate`：

> 源码示例：`core/src/domain/skill/skill-manager.ts` (全文 50 行)

**适配建议**：如果 AgentDashboard 未来需要管理 Prompt 模板、Skill 库等领域组件，这个基类提供了很好的参考。

---

## 7. AgentDashboard 现有优势（不应丢失）

在学习 Actant 的过程中，以下 AgentDashboard 的设计优势需要保持：

| 优势 | 说明 | 现有实现 |
|------|------|---------|
| **Story 两级拆解** | Story → Task 比 Actant 的扁平 AgentTask 更适合复杂需求 | `agentdash-domain/src/story/` |
| **声明式上下文 (source_refs)** | `ContextSourceRef` 支持 ManualText/FileContent/HttpFetch 等多种来源 | `agentdash-injection/` |
| **MCP 双向工具** | Task/Story MCP 让 Agent 可以回报状态和查询上下文 | `agentdash-mcp/src/servers/` |
| **AwaitingVerification** | 人工审核环节是 Actant 完全没有的 | `task/value_objects.rs` TaskStatus |
| **ContextComposer 分槽合并** | slot + order + MergeStrategy 提供细粒度控制 | `agentdash-injection/` |
| **Rust 编译期安全** | 类型系统比 TypeScript + Zod 更强 | 全局 |
| **SSE 增量事件** | StateChange 支持 since_id 增量恢复 | `agentdash-api/src/events.rs` |

---

## 8. 落地路线图建议

### Phase 1：健壮性基础（建议 1-2 周）

| 序号 | 任务 | 参考 | 影响范围 | 状态 |
|------|------|------|---------|------|
| 1 | 实现 Per-Task 操作锁 | §3.2 | `task/lock.rs` + `task_execution_gateway.rs` | ✅ 已落地 |
| 2 | 实现 RestartTracker | §3.1 | `task/restart_tracker.rs` + Turn Monitor + State Reconciler | ✅ 已落地 |
| 3 | 引入 TaskExecutionMode | §3.3 | `value_objects.rs` + `task_execution_gateway.rs` + `state_reconciler.rs` | ✅ 已落地 |

> **落地说明（2026-03-09）**：
> - §3.2 Per-Task 操作锁：`agentdash-application/src/task/lock.rs` 实现 `TaskLockMap`，集成到 `AppState`，所有 Task 生命周期操作（start/continue/cancel）通过 `with_lock()` 串行化。
> - §3.1 RestartTracker：`agentdash-application/src/task/restart_tracker.rs` 实现指数退避重启策略。Turn Monitor 中 `turn_failed` 和 `turn_monitor_closed` 分支现在会咨询 RestartTracker，允许重试时自动发起 `continue_task`。State Reconciler 启动回收时也应用 RestartTracker 策略，失败但有重试额度的 Task 标记为 `AwaitingVerification` 而非 `Failed`。
> - §3.3 TaskExecutionMode：`agentdash-domain/src/task/value_objects.rs` 定义 `TaskExecutionMode` 枚举（Standard / AutoRetry / OneShot），Task 实体新增 `execution_mode` 字段。Turn Monitor 的 `resolve_failure_outcome` 和 State Reconciler 的 `plan_for_running_task` 现在根据 `execution_mode` 分派行为——仅 `AutoRetry` 模式才咨询 RestartTracker 进行自动重试，`Standard` 直接标记 Failed，`OneShot` 标记 Failed 并清理。SQLite 持久化层和前端类型已同步。
>
> **Phase 1 全部完成 ✅**

### Phase 2：上下文增强（建议 2-3 周）

| 序号 | 任务 | 参考 | 影响范围 |
|------|------|------|---------|
| 4 | ContextContributor 注册表化 | §4.3 | `task_agent_context.rs` |
| 5 | MCP 工具 Scope 分级 | §4.4 | `agentdash-mcp/` |
| 6 | 工作区文件预物化（双层策略） | §4.2 | `task_execution_gateway.rs` |

### Phase 3：工作流自动化（建议 2-4 周）

| 序号 | 任务 | 参考 | 影响范围 |
|------|------|------|---------|
| 7 | Story 级 Task 自动调度 | §5.1 | 新模块 `task_scheduler.rs` |
| 8 | StateChange 扩展为通用事件总线 | §5.2 | `state_change.rs` + `events.rs` |
| 9 | Workspace 初始化 Pipeline | §5.3 | `workspace/` 模块 |

### Phase 4：可扩展性（按需）

| 序号 | 任务 | 参考 | 影响范围 |
|------|------|------|---------|
| 10 | Session Token 鉴权 | §4.5 | `agentdash-mcp/` |
| 11 | Connector 声明式适配 | §6.2 | `agentdash-executor/` |
| 12 | 插件系统原型 | §6.1 | 新 crate |

---

## 附录：快速源码索引

所有路径相对于 `references/Actant/packages/`：

| 模块 | 关键文件 | 行数 | 核心抽象 |
|------|---------|------|---------|
| 重启策略 | `core/src/manager/restart-tracker.ts` | 114 | `RestartPolicy`, `RestartTracker` |
| 操作锁 | `core/src/manager/agent-manager.ts` | L971-985 | `withAgentLock()` |
| 启动模式 | `core/src/manager/launch-mode-handler.ts` | 97 | `LaunchModeHandler`, `ProcessExitAction` |
| Agent 管理 | `core/src/manager/agent-manager.ts` | 1196 | `AgentManager` |
| 进程监控 | `core/src/manager/launcher/process-watcher.ts` | — | `ProcessWatcher` |
| 上下文注入 | `core/src/context-injector/session-context-injector.ts` | 194 | `SessionContextInjector`, `ContextProvider` |
| 核心身份 | `core/src/context-injector/core-context-provider.ts` | 31 | `CoreContextProvider` |
| Canvas 上下文 | `core/src/context-injector/canvas-context-provider.ts` | 41 | `CanvasContextProvider` |
| Token 存储 | `core/src/context-injector/session-token-store.ts` | 101 | `SessionTokenStore` |
| 上下文物化 | `core/src/initializer/context/context-materializer.ts` | 207 | `ContextMaterializer` |
| 工作区构建 | `core/src/builder/workspace-builder.ts` | 167 | `WorkspaceBuilder` |
| 后端构建接口 | `core/src/builder/backend-builder.ts` | 48 | `BackendBuilder` |
| 声明式构建 | `core/src/builder/declarative-builder.ts` | 263 | `DeclarativeBuilder` |
| 组件 Handler | `core/src/builder/component-type-handler.ts` | 20 | `ComponentTypeHandler` |
| Handler 示例 | `core/src/builder/handlers/skills-handler.ts` | 14 | `skillsHandler` |
| Handler 示例 | `core/src/builder/handlers/mcp-servers-handler.ts` | 34 | `mcpServersHandler` |
| 模板 Schema | `core/src/template/schema/template-schema.ts` | 150 | `AgentTemplateSchema` |
| 模板引擎 | `core/src/prompts/template-engine.ts` | 54 | `loadTemplate`, `renderTemplate` |
| 组件基类 | `core/src/domain/base-component-manager.ts` | 314 | `BaseComponentManager<T>` |
| Skill 管理 | `core/src/domain/skill/skill-manager.ts` | 50 | `SkillManager` |
| 任务队列 | `core/src/scheduler/task-queue.ts` | 75 | `TaskQueue` |
| 任务分发 | `core/src/scheduler/task-dispatcher.ts` | 135 | `TaskDispatcher` |
| 雇员调度 | `core/src/scheduler/employee-scheduler.ts` | 143 | `EmployeeScheduler` |
| 输入路由 | `core/src/scheduler/inputs/input-router.ts` | 73 | `InputRouter` |
| 事件总线 | `core/src/hooks/hook-event-bus.ts` | 175 | `HookEventBus` |
| 初始化流水线 | `core/src/initializer/pipeline/initialization-pipeline.ts` | 163 | `InitializationPipeline` |
| 插件宿主 | `core/src/plugin/plugin-host.ts` | 329 | `PluginHost` |
| VFS 上下文 | `core/src/vfs/vfs-context-provider.ts` | 52 | `VfsContextProvider` |
| 实例注册表 | `core/src/state/instance-registry.ts` | 188 | `InstanceRegistry` |
