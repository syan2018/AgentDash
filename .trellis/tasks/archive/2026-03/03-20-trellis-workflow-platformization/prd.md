# Trellis Workflow 平台化映射

## Goal

把当前已经真实存在于 `.trellis/` 目录、task 目录、jsonl context、journal / archive 机制中的 Trellis 工作流要素，系统性映射为 AgentDash 平台内的 workflow 对象、phase 规则和输入输出契约。

这个任务的目标不是立即实现运行时代码，而是先把第一条真实 workflow 的“平台对象映射”做完整，作为后续建模和实现的输入。

## Background

当前项目里已经存在大量 workflow 雏形：

- `.trellis/workflow.md` 约定了阶段化流程
- task 目录承载需求、上下文和状态
- `implement/check/debug.jsonl` 已承担 phase-specific context 选择职责
- journal / archive 已承担 workflow 输出沉淀职责
- AgentDash 本身已经具备 Session / Address Space / Context Builder / Project-Story-Task 三层模型

但这些能力还没有被明确收束成正式平台对象。

## Requirements

- 梳理 Trellis 当前 workflow 的核心阶段，并形成明确 phase 列表。
- 明确每个 phase 的输入、输出、上下文来源和完成条件。
- 梳理 `.trellis/tasks/*` 目录内的 `prd.md / task.json / *.jsonl` 在平台中的对象映射。
- 梳理 journal / archive 在平台中的产物类型和责任边界。
- 明确哪些现有 AgentDash 能力可以直接复用，哪些需要新增对象或接口。
- 产出一版“现有 Trellis 元素 -> 平台对象 / phase / artifact”的映射表。
- 明确本任务不直接实现什么，避免与后续建模 task 或黄金路径 task 范围重叠。

## Acceptance Criteria

- [ ] 明确 `Trellis Dev Workflow` 的 phase 列表。
- [ ] 明确每个 phase 的输入 / 输出 / 完成条件。
- [ ] 明确 task 目录结构在平台中的对象映射。
- [ ] 明确 record / archive 的平台化责任。
- [ ] 明确可直接复用的既有代码能力清单。
- [ ] 形成可供 `workflow-definition-and-assignment-model` 直接消费的映射结果。

## Out of Scope

- 不直接实现 WorkflowDefinition 领域对象
- 不直接实现 WorkflowRun 持久化
- 不直接改造前后端 runtime
- 不直接做长期自动化控制面

## Related Files

- `.trellis/workflow.md`
- `.trellis/tasks/03-19-symphony-case-workflow-scaffold-closure/implementation-strategy.md`
- `.trellis/tasks/03-20-symphony-flow-long-term-tracking/execution-roadmap.md`
- `crates/agentdash-application/src/session_plan.rs`
- `crates/agentdash-domain/src/session_binding/entity.rs`
- `frontend/src/pages/SessionPage.tsx`
