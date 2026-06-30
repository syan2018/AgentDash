# 架构二三档收敛跟踪

## Goal

把本轮架构 review 中二三档候选收敛为一个单一 Trellis task，不创建子任务；通过组内工作项跟踪可并行推进的局部架构修复，优先降低跨层 contract drift、测试 locality 差和 Agent-facing tool adapter 过浅的问题。

本任务不承接第一档两个大改造：`WorkspaceModuleAgentSurface` 与 `AgentRunWorkspaceControlPlane`。这两个方向需要单独讨论 interface 和切分策略。

## Background

- 本轮 `$improve-codebase-architecture` review 已生成整合报告：`C:\Users\yihao.liao\AppData\Local\Temp\architecture-review-20260630-123422.html`。
- 二三档候选的共同特征是：外部产品语义相对稳定，第一刀可以通过内部 module/interface 收口获得收益，不需要先改数据库或重塑核心运行协议。
- 当前项目处于预研阶段，不需要保留兼容性分支；实现时应把 contract、DTO、测试和调用方调整到正确形态。

## Requirements

- R1. 单一任务跟踪：所有二三档工作项保留在本 task 内部清单中，不创建 child task。
- R2. Contract generation 收口：将 TypeScript contract 生成器从 CLI-only seam 推向可测试的 generation module，至少覆盖 dedup/import/header/common type 这类生成规则的局部测试面。
- R3. Runtime snapshot contract 收口：优先处理 backend runtime summary 的 route-local DTO 与前端手写 mirror；desktop local runtime snapshot 作为后续同组工作项，先明确 wire DTO 与 diagnostics view 的边界。
- R4. Task tool 局部深模块：先拆 `ExecutionContext -> TaskPlanScope` 的 scope resolver，再评估 `TaskPlanWorkspace` 的 `read/apply` interface，使 AgentTool adapter 变薄。
- R5. NDJSON validator 探索：评估是否从 contract generation 派生 runtime validator 或 validation metadata，减少前端 stream parser 对 generated union 的手写重述。
- R6. 质量门/route shim 收口：把 quality gate 与 route shim service tests 作为低优先级跟踪项，优先补可组合 gate manifest 或 feature command/query interface 的设计入口。
- R7. 并行修复友好：每组工作项必须有清晰文件范围、验证命令和回滚点，便于多 agent 并行但不互相踩文件。

## Acceptance Criteria

- [ ] PRD 明确二三档范围、第一档 out of scope、工作组和验收口径。
- [ ] `implement.md` 列出不建子任务的组内工作项，并标出建议顺序、可并行项、验证命令和风险文件。
- [ ] Contract generation 第一刀完成后，存在可运行的局部测试或等价验证，能够在不依赖整棵 generated tree diff 的情况下定位生成规则错误。
- [ ] Backend runtime summary 收口完成后，browser-facing DTO 来源进入 generated contract 或有明确设计记录说明为何暂缓；前端不再维护同字段手写 mirror。
- [ ] Task tool 第一刀完成后，scope resolution 与 AgentTool JSON adapter 分离，相关测试能直接命中 typed scope/use case interface。
- [ ] 若推进 NDJSON validator、quality gate 或 route shim 工作项，必须先在本任务内补充对应 work group 的具体 acceptance，再进入实现。
- [ ] 任何阶段不得修改或回滚并行会话已有工作区改动；验证失败若来自无关未提交修改，只记录影响，不为本任务清理。

## Out Of Scope

- `WorkspaceModuleAgentSurface` 深模块设计与实现。
- `AgentRunWorkspaceControlPlane` / `SessionChatView` 大规模 interface 收口。
- 数据库 schema 重塑，除非某个已批准工作项实现中发现必须迁移。
- 为兼容旧 DTO 或旧前端手写类型保留双路径。

## Open Questions

- 是否先把本任务启动为“快速修复集合”，还是先分别讨论第一档两个大任务后再回来执行二三档？推荐：先保持本任务 planning，等两个大任务方向讨论完，再按优先级启动本任务的第一组 work item。
