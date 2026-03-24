# 完整补齐 Hook 机制闭环与 Companion Adoption 控制流

## Goal
把当前已经具备基础观测面的 hook runtime，升级成真正的执行层闭环机制，重点补齐 companion / subagent 的控制流、结果回流、capability slice，以及审批与动态上下文的统一可追踪语义。

## Requirements
- `companion_complete` 在父 session 不活跃、runtime 不在内存中的情况下，仍然必须能够稳定触发 `SubagentResult` hook 评估。
- `companion_dispatch` 的 `slice_mode` 不能只裁剪 prompt 注入内容，必须同步约束子 session 的执行层继承能力，至少覆盖 `address_space` / `mcp_servers`。
- `adoption_mode` 不能长期停留在 diagnostics 标注层，后续必须进入主 session 外循环的正式控制语义。
- `approval / ask` 需要在主事件流中具备统一、可审计的语义，而不只是工具卡片状态。
- Hook Runtime 前端同步应逐步从页面轮询过渡到事件驱动失效 / 刷新。
- Hook Runtime 需要暴露待处理 adoption / hook 干预项，确保主 session 与前端都能看到“当前还有什么回流待消费”。
- Hook pending action 不能是一次性 drain 队列，必须具备 `pending / injected / resolved / dismissed` 生命周期，并允许主 session 显式结案。

## Acceptance Criteria
- [ ] `companion_complete` 在父 session 非活跃时，仍能重建 hook runtime 并记录 `SubagentResult` trace / event。
- [ ] `slice_mode` 已同步影响子 session 的执行能力继承，而不只是 prompt slice。
- [ ] companion 生命周期事件、hook trace 与运行时 snapshot 在主 session 中可互相对照。
- [ ] 至少完成一轮真实联调，覆盖 dispatch -> result return -> parent hook trace / event。
- [ ] 本任务结束时，剩余未完成项仅允许是更高阶的 adoption 自动控制流，不再包含基础回流与 capability 边界缺失。
- [ ] `follow_up_required / blocking_review` 已进入 runtime pending action，并能在 loop 边界被消费。
- [ ] 会话页 Hook Runtime 面板由主事件流驱动刷新，不再依赖固定 3 秒轮询。
- [ ] 主 session 处理完 adoption / follow-up 后，能够通过正式 runtime tool 显式调用 `resolve_hook_action` 结案，并把结果写回主事件流与 runtime snapshot。
- [ ] 未结案的 `blocking_review` 会持续阻止自然 stop；结案后 stop gate 解除。

## Scope
- 第一阶段先修两个 blocker：
  - parent session runtime 的可重建回流
  - companion capability slice
- 第二阶段继续补：
  - approval 独立语义事件
  - adoption_mode 进入外循环控制
  - Hook Runtime 前端事件驱动刷新
  - pending action 正式生命周期与 resolve 闭环

## Notes
- 该任务承接 `03-22-formalize-hook-evidence-and-dynamic-context` 的后续收尾，不再重复结构化 evidence 与基础 hook event 主流接入工作。
- 当前阶段优先处理“会导致 hook 机制看似打通、实则在异步 companion 场景下失效”的问题。
