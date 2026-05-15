# Session Refactor Batch 7 Final Convergence

## Goal

完成父任务剩余代码收口；归档文档、重命名旧结构或保留半成品边界都不满足本 batch。

本 batch 的目标是把生产主链路推进到：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

并删除当前仍在遮蔽目标的 payload/facade/隐式行为。

## Current Status

已完成：

- working_dir 路径策略已收紧。
- 旧 pending meta JSON 字段主线已清理。
- persistence store adapter 边界已拆出。
- AppState ready gate 已阶段性收窄。
- `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput` 已删除。
- `SessionConstructionPlanner`、`SessionLaunchPlanner`、`SessionLaunchExecutor` 已存在。
- terminal effect outbox 支持 replay/retry/dead-letter。

未完成：

- `LaunchCommand` 已不再持有 `PromptAugmentInput`，local relay 也不再携带已组装 `Vfs`，`to_augment_input()` 已删除。
- task `post_turn_handler` 与 companion parent snapshot 已迁出 command；当前 task effect binding 与 companion parent capability 临时投影仍在 API bootstrap，后续必须进入 construction/effects 边界。
- `PromptAugmentInput` 已删除，不再承载 construction / launch 产物。
- API bootstrap 仍返回 `SessionLaunchRequest` 过渡 envelope。
- `SessionLaunchPlanner` 已不再消费旧 payload；`prompt_pipeline` 仍接收过渡 envelope 并拆字段。
- `SessionConstructionPlan` 已保留完整 context bundle，但还不是完整 context frame / audit / inspector 事实源。
- `SessionHub` 仍承载业务方法。
- effects / pending / persistence 还缺最终验证。

## Requirements

- 删除 `PromptAugmentInput` 在生产主链路中的 handoff/planner 输入/增强后输出职责。
- 补全 `SessionConstructionPlan` 字段，使 launch/query/audit/inspector 同源。
- 将 `SessionLaunchPlanner` 输入改为 `LaunchCommand + SessionConstructionPlan + runtime facts`。
- `prompt_pipeline` 只执行计划，不做 construction/launch fallback。
- 拆除有职责 `SessionHub`。
- 补齐 terminal effects、pending runtime command、persistence store 的最终验证。
- 更新父任务 tracker/spec，只记录真实代码状态。

## Acceptance Criteria

- [x] `PromptAugmentInput` 不再出现在生产主链路 handoff 中。
- [ ] `SessionLaunchRequest` 过渡 envelope 被删除，字段进入 construction / launch / effects 目标边界。
- [ ] `LaunchCommand` 是纯入口意图。
- [ ] `SessionConstructionPlan` 是 launch/query/audit/inspector 的事实源。
- [ ] `LaunchExecution` 是唯一 per-launch 策略计划。
- [ ] `prompt_pipeline` 不再读取 request/meta/profile 做策略 fallback。
- [ ] `SessionHub` 不再承载业务判断。
- [ ] terminal effect handlers 可 durable replay。
- [ ] pending runtime command apply-once 与 failure recovery 有验证。
- [ ] 最终验证矩阵通过。
- [ ] 提交历史按业务边界整理。
