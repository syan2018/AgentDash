# brainstorm: workflow lifecycle runtime design

## Goal

重新定义项目中的 workflow 系统，使它不再只是 phase/status 切换器，而是真正能在 agent 全生命周期内持续生效的运行时治理系统：既能声明阶段目标，也能声明阶段内约束、上下文注入、检查逻辑、完成判据与失败/回退策略。

## What I already know

* 当前 `WorkflowPhaseDefinition` 主要字段是 `agent_instructions`、`context_bindings`、`requires_session`、`completion_mode`、`default_artifact_*`。
* 当前 phase 约束主要通过 `agent_instructions -> HookConstraint` 落入 runtime。
* 当前 `context_bindings` 更偏资源挂载/文本注入，不是行为级治理逻辑。
* 当前 completion 只有 `manual / session_ended / checklist_passed` 三种固定模式。
* 当前 `checklist_passed` 的检查逻辑实质上是“Task 状态 + evidence presence”的硬编码判定，不是 workflow 内部声明式定义的检查器。
* 当前 builtin workflow 已经表达了 Start / Implement / Check / Record 的业务意图，但“约束、检查、推进”三件事没有统一建模。

## Assumptions (temporary)

* 用户希望 workflow 成为 agent runtime 的正式治理层，而不是 prompt 模板层。
* 用户更偏向“正确模型优先”，接受对现有 workflow schema 做结构性重构。
* 这轮主要产出设计方向与重构方案，不要求立即完成所有代码迁移。

## Open Questions

* 第一版 `constraints` 是否也先限制为内置类型组合，而不开放任意脚本化表达？

## Requirements (evolving)

* workflow 必须同时定义：
  * phase 内 agent 约束
  * phase 内上下文装配
  * phase 完成判据
  * phase 失败/阻塞后的处理策略
* workflow 还必须支持“不分 phase 的单阶段行为工作流”，用于定义标准化 agent 行为与产出格式，并能在 session 运行中动态注入。
* `Lifecycle` 应作为独立实体存在，用于编排 agent 生命周期、选择与切换 workflow，而不应继续塞进 `WorkflowDefinition` 顶层。
* workflow 约束必须能在 agent 全生命周期边界持续生效，而不是只在 prompt 开头出现一次。
* “检查逻辑”必须成为正式模型的一部分，而不是散落在 hook runtime if/else 里。
* workflow 的静态声明层与 runtime 执行层必须分离，但要能清晰映射。
* 第一版检查逻辑采用“内置 evaluator + 预留插件扩展点”，不直接引入通用 DSL。

## Acceptance Criteria (evolving)

* [ ] 能清楚指出当前 workflow 模型缺失了哪些一级概念
* [ ] 能给出一套更完整的 workflow 目标模型
* [ ] 能定义 workflow declaration / runtime projection / runtime rule 三层边界
* [ ] 能给出适合当前项目的渐进式重构路径
* [ ] 能明确第一版哪些能力进入 MVP，哪些仅预留扩展点
* [ ] 能解释 lifecycle、phase workflow、动态注入 workflow 三者如何共用一套底层 contract 模型

## Definition of Done (team quality bar)

* 关键设计判断有代码/文档依据
* 方案区分 MVP 与未来扩展
* 需要落档到 spec / task 文档的内容有明确位置

## Out of Scope (explicit)

* 本轮不直接重写全部 workflow schema
* 本轮不直接实现新的 rule DSL
* 本轮不处理数据库兼容/迁移方案

## Technical Notes

* 关键文件：
  * `crates/agentdash-domain/src/workflow/value_objects.rs`
  * `crates/agentdash-application/src/workflow/binding.rs`
  * `crates/agentdash-application/src/workflow/completion.rs`
  * `crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json`
  * `crates/agentdash-api/src/execution_hooks.rs`
  * `.trellis/spec/backend/execution-hook-runtime.md`
* 初步判断：
  * 当前 workflow 更像 “phase metadata + resource bindings + tiny completion enum”
  * 真正 runtime 治理 authority 仍主要在 `execution_hooks.rs` 的 rule registry
  * 需要把“检查是什么”“约束如何作用”“完成如何判定”提升为 workflow 一级概念

## Technical Approach

workflow 后续重构采用三层模型：

* `Workflow Declaration`
  * 负责声明 phase 的目标、上下文、约束、检查逻辑、转移策略
  * 它是产品/配置层，不直接承担执行逻辑
* `Workflow Projection`
  * 把声明层解析成当前 active phase 的运行时 contract
  * 负责生成 resolved context、resolved constraints、check state、transition state
* `Workflow Runtime`
  * 在 `SessionStart / UserPromptSubmit / BeforeTool / AfterTool / AfterTurn / BeforeStop / SessionTerminal / Subagent` 等边界消费 projection
  * 执行 allow/deny/rewrite/ask、stop gate、pending action、phase advance 等具体决策

第一版不做任意脚本化 rule DSL，而是采用“内置 evaluator + 预留插件扩展”的模式：

* 内置 evaluator：
  * `task_status_in`
  * `artifact_exists`
  * `artifact_count_gte`
  * `session_terminal_in`
  * `pending_action_none`
  * `subagent_result_status_in`
* 预留扩展点：
  * `custom_evaluator_key`
  * `custom_constraint_key`
  * 第一版只保留字段，不开放任意动态脚本

## Decision (ADR-lite)

**Context**

当前 workflow 定义能表达 phase 名称、少量注入文本和粗粒度 completion mode，但无法真正表达 phase 内治理逻辑。真正影响 agent 行为的 authority 仍在 hook runtime 的实现里，导致“workflow 看似在定义流程，实际执行语义却散落在 runtime if/else 中”。

**Decision**

workflow 未来重构为“声明层 + 投影层 + 运行时层”的三层模型，并把 phase 内的 `constraints` 与 `checks` 提升为一等概念。检查逻辑第一版采用“内置 evaluator + 预留插件扩展点”，不直接上通用 DSL。

**Consequences**

* 好处：
  * workflow 真正成为 agent 全生命周期治理层
  * hook runtime 改为消费 contract，而不是继续发明业务语义
  * checklist / completion / stop gate / subagent adoption 可以落到同一模型中
* 代价：
  * 需要重构 `WorkflowPhaseDefinition` 及其 projection
  * 需要把一部分 `execution_hooks.rs` 中的硬编码语义上提为 workflow contract
  * 第一版会存在“旧 schema + 新 schema”短期并存的过渡阶段

## Schema Direction

推荐改成两个独立实体：

* `WorkflowDefinition`
  * 表示一个独立的 agent 行为 contract 单元
  * 可以被单独 attach，也可以被 lifecycle step 引用
* `LifecycleDefinition`
  * 表示 agent 生命周期编排器
  * 负责按顺序和规则为 agent 指派 workflow

也就是说：

* phase 不再是 workflow 内部必须自带的结构
* lifecycle step 可以视作“对一个 workflow 的一次编排引用”
* 不分 phase 的单阶段标准化行为，也只是普通 workflow，只是没有被 lifecycle 串起来

二者共享一套底层 contract 原语：

* `goal`
* `context`
* `constraints`
* `checks`
* `outputs`
