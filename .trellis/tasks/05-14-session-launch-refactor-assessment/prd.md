# Session 构建与 Launch 唯一数据流

## Goal

完成 session 及外围 owner / task / workflow / routine / companion / local relay / prompt 拉起流程的系统性重构。

终态只有一条生产主链路：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

这次重构的原始动机是明确 session 构建边界和 launch 信息边界，用唯一数据流收口 owner、workspace、VFS、MCP、capability、context、identity、restore、hook、pending command、terminal effects，删除多入口半成品 request、隐式 fallback、重复 context 组装和有职责 `SessionHub` facade。

## Core Decisions

- 外部 review 只作为目标态输入；本任务的权威目标是 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution`。
- `LaunchCommand` 只表达来源意图，不承载 construction / launch 产物。
- `SessionConstructionPlan` 是 session 构建事实源，供 launch、context endpoint、audit、inspector 共同投影。
- `LaunchExecution` 是一次 launch 的执行计划，承载 prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect 与 connector input。
- `ExecutionContext` 只是 connector SPI 投影。
- `Turn` 保持薄边界，只负责 reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。
- `PromptSessionRequest`、`PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput` 不作为最终边界保留。
- `PromptAugmentInput` 不得作为生产主链路 handoff、planner 输入或增强后输出保留。
- `SessionHub` 最终不保留有职责 facade；最终代码中不能承载业务判断。
- owner 解析使用单一 `ResolvedSessionOwner` / `SessionOwnerResolver`。
- terminal event 先持久化，effect 进入 durable outbox。
- pending runtime command 使用 domain event + derived projection，不藏在 `SessionMeta`。

## 当前项目状态

当前分支已经完成一部分迁移基础：

- 主要生产入口已进入 `LaunchCommand`。
- `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput` 已删除。
- `SessionConstructionPlan`、`SessionConstructionPlanner`、`LaunchExecution`、`SessionLaunchPlanner`、`SessionLaunchExecutor` 已存在。
- context endpoint 大部分 route-local 重建已迁出。
- runtime registry、turn supervisor、terminal effect outbox、runtime command store 已有基础实现。
- working_dir、ready gate、旧 pending meta 字段已有阶段性收口。

仍未满足目标态：

- `LaunchCommand` 已不再持有 `PromptAugmentInput`，`to_augment_input()` 已删除。
- `PromptAugmentInput`、`SessionLaunchRequest` 与 `SessionConstructionFacts` 已从生产代码删除。
- `SessionLaunchPlanner` 已消费 `LaunchCommand + SessionConstructionPlan + runtime facts`；后续重点是让 context/query/audit/inspector 与 launch 同源，并收缩 pipeline 职责。
- `SessionConstructionPlan` 已保留完整 context bundle 与 continuation context frame，但 audit / inspector projection 仍不完整，launch/query/audit/inspector 尚未完全同源。
- `SessionHub` 仍承载业务方法和服务定位职责。
- terminal effects、pending runtime command、persistence store 还缺最终验证矩阵。

## Requirements

- 所有来源只构造 `LaunchCommand`。
- `LaunchCommand` 不持有增强后 payload 或 construction / launch 产物。
- `SessionConstructionPlan` 必须包含 owner、source contract、workspace、typed working dir、executor profile、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projections、resolution trace。
- context endpoint、audit、inspector 只能投影 `SessionConstructionPlan`。
- `LaunchExecution` 必须包含 resolved prompt payload、construction、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input、launch trace。
- connector input 在 connector 边界投影为 `ExecutionContext`。
- runtime 不临时解析 owner / VFS / MCP / capability / context。
- terminal effect 必须通过 durable outbox 执行，具备 idempotency、retry、dead-letter。
- pending runtime command 必须具备 requested / applied / failed event 和可重建 projection。
- persistence 语义边界必须拆清：meta、event、projection、outbox、runtime-command projection。
- API route 只保留 auth、DTO 转换、调用 use case。

## Acceptance Criteria

- [ ] `LaunchCommand -> SessionConstructionPlan -> LaunchExecution` 是唯一生产主链路。
- [x] `PromptAugmentInput` 不再作为生产主链路 handoff、planner 输入或增强后输出。
- [x] `SessionLaunchRequest` 过渡 envelope 不再作为生产主链路 handoff。
- [ ] `SessionConstructionPlan` 是 launch/query/audit/inspector 的唯一事实源。
- [ ] `LaunchExecution` 是唯一 per-launch 策略计划。
- [ ] `prompt_pipeline` 不再承担 construction/launch planner 职责。
- [ ] `SessionHub` 不再是业务能力入口。
- [ ] terminal effects 全部 durable replay/retry/dead-letter。
- [ ] pending runtime command apply-once 与失败恢复可审计。
- [ ] 最终验证矩阵通过。
- [ ] `.trellis/tasks/05-14-session-launch-refactor-assessment/docs/final-convergence-execution-tracker.md` 中所有完成定义为通过。

## Out Of Scope

- 不做兼容旧内部 API 的双主线。
- 不把 connector `ExecutionContext` 升级为 application 事实源。
- 不新增只转发旧 payload 的 service wrapper。
- 不为了保留历史结构而拆分无业务边界的中间 DTO。
