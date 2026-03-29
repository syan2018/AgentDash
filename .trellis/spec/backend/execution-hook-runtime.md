# Execution Hook Runtime

> AgentDash Hook Runtime 的跨层执行契约。

---

## Overview

当前项目已经正式形成一条 Hook Runtime 链路：

- `agentdash-agent` 暴露纯运行时控制边界
- `agentdash-executor` 承担 session 级 hook runtime 编排、缓存与适配
- `agentdash-application` 实现 `ExecutionHookProvider`（位于 `application::hooks`），从 workflow / task / story / project 等业务对象中解析 Hook 信息
- 前端通过 `/api/sessions/{id}/hook-runtime` 观察当前 session 实际生效的 runtime snapshot

这套机制的目标不是把 workflow 再做成一套特化 prompt 拼接系统，而是把“动态注入、工具前后 gate、turn/stop 控制”收敛为一条正式的跨层契约。

当前 authority 已经更新为：

- `WorkflowDefinition.contract`：只定义三段核心 contract：
  - `injection`：输入时注入什么内容
  - `hook_policy`：hook 时如何放行 / 阻挡 / 改写 / 注入
  - `completion`：结束检查与默认记录产物
- `LifecycleDefinition.steps[*]`：定义生命周期 step、primary workflow、transition
- `ActiveWorkflowProjection.effective_contract`：会话运行时的唯一注入/治理 contract
- `execution_hooks.rs`：只负责解释 `effective_contract + active_step.transition`，不再把 lifecycle step 之外的兼容 view 当 authority

补充约定：

- `WorkflowBindingKind` / `WorkflowBindingRole` 只是 workflow 的绑定层元数据，用来描述“可挂载到哪类 owner / 建议由哪类 session 使用”
- 它们不代表 workflow 自身的业务语义，更不应反向把 task/story/project 语义内建进 hook runtime 或 workflow rule

---

## Scenario: Session Hook Runtime（Pi Agent / Workflow / Frontend）

### 1. Scope / Trigger

- Trigger: 新增或修改 Pi Agent 在 `transform_context / before_tool_call / after_tool_call / after_turn / before_stop` 的行为
- Trigger: 新增或修改 workflow contract / lifecycle step 对工具、结束条件、上下文注入的约束
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
    pub trace: Vec<HookTraceEntry>,
    pub pending_actions: Vec<HookPendingAction>,
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
- `agentdash-application` 负责：
  - 从业务对象“向外捞” Hook 信息
  - 生成 `SessionHookSnapshot`
  - 根据 trigger 评估 `HookResolution`
  - `ExecutionHookProvider` 实现
- `agentdash-api` 负责：
  - 通过 re-export（`api::execution_hooks → application::hooks`）保持路由层引用兼容
  - 提供 HTTP surface（`/api/sessions/{id}/hook-runtime`）

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
- `HookContextFragment` / `HookConstraint` / `HookPolicyView` / `HookDiagnosticEntry` 都必须携带：
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

- `active_workflow.lifecycle_id`
- `active_workflow.lifecycle_key`
- `active_workflow.lifecycle_name`
- `active_workflow.run_id`
- `active_workflow.run_status`
- `active_workflow.step_key`
- `active_workflow.step_title`
- `active_workflow.transition_policy`
- `active_workflow.primary_workflow_id`
- `active_workflow.primary_workflow_key`
- `active_workflow.primary_workflow_name`
- `active_workflow.requires_session`
- `active_workflow.effective_contract`
- `active_workflow.step_transition`

#### 3.3 Trigger 行为契约

| Trigger | 必须行为 |
|---|---|
| `SessionStart` | 在 hook runtime 加载完成后执行 baseline setup / trace / 可选 refresh；可以返回当前规则面供调试，但**不得**作为第二条普通文本注入通道 |
| `UserPromptSubmit` | 返回当前应注入的 `context_fragments + constraints + policies`，它是每轮 prompt 的唯一动态文本注入主通道 |
| `BeforeTool` | 可以 `Allow / Deny / Ask / Rewrite`，其中 `Ask` 必须在当前 tool call 边界同步挂起等待审批，不得退化成“先报错，下一轮再猜” |
| `AfterTool` | 可以附加 diagnostics，并决定是否 `refresh_snapshot` |
| `AfterTurn` | 可以追加针对“本轮结果”的 steering / follow-up，但不能重复注入 step 基线约束，避免 loop 因永续 steering 而无法抵达 `BeforeStop` |
| `BeforeStop` | 必须在 loop 退出前同步返回 stop gate 决策。**关键约束：无 workflow 绑定时 `completion = None`，此时必须允许自然结束（视为无 gate），不得因 `completion_satisfied = false` 而错误阻止退出。** |
| `BeforeSubagentDispatch` | 必须在 companion/subagent 真正启动前同步决定是否允许派发，并返回子 agent 应继承的 context/constraints |
| `AfterSubagentDispatch` | 必须记录派发结果、目标 session/turn，并写入 trace/diagnostics |
| `SessionTerminal` | 当 executor 观察到 session 进入终态时，必须让 hook runtime 有机会同步产出 completion judgment 并推进 workflow |

#### 3.4 Workflow -> Hook Policy 契约

- workflow contract 是 Hook 信息来源之一，但 authority 是 `effective_contract`
- `effective_contract.injection.instructions` 通过注入 fragment 进入 loop，不再伪装成第二份 `constraints`
- `effective_contract.hook_policy.constraints` 才是 hook 策略面的正式来源
- `active_workflow_step` 这类 summary fragment 只负责说明当前 lifecycle step 背景，不得再重复展开 workflow 指令
- `active_step.transition` 生成 completion / transition policy
- step/tool/status gate 由 `hook_policy.constraints + completion.checks + transition` 共同生成 `policies`
- provider 必须先把不同来源解释成 `HookContributionSet`，再 merge 进 session snapshot
- policy / constraint / diagnostic 的来源必须可通过 `source_summary` / `source_refs` / `metadata` 解释
- global builtin hook 也是正式来源层的一部分，不能绕过来源注册表直接塞进 rule engine

当前已落地的 policy 示例：

- `tool:shell_exec:rewrite_absolute_cwd`
- `workflow:*:*:transition_policy`
- `workflow:*:*:constraint:*`
- `workflow:*:*:check_gate`

其中 `check_gate` 的当前语义已经明确为：

- `BeforeStop` 必须结合当前回合是否已经形成 checklist evidence（例如带检查结论/风险说明的阶段性总结）同步判定是否允许自然结束
- check step 不能只靠某个外部业务对象的状态变化就放行，也不能把 evidence 约束做成每轮永续 `after_turn` steering

#### 3.5 Frontend 契约

前端会话页展示的是“执行层真实生效”的 runtime surface，而不是静态 workflow 模板说明。

必须区分：

- `snapshot.sources` / `snapshot.tags` / `snapshot.metadata`：当前 runtime 基线与来源注册表
- `snapshot.policies` / `snapshot.constraints`：当前会话真实规则面
- `diagnostics`：session runtime 命中记录
- `trace`：per-trigger 的运行态轨迹，必须能看到 trigger / decision / matched_rule_keys / refresh / completion
- `pending_actions`：已进入执行层、等待主 session 下一次 loop 消费的 companion/hook 干预队列
- 若某条 policy / diagnostic / constraint 来自 workflow 或 global builtin，前端应能直接看到来源标签，而不是只剩自然语言描述

#### 3.5.0 Hook Event Stream 契约

除 `/api/sessions/{id}/hook-runtime` 的静态观察面外，执行层还必须把“有意义的 hook 决策”镜像进主会话事件流：

- 事件载体：`SessionUpdate::SessionInfoUpdate`
- `_meta.agentdash.event.type`：固定为 `hook_event`
- `_meta.agentdash.trace.turn_id`：必须回填当前 turn，确保前端能并入同一轮会话流
- `event.data` 至少应包含：
  - `trigger`
  - `decision`
  - `sequence`
  - `revision`
  - `matched_rule_keys`
  - `refresh_snapshot`
  - `block_reason`
  - `completion`
  - `diagnostic_codes`
  - `diagnostics`

当前约定：

- `noop / allow / effects_applied` 这类纯噪音 trace 默认不强制发入事件流
- 但只要该 trace 带有 `matched_rule_keys / diagnostics / completion / block_reason` 中任一信息，就必须发 `hook_event`
- `SessionTerminal` 产生的 hook trace 也必须发入事件流，不能只保留在 runtime trace 面板

补充约定：

- `SessionStart` trace 用于观测 baseline setup 是否真实发生，不代表新增了一条用户消息注入
- connector system prompt 不应再重复展开 workflow constraint / hook summary 静态副本；这些动态治理信息必须以 hook runtime 注入为准

这样前端主事件流才能直接看到：

- `user_prompt_submit` 注入了哪些流程上下文
- `before_stop` 为何继续 / 阻止结束
- `session_terminal` 是否推进了 step、是否只是记录终态
- hook 判定与普通 `turn_started / turn_completed / tool_call` 事件在时间线上如何交错

#### 3.5.1 Ask / Approval / Resume 契约

当前项目已经把 `Ask` 推进为正式的人机审批链路：

- Hook provider / runtime delegate 负责在 `BeforeTool` 返回 `Ask`
- `agentdash-agent` 负责：
  - 产出 pending approval 事件
  - 在当前 tool call 边界等待审批结果
  - 审批通过后继续执行同一个 tool call
  - 审批拒绝后产出结构化 `tool_result` 并继续 loop
- `agentdash-executor` / `agentdash-api` 负责暴露审批控制面：
  - `POST /api/sessions/{id}/tool-approvals/{tool_call_id}/approve`
  - `POST /api/sessions/{id}/tool-approvals/{tool_call_id}/reject`
- 前端会话流必须满足：
  - `tool_call_update.status=pending` 表示等待审批
  - 用户可以直接在工具卡片中点击批准/拒绝
  - 若 `rawOutput.approval_state=rejected`，即使 ACP 标准状态仍为 `failed`，UI 也要渲染为“已拒绝执行”

当前默认 Ask 来源：

- `global_builtin:supervised_tool_approval`
- 当 `permission_policy=SUPERVISED` 时，执行/编辑/删除/移动类工具会进入审批

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
- `adoption_mode` 不允许只停留在 diagnostics：
  - `suggestion`：允许只记录 trace / diagnostics
  - `follow_up_required`：必须进入 `HookSessionRuntime.pending_actions`，并在外循环边界转成 follow-up 注入
  - `blocking_review`：必须进入 `HookSessionRuntime.pending_actions`，并在外循环边界转成 steering / continue 信号
- 这类 pending action 由 runtime 持有和消费，不能反向侵入 `agent_loop` 业务逻辑；`agent_loop` 只在既有 delegate 边界接收结果。

#### 3.6.1 Pending Action / Adoption Control 契约

- `HookPendingAction` 是执行层正式 surface，不是前端私有状态：
  - `id`
  - `created_at_ms`
  - `title`
  - `summary`
  - `action_type`
  - `turn_id`
  - `source_trigger`
  - `status`：`pending | injected | resolved | dismissed`
  - `last_injected_at_ms`
  - `resolved_at_ms`
  - `resolution_kind`：`adopted | rejected | completed | superseded | user_dismissed`
  - `resolution_note`
  - `resolution_turn_id`
  - `context_fragments`
  - `constraints`
- Companion 回流若生成 `context_fragments / constraints`，执行层必须把它们排入 `pending_actions`，并递增 runtime `revision`
- runtime delegate 在以下边界把 `pending` action 标记为 `injected` 并注入 loop：
  - `AfterTurn`
  - `BeforeStop`
  - `TransformContext`（用于 parent session 已停住、下次用户重新进入时）
- `injected` action 不能因“已经注入过一次”就直接消失；它必须继续保留在 runtime surface，直到被显式结案。
- 主 session 处理完 hook 回流后，必须通过 runtime tool `resolve_hook_action` 显式写回结案结果，而不是依赖隐式 drain。
- `blocking_review` 在未结案前必须持续阻止 `BeforeStop` 自然结束；`follow_up_required` 至少要保持可观测、可结案。
- 结案后，执行层必须保留对应 action 记录，并把状态切到 `resolved / dismissed`，供前端与调试面回看。

#### 3.6.2 Frontend Refresh 契约

- 会话页的 Hook Runtime 面板不应长期依赖固定轮询
- 主路径应为：
  - `hook_event`
  - `hook_action_resolved`
  - `companion_dispatch_registered`
  - `companion_result_available`
  - `companion_result_returned`
  - `turn_completed / turn_failed`
  这些主事件流信号触发 runtime 刷新
- 首次进入页面或 session 切换时允许主动拉一次 `/api/sessions/{id}/hook-runtime`
- 定时轮询只可作为临时 debug 手段，不能作为正式产品语义

### 4. Validation & Error Matrix

| 场景 | 预期行为 | 错误/结果 |
|---|---|---|
| session 无可用 hook runtime | API 返回 404 | `session {id} 当前没有可用的 hook runtime` |
| `BeforeTool` 命中 `shell_exec.cwd` 绝对工作区路径 | 同步改写为相对 workspace root | `ToolCallDecision::Rewrite` |
| `BeforeTool` 命中 implement phase 且尝试上报 `session_summary` / `archive_suggestion` | 同步拒绝 | `ToolCallDecision::Deny` |
| `BeforeTool` 命中 `permission_policy=SUPERVISED` 且工具属于执行/编辑类 | 同步挂起等待审批 | `ToolCallDecision::Ask` |
| `AfterTool` 命中会改变 workflow 观察面的工具 | 请求刷新 snapshot | `refresh_snapshot = true` |
| `BeforeStop` 命中 `session_ended` | 允许自然结束 | 仅 diagnostics，不阻止退出 |
| `BeforeStop` 命中 `checklist_passed` 且 evidence 缺失 | 注入 stop gate，继续 loop | 返回 steering/constraints |
| `BeforeStop` 命中 `checklist_passed` 且 evidence 已齐备 | 允许结束 | diagnostics 标记 satisfied |
| `BeforeStop` 存在未结案 `blocking_review` action | 持续阻止自然结束 | `StopDecision::Continue` + runtime pending action stop gate |
| 用户批准 pending tool call | 原 tool call 继续执行 | `tool_call_update(status=in_progress)` |
| 用户拒绝 pending tool call | 原 tool call 不执行，但 loop 继续推进 | `tool_result.details.approval_state=rejected` |
| `companion_dispatch` 命中 `BeforeSubagentDispatch` deny | 同步拒绝派发 | `AgentToolError::ExecutionFailed` |
| `companion_complete` 在非 companion session 调用 | 同步拒绝回流 | `当前 session 不是通过 companion_dispatch 建立的上下文` |
| `companion_complete` 回流到仍持有 hook runtime 的父 session | 同步触发 `SubagentResult` | trace + diagnostics 同步可见 |
| 主 session 调用 `resolve_hook_action` | 显式结案当前 hook action | runtime snapshot 保留记录，主事件流收到 `hook_action_resolved` |

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
业务规则仍由 provider 解释，但已经统一通过 `HookPolicyView` / `HookResolution` 输出
```

#### Design Decision: `HookPolicyView` 只是 runtime 观测面

**Context**:

- 当前 session hook runtime 同时需要“真正执行规则”和“向前端暴露当前生效规则面”
- 若把两者都命名成 `HookPolicy`，后续很容易误判这里已经是可执行 rule engine

**Decision**:

- 合同层将该结构命名为 `HookPolicyView`
- 它是从 workflow projection / snapshot / rule registry 派生出来的只读视图
- 直接解释执行逻辑仍应收口在 hook rule registry，而不是在多个 view model 上分散实现

**Wrong**:

```text
把 HookPolicyView 当作第二套执行 authority，继续手写一份与 rule engine 平行的行为逻辑。
```

**Correct**:

```text
让 HookPolicyView 负责展示“当前 runtime 实际生效的规则面”，
让单一 hook rule registry 负责真正的 matches/apply。
```

#### Bad

```text
在 route/gateway 里继续直接拼 prompt 字符串表达流程 gate
在 agent_loop 里直接查 repo / workflow run / task status
前端只展示 workflow step 摘要文本，不展示实际 runtime policies
```

### 6. Tests Required

至少应覆盖以下断言点：

- `execution_hooks` 单测：
  - `shell_exec` 绝对 `cwd` 会在 hook runtime 中被 rewrite
  - `permission_policy=SUPERVISED` 时，执行类工具会返回 approval request
  - checklist step 未满足时 `before_stop` 注入 gate
  - checklist step 满足时 `before_stop` 允许结束
  - `BeforeSubagentDispatch` 会继承 runtime context / constraints
  - `companion_dispatch` 会按 slice mode 过滤 fragments / constraints，并生成 return-channel 指令
  - `SubagentResult` 会记录结构化 return-channel diagnostic
  - snapshot 会合并 `global_builtin + workflow` 来源，并暴露去重后的 `sources`
  - workflow 产出的 `policy / constraint / diagnostic` 必须带 `source_refs`
- `cargo check`：
  - `agentdash-agent`
  - `agentdash-executor`
  - `agentdash-api`
- `agentdash-agent` 单测：
  - `Ask` 会进入 pending approval
  - reject 时不真正执行工具，但会产出结构化 rejection tool_result
- 前端构建/类型：
  - `pnpm --filter frontend exec tsc --noEmit`
  - `pnpm --filter frontend build`
- 前端测试：
  - pending approval 工具卡片会显示审批按钮
  - `approval_state=rejected` 会渲染为“已拒绝”
  - `hook_event` 会进入主事件流并展示 trigger / decision / completion / diagnostics 摘要
- 联调：
  - `GET /api/sessions/{id}/hook-runtime` 返回 `policies + metadata + diagnostics`
  - 会话页能看到 `policies: N` 与具体 policy 列表
  - 真实 session jsonl 能看到 `hook_event`，且页面主事件流能显示 `before_stop / session_terminal` 的 hook 卡片

### 7. Wrong vs Correct

#### Wrong

```text
workflow runtime 直接长成“工具前后如何决策”的一大坨 if/else 中心；
agent_loop 再直接去问 workflow 当前 step 和外部业务状态；
前端只能看到 workflow step 文本，却看不到规则真正来自哪一层。
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

## Scenario: Checklist Evidence Artifact Contract

### 1. Scope / Trigger

- Trigger: workflow phase 使用 `checklist_passed` completion
- Trigger: Pi Agent / runtime tool 需要向当前 active workflow phase 写入结构化 evidence
- Trigger: 前端 / API / hook runtime 需要统一识别 `checklist_evidence`

### 2. Signatures

#### Domain Enum

```rust
pub enum WorkflowRecordArtifactType {
    SessionSummary,
    JournalUpdate,
    ArchiveSuggestion,
    PhaseNote,
    ChecklistEvidence,
}
```

#### Runtime Tool

```rust
report_workflow_artifact({
  content: string,
  artifact_type?: "phase_note" | "checklist_evidence" | "session_summary" | "journal_update" | "archive_suggestion",
  title?: string,
})
```

#### Workflow Completion Definition

```json
{
  "completion": {
    "checks": [
      {
        "key": "checklist_evidence_present",
        "kind": "checklist_evidence_present",
        "description": "必须产出 checklist evidence"
      }
    ],
    "default_artifact_type": "checklist_evidence",
    "default_artifact_title": "检查证据"
  }
}
```

#### Snapshot Metadata

`active_workflow` 当前至少必须暴露：

- `default_artifact_type`
- `default_artifact_title`
- `checklist_evidence_artifact_type`
- `checklist_evidence_present`
- `checklist_evidence_count`
- `checklist_evidence_artifact_ids`
- `checklist_evidence_titles`

### 3. Contracts

#### 3.1 Evidence 判据必须跟随 Completion 配置

- `checklist_passed` 的正式 evidence 判据，不再硬编码为“某段 assistant 文本长得像检查结论”
- provider 必须读取当前 active workflow `completion.default_artifact_type`
- hook runtime 必须以“当前 step 下、artifact_type 等于该配置值、且内容非空”的 record artifacts 作为 checklist evidence
- 若 workflow completion 未声明 `default_artifact_type`，才允许回退到 `phase_note`

#### 3.2 Builtin Workflow 仍然只是声明层

- Trellis builtin workflow 只能通过 JSON phase 配置声明：
  - `completion.checks`
  - `completion.default_artifact_type`
  - `completion.default_artifact_title`
- 禁止在 hook rule / route / tool 层写死：
  - `check phase -> phase_note`
  - `check phase -> 检查证据`
  - `phase_key == check` 就代表某种特殊 artifact 语义

#### 3.3 Tool 写入契约

- `report_workflow_artifact` 只能向当前 active workflow phase 追加记录产物
- 当调用方未显式传 `title` 时，应优先使用 snapshot 中的 `default_artifact_title`
- 当调用方显式传入不支持的 `artifact_type` 时，必须同步报错，不允许静默降级
- tool 写入成功后，必须主动 refresh hook runtime snapshot，保证 `before_stop` 能在同一轮外循环中看到最新 evidence

#### 3.4 Completion 产物契约

- hook runtime 在 `checklist_passed` 满足并自动完成 step 时，生成的 completion artifact 也必须沿用当前 workflow completion 的 `default_artifact_type`
- 不能再把自动 completion artifact 固定写成 `phase_note`

### 4. Validation & Error Matrix

| 场景 | 预期行为 | 错误/结果 |
|---|---|---|
| 当前 session 没有 hook runtime | 拒绝写入 | `当前 session 没有 hook runtime，无法写入 workflow 记录产物` |
| 当前 session 没有关联 active workflow | 拒绝写入 | `当前 session 没有关联 active workflow，无法写入 workflow 记录产物` |
| `content` 为空 | 拒绝写入 | `content 不能为空` |
| `artifact_type=checklist_evidence` | 正常写入 | run.record_artifacts 新增 `checklist_evidence` |
| `artifact_type` 未知 | 拒绝写入 | `artifact_type 不支持` |
| `checklist_passed` phase 未声明 `default_artifact_type` | 允许回退 | evidence 类型按 `phase_note` 处理 |
| `checklist_passed` phase 声明 `default_artifact_type=checklist_evidence` | 不得降级 | before_stop 只认 `checklist_evidence` |

### 5. Good / Base / Bad Cases

#### Good

```text
builtin workflow JSON 声明 check phase 使用 checklist_evidence
    ↓
report_workflow_artifact 写入 checklist_evidence
    ↓
hook snapshot 刷新并记录 checklist_evidence_present=true
    ↓
before_stop 依据 task_status + checklist_evidence 允许 stop
```

#### Base

```text
phase 未声明 default_artifact_type
→ 仍然允许沿用 phase_note 作为默认记录类型
```

#### Bad

```text
只要 phase_key == "check" 就把 artifact 当成 checklist evidence
根据 last_assistant_text 猜“像不像检查结论”
前端显示 checklist_evidence，但后端 stop gate 实际只认 phase_note
```

### 6. Tests Required

- `execution_hooks`：
  - `checklist_passed` 未见 evidence 时，`before_stop` 必须继续
  - phase 默认 artifact type 为 `checklist_evidence` 时，`before_stop` 必须认 `checklist_evidence`
  - 自动 phase completion 生成的 artifact type 必须跟随 phase 配置
- `application::address_space` / runtime tool：
  - `report_workflow_artifact` 支持 `checklist_evidence`
  - 非法 `artifact_type` 返回明确错误
  - 写入成功后 snapshot 已刷新
- API / DTO：
  - `/api/workflow-runs/{id}`
  - `/api/lifecycle-runs/bindings/{kind}/{id}`
  - 返回的 `record_artifacts[].artifact_type` 必须保留 `checklist_evidence`
- 联调：
  - 真实 session 至少一轮验证 `continue -> stop -> phase advance -> record`
  - 真实 workflow run 至少包含一条 `artifact_type=checklist_evidence`

### 7. Wrong vs Correct

#### Wrong

```text
checklist_passed 本质上仍靠 assistant 自然语言启发式判定；
builtin workflow 只是名字上配置了 phase，真正 evidence 逻辑写死在 hook if/else 里。
```

#### Correct

```text
workflow phase 配置声明 evidence 类型
    ↓
runtime tool / auto completion 都按该类型写入 artifact
    ↓
hook runtime 从当前 phase 配置反推 evidence 判据
    ↓
before_stop / phase advance / frontend 展示使用同一事实来源
```

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

---

> Pi Agent 流式 chunk 合并协议已拆分到 [pi-agent-streaming.md](./pi-agent-streaming.md)。
