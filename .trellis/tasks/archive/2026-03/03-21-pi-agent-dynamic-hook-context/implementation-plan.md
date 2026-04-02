# Pi Agent 动态 Hook 上下文与伴随 Agent 机制实施方案

## 1. 总体结论

本阶段要落的不是“多几个 prompt 模板”，而是一套正式 Hook Runtime。

这套 Runtime 需要同时解决两个问题：

1. 如何在执行层及时拿到 Hook 决策，影响当前 loop 控制流
2. 如何在不污染 `agent_loop` 的前提下，把 workflow/task/trellis/workspace 等业务信息稳定提供给 Hook

因此最终方案必须把 Hook 分成两条链路：

- 控制面：loop 同步调用，立刻返回决策
- 信息面：loop 外查询、缓存、刷新业务 Hook 信息

## 2. 目标架构

```text
agentdash-api / agentdash-application
  └─ 实现 ExecutionHookProvider
     └─ 组合 workflow / task / story / project / trellis / workspace 信息

agentdash-executor
  └─ HookRuntimeCoordinator
     ├─ HookSessionRuntime
     ├─ snapshot cache / refresh
     ├─ Hook diagnostics / trace
     └─ AgentRuntimeDelegate adapter

agentdash-agent
  └─ agent_loop
     ├─ transform_context
     ├─ before_tool_call
     ├─ after_tool_call
     ├─ after_turn
     └─ before_stop
```

### 依赖方向

- `agentdash-agent` 不依赖 executor / api / repo
- `agentdash-executor` 依赖 `agentdash-agent`
- `agentdash-api` / `agentdash-application` 依赖 executor 中定义的 Hook port 并实现它

## 3. 职责划分

### 3.1 `agentdash-agent`

职责：

- 维持 Pi 对齐的纯 runtime loop
- 暴露同步扩展点与事件观察面
- 不直接知道 workflow/task/trellis/project/story/workspace

允许改动：

- 将散落 callbacks 收敛为统一 delegate 形式
- 增加 outer loop 控制点
- 增加 control handle / observer

不允许改动方向：

- 不在 loop 里直接查 repo
- 不在 loop 里直接做 AppState 业务判断
- 不把 Trellis / Workflow 写死进 runtime

### 3.2 `agentdash-executor`

职责：

- 编排 Hook Runtime
- 在 session 维度持有 Hook snapshot
- 负责 refresh / trace / diagnostics
- 把 HookRuntime 适配为 Agent runtime delegate

这是整个 Hook 系统的核心承载层。

### 3.3 `agentdash-api` / `agentdash-application`

职责：

- 实现 Hook 信息查询与组合
- 从业务对象中构建 Hook snapshot
- 继续复用并扩展已有的 workflow runtime / context contributor / declared source resolver

这一层是“向外捞信息”的地方。

### 3.4 `workflow_runtime`

职责调整为：

- `WorkflowHookInfoProvider`
- `WorkflowPhaseConstraintResolver`
- `WorkflowBindingContextResolver`

也就是：它负责提供 phase 级 Hook 信息，但不负责 hook 生命周期。

## 4. 关键设计：控制面 vs 信息面

## 4.1 控制面

控制面必须发生在 `agent_loop` 的同步控制边界上。

它负责：

- 决定工具是否执行
- 决定工具输入是否需要改写
- 决定工具结果是否需要后处理
- 决定当前 turn 结束后是否追加 steering / follow_up
- 决定即将 stop 时是否阻止退出并继续 loop

这些点不能只靠异步 observer，否则返回给 loop 不及时。

### 控制面生命周期

- `transform_context`
- `before_tool_call`
- `after_tool_call`
- `after_turn`
- `before_stop`
- 未来扩展：`before_subagent_dispatch` / `after_subagent_dispatch`

## 4.2 信息面

信息面发生在 loop 外。

它负责：

- 查询 workflow 当前 phase
- 查询 session owner / binding
- 查询 task/story/project/workspace
- 读取 Trellis task json / jsonl / prd / info / workspace journal
- 根据 event/tool/owner/phase 计算当前命中的 hook
- 缓存与刷新 Hook snapshot

### 信息面核心产物

- `SessionHookSnapshot`
- `HookDiagnosticEntry`
- `HookResolution`

## 5. 核心抽象草图

## 5.1 `agentdash-agent` 新接口

建议把现有散落 callback 收敛为统一 delegate：

```rust
#[async_trait]
pub trait AgentRuntimeDelegate: Send + Sync {
    async fn transform_context(
        &self,
        input: TransformContextInput,
    ) -> Result<TransformContextOutput, AgentRuntimeError>;

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
    ) -> Result<ToolCallDecision, AgentRuntimeError>;

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError>;

    async fn after_turn(
        &self,
        input: AfterTurnInput,
    ) -> Result<TurnControlDecision, AgentRuntimeError>;

    async fn before_stop(
        &self,
        input: BeforeStopInput,
    ) -> Result<StopDecision, AgentRuntimeError>;
}
```

### 原则

- loop 只依赖这个 delegate
- delegate 背后怎么查数据、怎么 refresh snapshot，loop 不关心

## 5.2 `agentdash-executor` Hook port

```rust
#[async_trait]
pub trait ExecutionHookProvider: Send + Sync {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn evaluate_hook(
        &self,
        query: HookEvaluationQuery,
    ) -> Result<HookResolution, HookError>;
}
```

这个 trait 定义在 `agentdash-executor`，由 `agentdash-api` / `agentdash-application` 实现。

## 5.3 Hook session runtime

```rust
pub struct HookSessionRuntime {
    pub session_id: String,
    pub snapshot: SessionHookSnapshot,
    pub diagnostics: Vec<HookDiagnosticEntry>,
    pub revision: u64,
}
```

用途：

- 让 session 级 Hook 信息成为执行运行时的一部分
- 支持 refresh
- 支持把诊断信息暴露到 session snapshot / API / 前端

## 5.4 Tool 决策模型

当前 `BeforeToolCallResult` 只支持 `block + reason`，不够。

建议升级为：

```rust
pub enum ToolCallDecision {
    Allow,
    Deny { reason: String },
    Ask { reason: String },
    Rewrite {
        args: serde_json::Value,
        note: Option<String>,
    },
}
```

这样才足以表达 Claude Code `PreToolUse` 的基线能力。

## 5.5 Turn / Stop 控制模型

```rust
pub struct TurnControlDecision {
    pub steering: Vec<AgentMessage>,
    pub follow_up: Vec<AgentMessage>,
    pub refresh_snapshot: bool,
}

pub enum StopDecision {
    Stop,
    Continue {
        steering: Vec<AgentMessage>,
        follow_up: Vec<AgentMessage>,
        reason: Option<String>,
    },
}
```

这是为了让 Stop / SubagentStop 风格控制不依赖“事后 observer”。

## 6. Hook Snapshot 结构

建议 `SessionHookSnapshot` 至少包含：

- session 基本信息
- owner binding
- effective agent type / role
- workflow runtime snapshot
- task / story / project / workspace 摘要
- trellis task context 摘要
- hook matcher 所需标签
- 可注入 fragments / constraints / policies
- diagnostics seed / source summary

示意：

```rust
pub struct SessionHookSnapshot {
    pub session_id: String,
    pub owner: HookOwnerSummary,
    pub workflow: Option<WorkflowHookSnapshot>,
    pub trellis: Option<TrellisHookSnapshot>,
    pub context_fragments: Vec<HookContextFragment>,
    pub constraints: Vec<HookConstraint>,
    pub policies: Vec<HookPolicy>,
    pub tags: Vec<String>,
    pub diagnostics: Vec<HookDiagnosticEntry>,
}
```

## 7. Provider 组合模型

`ExecutionHookProvider` 的实现不应该是一大坨单体逻辑，而应继续保持 provider 组合。

建议拆成：

- `WorkflowHookInfoProvider`
- `OwnerContextHookInfoProvider`
- `TrellisTaskHookInfoProvider`
- `WorkspaceMemoryHookInfoProvider`
- `SubagentDispatchHookInfoProvider`
- `HookMatcherEvaluator`

### 与现有代码的衔接

- `workflow_runtime.rs` 迁入 `WorkflowHookInfoProvider`
- 已有 context contributor / declared sources 逻辑继续复用
- route / gateway 中的 augment prompt 逻辑逐步迁出

## 8. 第一阶段要迁出的散点逻辑

以下逻辑后续都不应继续直接留在 route / gateway 做 prompt augment：

- [task_execution_gateway.rs](crates/agentdash-api/src/bootstrap/task_execution_gateway.rs)
- [acp_sessions.rs](crates/agentdash-api/src/routes/acp_sessions.rs)
- [workflow_runtime.rs](crates/agentdash-api/src/workflow_runtime.rs)

目标是把它们统一迁移为：

- Session snapshot 构建
- Hook provider 解析
- Hook runtime 决策输入

## 9. 分阶段落地路线

## Phase 1：立 Hook Runtime 骨架

目标：

- 明确依赖边界
- 把 Hook 从“散落 prompt augment”升级成正式执行层 abstraction

主要改动：

- `crates/agentdash-agent`
  - 引入统一 `AgentRuntimeDelegate`
  - 补 `after_turn` / `before_stop`
- `crates/agentdash-executor`
  - 新增 `ExecutionHookProvider`
  - 新增 `HookSessionRuntime`
  - `ExecutionContext` 支持携带 hook session state
- `crates/agentdash-api`
  - 新增 provider 组合入口

交付物：

- 编译通过的 hook 架构骨架
- 仍可兼容当前 workflow runtime 注入链路

## Phase 2：统一 SessionStart / UserPromptSubmit

目标：

- `ExecutorHub` 接管 session 级 hook 生命周期
- route/gateway 不再各自做 prompt augment

主要改动：

- `crates/agentdash-executor/src/hub.rs`
- `crates/agentdash-executor/src/connector.rs`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

交付物：

- Task / Story / Project 三条链路统一走 Hook Session snapshot
- workflow runtime 产物通过 hook snapshot 注入
- session diagnostics 可输出

## Phase 3：打通 PreToolUse / PostToolUse

目标：

- 让 Hook 正式进入控制流
- 不再只是“prompt 开头注入一段文本”

主要改动：

- `crates/agentdash-agent/src/types.rs`
- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-executor/src/connectors/pi_agent.rs`

交付物：

- tool allow/deny/rewrite
- tool result 后处理
- refresh snapshot / diagnostics trace

## Phase 4：打通 after_turn / before_stop

目标：

- 支持 Claude Code / Trellis 风格 stop control
- 为后续 companion/subagent 收口做准备

主要改动：

- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-executor` hook adapter

交付物：

- turn 结束后可追加 steering/follow_up
- 即将 stop 时可阻止退出并继续

## Phase 5：伴随 Agent / Subagent Dispatch

目标：

- 正式建模 subagent dispatch lifecycle
- 让 dispatch 时上下文切片、来源追踪和 phase 推进成为平台能力

主要改动：

- `crates/agentdash-executor`
- `crates/agentdash-api`
- 未来如果 companion tool / subagent runtime 形成专门 crate，再进一步下沉

交付物：

- `before_subagent_dispatch`
- `after_subagent_dispatch`
- `subagent_stop`
- agent-specific hook context slicing

## 10. 当前推荐优先顺序

不要先做 Hook DSL，也不要先做 UI。

当前最值当的顺序是：

1. Hook Runtime 骨架
2. Session snapshot 收敛
3. Tool 控制流 hook
4. Turn/Stop 控制
5. companion/subagent

## 11. 关键风险与防偏移提醒

### 风险 1：又把 workflow 做成 hook engine

规避：
- workflow 只提供信息，不负责 lifecycle

### 风险 2：route/gateway 继续偷偷拼 prompt

规避：
- 明确收敛到 `ExecutorHub + HookProvider`

### 风险 3：把业务查询塞进 `agent_loop`

规避：
- `agent_loop` 只依赖 delegate，不依赖 provider

### 风险 4：只做 observer，不做同步控制

规避：
- `PreToolUse` / `after_turn` / `before_stop` 必须是 awaited control path

## 12. 近期直接实施项

### P1

- 在 `agentdash-agent` 正式定义 `AgentRuntimeDelegate`
- 在 `agentdash-executor` 定义 `ExecutionHookProvider` / `HookSessionRuntime`
- 让 `ExecutionContext` 携带 hook session 句柄

### P2

- 把 Task / Story / Project 当前的 augment prompt 逻辑迁到 provider
- 让 `ExecutorHub` 统一加载 session hook snapshot

### P3

- 升级 `before_tool_call` / `after_tool_call` 的返回模型
- 打通 tool 控制流与 diagnostics

### P4

- 增加 `after_turn` / `before_stop`
- 基于 turn control 开始设计 companion/subagent dispatch hook
