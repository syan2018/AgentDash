# 架构 review 机械化重构收口

## Goal

把 `06-21-module-topology-coupling-review` 中已经具备稳定执行边界、可以零散推进的机械性重构项拆成可追踪 work items。该任务用于承接 contract 化、generated DTO 消费收口、残留入口删除、命名清理、诊断/测试补齐和 UI 文案修正等低设计分歧工作。

## Context

来源父任务：

- `.trellis/tasks/06-21-module-topology-coupling-review/research/coupling-matrix.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/research/followup-backlog.md`

本任务只收纳“执行前不需要重新决定事实源或控制面 owner”的工作。需要先讨论模型边界、事实源归属、控制面语义的内容留在父任务的 `design-coupling-tracker.md`。

## Requirements

- 每个 work item 必须有明确文件范围、来源 research、验收方向和建议验证命令。
- 允许逐项独立执行、独立检查、独立提交。
- 执行时不设计兼容层，不保留旧 DTO/旧入口并行路径。
- Contract 类 item 完成后必须运行 `pnpm run contracts:check`，并按影响范围运行前端/后端检查。
- 删除/封装类 item 必须先通过 `rg` 确认产品路径消费面，再移除或收窄公开 surface。
- UI 文案/类型拆分类 item 不应改变业务事实源，只改变消费归属和呈现语义。

## Out Of Scope

- 不处理 RuntimeSessionExecutionAnchor selection policy、AgentFrame exposure model、PermissionGrant runtime fact、Extension backend target resolver、Relay command target taxonomy 等需要设计决策的问题。
- 不修改长期 `.trellis/spec/`，除非某个机械 item 完成后发现稳定工程约定需要沉淀，并由单独 spec update 执行。
- 不把所有 backlog 项一次性实现；本任务是机械重构池，实际执行可以分批。

## Acceptance Criteria

- [ ] `work-items/` 中列出所有机械性重构项，并标明来源、范围、验收和验证命令。
- [ ] 每个 item 可以被单独派发给 `trellis-implement` 或人工执行。
- [ ] 设计层面耦合问题没有混入机械执行池。
- [ ] 执行任一 item 前能从 `implement.md` 找到建议顺序与检查方式。

