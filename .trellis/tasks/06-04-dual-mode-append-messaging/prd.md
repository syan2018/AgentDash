# [child-4] 双模追加消息（排队 / Steer）

Parent: [06-04-session-composer-redesign-image-input](../06-04-session-composer-redesign-image-input/prd.md)
依赖：child-1（统一 `Vec<UserInputBlock>` 入参）；与 child-3（composer UI / 选择器只读态 / 发送按钮形态）协同。
研究：parent [research/send-steer-protocol-and-dual-mode-gap.md](../06-04-session-composer-redesign-image-input/research/send-steer-protocol-and-dual-mode-gap.md)。
参考 UI：[assets/reference-ui.md](assets/reference-ui.md)。

## Goal

运行中追加消息支持双模（参考 codex/@references）：
- **排队（pending）**：等当前 turn 完成后自动作为下一轮接续。
- **Steer**：立即注入运行中 turn。

## 架构决策：pending 走「服务端托管完整状态」（已评估）

**结论：pending 必须是服务端托管的完整状态，不是前端内存编排。** 评估：

- **一致性**：本系统 UI 全是服务端真值的投影（runtime-control / execution state / 事件流）。前端内存队列是架构异类——刷新即丢、多端不一致、trace 无记录、与 `turn_completed` 时序竞态。符合 [[feedback_converge_full_chain]]：要彻底、服务端为真值。
- **turn 生命周期归属**：turn 的 started/completed 由服务端（turn_supervisor / execution anchor / 事件）拥有。"turn 完成后自动发下一轮"是 turn 生命周期反应，应在服务端发生，而非前端竞速。
- **Codex 印证**：Codex 协议只有 turn/start、turn/steer、turn/interrupt，**没有** server queue-next-turn 原语——排队是这些原语**之上的编排层**。在我们这层（lifecycle/session 已托管 turn）落地，正是该编排层的家。

因此 child-4 = **后端（pending 队列领域状态 + API + 服务端自动派发 + 事件）+ 前端（投影 UI）**。规模较大，design 阶段可再判断是否自拆子任务（后端 pending 状态 / 前端投影 UI）。

## Requirements

### 后端
- BR1 pending 领域状态：为 runtime session 引入「待发送用户消息队列」持久化实体（有序、可多条），内容用统一 `Vec<UserInputBlock>`（child-1）。复用/对齐既有 pending 概念前先核查（如 pending-action context frame / `PendingCapabilityStateTransition`，见 design）。
- BR2 服务端自动派发：监听当前 turn `turn_completed`（失败/中断的处理在 design 定），自动取队首作为**新 turn**（走 message 路径，可携带其 `executor_config`）发送；派发/出队/失败重试有明确状态机。
- BR3 命令 API：enqueue（排队）/ list / delete / **promote-to-steer（"引导"）** / **dequeue-for-edit（"编辑消息"：取消该排队项并把内容返回前端供编辑）**；steer 复用现有 steering 接口（child-1 后入参已统一）。（「关闭排队」本期不做。）
- BR4 事件投影：pending 队列变更经事件/`runtime-control` 暴露，供前端与 trace 投影；排队消息可被多端一致观察。
- BR5 action 模型扩展：`runtime-control` 在 running 态同时暴露「排队」与「steer」可用性 + 当前 pending 列表，不再用 `delivery_running` 把 send_next/steer 互斥为单一动作。

### 前端
- FR1 键盘分流（用户最终确认）：**Enter = 排队**、**Ctrl+Enter = 强制发送（立即/steer）**、**Shift+Enter = 换行**。idle 态 Enter/Ctrl+Enter 均直接发送；running 态 Enter=排队、Ctrl+Enter=立即 steer。开放点：@ 选择器打开时 Enter 优先级。
- FR2 发送按钮形态（与 child-3 协同）：单上箭头发送按钮，**无麦克风**；running+输入空 → ■ 停止（cancel/interrupt）；running+有新输入 → ↑ 发送（默认排队）；idle → ↑ 发送。
- FR3 已排队消息行（参考截图，**投影服务端状态**）：消息预览 + 「↪ 引导」(promote-to-steer) + 「🗑 删除」 + 「⋯」→「✏️ 编辑消息」（= 后端出队 + 内容回填 composer 供编辑，用户改完再发/重排）；多条可拖拽排序；排队同时下方 composer 仍可输入。（「关闭排队」本期不做。）
- FR4 capability 退化：执行器不支持 steer（`actions.steer.enabled=false`）时，Ctrl+Enter/「引导」不可用，仅排队。
- FR5 steer 态：模型/推理选择器只读（child-3 协同）；排队态可改（走新 turn 的 executor_config）。

## Acceptance Criteria

- [ ] pending 为服务端持久化状态：刷新/换端后排队消息仍在；trace/事件可见。
- [ ] running 态：Enter 排队（出现已排队行）、Ctrl+Enter 立即 steer、Shift+Enter 换行；idle 态 Enter/Ctrl+Enter 直接发送。
- [ ] 发送按钮形态：idle=↑；running+空=■停止；running+有输入=↑（默认排队）。无麦克风。
- [ ] turn 完成后服务端自动派发队首为新 turn；多条按序；失败有可见状态不静默丢。
- [ ] 已排队行可「引导」(转 steer) / 「编辑消息」(后端出队 + 回填 composer) / 「🗑 删除」；多条可拖拽排序。
- [ ] 不支持 steer 的执行器：仅排队；steer 态选择器只读、排队态可改。
- [ ] **Steer 与排队均端到端完整**：发起/可视状态/capability 门控/失败回退（消息不丢）/互转，非半成品。
- [ ] 后端相关 crate `cargo test` + 前端 `pnpm -F app-web lint/typecheck/test` 通过；pending 状态机、自动派发、命令、键盘/按钮、投影均有测试。

## 范围外

- 语音/麦克风输入。
- `push_session_notification`（合并进当前轮）这条"软注入"语义本 child 不强制采用（与 queue-as-next-turn 区分；如 design 判定有用可纳入）。

## Notes

- action 可用性现产于 [routes/sessions.rs](../../../crates/agentdash-api/src/routes/sessions.rs)；前端控制态在 [SessionPage.tsx](../../../packages/app-web/src/pages/SessionPage.tsx) `chatControlState`；键盘在 [SessionChatView.tsx](../../../packages/app-web/src/features/session/ui/SessionChatView.tsx) `handleKeyDown`。
- design 阶段：核查既有 pending-action / turn_supervisor 可复用面；定 pending 状态机（enqueue→dispatch→done/failed）、turn_failed/interrupted 时的队列处理、并发与顺序语义。
