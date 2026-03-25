# Workflow / Lifecycle 双实体重构计划

## 0. 当前落地状态（2026-03-25）

本次重构已经正式落地，当前系统 authority 如下：

* domain 层：
  * `WorkflowDefinition.contract` 已进一步收敛为：
    * `injection`
    * `hook_policy`
    * `completion`
  * `LifecycleDefinition.steps[*]` 负责定义 lifecycle step、primary workflow、transition policy
  * `LifecycleRun` 负责 step 级运行状态、runtime attachments、record artifacts
* application 层：
  * `resolve_active_workflow_projection()` 解析 active lifecycle + active step + primary/overlay workflows
  * runtime 最终消费 `EffectiveSessionContract`
  * completion evaluator 已切到 `evaluate_step_transition()`
* API / infra：
  * SQLite 仓储已改成 workflow / lifecycle / lifecycle_run 新表意
  * DTO 与 routes 已切到 `lifecycle_id`、`current_step_key`、`step_states`
  * 新的 canonical API surface 已补充：
    * `/workflow-definitions`
    * `/lifecycle-definitions`
    * `/lifecycle-runs/.../steps/...`
* hook runtime：
  * `SessionStart` 不再作为第二条普通文本注入通道
  * active metadata 已切到 `lifecycle_* / step_* / primary_workflow_*`
  * runtime 会把 `effective_contract + step_transition` 写入 snapshot metadata，并在 `BeforeStop / SessionTerminal` 用它们做正式判定
* frontend：
  * Workflow 视图已改成“Workflow Contracts / Lifecycles”双面结构
  * Task 面板已改为 lifecycle run / step 语义
  * Workflow 编辑器已改为直接编辑 workflow contract，而非 phase 列表
  * Lifecycle 编辑器已正式落地，支持 CRUD / validate / enable-disable / step 级结构化编辑

当前系统已经完成最后一轮命名收口：

* runtime / executor / API / frontend 统一使用 `lifecycle + step`
* `ActiveWorkflowProjection.phase` / `HookPhaseAdvanceRequest.phase_key` 等兼容 facade 已移除
* `phase_note` 仅保留为业务 artifact type，不再代表 lifecycle authority

## 1. 背景结论

当前项目已经拥有一条可工作的 hook runtime 主链，但抽象边界仍不够清晰：

* `workflow` 同时承担了 phase 描述、行为约束、上下文挂载、completion 提示等多种职责
* 真正的运行时 authority 主要仍在：
  * `crates/agentdash-api/src/execution_hooks.rs`
  * `crates/agentdash-application/src/workflow/completion.rs`
* phase 与 lifecycle 被绑在同一个 entity 里，导致“一个 phase 是否就是一个 workflow”这个概念始终不清楚

因此当前系统的核心问题已经不只是“workflow 太薄”，而是：

* workflow 与 lifecycle 没有拆开
* workflow 本体与编排器混在一起
* phase 被做成 workflow 的内嵌结构，复用性与动态注入能力都受限

---

## 2. 核心判断

推荐明确拆成两个领域实体：

### A. `WorkflowDefinition`

表示一个独立的 agent 行为 contract。

它负责定义：

* 目标
* 上下文
* 约束
* 检查逻辑
* 标准化输出

它不负责多阶段编排。

### B. `LifecycleDefinition`

表示 agent 生命周期编排器。

它负责定义：

* 生命周期步骤
* 当前步骤使用哪个 workflow
* 步骤切换条件
* session 复用/新建策略
* 失败/阻塞/恢复策略

它不负责描述单个 workflow 的具体行为 contract。

一句话说：

* workflow 是“做事方式”
* lifecycle 是“什么时候切到哪种做事方式”

---

## 3. 目标模型

### 3.1 领域层

#### `WorkflowDefinition`

原子行为单元，可被：

* 单独 attach 到 session
* 作为 lifecycle step 的 primary workflow
* 作为 overlay workflow 动态注入

#### `LifecycleDefinition`

编排单元，用于驱动：

* step 顺序
* workflow 指派
* 转移与退出

### 3.2 应用层

需要一个 projection 层，把：

* 当前绑定的 lifecycle
* 当前 active step
* 当前主 workflow
* 当前动态 overlay workflows
* 当前 runtime facts

解析成 session 的 effective contract。

### 3.3 运行时层

hook runtime / executor runtime 只消费 projection 输出的 active contract：

* 注入 context
* 执行 constraints
* 评估 checks
* 判定 stop gate
* 驱动 lifecycle transition
* attach / detach overlay workflows

---

## 4. WorkflowDefinition 应具备的一级概念

未来 workflow 不应继续依赖：

* `agent_instructions`
* `context_bindings`
* `completion_mode`

来拼出行为语义。

它至少应拥有：

### 4.1 Injection

当前 workflow 输入时需要注入的内容：

* goal
* instructions

* document
* runtime context
* checklist source
* artifact ref
* action ref

### 4.2 Hook Policy

当前 workflow 的持续治理规则：

* deny tool
* rewrite arg
* require approval
* deny status transition
* require artifact before exit
* output style enforcement

### 4.3 Completion

当前 workflow 的正式检查逻辑：

* task status
* artifact evidence
* session terminal state
* pending action clearance
* subagent result state

---

## 5. LifecycleDefinition 应具备的一级概念

Lifecycle 不是 workflow 的一个 `kind`，而是单独实体。

它至少应拥有：

### 5.1 Steps

生命周期步骤列表。

每个 step 至少包括：

* `key`
* `title`
* `primary_workflow_ref`

### 5.2 Workflow Attachments

除 primary workflow 外，还应允许 step 附加：

* 默认 overlay workflows
* enter 时 attach 的 workflows
* exit 时 detach 的 workflows

### 5.3 Transition Rules

定义：

* 何时从 step A 切到 step B
* 依赖哪些 runtime signal / check result / user action

### 5.4 Session Policy

定义：

* 当前 step 是否需要 session
* 复用现有 session 还是开启新 session
* step 切换时 session 是否延续

### 5.5 Failure / Recovery Policy

定义：

* stay
* block
* retry
* fail lifecycle
* handoff / manual intervention

---

## 6. 无 phase 标准化行为如何建模

用户提到的这类场景：

* 不分 phase
* 用于规范 agent 行为
* 在运行时动态注入

在新模型下不需要单独创造新物种。

它就是普通 `WorkflowDefinition`，只是不被 lifecycle 串成多步，而是以 overlay 方式 attach 到 session：

* 代码 review 输出规范 workflow
* 研究报告结构 workflow
* 记录沉淀 workflow
* 子 agent 回流 adoption workflow

也就是说：

* phase step 里的 workflow
* 动态 attach 的 workflow

都是同一种 entity，只是 attachment 来源不同。

---

## 7. 第一版推荐方案

### 7.1 选择：内置 evaluator + 预留插件扩展

这是本轮已确认方向。

原因：

* 当前主要问题是概念缺失，不是表达力不足
* 直接上 DSL 会把系统过早做成解释器
* 但完全不预留扩展点，会迫使未来继续把语义塞回 runtime if/else

### 7.2 第一版原则

* workflow contract 使用内置 `checks / constraints / outputs`
* lifecycle transition 使用内置 transition policy
* `custom_*_key` 只预留字段，不开放任意脚本

---

## 8. 第一版内置能力范围

### 8.1 内置 checks

推荐支持：

* `task_status_in`
* `artifact_exists`
* `artifact_count_gte`
* `session_terminal_in`
* `pending_action_none`
* `subagent_result_status_in`

### 8.2 内置 constraints

推荐支持：

* `deny_tool`
* `rewrite_tool_arg`
* `require_approval`
* `deny_task_status_transition`
* `require_artifact_before_exit`
* `block_stop_until_checks_pass`
* `subagent_adoption_mode`
* `require_output_section`
* `require_output_artifact`
* `enforce_response_style`

### 8.3 内置 lifecycle transition policy

推荐支持：

* `manual`
* `all_checks_pass`
* `any_checks_pass`
* `session_terminal_matches`
* `explicit_action`

---

## 9. 生命周期边界映射

effective contract 必须在以下边界生效：

### 9.1 `SessionStart`

* 激活当前 active step 对应的 workflow attachments
* 建立 baseline trace

### 9.2 `UserPromptSubmit`

* 注入当前 active workflows 的 context / obligations / output contract

### 9.3 `BeforeTool`

* 执行 active constraints

### 9.4 `AfterTool`

* 刷新 evidence / check state

### 9.5 `AfterTurn`

* 处理 follow-up / pending action / 动态 attach logic

### 9.6 `BeforeStop`

* 执行 checks / stop gate

### 9.7 `SessionTerminal`

* 执行 lifecycle transition / failure handling

### 9.8 `BeforeSubagentDispatch / SubagentResult`

* 继承 active contract
* 回流结果触发 checks / overlays / transitions

---

## 10. 与当前代码的映射关系

### 10.1 可以保留的部分

* `HookSessionRuntime`
* `HookRuntimeDelegate`
* 现有 trigger 边界：
  * `SessionStart`
  * `UserPromptSubmit`
  * `BeforeTool`
  * `AfterTool`
  * `AfterTurn`
  * `BeforeStop`
  * `SessionTerminal`

### 10.2 需要降级或改造的部分

* `agent_instructions`
  * 从主治理来源降级为辅助说明
* `completion_mode`
  * 从 workflow 核心字段降级为 lifecycle transition preset 的 legacy shortcut
* `checklist` binding
  * 从“检查逻辑”降级回“checklist 资源来源”

### 10.3 需要新增的 projection 结果

建议新增：

* `ResolvedWorkflowContract`
* `ResolvedWorkflowAttachment`
* `ResolvedLifecycleStep`
* `EffectiveSessionContract`

runtime 应消费这些 projection 结果，而不是直接消费 workflow/lifecycle 原始字段。

---

## 11. 分阶段重构计划

### Phase A：补足领域模型

目标：

* 新增 `LifecycleDefinition`
* 扩展 `WorkflowDefinition` 成为原子 contract 实体

建议改动：

* `WorkflowDefinition` 增加 `constraints / checks / outputs`
* 新建 `LifecycleDefinition`
* 保留旧 `WorkflowPhaseDefinition` 作迁移桥，但开始标注 legacy

### Phase B：建立 projection

目标：

* 解析 active lifecycle + attached workflows

建议改动：

* 扩展 `ActiveWorkflowProjection`
* 新增 `EffectiveSessionContract`
* 支持 step primary workflow + overlay workflows 的统一合成

### Phase C：让 runtime 消费 effective contract

目标：

* 把硬编码逻辑从 if/else 迁到 contract consumer

建议改动：

* `BeforeTool` 消费 constraints
* `BeforeStop` 消费 checks
* `SessionTerminal` 消费 lifecycle transition
* `AfterTurn` 支持 overlay attach / detach

### Phase D：清理旧 phase/completion 模型

目标：

* 收掉 `phase + completion_mode` 旧路径中的核心 authority

建议改动：

* `checklist_passed` 改写为 builtin checks + lifecycle transition
* 逐步移除只剩历史意义的 completion if/else

---

## 12. MVP 与非 MVP

### MVP

* `WorkflowDefinition` 正式拥有 `constraints / checks / outputs`
* `LifecycleDefinition` 正式拥有 `steps / workflow_ref / transition`
* runtime 消费 `EffectiveSessionContract`
* `check` step 改为 workflow + lifecycle 组合驱动
* 支持至少一种动态 attach workflow

### 非 MVP

* 通用 rule DSL
* 用户自定义脚本 evaluator
* 复杂 rollback graph
* 全量可视化 editor

---

## 13. 风险与注意点

### 风险 1：workflow 与 lifecycle 再次混用

约束：

* workflow 只描述行为 contract
* lifecycle 只描述编排

### 风险 2：新旧模型双轨期混乱

约束：

* 明确 authority 字段
* spec 中明确 legacy 字段只作兼容桥

### 风险 3：context / constraint / check / output 混用

约束：

* `context` = 输入
* `constraint` = 行为治理
* `check` = 达成判据
* `output` = 产出契约

---

## 14. 建议的下一步实施顺序

1. 在 spec 中先补“workflow contract / lifecycle orchestration”正式定义
2. 设计 `WorkflowDefinition / LifecycleDefinition / EffectiveSessionContract` 结构
3. 先拿 `check` step 做第一批试点迁移
4. 把 `checklist_passed` 重写为 builtin checks + lifecycle transition preset
5. 再做一个动态 attach workflow 试点
6. 最后迁移 start / implement / record 等旧 phase 模型

---

## 15. 当前讨论结论摘要

本轮已确认：

* lifecycle 不应继续作为 workflow 顶层 kind
* workflow 与 lifecycle 应拆成两个独立实体
* 每个 lifecycle step 都可以视为“对一个 workflow 的一次编排引用”
* 无 phase 的标准化行为也是普通 workflow，只是通过动态 attach 注入 session
* runtime 应始终消费 effective contract，而不是继续发明 workflow 语义
* 第一版采用“内置 evaluator + 预留插件扩展点”，不直接引入通用 DSL
