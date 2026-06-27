# 收束 Session 前端事件流与 AgentRun 列表事实源

## Goal

修复 Session 前端事件流与 AgentRun 列表在运行中出现的顺序错乱、进度卡状态回退、列表插入位置不稳定等问题。核心目标是明确每条 UI 展示链路的唯一事实源，避免前端在 durable session event、ephemeral progress event、AgentRun run activity 与 agent shell activity 之间用多个时间坐标互相重排。

用户明确补充：tool burst 聚合本身没有问题；新出现的 tool 不需要等完成后才进入 tool burst，可以默认直接进入 tool burst。进行中 tool 的进度仍需要可见，但不应因为“active tool 单独 lane”导致完成前后列表形态大幅跳动。

用户进一步确认：AgentRun workspace 引发的列表刷新应统一走事件驱动失效/刷新，不保留轮询兜底。轮询会让正路失效机制缺乏压力，长期形成“有兜底所以不修事件”的坏路径。

本任务先沉淀排查结论与修复计划，不在创建任务时直接开始实现。

## Confirmed Facts

- Session NDJSON 主链路为 `streamTransport.ts -> useSessionStream.ts -> sessionStreamReducer.ts -> useSessionFeed.ts -> SessionEntry.tsx / SessionChatView.tsx`。
- 后端 `SessionEventingService` 将 `agent_message_delta`、`reasoning_*_delta`、`command_output_delta`、`file_change_delta`、`mcp_tool_call_progress`、`item_updated` 归类为 ephemeral progress event，不写入 durable session event log。
- ephemeral event 的 `event_seq` 字段承载 per-session `ephemeral_seq`，语义上不是 durable `event_seq`。
- 前端 reducer 当前先处理 incoming batch 内的全部 ephemeral events，再处理 durable events。
- 前端 display 层仍把 `SessionDisplayEntry.eventSeq` 当作统一排序值使用，`mergeThinkingIntoDisplayItems` 会按 `displayItemSeq` 重排 display item。
- `item_started` / `item_updated` 在前端 reducer 中按同一 `item:{item_id}` 更新条目；如果较新的 ephemeral `item_updated` 先应用，较旧 durable `item_started` 后应用，UI 状态可能被回写为旧 tool 状态。
- 后端 `/projects/{project_id}/agent-runs` 列表分页排序使用 `LifecycleRun.last_activity_at`。
- AgentRun list entry 的 `shell.last_activity_at` 当前来自 `LifecycleAgent.updated_at`。
- 前端 `AgentRunShortcutList` 又按 `entry.shell.last_activity_at` 本地重排，所以列表存在 run-level 排序与 agent-level 展示排序不一致。
- AgentRun workspace 详情页在 turn end、session meta、mailbox、capability 等事件上刷新 workspace projection；左侧 shortcut list 依赖 30 秒轮询，完整 AgentRun 列表首屏加载后不会随详情页事件自动失效。
- 现有前端 session reducer/feed 测试通过，但未覆盖 durable + ephemeral 同批交错、同 item 回写、mixed seq display reorder、AgentRun list 排序事实源不一致等场景。

## Requirements

- Session timeline UI 必须有单一、可解释的展示顺序事实源；durable `event_seq` 与 ephemeral `ephemeral_seq` 不得作为同一个排序轴直接比较。
- Ephemeral progress event 只能表达 in-flight UI 进度，不得覆盖更权威的 durable lifecycle fact。
- 同一 tool item 的 `item_started`、`item_updated`、progress delta 与 `item_completed` 必须按 item identity 合并，并保持单一 UI entry / group identity。
- 新出现的 tool 默认直接进入 tool burst 聚合；tool burst 可以包含 in-progress tool。进行中状态、审批状态、输出裁切等重要状态仍需在 burst 内可见。
- Tool burst 聚合不能跨越用户输入、agent message、可见错误、approval、context_frame 等 hard boundary。
- AgentRun 列表必须确定唯一排序事实源。列表 API、列表 entry shell、前端 shortcut、完整列表页应消费同一个 activity timestamp 语义。
- AgentRun workspace 详情页发生用户命令提交、turn 结束、mailbox 状态变化、session meta 更新时，相关 AgentRun list projection 必须通过事件驱动失效或刷新，避免详情页与侧栏列表不一致。
- AgentRun list projection 不保留固定轮询兜底；刷新正确性应由事件 contract、store invalidation 与显式写后刷新承担。
- 修复应补齐测试，覆盖这次排查出的具体竞态和排序错位，而不是只验证 happy path。
- 项目处于预研阶段，不需要兼容旧字段语义；修复应让 contract 回到最正确状态。如涉及 contract 或数据库字段调整，按项目规范处理生成与 migration。

## Non-Goals

- 不移除 tool burst 功能。
- 不回退到旧的 session history store 或 session-first AgentRun 归属模型。
- 不通过前端猜测 title、status、AgentRun ownership 或业务执行状态来修复列表；业务事实仍以后端 projection 为准。
- 不把 ephemeral progress event 改成 durable event 来掩盖排序问题；是否持久化进度态应由后端事件合同决定。
- 不用固定周期轮询作为 AgentRun list projection 的一致性兜底。

## Acceptance Criteria

- [ ] 新 tool 出现时默认被纳入 tool burst，即使该 tool 仍处于 `inProgress`。
- [ ] Tool burst 中的 in-progress tool 可以展示运行中状态、审批等待、输出/进度摘要，并在完成后保持稳定位置与 identity。
- [ ] durable `item_started` 不会在同批或后续批次中覆盖同一 item 更晚的 ephemeral `item_updated` UI 状态。
- [ ] ephemeral text/progress event 不会凭 `ephemeral_seq` 插入到 durable timeline 的错误位置。
- [ ] `mergeThinkingIntoDisplayItems` 不再把 durable event 与 ephemeral event 放到同一个 numeric seq 轴上重排。
- [ ] AgentRun project list API 返回的 `shell.last_activity_at` 与服务端分页排序使用同一事实源。
- [ ] `AgentRunShortcutList` 与 `ActiveAgentRunList` 不再用与后端分页不同的时间源重排同一批列表。
- [ ] 在 AgentRun workspace 页面提交命令、turn end、mailbox 更新或 session meta 更新后，侧栏/列表 projection 通过事件驱动在可感知时间内刷新或失效。
- [ ] `AgentRunShortcutList` / `ActiveAgentRunList` 不再依赖固定周期轮询维持正确性。
- [ ] 新增 reducer/feed 测试覆盖 durable + ephemeral 混合批次、同 item started/updated 回写、in-progress tool burst。
- [ ] 新增 AgentRun list projection 测试覆盖排序字段与 shell 字段一致。
- [ ] 运行相关前端测试与必要的后端/API 测试；如果改 contract，运行 `pnpm run contracts:check`。

## Evidence And References

- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.test.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.test.ts`
- `packages/app-web/src/components/layout/AgentRunShortcutList.tsx`
- `packages/app-web/src/features/agent/active-agent-run-list.tsx`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `crates/agentdash-application-runtime-session/src/session/eventing.rs`
- `crates/agentdash-application-runtime-session/src/session/runtime_registry.rs`
- `crates/agentdash-api/src/routes/sessions.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs`

## Open Questions

- 是否将 AgentRun list projection 抽成前端全局 store，还是先在现有侧栏/完整列表组件内通过 lightweight invalidation 解决。推荐：抽成一个小的 Project-scoped AgentRun list projection store，因为唯一事实源需要一个明确缓存边界。
