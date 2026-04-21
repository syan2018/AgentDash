# Workflow 单 step 数据模型与激活路径统一

## Goal

把围绕"单个 lifecycle step"的**数据投影层**和**运行时激活路径**两件事同时收敛:

1. **投影压扁**：把 `WorkflowContract → EffectiveSessionContract → ActiveWorkflowProjection → SessionWorkflowContext` 这条四层派生链,压缩到 **`WorkflowContract` (定义) + `ActiveWorkflowProjection` (运行时聚合)** 两层;其余结构要么 inline 到消费点,要么让 `CapabilityResolver` 直接吃原子类型。
2. **激活统一**：把 bootstrap / AgentNode / PhaseNode 三条"激活 step"的路径,拆成 **`activate_step(StepActivationInput) → StepActivation` 纯函数 + 3 个 applier**;把散布在 orchestrator / advance_node / plan_builder / session_runtime_inputs / turn_context 五处的"查 step → 算 caps → 调 Resolver → 拼 MCP list → 写入运行时"重复逻辑收回一处。

## Why（问题陈述）

### 现状 1：四层派生,每层只加一点点

| 结构 | 字段组成 | 新增的信息 |
|---|---|---|
| `WorkflowContract` | injection / hook_rules / constraints / completion / recommended_*_ports | —— 唯一定义 |
| `EffectiveSessionContract` | lifecycle_key + active_step_key + **contract 前 4 字段拷贝** | 仅两个 key 标签 |
| `ActiveWorkflowProjection` | run / lifecycle / active_step / primary_workflow / effective_contract | aggregate root |
| `SessionWorkflowContext` | has_active_workflow (bool) + workflow_capabilities (Option<Vec<String>>) | 两字段三态语义 |

- [build_effective_contract](crates/agentdash-domain/src/workflow/entity.rs#L418-L438) 把 `primary_workflow.contract` 的 4 个字段平铺拷一份,projection 已经持有 `active_step + primary_workflow`,调用方 `primary_workflow.contract.hook_rules` 同样能拿到——**冗余一层**。
- [resolver.rs:36-42](crates/agentdash-application/src/capability/resolver.rs#L36-L42) `SessionWorkflowContext` 用 `(bool, Option<Vec<String>>)` 表达三态语义 (`NONE` / active+caps / active+无 caps),本质上只是在描述"`active_step` 存不存在"——表达力**被人为压扁**。

### 现状 2：三条激活路径各写一遍 CapabilityResolver

| 阶段 | A. Bootstrap 新 session | B. AgentNode 激活 (orchestrator) | C. PhaseNode / complete_lifecycle_node |
|---|---|---|---|
| 查 step | `resolve_session_workflow_context` | `lifecycle.steps.iter().find(key)` | `projection.active_step` |
| baseline caps | 空集 | `hook_session.current_capabilities()` 读父 session | `hook_session.current_capabilities()` 读自身 |
| 算 effective caps | `capabilities_from_active_step` | `compute_effective_capabilities` | `compute_effective_capabilities` |
| 调 resolver | 是 | 是 | 是 |
| prompt/mount | plan_builder 构 context markdown | orchestrator 手写 kickoff prompt + `build_lifecycle_mount_with_ports` | 无,但要 `push_session_notification` 注入 delta MD |
| 写入运行时 | `PromptSessionRequest` | 新 session + `SessionBinding` + `mark_owner_bootstrap_pending` | `hook_session.update_capabilities` + `replace_runtime_mcp_servers` + `emit_capability_changed_hook` |

具体重复:
- [orchestrator.rs:477-527](crates/agentdash-application/src/workflow/orchestrator.rs#L477-L527) (AgentNode) 与 [advance_node.rs:381-476](crates/agentdash-application/src/workflow/tools/advance_node.rs#L381-L476) (PhaseNode) 几乎一字不差地重复 "读 baseline → compute_effective → Resolver → 拼 mcp_servers list"。
- [turn_context.rs:94-147](crates/agentdash-application/src/task/gateway/turn_context.rs#L94-L147) 与 [session_runtime_inputs.rs:56-82](crates/agentdash-application/src/task/session_runtime_inputs.rs#L56-L82) **同一个 task turn 内 Resolver 被调用两次**,输入几乎相同。
- kickoff prompt 的 output/input port Markdown 硬编码在 orchestrator,PhaseNode 切换时即便也想给 agent 同款 port 说明,没有复用路径。
- MCP 热更新 (`replace_runtime_mcp_servers`) 仅存在于 PhaseNode 路径;Bootstrap / AgentNode 没有对等能力——能力变化只能通过"重建 session"表达。

### 连带影响

- **前端对齐困难**:`SessionWorkflowContext` 的三态语义只在注释里,前端 mapper 写起来全靠猜;`EffectiveSessionContract` 在 API 层又多暴露一份,前端要同时看两份字段来源。
- **新 node_type 无处加**:未来若加 SubRunNode / DecisionNode 等变体,现状需要分别改 3 处激活路径,注定会继续长出新的"几乎一样"的代码块。
- **testing 散**:capability 计算的边界测试散落在 resolver / session_workflow_context / advance_node / orchestrator 各自的 test 模块,没有单点可靠真相。

## Requirements

### Part 1：压扁投影层

**删除 / 内联:**
- 删掉 `EffectiveSessionContract` 这个 struct 及 `build_effective_contract` 函数;消费点直接用 `ActiveWorkflowProjection.primary_workflow.contract.{injection, hook_rules, constraints, completion}` 访问。
- 删掉 `SessionWorkflowContext`;`CapabilityResolverInput` 改为接收 `active_step: Option<&LifecycleStepDefinition>` (或未来 contract 语义收到 workflow 后,`active_workflow: Option<&WorkflowDefinition>`——与 `04-21-workflow-contract-capabilities` 结果对齐)。
- `has_active_workflow` 布尔在 resolver 内部由 `active_step.is_some()` 推导;visibility rule 的 workflow 活跃判定随之简化。
- 删掉 `capabilities_from_active_step` helper,逻辑内联进 resolver(或后续 `activate_step` 函数)。

**保留:**
- `WorkflowContract` —— 唯一定义源。
- `ActiveWorkflowProjection` —— 运行时聚合;但删除 `effective_contract` 字段。

### Part 2：抽出 `activate_step` 纯函数

**纯计算层(新建,放在 `crates/agentdash-application/src/workflow/step_activation.rs`):**

```rust
pub struct StepActivationInput<'a> {
    pub owner_ctx: SessionOwnerCtx,              // 依赖 04-20-session-owner-sum-type 完成
    pub active_step: &'a LifecycleStepDefinition,
    pub workflow: Option<&'a WorkflowDefinition>, // 可能为 None(未绑定 workflow 的 step)
    pub baseline_capabilities: Vec<String>,       // bootstrap 时传空
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    pub platform: &'a PlatformConfig,
}

pub struct StepActivation {
    pub flow_capabilities: FlowCapabilities,
    pub mcp_servers: Vec<McpServer>,
    pub capability_keys: BTreeSet<String>,
    pub capability_delta: Option<CapabilityDelta>,  // 对比 baseline,用于 PhaseNode
    pub kickoff_prompt_fragment: Option<KickoffPromptFragment>,
    pub lifecycle_mount: Option<VfsMount>,
}

pub fn activate_step(input: StepActivationInput) -> StepActivation;
```

**三个 applier(各自只关心"往哪里写"):**

```rust
// bootstrap 路径:合入 PromptSessionRequest
pub fn apply_to_prompt_request(activation: &StepActivation, req: &mut PromptSessionRequest);

// orchestrator 创建 AgentNode session 路径
pub async fn apply_to_new_lifecycle_session(
    activation: &StepActivation,
    session_hub: &SessionHub,
    binding_repo: &dyn SessionBindingRepository,
    /* session_id, parent binding, ... */
) -> Result<String, String>;

// PhaseNode / complete_tool 路径:热更新运行中的 session
pub async fn apply_to_running_session(
    activation: &StepActivation,
    hook_session: &SharedHookSessionRuntime,
    session_hub: &SessionHub,
    emit_delta_notification: bool,
) -> Result<(), String>;
```

### Part 3：替换 5 个消费点

- `crates/agentdash-application/src/session/plan_builder.rs` —— 删除 `workflow_ctx` 字段构造,改为内部调 `activate_step`;`SessionPlanInput` 携带 `active_step` 即可。
- `crates/agentdash-application/src/task/session_runtime_inputs.rs` —— 删除就地 Resolver 调用,改走 `activate_step`。
- `crates/agentdash-application/src/task/gateway/turn_context.rs` —— 与上条共享一次 activate_step 输出(解决"同 turn 两次 Resolver"问题)。
- `crates/agentdash-application/src/workflow/orchestrator.rs` AgentNode 路径 —— 用 `activate_step` + `apply_to_new_lifecycle_session`,kickoff prompt 构造迁移到 `KickoffPromptFragment`。
- `crates/agentdash-application/src/workflow/tools/advance_node.rs` PhaseNode 路径 —— 用 `activate_step` + `apply_to_running_session`。

## Acceptance Criteria

- [ ] `grep -rn "EffectiveSessionContract\|build_effective_contract" crates/` 归零
- [ ] `grep -rn "SessionWorkflowContext\|capabilities_from_active_step" crates/` 归零
- [ ] `ActiveWorkflowProjection` 不再持有 `effective_contract` 字段
- [ ] `CapabilityResolver` 直接调用点只剩 1 处(在 `activate_step` 内部);其他路径全部走 `activate_step`
- [ ] `apply_to_prompt_request` / `apply_to_new_lifecycle_session` / `apply_to_running_session` 三个 applier 覆盖现有 5 处消费点
- [ ] 单 turn 内 CapabilityResolver 调用次数 ≤1(通过 turn_context 与 session_runtime_inputs 共享 activation)
- [ ] 新增 step_activation 模块的单元测试:三种 `node_type × 有/无 baseline × 有/无 workflow_key` 的组合矩阵
- [ ] 现有 `resolver.rs` / `orchestrator.rs` / `advance_node.rs` 的测试全绿(行为不变)
- [ ] `cargo build` / `cargo test` / `cargo clippy` 全绿
- [ ] Spec `.trellis/spec/backend/tool-capability-pipeline.md` 更新:描述 `activate_step` 为唯一 capability 解析入口

## Technical Approach

### 分阶段推进

**PR1 — 投影压扁(Part 1)**
- 删 `EffectiveSessionContract` / `build_effective_contract`
- `ActiveWorkflowProjection` 去掉 `effective_contract` 字段
- 消费点改读 `projection.primary_workflow.contract.*`
- `SessionWorkflowContext` 先不动(PR2 才能删)

**PR2 — 引入 `activate_step` 纯函数(Part 2 的计算侧)**
- 新建 `workflow/step_activation.rs`
- 实现 `activate_step` + 3 个 applier(只写签名和骨架)
- 单元测试覆盖计算层
- 不改消费点;保持 `SessionWorkflowContext` 继续服役

**PR3 — 迁移 bootstrap 路径**
- `plan_builder.rs` + `session_runtime_inputs.rs` + `turn_context.rs` 改走 `activate_step`
- 删 `SessionWorkflowContext` / `capabilities_from_active_step`
- turn_context 与 session_runtime_inputs 共享一次 activation(消除双调用)

**PR4 — 迁移 orchestrator / advance_node**
- AgentNode 路径换 `apply_to_new_lifecycle_session`
- PhaseNode 路径换 `apply_to_running_session`
- kickoff prompt Markdown 生成迁移到 `KickoffPromptFragment`

### 依赖

**必须先完成:**
- `04-20-session-owner-sum-type`(`SessionOwnerCtx` sum type,`StepActivationInput.owner_ctx` 依赖此类型)

**建议先完成(否则 PR 顺序需要调整):**
- `04-21-workflow-contract-capabilities`(capabilities 从 step 迁到 contract)——若先完成,`StepActivationInput.active_step` 可以进一步简化为 `StepActivationInput.active_workflow`,且 kickoff prompt 里的 port 语义也随之稳定。

**不强依赖,但会影响测试面:**
- `04-20-dynamic-capability-followup`(验收 PhaseNode cap delta 链路)

### 回归风险

- **capability 计算等价性**:现状三处各自的 caps 算法必须与 `activate_step` 产出字节级相等——需要在 PR3/PR4 迁移时,先跑对比测试(旧逻辑 vs 新逻辑输出同一 `BTreeSet<String>`)。
- **kickoff prompt 文本稳定性**:orchestrator 当前硬编码的 Markdown(output_section / input_section)如果在迁移中措辞改了,会让 Agent 行为发生漂移——PR4 迁移时保持文本逐字对齐,后续另开任务优化。
- **PhaseNode 热更新时序**:`apply_to_running_session` 里 `update_capabilities` / `replace_runtime_mcp_servers` / `push_session_notification` / `emit_capability_changed_hook` 的调用顺序必须与现状完全一致——advance_node 的现有时序是经历过问题修复稳定下来的,见 relevant tests。

## Decision (ADR-lite)

**Context**:workflow 单 step 的数据派生与运行时激活是两个关注点,但当前代码把两者的混乱互相放大——派生结构多让激活路径更难写,激活路径多让派生结构更难删。这次一起动,避免"先改一边导致另一边更复杂"。

**Decision**:
1. 四层投影压到 2 层(`WorkflowContract` + `ActiveWorkflowProjection`)
2. 三条激活路径收敛到 `activate_step` 纯函数 + 3 个 applier
3. 分 4 个 PR 推进,按"数据层 → 计算层 → bootstrap → 编排"的依赖顺序

**Consequences**:
- + 新增 node_type 只需要新增 applier,不用碰计算层
- + capability 计算有唯一真相源,测试可以围绕 `activate_step` 写
- + MCP 热更新能力从"只有 PhaseNode 有"扩展到"任何 applier 都能触发",未来 hot-swap 场景开放
- + 前端对 workflow 状态的 mapper 只需要看 `ActiveWorkflowProjection`,不用并行看 effective_contract/workflow_ctx
- − 4 个 PR 的拆分需要严格按依赖顺序 merge,中间状态可能存在短期"两套共存"
- − PhaseNode 热更新 API 在 applier 层暴露后,需要注意 misuse(例如在 bootstrap 路径误调 running_session applier)——通过 applier 签名里的参数区分规避

## Out of Scope

- 修改 `LifecycleStepDefinition` 字段布局(capabilities 迁移已在 `04-21-workflow-contract-capabilities`)
- 修改 `WorkflowBindingKind` / `WorkflowBindingRole` 双类型(并行演化,独立任务)
- 修改前端 `LifecycleStepDefinition` TS 类型(前端对齐在 assets/workflow 管理面板迭代中做)
- 修改 `LifecycleRun` / `LifecycleRunStatus` 状态机(与本任务正交)
- 修改 kickoff prompt 的 Markdown 文本(本任务只做迁移,不做优化)
- 修改 VFS `lifecycle://` mount 语义(本任务只搬运 `build_lifecycle_mount_with_ports` 调用点)

## Technical Notes

- 关键文件:
  - [crates/agentdash-domain/src/workflow/entity.rs](crates/agentdash-domain/src/workflow/entity.rs) — `build_effective_contract`
  - [crates/agentdash-domain/src/workflow/value_objects.rs](crates/agentdash-domain/src/workflow/value_objects.rs) — `EffectiveSessionContract`
  - [crates/agentdash-application/src/workflow/projection.rs](crates/agentdash-application/src/workflow/projection.rs) — `ActiveWorkflowProjection`
  - [crates/agentdash-application/src/capability/resolver.rs](crates/agentdash-application/src/capability/resolver.rs) — `CapabilityResolverInput.workflow_ctx`
  - [crates/agentdash-application/src/capability/session_workflow_context.rs](crates/agentdash-application/src/capability/session_workflow_context.rs) — 待删
  - [crates/agentdash-application/src/workflow/orchestrator.rs](crates/agentdash-application/src/workflow/orchestrator.rs) — AgentNode 激活
  - [crates/agentdash-application/src/workflow/tools/advance_node.rs](crates/agentdash-application/src/workflow/tools/advance_node.rs) — PhaseNode 激活
  - [crates/agentdash-application/src/session/plan_builder.rs](crates/agentdash-application/src/session/plan_builder.rs) — bootstrap
  - [crates/agentdash-application/src/task/session_runtime_inputs.rs](crates/agentdash-application/src/task/session_runtime_inputs.rs) — task runtime
  - [crates/agentdash-application/src/task/gateway/turn_context.rs](crates/agentdash-application/src/task/gateway/turn_context.rs) — task turn
- 相关 memory:`memory/workflow_design_principle.md` — Workflow 是 agent 单步行为约束的原则,本任务的"压扁投影 + 统一激活"正是对这一原则的结构化落地。
- 前置任务:
  - `04-20-session-owner-sum-type`(硬依赖)
  - `04-21-workflow-contract-capabilities`(强烈建议先完成)
- 参考 commit:`4cf8c94` 和 `7c8ecef` — `workflow_ctx` 与 `ExecutorResolution` 的收口范式,可作为本任务的重构样板。
