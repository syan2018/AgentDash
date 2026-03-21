# Execution Hook Runtime

> AgentDash Hook Runtime 的跨层执行契约。

---

## Overview

当前项目已经正式形成一条 Hook Runtime 链路：

- `agentdash-agent` 暴露纯运行时控制边界
- `agentdash-executor` 承担 session 级 hook runtime 编排、缓存与适配
- `agentdash-api` 实现 `ExecutionHookProvider`，从 workflow / task / story / project 等业务对象中解析 Hook 信息
- 前端通过 `/api/sessions/{id}/hook-runtime` 观察当前 session 实际生效的 runtime snapshot

这套机制的目标不是把 workflow 再做成一套特化 prompt 拼接系统，而是把“动态注入、工具前后 gate、turn/stop 控制”收敛为一条正式的跨层契约。

---

## Scenario: Session Hook Runtime（Pi Agent / Workflow / Frontend）

### 1. Scope / Trigger

- Trigger: 新增或修改 Pi Agent 在 `transform_context / before_tool_call / after_tool_call / after_turn / before_stop` 的行为
- Trigger: 新增或修改 workflow phase 对工具、结束条件、上下文注入的约束
- Trigger: 前端需要展示 session 级 hook runtime、policy、diagnostics、metadata
- Trigger: 任何需求涉及“业务信息在 loop 外获取，但控制决策要在 loop 边界同步生效”

### 2. Signatures

#### Agent Runtime Delegate

```rust
#[async_trait]
pub trait AgentRuntimeDelegate: Send + Sync {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError>;

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError>;

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError>;

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError>;

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError>;
}
```

#### Executor Hook Provider

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

#### Session Runtime Surface

```rust
pub struct HookSessionRuntimeSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub snapshot: SessionHookSnapshot,
    pub diagnostics: Vec<HookDiagnosticEntry>,
}
```

#### HTTP Surface

- `GET /api/sessions/{id}/hook-runtime`

### 3. Contracts

#### 3.1 依赖边界契约

- `agentdash-agent` 只依赖 `AgentRuntimeDelegate`，不直接查询 workflow/task/story/project/repo
- `agentdash-executor` 负责：
  - 持有 `HookSessionRuntime`
  - 缓存 snapshot / diagnostics / revision
  - 把 runtime 适配为 `AgentRuntimeDelegate`
- `agentdash-api` / `agentdash-application` 负责：
  - 从业务对象“向外捞” Hook 信息
  - 生成 `SessionHookSnapshot`
  - 根据 trigger 评估 `HookResolution`

#### 3.2 Snapshot 契约

`SessionHookSnapshot` 至少应包含：

- `session_id`
- `owners`
- `sources`
- `tags`
- `context_fragments`
- `constraints`
- `policies`
- `diagnostics`
- `metadata`

当前 `sources` / `source_summary` / `source_refs` 已约定：

- `sources` 是 session 当前真实生效的来源注册表，不是静态 workflow 模板说明
- `HookContextFragment` / `HookConstraint` / `HookPolicy` / `HookDiagnosticEntry` 都必须携带：
  - `source_summary: Vec<String>`
  - `source_refs: Vec<HookSourceRef>`
- `HookSourceRef.layer` 当前支持：
  - `global_builtin`
  - `workflow`
  - `project`
  - `story`
  - `task`
  - `session`
- `source_summary` 必须使用稳定 tag，不能依赖 Rust `Debug` 输出等偶然字符串
- `source_refs` 为空只允许出现在暂时无法给出结构化来源、只能保留 summary 的退化场景

当前 `metadata` 已约定包含：

- `active_workflow.workflow_id`
- `active_workflow.workflow_key`
- `active_workflow.workflow_name`
- `active_workflow.run_id`
- `active_workflow.run_status`
- `active_workflow.phase_key`
- `active_workflow.phase_title`
- `active_workflow.completion_mode`
- `active_workflow.requires_session`
- `active_task.task_id`
- `active_task.task_title`
- `active_task.status`

#### 3.3 Trigger 行为契约

| Trigger | 必须行为 |
|---|---|
| `SessionStart` / `UserPromptSubmit` | 返回当前应注入的 `context_fragments + constraints + policies` |
| `BeforeTool` | 可以 `Allow / Deny / Ask / Rewrite`，不得异步观测后再补救 |
| `AfterTool` | 可以附加 diagnostics，并决定是否 `refresh_snapshot` |
| `AfterTurn` | 可以追加 steering / constraints / follow-up |
| `BeforeStop` | 必须在 loop 退出前同步返回 stop gate 决策 |
| `BeforeSubagentDispatch` | 必须在 companion/subagent 真正启动前同步决定是否允许派发，并返回子 agent 应继承的 context/constraints |
| `AfterSubagentDispatch` | 必须记录派发结果、目标 session/turn，并写入 trace/diagnostics |
| `SessionTerminal` | 当 executor 观察到 session 进入终态时，必须让 hook runtime 有机会同步产出 completion judgment 并推进 workflow |

#### 3.4 Workflow -> Hook Policy 契约

- workflow phase 是 Hook 信息来源之一，不是 Hook 生命周期引擎
- `agent_instructions` 生成 `constraints`
- `completion_mode` 生成 completion policy
- phase/tool/status gate 生成 `policies`
- provider 必须先把不同来源解释成 `HookContributionSet`，再 merge 进 session snapshot
- policy / constraint / diagnostic 的来源必须可通过 `source_summary` / `source_refs` / `metadata` 解释
- global builtin hook 也是正式来源层的一部分，不能绕过来源注册表直接塞进 rule engine

当前已落地的 policy 示例：

- `tool:shell_exec:rewrite_absolute_cwd`
- `workflow:*:*:completion_mode`
- `workflow:*:*:task_status_gate`
- `workflow:*:*:record_gate`
- `workflow:*:*:checklist_gate`

#### 3.5 Frontend 契约

前端会话页展示的是“执行层真实生效”的 runtime surface，而不是静态 workflow 模板说明。

必须区分：

- `snapshot.sources` / `snapshot.tags` / `snapshot.metadata`：当前 runtime 基线与来源注册表
- `snapshot.policies` / `snapshot.constraints`：当前会话真实规则面
- `diagnostics`：session runtime 命中记录
- `trace`：per-trigger 的运行态轨迹，必须能看到 trigger / decision / matched_rule_keys / refresh / completion
- 若某条 policy / diagnostic / constraint 来自 workflow 或 global builtin，前端应能直接看到来源标签，而不是只剩自然语言描述

#### 3.6 Companion / Subagent Dispatch 契约

当前项目的第一版 companion/subagent 生命周期，采用“runtime tool + hook trigger”方式落地：

- runtime tool：`companion_dispatch`
- result tool：`companion_complete`
- dispatch 前：工具执行层显式调用 `BeforeSubagentDispatch`
- dispatch 后：工具执行层显式调用 `AfterSubagentDispatch`
- result 回流：工具执行层显式调用 `SubagentResult`
- dispatch 目标：当前 owner 关联的 `label=companion` session；若不存在且允许自动创建，则由执行层创建并绑定

当前 companion 机制的正式契约：

- `companion_dispatch` 必须支持：
  - `slice_mode`: `compact | full | workflow_only | constraints_only`
  - `adoption_mode`: `suggestion | follow_up_required | blocking_review`
  - `max_fragments` / `max_constraints`：限制切片体积
- 子 agent 继承的 `context_fragments` / `constraints` 必须由 `BeforeSubagentDispatch` resolution 生成，再由执行层按 `slice_mode` 过滤；不能在工具内部重新发明一套硬编码 workflow prompt。
- `compact` 模式允许执行层补一个 `owner_summary` 片段作为上下文前言，但仍然属于 dispatch slice，而不是 workflow 专用逻辑。
- `workflow_only` 只能保留带 `workflow` 来源的 fragments / constraints。
- `constraints_only` 不继承普通 context fragment，只继承约束集合。
- dispatch 执行前，执行层必须把 `dispatch_id / parent_session_id / parent_turn_id / slice_mode / adoption_mode / inherited_*` 写入目标 companion session 的 `SessionMeta.companion_context`。
- 当前 session 若已是目标 companion session，不允许递归向自身再次派发。

当前 return channel 契约：

- companion 在完成子任务后，必须显式调用 `companion_complete` 回传结构化结果。
- `companion_complete` 结果至少包含：
  - `summary`
  - `status`: `completed | blocked | needs_follow_up`
  - `findings`
  - `follow_ups`
  - `artifact_refs`
- 执行层必须通过 `SessionMeta.companion_context` 找到父 session / 父 turn，并同时产出两条通道：
  - session stream event：给前端与会话历史展示
  - hook runtime trace / diagnostics：给主 session 的 runtime surface 消费
- 父 session 若存在 hook runtime，执行层必须同步触发一次 `HookTrigger::SubagentResult`，并把 `dispatch_id / adoption_mode / summary / status / companion_session_id` 等结构化字段写入 payload。
- `adoption_mode` 当前阶段只是“回流语义标注”，尚未自动驱动 approve/resume；后续 Ask/Approval/Resume 任务会在此基础上把采纳链路补成正式控制流。

### 4. Validation & Error Matrix

| 场景 | 预期行为 | 错误/结果 |
|---|---|---|
| session 无可用 hook runtime | API 返回 404 | `session {id} 当前没有可用的 hook runtime` |
| `BeforeTool` 命中 implement phase 且尝试直接 `completed` | 同步拒绝 | `ToolCallDecision::Deny` |
| `BeforeTool` 命中 `shell_exec.cwd` 绝对工作区路径 | 同步改写为相对 workspace root | `ToolCallDecision::Rewrite` |
| `BeforeTool` 命中 implement phase 且尝试上报 `session_summary` / `archive_suggestion` | 同步拒绝 | `ToolCallDecision::Deny` |
| `AfterTool` 命中会改变 task/workflow 观察面的工具 | 请求刷新 snapshot | `refresh_snapshot = true` |
| `BeforeStop` 命中 `session_ended` | 允许自然结束 | 仅 diagnostics，不阻止退出 |
| `BeforeStop` 命中 `checklist_passed` 且 task 未达成 | 注入 stop gate，继续 loop | 返回 steering/constraints |
| `BeforeStop` 命中 `checklist_passed` 且 task 已 `awaiting_verification/completed` | 允许结束 | diagnostics 标记 satisfied |
| `companion_dispatch` 命中 `BeforeSubagentDispatch` deny | 同步拒绝派发 | `AgentToolError::ExecutionFailed` |
| `companion_complete` 在非 companion session 调用 | 同步拒绝回流 | `当前 session 不是通过 companion_dispatch 建立的上下文` |
| `companion_complete` 回流到仍持有 hook runtime 的父 session | 同步触发 `SubagentResult` | trace + diagnostics 同步可见 |

### 5. Good / Base / Bad Cases

#### Good

```text
workflow/global builtin -> provider 生成 contribution
provider merge sources/policies/constraints 形成 snapshot
executor runtime 缓存 snapshot
runtime delegate 在 before_tool/before_stop 同步消费 resolution
frontend 展示实际生效的 policies/diagnostics/source registry
```

#### Base

```text
业务规则仍由 provider 解释，但已经统一通过 HookPolicy / HookResolution 输出
```

#### Bad

```text
在 route/gateway 里继续直接拼 prompt 字符串表达流程 gate
在 agent_loop 里直接查 repo / workflow run / task status
前端只展示 workflow phase 文本，不展示实际 runtime policies
```

### 6. Tests Required

至少应覆盖以下断言点：

- `execution_hooks` 单测：
  - implement phase 阻止直接 `completed`
  - `shell_exec` 绝对 `cwd` 会在 hook runtime 中被 rewrite
  - checklist phase 未满足时 `before_stop` 注入 gate
  - checklist phase 满足时 `before_stop` 允许结束
  - `BeforeSubagentDispatch` 会继承 runtime context / constraints
  - `companion_dispatch` 会按 slice mode 过滤 fragments / constraints，并生成 return-channel 指令
  - `SubagentResult` 会记录结构化 return-channel diagnostic
  - snapshot 会合并 `global_builtin + workflow` 来源，并暴露去重后的 `sources`
  - workflow 产出的 `policy / constraint / diagnostic` 必须带 `source_refs`
- `cargo check`：
  - `agentdash-agent`
  - `agentdash-executor`
  - `agentdash-api`
- 前端构建/类型：
  - `pnpm --filter frontend exec tsc --noEmit`
  - `pnpm --filter frontend build`
- 联调：
  - `GET /api/sessions/{id}/hook-runtime` 返回 `policies + metadata + diagnostics`
  - 会话页能看到 `policies: N` 与具体 policy 列表

### 7. Wrong vs Correct

#### Wrong

```text
workflow runtime 直接长成“工具前后如何决策”的一大坨 if/else 中心；
agent_loop 再直接去问 workflow 当前 phase 和 task 状态；
前端只能看到 workflow phase 文本，却看不到规则真正来自哪一层。
```

问题：

- 破坏执行层纯 runtime 边界
- workflow 不再是声明层，而重新变成硬编码引擎
- 业务查询和控制决策耦合进 loop

#### Correct

```text
global builtin / workflow / task / story / project / session
        ↓
ExecutionHookProvider（解析 contribution source）
        ↓
HookContributionSet merge -> SessionHookSnapshot + HookResolution
        ↓
HookSessionRuntime
        ↓
AgentRuntimeDelegate / Runtime Tool
        ↓
agent_loop 的同步控制边界 / companion dispatch 执行层
```

这样才能同时满足：

- 编排/注入可插拔
- 执行层不侵入业务
- runtime 决策能及时生效
- 来源可观测、可追踪、可前端解释

---

## Design Decision

### 决策：Hook 信息获取在 loop 外，Hook 控制决策在 loop 边界同步发生

**Context**:

- 早期设计明确“编排无侵入”“注入是策略组合”“执行层只管理实际执行”
- Hook 如果只做异步 observer，`BeforeTool` / `BeforeStop` 就来不及影响当前控制流
- Hook 如果把业务 repo 查询塞进 `agent_loop`，又会破坏 Pi runtime 的纯内核定位

**Decision**:

- loop 外：查询 workflow/task/story/project 等业务信息，构造 snapshot
- loop 边界：await delegate，拿到 `HookResolution`

**Why**:

- 对齐项目早期的可插拔策略哲学
- 保持执行层与编排/注入层边界清晰
- 让 workflow 继续作为声明信息源，而不是执行引擎
