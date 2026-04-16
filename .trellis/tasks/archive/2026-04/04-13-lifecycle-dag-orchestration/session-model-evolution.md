# Session 模型演进：观测层与执行层分离

> 本文档记录 lifecycle DAG 化后，session 模型从 1:1 扁平结构演进为分层结构的设计讨论。

## 1. 当前模型

```
前端会话 ═══ AgentSession ═══ Task owner ═══ LifecycleRun
         全部 1:1:1:1
```

关键数据流：
- 用户在前端打开 task → 看到一个 session 对话流
- 这个 session 就是执行 agent session，也是唯一的 lifecycle 观测窗口
- `Task.session_id: Option<String>` 硬编码了 1:1 关系
- lifecycle run 通过 `(binding_kind=Task, binding_id=task.id)` 间接关联

## 2. 目标模型

### 2.1 核心变化：两层分离

```
┌──────────────────────────────────────────────────────────┐
│  Lifecycle Session（前端观测容器）                         │
│  绑定: LifecycleRun                                      │
│  本身不是一个 agent session，是 lifecycle run 的前端视图    │
│                                                          │
│  ┌── agent_node_1: research ──────────────────────────┐  │
│  │  AgentSession_A（独立 agent session）               │  │
│  │  状态: completed ✅                                 │  │
│  │  [展开查看完整对话]                                  │  │
│  └────────────────────────────────────────────────────┘  │
│                      ↓                                    │
│  ┌── agent_node_2: implement ─────────────────────────┐  │
│  │  AgentSession_B（独立 agent session）               │  │
│  │  状态: running 🔄                                   │  │
│  │  [展开查看完整对话]                                  │  │
│  └────────────────────────────────────────────────────┘  │
│                      ↓                                    │
│  ┌── agent_node_3: check ─────────────────────────────┐  │
│  │  (尚未启动)                                         │  │
│  │  状态: pending ⏳                                   │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  [lifecycle 全局状态: 2/3 nodes completed]                │
└──────────────────────────────────────────────────────────┘
```

### 2.2 关键设计决策

**Lifecycle session ≠ agent session。**

- Lifecycle session 是 lifecycle run 的 **前端观测视图**，不绑定自己的 agent
- Agent node 创建独立 agent session，phase node 在前一个 session 内切换 workflow contract
- 用户和哪个 session 交互，完全是前端层面的能力——底层所有 session 都完全支持输入
- Lifecycle 的编排由 `LifecycleOrchestrator` 服务承担，推进由 agent 主动 tool call 触发

### 2.3 Node 类型

| 类型 | 行为 |
|---|---|
| **Agent Node** | 创建独立 agent session 执行工作。 |
| **Phase Node** | 不创建新 session，在前一个 session 内切换 workflow contract。 |

两者都是 DAG 中的一级节点。连续的 phase node 共享同一个 session（由前一个 agent node 创建）。

命名理由：
- "dispatch node" → "agent node"：不暗示有 dispatcher，直接表达"跑一个 agent"
- "phase node" 保持：直接表达"阶段切换"

## 3. 前端 Lifecycle Session 的构成

### 3.1 Lifecycle Session = lifecycle run 的前端视图

前端拉起一个 "lifecycle session" 时：

1. 获取 lifecycle run 状态（nodes、当前进度、artifacts）
2. 获取每个 agent node 关联的 session_id
3. 渲染为一个统一视图：
   - 顶部：lifecycle 进度条 / DAG 视图
   - 主体：按 node 顺序排列的 agent session 卡片（可展开）
   - 每个卡片内嵌 `AcpSessionList`（完整消息流）

### 3.2 用户交互

所有 agent session 在底层都完全支持用户输入。前端决定是否为某个 session 提供输入框：

- **当前活跃的 agent node session** → 提供输入框，用户可以直接和 agent 对话
- **已完成的 agent node session** → 默认只读展示，但可以选择"重新对话"
- **未启动的 node** → 不可交互

这个决策完全在前端组件层面，不需要后端区分"可交互"和"只读"。

### 3.3 Lifecycle session 是 node 列表视图

Lifecycle session 本身不产生消息，没有"主消息流"。它是一个 node 列表视图，每个 node 有自己的 session 消息流。类似项目看板——每个 node 是一张卡片，展开后看到该 agent 的完整对话。

## 4. 数据模型

### 4.1 Session 和 Lifecycle Run 的关联

不需要新增 "LifecycleSession" 实体。关联通过以下方式建立：

**在 LifecycleRun 的 node state 上记录 session_id：**

```rust
pub struct LifecycleNodeState {
    pub node_key: String,
    pub status: LifecycleNodeExecutionStatus,
    pub session_id: Option<String>,      // 该 node 创建的 agent session
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub context_snapshot: Option<Value>,
}
```

**在 SessionBinding 上标记 lifecycle 归属：**

```
SessionBinding {
  session_id: agent_session_id,
  owner_type: "task",
  owner_id: task_uuid,
  label: "lifecycle_node:{node_key}"
}
```

**不扩展 CompanionSessionContext。** Lifecycle 身份信息通过 SessionBinding label + LifecycleRun.node_state.session_id 建立，不污染 companion 的数据结构（见 D5）。

### 4.2 查询路径

前端获取 lifecycle session 视图：

```
1. GET lifecycle_run by (binding_kind, binding_id) → 获取 run + node_states
2. 每个 node_state.session_id → 获取 agent session 状态
3. 用户展开某个 node → 建立 WebSocket 连接获取该 session 的消息流
```

## 5. 和 Companion 机制的关系

### 5.1 Agent Node 的 session 创建

Agent node 的 session 创建不通过 companion_request 工具（因为没有 "host agent" 来调用工具）。

而是由 **LifecycleOrchestrator 服务** 直接创建，走标准 session 创建流程：

```
LifecycleOrchestrator 评估 node 可达性
  → node ready
  → 调用 SessionHub.create_session
  → 设置 SessionBinding(label="lifecycle_node:{node_key}")
  → 记录 session_id 到 LifecycleRun.node_state
  → 根据 node 的 workflow_key 加载 WorkflowContract
  → context_bindings resolve（含 lifecycle:// VFS locator）
  → Hook injection 注入 instructions + context
  → 启动 prompt
```

### 5.2 Companion 仅用于 agent node 内部 subagent

Companion 和 lifecycle 是独立机制。唯一交集：agent node session 内部通过 companion_request 派发 subagent。

Node 间产物传递走 lifecycle VFS locator（D3），node 推进走 agent tool call（D2），都不涉及 companion。

### 5.3 Node 完成后的 Orchestrator 行为

Agent 通过 tool call 声明 node 完成（D2）后，session 结束。Orchestrator 随后：

1. 从 lifecycle run 读取已更新的 node state
2. 评估后继 node 是否可达（all-complete join）
3. 如果可达，启动下一个 node（创建 session 或切换 contract）

注意：Orchestrator 不做"推进"——推进在 agent tool call 时已经完成。Orchestrator 只是响应已发生的状态变更，启动后继 node。

## 6. 生命周期边界

### 6.1 Lifecycle Session 的生命周期

跟 **lifecycle run** 对齐，不跟任何 agent session 对齐：

- Lifecycle run created → lifecycle session 可被打开
- Lifecycle run running → 前端显示进度，可展开活跃 node
- Lifecycle run completed → 前端显示完成状态，所有 node 可回顾
- 用户关闭前端再打开 → lifecycle session 恢复（因为它只是 run 的视图）

### 6.2 Agent Session 的生命周期

跟 **node 执行** 对齐：

- Node activated → session 创建并启动
- Node running → session 执行中
- Node completed → session 可能已 terminal，产物已回写
- Agent session 可以被重新打开查看历史（只读或继续交互，前端决定）

## 7. 和当前 TaskWorkflowPanel 的关系

现有的 `TaskWorkflowPanel` 已经在做类似的事情——展示 lifecycle run 的状态和 step 列表。它可以演进为新 lifecycle session 视图的基础：

- 现有：显示 step 列表 + 状态 badge + session 绑定信息
- 演进：每个 node 可展开为完整 session 消息流 + lifecycle 进度条 + artifact 展示

## 8. 关键不确定点

### Q1: Lifecycle Session 的前端入口

用户如何"打开"一个 lifecycle session？

- (a) 从 task 详情页点击 "查看 Lifecycle" → 在当前页面内展开
- (b) Lifecycle session 有独立的 URL/路由（`/lifecycle-session/{run_id}`）
- (c) 在 session 列表中直接显示 lifecycle session 作为一级条目

### Q2: DAG 推进的唯一路径（已决策 D2）

Agent 通过 tool call 主动声明 node 完成 → Orchestrator 验证门禁 → 放行或拒绝。

不满足条件时 session 不允许结束（before_stop hook 拦截）。不存在 "terminal 事件自动推进" 的隐式退路。

### Q3: 跨 lifecycle 层级的 session 关系可视化

当 agent node session 内部又通过 companion 派发了 subagent，前端需要展示：

```
Lifecycle Session
  └── agent_node: implement
        └── AgentSession_B
              └── companion: code_review (subagent)
                    └── AgentSession_C
```

这是三层嵌套。现有的 `active-session-list.tsx` 已支持 parent-child 嵌套，但在 lifecycle 视图中如何优雅展示需要设计。
