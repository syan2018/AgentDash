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

## 核心 Trait

### AgentRuntimeDelegate（`agentdash-agent-types::runtime::delegate`）

Agent Loop 在关键生命周期节点调用的委托接口。方法包括：`evaluate_compaction`、`after_compaction`、`after_compaction_failed`、`transform_context`、`before_tool_call`、`after_tool_call`、`after_turn`、`before_stop`、`on_before_provider_request`。具体签名查代码。

### ExecutionHookProvider（`agentdash-spi::hooks`）

从业务对象解析 Hook 信息的提供者。方法包括：`load_session_snapshot`、`refresh_session_snapshot`、`evaluate_hook`。

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
| `BeforeTool` | `Ask` 必须在 tool call 边界同步挂起等待审批，不得退化为"先报错下一轮再猜" |
| `BeforeStop` | 无 workflow 绑定时 `completion = None`，必须允许自然结束，不得因 `completion_satisfied = false` 错误阻止退出 |
| `AfterTurn` | 只做生命周期观察与 effect，不消费 turn-start 暂存事件，避免重复注入 step 基线约束 |

### Runtime Event 行为要点

- `HookTraceTrigger` 只表示 AgentLoop 生命周期节点：`SessionStart` /
  `UserPromptSubmit` / tool 边界 / `AfterTurn` / `BeforeStop` /
  `SessionTerminal` / compact/provider request 等。
- `runtime_context_update` 与 `companion_result` 是 runtime event，不是
  `HookTraceTrigger`。它们可以驱动 hook rule 评估、turn_delta/audit 回灌、
  `HookTurnStartNotice` 或 `HookPendingAction` 入队，但不写 HookTrace。
- TurnStart 注入统一发生在 `transform_context(UserPromptSubmit)`：先消费
  `HookTurnStartNotice`（一次性消息），再消费 `HookPendingAction`（带状态与
  resolution 的可处置事件）。`AfterTurn` / `BeforeStop` 不得取走这些队列。
- `HookTurnStartNotice` 与 `HookPendingAction` 的区别只在生命周期：前者一次性
  告知，消费即清；后者有 `pending/resolved`、`adopted/dismissed`、阻塞 stop 等
  状态语义。
- 启动期的动态上下文不得重新塞回 core system prompt。Workspace、Skill、Hook
  Runtime、Bootstrap Project Context、初始 Tool Schema 必须分别产出独立
  `ContextFrame`（例如 `workspace_surface` / `skill_surface` /
  `hook_runtime_surface` / `bootstrap_context` / `tool_surface`），再进入同一个
  turn-start 队列。前端 feed 可以把相邻 frame 折叠成批量更新卡片，但不得把这些
  frame 在后端语义上合并成一个含糊的 surface。
- Hook auto-resume 必须生成独立 `ContextFrame(kind="auto_resume")`。该 frame 的
  `delivery_channel` 是 `user_prompt`，`rendered_text` 必须等于系统实际发起的
  auto-resume prompt；系统续跑提示需要保留可审计 UI 语义。
- `context_compacted` 事件必须伴随生成独立
  `ContextFrame(kind="compaction_summary")`。该 frame 的 `delivery_channel` 是
  `continuation`，section 至少包含 summary、tokens_before、messages_compacted、
  compacted_until_ref 与 timestamp_ms，让用户能看到后续上下文受哪份压缩摘要影响。
- 结构性压缩失败通过 `after_compaction_failed` 进入 Hook runtime diagnostic。连续失败计数属于 runtime 状态，达到阈值后 `evaluate_compaction` 返回 no-op，避免自动压缩在同一失败条件下反复消耗上下文窗口；成功 `after_compaction` 会复位该计数。

### Workflow -> Hook Policy

- authority 是 `ActiveWorkflowProjection.effective_contract`，不是静态 workflow 模板
- `WorkflowDefinition.contract` 三段核心：`injection` / `hook_policy` / `completion`
- provider 先解释为 `HookContributionSet`，再 merge 进 snapshot
- `HookPolicyView` 只是 runtime 观测面，不是第二套执行 authority
- PhaseNode 激活后即使 tool/MCP capability surface 没有增减，只要 active workflow
  step / effective contract 发生变化，也必须产生 capability context update。原因是
  workflow guidance / context binding 属于动态上下文变化，不能依赖工具 surface
  delta 才进入下一次 AgentLoop 边界。
- PhaseNode 的 live apply、pending next turn、applied on next turn 三条路径通过
  同一份 runtime context transition 结构派生 `capability_state_changed`
  事件 payload 与 pending metadata。
- 生产路径通过 `SessionCapabilityService` 应用 transition。`replace_current_capability_state`、
  `emit_capability_state_changed`、runtime context update injection 收集、pending
  transition 写入等底层方法只作为 service 内部 primitive 使用。
- runtime context update 不应作为第二条即时 live notification 推给 Agent。transition
  applier 应先更新 `CapabilityState` 与 tool set，再收集当前 hook snapshot 中的
  workflow/context 注入，将合并后的能力变化、工具定义摘要与 workflow 注入写入
  `HookTurnStartNotice` 队列；
  下一次 `transform_context` 边界统一消费。
- runtime steering 的一等展示结构是 `ContextFrame`。runtime capability delta、
  tool schema、workflow context、普通 hook injection、system notice 先由所属模块的
  typed metadata 构造为 `ContextFrame.sections` 与 `ContextFrame.rendered_text`。
  各消费者共享同一份 frame 数据。
- 各 `ContextFrame.kind` 由所属模块内聚渲染：例如 workspace surface 只由
  workspace/VFS metadata 渲染，skill surface 只由 skill capability metadata 渲染。
  多个 frame 的汇聚点是 delivery boundary 与前端 feed 聚合，不是一个后端“大杂烩”
  frame。
- `HookTurnStartNotice.content` 必须等于对应 `ContextFrame.rendered_text`；若这次
  TurnStart 注入属于 runtime context，必须同时携带 `context_frame`。多个
  `HookTurnStartNotice` 可以在同一 turn-start delivery boundary 被 batch envelope
  汇聚后投递给 Agent，但每个 frame 仍保持独立 metadata、sections 与 rendered_text。
  前端的 Agent 行为可视化以 `SessionMetaUpdate { key: "context_frame" }` 为准；
  相邻 frame 应在 feed 层聚合展示，`hook_trace context_injected` 不得冒充 context
  frame 的完整可视化。
- `baseline_initialized`、`context_injected`、`steering_injected` 等 hook trace
  决策属于 lifecycle/audit 事件；Agent-visible context card 来自 `ContextFrame`。

### Ask / Approval

- `BeforeTool` 返回 `Ask` → agent 产出 pending approval → 同步等待
- 审批 API：`POST /sessions/{id}/tool-approvals/{tool_call_id}/approve|reject`
- 拒绝时不执行工具，但产出结构化 rejection tool_result 并继续 loop

### Companion / Subagent Dispatch

- runtime tool：`companion_dispatch` / `companion_complete`
- dispatch 前后显式调用 `BeforeSubagentDispatch` / `AfterSubagentDispatch`
- 子 agent 继承的 context/constraints 由 dispatch resolution 生成，按 `slice_mode` 过滤
- 回流结果以 `companion_result` runtime event 进入 hook rule；需要后续处置时写入
  `HookSessionRuntime.pending_actions`，统一由 runtime delegate 在
  `TransformContext(UserPromptSubmit)` TurnStart 边界消费。

### Hook Event Stream

- `BackboneEvent::Platform(PlatformEvent::HookTrace(payload))`
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
