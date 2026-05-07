# Execution Hook Runtime

> AgentDash Hook Runtime 的跨层执行契约。
> 实现细节直接查代码：`application::hooks`、`agentdash-spi::hooks`、`agentdash-agent-types` delegate。

---

## 架构分层

```
global builtin / workflow / task / story / project / session
        ↓
ExecutionHookProvider（解析 contribution source）
        ↓
HookContributionSet merge -> SessionHookSnapshot + HookResolution
        ↓
HookSessionRuntime（executor 持有，缓存 snapshot/diagnostics/revision）
        ↓
AgentRuntimeDelegate（agent loop 边界同步消费）
```

### 分层职责

| Crate | 职责 | 不允许 |
|-------|------|--------|
| `agentdash-agent` | 只依赖 `AgentRuntimeDelegate`，在 loop 边界 await | 查询 workflow/task/story/project/repo |
| `agentdash-executor` | 持有 `HookSessionRuntime`，缓存 snapshot，适配为 delegate | 直接实现业务解析逻辑 |
| `agentdash-application::hooks` | 实现 `ExecutionHookProvider`，从业务对象解析 Hook 信息 | — |
| `agentdash-api` | HTTP surface `/api/sessions/{id}/hook-runtime` | 持有 hook 解析逻辑 |

---

## 核心 Trait 签名

### AgentRuntimeDelegate

```rust
#[async_trait]
pub trait AgentRuntimeDelegate: Send + Sync {
    async fn transform_context(&self, input: TransformContextInput, cancel: CancellationToken) -> Result<TransformContextOutput, AgentRuntimeError>;
    async fn before_tool_call(&self, input: BeforeToolCallInput, cancel: CancellationToken) -> Result<ToolCallDecision, AgentRuntimeError>;
    async fn after_tool_call(&self, input: AfterToolCallInput, cancel: CancellationToken) -> Result<AfterToolCallEffects, AgentRuntimeError>;
    async fn after_turn(&self, input: AfterTurnInput, cancel: CancellationToken) -> Result<TurnControlDecision, AgentRuntimeError>;
    async fn before_stop(&self, input: BeforeStopInput, cancel: CancellationToken) -> Result<StopDecision, AgentRuntimeError>;
}
```

### ExecutionHookProvider

```rust
#[async_trait]
pub trait ExecutionHookProvider: Send + Sync {
    async fn load_session_snapshot(&self, query: SessionHookSnapshotQuery) -> Result<SessionHookSnapshot, HookError>;
    async fn refresh_session_snapshot(&self, query: SessionHookRefreshQuery) -> Result<SessionHookSnapshot, HookError>;
    async fn evaluate_hook(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError>;
}
```

---

## 关键设计约束

### Hook 信息获取在 loop 外，控制决策在 loop 边界同步

- loop 外：查询 workflow/task/story/project 等业务信息，构造 snapshot
- loop 边界：await delegate，拿到 `HookResolution`
- 目的：保持 agent_loop 纯净，workflow 作为声明信息源而非执行引擎

### Trigger 行为要点

| Trigger | 核心约束 |
|---|---|
| `UserPromptSubmit` | **唯一**动态文本注入主通道（context_fragments + constraints + policies） |
| `CapabilityChanged` | Workflow/phase 能力变化后的 out-of-band 动态注入先经统一 sink 回灌 `TurnExecution.context_bundle.turn_delta` / audit；若需要当前 running turn 立即消费，再额外走 session notification / steering；不得通过 live system prompt 热更表达 |
| `BeforeTool` | `Ask` 必须在 tool call 边界同步挂起等待审批，不得退化为"先报错下一轮再猜" |
| `BeforeStop` | 无 workflow 绑定时 `completion = None`，必须允许自然结束，不得因 `completion_satisfied = false` 错误阻止退出 |
| `AfterTurn` | 不能重复注入 step 基线约束，避免永续 steering 导致无法抵达 `BeforeStop` |

### Workflow -> Hook Policy

- authority 是 `ActiveWorkflowProjection.effective_contract`，不是静态 workflow 模板
- `WorkflowDefinition.contract` 三段核心：`injection` / `hook_policy` / `completion`
- provider 先解释为 `HookContributionSet`，再 merge 进 snapshot
- `HookPolicyView` 只是 runtime 观测面，不是第二套执行 authority
- PhaseNode 激活后即使 tool/MCP capability surface 没有增减，只要 active workflow
  step / effective contract 发生变化，也必须触发 `CapabilityChanged` hook。原因是
  workflow guidance / context binding 属于动态上下文变化，不能依赖工具 surface
  delta 才投递给当前 running Agent。
- PhaseNode 的 live apply、pending next turn、applied on next turn 三条路径必须
  通过同一份 runtime context transition 结构派生 `capability_surface_changed`
  事件 payload 与 pending metadata；禁止各入口手写互不一致的事件 JSON。
- 生产路径必须通过 `SessionHub` 的 runtime context transition applier 应用
  transition。`replace_current_capability_surface`、`emit_capability_surface_changed`、
  `emit_capability_changed_hook`、pending transition 写入等低层方法只允许作为
  applier 内部 primitive 使用。
- `CapabilityChanged` 的 live notification 必须通过顶层 connector 路由到真正持有
  live session 的子 connector（例如 `CompositeConnector -> PiAgentConnector`）。
  顶层 connector 返回 unsupported 会造成 trace 看得到但模型收不到。

### Ask / Approval

- `BeforeTool` 返回 `Ask` → agent 产出 pending approval → 同步等待
- 审批 API：`POST /sessions/{id}/tool-approvals/{tool_call_id}/approve|reject`
- 拒绝时不执行工具，但产出结构化 rejection tool_result 并继续 loop

### Companion / Subagent Dispatch

- runtime tool：`companion_dispatch` / `companion_complete`
- dispatch 前后显式调用 `BeforeSubagentDispatch` / `AfterSubagentDispatch`
- 子 agent 继承的 context/constraints 由 dispatch resolution 生成，按 `slice_mode` 过滤
- 回流结果进入 `HookSessionRuntime.pending_actions`，由 runtime delegate 在 `AfterTurn` / `BeforeStop` / `TransformContext` 边界消费

### Hook Event Stream

- `SessionUpdate::SessionInfoUpdate` + `_meta.agentdash.event.type=hook_event`
- 纯噪音 trace（noop/allow/effects_applied）不强制发入事件流
- 但只要带 `matched_rule_keys / diagnostics / completion / block_reason` 任一信息，必须发

### Source Traceability

- 所有 policy / constraint / diagnostic 必须携带 `source_summary` + `source_refs`
- `HookSourceRef.layer` 支持：`global_builtin / workflow / project / story / task / session`

---

## 禁止模式

```
❌ 在 route/gateway 里直接拼 prompt 字符串表达流程 gate
❌ 在 agent_loop 里直接查 repo / workflow run / task status
❌ 前端只展示 workflow step 文本，不展示实际 runtime policies
❌ connector system prompt 重复展开 workflow constraint 静态副本
❌ 为了让当前 running turn 看到 workflow/hook 动态变化而重设 system prompt
❌ 把 HookPolicyView 当第二套执行 authority
```

---

> Pi Agent 流式 chunk 合并协议已拆分到 [pi-agent-streaming.md](../session/pi-agent-streaming.md)。

*更新：2026-04-14 — 大幅精简，移除实现级冗余描述，保留跨层契约与设计约束*
