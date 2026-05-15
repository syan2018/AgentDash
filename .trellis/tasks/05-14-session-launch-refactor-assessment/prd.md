# Session 构建与 Launch 唯一数据流

## Goal

基于 `docs/reviews/AgentDash_session_refactor_plan.md` 和当前工作区现状，完成 session 及外围 owner / task / workflow / routine / companion / local relay / prompt 拉起流程的系统性架构规划。

目标不是拆出一个更大的 `LaunchPlan`，而是定义清楚：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

这条唯一数据流必须收口所有入口逻辑，删除多入口半成品 request、隐式 fallback、重复 context 组装和终态副作用内存回调。它只保留稳定数据边界，不把 resolver/planner/projector 这类实现函数固化为传递中间层。

## Core Decisions

- 参考 review 的目标继续保留，但其中的 `LaunchPlan` 被拆解为更明确的数据边界。
- `SessionConstructionPlan` 是 session 构建事实源，供 launch、context endpoint、audit、inspector 共同投影。
- `LaunchExecution` 是一次 launch 执行计划，承载 prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect 与 connector input。
- `ExecutionContext` 只是 connector SPI 投影；不强制保留独立 `ExecutionPlan` 中间层。
- `Turn` 保持薄边界，只负责 reservation、active、cancel、hook runtime、processor/adapter supervision、terminal release。
- `PromptSessionRequest` 从生产主链路删除。
- `SessionHub` 最终不保留有职责 facade；迁移期 wrapper 只能转发，不能承载业务判断。
- owner 解析使用单一 `ResolvedSessionOwner` / `SessionOwnerResolver`。
- terminal event 先持久化，effect 进入 durable outbox。
- pending runtime command 使用 domain event + derived projection，不再藏在 `SessionMeta`。

## Confirmed Current Facts

- `PromptSessionRequest` 仍贯穿 HTTP、Task、Workflow、Routine、Companion、Hook auto-resume、Local relay。
- `SessionLaunchIntent` 只表达 source / strictness / preparation / follow-up，不承载 session 构建边界。
- `start_prompt_with_follow_up` 仍混合 payload 解析、turn claim、meta 写入、pending transition 消费、VFS/MCP/capability fallback、hook/restore 判断、ExecutionContext 构造、connector 调用、processor 启动。
- context endpoint 与 launch owner 选择优先级不一致。
- project/story/task context endpoint 仍在 route 层重建 VFS / capability / context。
- `pending_capability_state_transitions` 仍在 `SessionMeta` 中作为 hidden queue。
- `SessionTurnProcessor` 仍直接执行 hook effects、post-turn handler、terminal callback、hook auto-resume。
- `working_dir` 当前仍允许绝对路径与 `..`。

## Requirements

- 所有来源只构造 `LaunchCommand`，不构造最终执行上下文。
- `SessionConstructionPlan` 必须包含 owner、source contract、workspace、VFS、typed working dir、executor profile、MCP、capability、context、identity、query/audit projections、resolution trace。
- context endpoint、audit、inspector 只能投影 `SessionConstructionPlan`。
- `LaunchExecution` 必须包含 lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input 和 launch trace。
- connector input 在 connector 边界投影为 `ExecutionContext`。
- runtime 不临时解析 owner / VFS / MCP / capability / context。
- terminal effect 必须通过 durable outbox 执行，具备 idempotency、retry、dead-letter。
- pending runtime command 必须具备 requested / applied / failed event 和可重建 projection。
- persistence 语义边界必须拆清：meta、event、projection、outbox、runtime-command projection。
- API route 只保留 auth、DTO 转换、调用 use case。

## Acceptance Criteria

- [ ] `docs/session-construction-launch-dataflow.md` 明确唯一数据流和字段归属规则。
- [ ] `docs/review.md` 明确参考 review 与当前目标的对齐和背离。
- [ ] `docs/target-architecture.md` 给出 construction / launch / execution / runtime / effects / pending 的目标架构。
- [ ] `design.md` 给出核心类型和边界不变量。
- [ ] `implement.md` 明确可分批执行、验证、提交的完整方案，以及是否可正式开始。
- [ ] `docs/current-to-target-migration.md` 明确当前链路每个环节的背离点、迁移动作和退出条件。
- [ ] `docs/closure-checklist.md` 给出最终收口不变量。
- [ ] 文档中不保留已放弃方案的过程性比较。

## Out of Scope

- 不在此 planning task 中修改 Rust 生产代码。
- 不把此 parent planning task 作为单体巨型实现任务直接 `task.py start`；正式实现应创建 batch child task 后再 start。
- 不在单个后续实现任务里完成全部 session 重构。
