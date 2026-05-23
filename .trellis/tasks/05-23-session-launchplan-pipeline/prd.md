# Session LaunchPlan 阶段化

## Goal

把 session 拉起链路从长流程和多入口隐式组装收敛为可审计的 `LaunchPlan`/阶段化执行结构，统一 HTTP、Task、Workflow、Routine、Companion、Local relay 等启动入口的核心语义。

## Source Material

- `docs/reviews/2026-05-16-zip-static-review/session-launch-refactor-plan.md`
- `docs/reviews/2026-05-23-architecture-review-round/architecture-review-synthesis.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`

## Requirements

- 定义不可变或阶段结果化的 launch plan，使 construction、preparation、connector launch、commit、event ingestion 边界清晰。
- 明确 connector accepted、turn_started、bootstrap commit、title generation、runtime command 状态更新之间的顺序。
- 统一主要启动入口进入同一 launch service/facade；旧入口可保留为薄适配层。
- 保持 Backbone event 事实层和现有 persistence 语义稳定，除非设计文档明确列出迁移。
- 为 owner bootstrap、connector failure、并发 prompt、turn terminal、local relay prompt 补测试矩阵。

## Acceptance Criteria

- [ ] 有 `LaunchCommand -> LaunchPlanner -> LaunchPlan -> LaunchExecutor/TurnSupervisor` 或等价阶段设计。
- [ ] `PromptSessionRequest` 不再作为跨阶段持续 mutation 的半成品计划对象，或其职责被明确收窄。
- [ ] 至少 HTTP prompt 与 local relay prompt 入口进入统一 launch 阶段。
- [ ] connector failure 不会提前提交 bootstrap 成功或留下 running 状态。
- [ ] 并发 prompt claim/activate/terminal 语义由测试锁住。
- [ ] session startup spec 更新为新的阶段边界。

## Out of Scope

- 不实现 session tree branching。
- 不引入 multi-agent 新语义。
- 不拆完整 `agentdash-application` crate。
