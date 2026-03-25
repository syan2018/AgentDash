# Workflow Hook 收口与精简实施计划

## 背景

当前 workflow hook 主链已经基本成形：

- `ActiveWorkflowProjection` 提供 active run / phase / binding 的业务投影
- `AppExecutionHookProvider` 负责把业务投影翻译成 session hook snapshot / resolution
- `HookSessionRuntime` 承担 session 级缓存、trace、pending action 与 post-evaluate bridge
- `HookRuntimeDelegate` 在 loop 边界同步消费 resolution

问题不在“没有主链”，而在“同一语义被多层重复表达”，导致后续扩展时难以判断 authority 与职责边界。

---

## 已确认决策

### 决策 1：可执行 authority 与观测视图分离

- `normalized_hook_rules()` 仍是当前唯一的直接解释执行逻辑
- `HookPolicyView` 是从 snapshot / projection / rule registry 派生出来的只读视图
- 后续若要让规则配置化，应该从 rule registry 统一派生 view，而不是让 view 与 rule 各自演化

### 决策 2：`SessionStart` 保留，但职责收敛为 baseline setup

- 不删除 `SessionStart`
- 后续把它接成 session baseline 初始化 trigger
- 不把它变成与 `UserPromptSubmit` 并列的第二条文本注入主通道

### 决策 3：静态上下文与动态治理分层

- SessionPlan / owner bootstrap：负责稳定背景与业务上下文
- connector system prompt：负责稳定环境能力与工具/path 规则
- dynamic hook：负责 active workflow、stop gate、pending action 等会变化的治理信息

---

## Phase 0：术语与契约收口

### 目标

先消除最容易误导协作者的命名歧义，让代码和 spec 都明确：

- `HookPolicyView` 是 view，不是 rule engine
- 继续扩张时不要把它当作第二套执行 authority

### 本轮落地

- 合同层 `HookPolicy` 更名为 `HookPolicyView`
- 执行层 / API / 前端类型同步更名
- spec 中补充“只读派生视图”约束

### 风险

- 主要是编译和类型引用更新风险，业务行为不应变化

### 验证

- Rust 检索无 `HookPolicy` 残留业务引用
- TypeScript 类型引用完成更新
- 基础编译通过

---

## Phase 1：authority 收口

### 目标

明确“规则在哪执行、视图从哪派生”，避免继续双轨维护。

### 建议改动

- 将 `normalized_hook_rules()` 提炼为具名 `HookRuleRegistry`
- 为每条 rule 增加稳定的 view 投影能力
- `build_workflow_policies()` 与 global builtin policies 改为从 rule registry / projection 派生

### 结果形态

- 只有一处写 `matches/apply`
- 前端仍可看到 `policies`
- 但 `policies` 不再是独立手写的第二套逻辑

### 风险

- 需要设计 rule 与 view 的最小共享结构，避免过早 DSL 化

### 验证

- 关键规则（rewrite / deny / ask / stop gate）行为保持不变
- Session Runtime 面板仍能展示相同或更好的 policy 信息

---

## Phase 2：注入层级收口

### 目标

把当前三条注入链降成两条主链，减少重复 token 与重复说明。

### 建议改动

- SessionPlan 中不再重复输出工具面 / runtime policy 的自然语言摘要给 Task Execution prompt
- connector system prompt 仅保留环境能力、工具面、路径锚点、MCP
- dynamic hook 仅保留 phase / constraints / stop gate / pending actions
- workflow contract 进一步收敛为三段：
  - `injection`：输入时注入什么内容
  - `hook_policy`：hook 时如何处理放行 / 阻挡 / 改写 / 注入
  - `completion`：完成检查与默认记录产物
- `injection.instructions` 在 hook 注入时只保留一种正文表达

### 建议优先级

1. 先去掉 `workflow_phase_constraints` fragment 与 `HookConstraint` 的双份表达
2. 再评估 SessionPlan 中的 `tool_visibility_summary` / `runtime_policy_summary` 是否对 Task Execution 仍有必要

### 风险

- 若裁剪过度，可能让某些执行器首次进入 session 时失去必要背景

### 验证

- 启动 prompt 与 hook 注入文本长度下降
- 关键路径上的工具/path 约束仍能被模型感知
- `before_stop` / `subagent_result` 等动态治理信息仍在 runtime hook 中完整出现

---

## Phase 3：SessionStart baseline setup

### 目标

让 `SessionStart` 从“合同层保留字”变成真实的 baseline setup trigger。

### 建议改动

- 在 session hook runtime 完成加载后、connector 启动前补发一次 `HookTrigger::SessionStart`
- 允许它：
  - 记录 baseline trace
  - 固化 session metadata
  - 触发必要 snapshot baseline refresh
- 不允许它直接产生第二份普通用户消息注入

### 风险

- 如果让 `SessionStart` 和 `UserPromptSubmit` 同时注入正文，会重新引入双注入问题

### 验证

- trace 面板中能观察到 `session_start`
- 不新增重复文本注入
- 后续 `UserPromptSubmit` 仍是唯一的 per-turn 文本注入入口

---

## 文件落点建议

- 契约与类型：`crates/agentdash-connector-contract/`
- 执行层：`crates/agentdash-executor/`
- provider / rule：`crates/agentdash-api/src/execution_hooks.rs`
- 静态 bootstrap：`crates/agentdash-application/src/session_plan.rs`
- code-spec：`.trellis/spec/backend/execution-hook-runtime.md`

---

## 本轮正式修复结果

本轮已实际落地以下收口项：

1. `HookPolicy -> HookPolicyView` 命名收口完成，明确 view 不是执行 authority。
2. `SessionStart` 已接入执行主链：
   - 在 session hook runtime 加载完成后、connector `prompt(...)` 前真实触发
   - 与 `SessionTerminal` 共享同一套 `evaluate -> refresh -> trace -> hook_event` helper
   - 仅做 baseline setup / trace / refresh，不再作为第二条文本注入通道
3. workflow contract 已进一步压缩为 `injection / hook_policy / completion` 三段：
   - `injection.instructions` 通过 fragment 注入
   - `hook_policy.constraints` 作为正式 hook 规则来源
   - 删除 `outputs`、`notes` 等当前不属于核心机制的设计
4. 规则 authority 命名已收口：
   - `normalized_hook_rules()` 更名为更明确的 `hook_rule_registry()`
5. PiAgent connector system prompt 不再重复展开 hook runtime 的静态约束摘要：
   - 只保留“当前启用了 Hook Runtime，动态治理信息会在边界注入”的轻量说明
   - 动态 workflow / constraints / pending action 以 runtime 注入结果为准

## 验证要求

本轮修复完成后必须至少验证：

1. `SessionStart` 在 connector 启动前真实发生，并写入 runtime trace。
2. workflow snapshot / context fragments 中不再出现 `workflow_phase_constraints`。
3. `cargo test -p agentdash-api execution_hooks`
4. `cargo test -p agentdash-executor hub`
5. `cargo check -p agentdash-executor -p agentdash-api`
6. `pnpm --filter frontend exec tsc --noEmit`
