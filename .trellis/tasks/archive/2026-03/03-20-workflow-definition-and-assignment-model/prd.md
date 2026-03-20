# Workflow 定义与分配模型

## Goal

为 AgentDash 补齐正式的 `WorkflowDefinition / WorkflowAssignment` 领域模型与接口边界，使 workflow 不再只是 prompt 约定或 task 目录隐式结构，而是可被 Project 收纳、分发、选择和复用的正式平台资产。

## Background

当前项目已经具备：

- Project / Story / Task / SessionBinding 的正式对象
- `session_composition`、`context_containers`、`mount_policy` 等上下文配置
- Project Agent / Story Session / Task Session 三层会话模型

但还缺少：

- WorkflowDefinition
- WorkflowAssignment
- Workflow 与 Agent Role 的关系
- Workflow 的版本化与启用边界

如果没有这层模型，后续 `Trellis Dev Workflow` 黄金路径只能继续靠约定拼接，无法成为正式平台能力。

## Requirements

- 定义 `WorkflowDefinition` 的最小字段集合。
- 定义 `WorkflowAssignment` 的最小字段集合。
- 明确 Workflow 与 Project、Story、Task、Session、Agent Role 的关系边界。
- 明确 Workflow 如何表达 phase、context rule、record policy、archive policy。
- 明确第一版是配置型定义、文档型定义还是持久化实体优先。
- 明确版本化、启用、停用、替换的最小语义。
- 明确第一版 API / repository / DTO 边界，避免后续实现时对象漂移。
- 明确与现有 `session_composition`、`context_containers` 的关系，避免重复建模。

## Acceptance Criteria

- [ ] 明确 `WorkflowDefinition` 最小字段模型。
- [ ] 明确 `WorkflowAssignment` 最小字段模型。
- [ ] 明确 Project 与 Workflow 的绑定边界。
- [ ] 明确 Agent Role 与 Workflow 的绑定边界。
- [ ] 明确 workflow phase / context / record 的表达方式。
- [ ] 明确第一版持久化与 API 边界建议。

## Out of Scope

- 不直接实现具体 workflow run
- 不直接实现 phase runtime
- 不直接实现前端管理界面
- 不直接实现 automation control plane

## Related Files

- `.trellis/tasks/03-20-symphony-flow-long-term-tracking/execution-roadmap.md`
- `.trellis/tasks/03-20-symphony-flow-long-term-tracking/trellis-dev-workflow-golden-path.md`
- `crates/agentdash-domain/src/project/value_objects.rs`
- `crates/agentdash-domain/src/session_binding/entity.rs`
- `crates/agentdash-domain/src/story/value_objects.rs`
- `crates/agentdash-domain/src/task/value_objects.rs`
