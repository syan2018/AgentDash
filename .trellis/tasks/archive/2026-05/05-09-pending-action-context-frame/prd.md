# PendingAction ContextFrame 投递与可视化收束

## Goal

将 `HookPendingAction` 从当前 bespoke Markdown 注入路径迁移为独立 `ContextFrame(kind="pending_action")`。目标是让用户能看到每一条 pending action 如何影响 Agent 行为、Agent 实际收到的处理要求是什么、该 action 当前是否 pending/resolved，以及后续 adoption/dismissal 如何改变上下文，而不是把这类强 steering 文本藏在 `hook_delegate.rs` 的临时 user message builder 里。

## Background

`context-frame-consolidation` 已经完成了几条 Agent-visible 信息入口的收束：

- 启动期动态上下文：`bootstrap_context`、`workspace_surface`、`skill_surface`、`hook_runtime_surface`。
- 系统自动续跑：`auto_resume`。
- 历史压缩摘要：`compaction_summary`。
- runtime capability / workflow / tool delta：`context_frame` 主线。

剩余最突出的 bespoke 路径是 `HookPendingAction`：

- `HookRuntimeDelegate::transform_context(...)` 在 turn-start 边界调用 `collect_pending_actions_for_injection()`。
- `build_pending_action_message(...)` 直接拼 Markdown user message。
- action 内容会影响 Agent 是否继续处理、是否执行 follow-up、是否吸收 companion review，但前端没有一张同源的 `ContextFrameCard` 可审计。
- pending action 的来源可能是 companion result、hook rule、blocking review 或 follow-up required，生命周期又带 `pending/resolved`、`adopted/dismissed` 等状态，这比普通 `hook_injection` 更强，应该作为一等 frame subtype。

## Review Findings

### P1: pending action 文本仍由 bespoke Markdown builder 生成

相关位置：

- `crates/agentdash-application/src/session/hook_delegate.rs`
  - `collect_turn_start_injection_messages(...)`
  - `build_pending_action_message(...)`
  - `pending_action_status_label(...)`
- `crates/agentdash-application/src/session/hook_messages.rs`
- `crates/agentdash-spi/src/hooks/mod.rs`
  - `HookPendingAction`
  - `HookPendingActionStatus`
  - `HookPendingActionResolutionKind`

这条路径没有 typed metadata -> sections/rendered_text 的 frame builder，容易再次出现“Agent 收到 B，前端只能猜 A”的漂移。

### P1: pending action 缺少持久化为 `context_frame` 的明确边界

当前 `HookTurnStartNotice` 的 runtime notice 可以携带 `context_frame`，但 pending action 是在 `transform_context(UserPromptSubmit)` 内部从 runtime 队列消费后直接构造 `AgentMessage::user(...)`。这个 delegate 层没有现成的 `SessionHub::emit_context_frame(...)` 持久化入口，因此需要设计一个干净边界：

- 在 action 入队时持久化 frame。
- 或在 action 被 turn-start 消费时持久化 frame。
- 或让 pending action 队列存储 frame envelope，并由 hub/runtime sink 负责写事件。

### P1: follow-up 与 steering 的 delivery 语义不同，但当前 UI 不可见

`collect_turn_start_injection_messages(...)` 会根据 `action.is_follow_up()` 把消息放进 `follow_up` 或 `steering`。这会改变 AgentLoop 消费路径，但前端目前看不到同一 pending action 是 follow-up prompt 还是 turn-start steering。

`pending_action` frame 至少应表达：

- `delivery_channel`: `turn_start` / `follow_up`
- `message_role`
- `action_id`
- `action_type`
- `status`
- `source`
- `turn_id`
- `resolution_kind`
- `resolution_note`
- `injections`
- `owners`
- `instruction`

### P2: action resolution 也应成为可追踪状态变化

`resolve_pending_action(...)` 会将 pending action 标记为 resolved，并记录 adoption/dismissal 信息。后续可以考虑生成：

- `pending_action` frame 的 resolved snapshot。
- 或 `pending_action_resolution` section/event。

MVP 可以先保证“Agent 被 pending action steer 时可见”，resolution 可作为第二批。

## Requirements

- 新增 `ContextFrameSection::PendingAction`，不要复用 `SystemNotice` 或 `HookInjection` 混淆语义。
- 新增 application 层 `pending_action_context_frame` builder，由 `HookPendingAction` typed metadata 生成 sections 与 `rendered_text`。
- `rendered_text` 必须与 Agent 实际收到的 pending action/follow-up 文本同源。
- `build_pending_action_message(...)` 不再手写独立 Markdown；它应消费 pending action frame 的 `rendered_text`，或被 frame builder 取代。
- pending action frame 必须持久化为 `SessionMetaUpdate { key: "context_frame" }`。
- 前端新增 `pending_action` section 渲染：
  - 默认摘要展示 action title/type/status/source/delivery。
  - 展开展示 action id、turn id、injections、owners、instruction。
  - 底部仍可展开 `rendered_text`。
- 普通 `hook_trace context_injected` 不能冒充 pending action UI。

## Open Questions

- 持久化时机应选哪一个：入队时、消费时，还是队列直接持有 frame envelope？
- resolved/adopted/dismissed 是否纳入本任务 MVP，还是只记录 pending action 被注入给 Agent 的时刻？

## Proposed Approach

推荐方向：**消费时生成并持久化 frame，队列保留业务 action**。

原因：

- pending action 是否进入 `steering` 还是 `follow_up` 是消费时才最终确定的 delivery 事实。
- action 队列保持纯业务状态，避免在 runtime 内存队列里缓存过期 frame 文本。
- frame builder 可以接受 `HookPendingAction + SessionHookSnapshot + HookSessionRuntimeSnapshot + delivery_channel`，一次生成前端 sections 与 Agent `rendered_text`。

需要解决的问题：

- `HookRuntimeDelegate` 当前没有直接持久化 session event 的能力。
- 可以考虑扩展 `RuntimeHookInjectionSink` 或新增 `RuntimeContextFrameSink`，由 `SessionHub` 注入，负责在 delegate 消费 pending action 时异步写 `context_frame`。
- 如果不希望 delegate 持久化，可在 `collect_pending_actions_for_injection()` 返回时将 frame 放入 `HookTurnStartNotice` 一类的统一队列，但要避免重复消费。

## Acceptance Criteria

- [ ] pending action 注入给 Agent 时，事件流中存在对应 `context_frame`。
- [ ] `ContextFrame.rendered_text` 与 Agent message 中实际文本一致。
- [ ] pending action section 包含 action id/type/status/source/turn_id/delivery/injections/instruction。
- [ ] follow-up pending action 与 steering pending action 在 frame 中有不同 `delivery_channel`。
- [ ] 前端能渲染 `pending_action` section，并展示完整 Agent 可见文本。
- [ ] `hook_trace context_injected` 不再是用户理解 pending action 的入口。
- [ ] `build_pending_action_message(...)` 不再保留独立手写 Markdown 作为第二套事实源。

## Test Plan

### Backend

- pending action frame builder 单测：sections 与 `rendered_text` 同源。
- steering pending action 消费单测：Agent message 与 frame `rendered_text` 一致。
- follow-up pending action 消费单测：`delivery_channel` 标记为 `follow_up`。
- persistence 单测：pending action 消费后存在 `context_frame` event。
- resolution 单测（若纳入 MVP）：adopted/dismissed 信息进入 frame 或明确不进入本批。

### Frontend

- parser 支持 `pending_action` section。
- `ContextFrameCard` 渲染 pending action 摘要、细节与 `rendered_text`。
- legacy hook trace 仍不冒充 pending action UI。

## Out of Scope

- 不重新设计 `HookPendingAction` 的业务状态机。
- 不改变 companion review / follow-up 的业务语义。
- 不把所有 hook trace 都 frame 化；本任务只处理 pending action 这个 Agent-visible 强 steering 路径。
- 不做旧字段兼容；项目预研期，允许硬切。

## Technical Notes

相关代码入口：

- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/session/hook_messages.rs`
- `crates/agentdash-application/src/session/hook_runtime.rs`
- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-spi/src/hooks/mod.rs`
- `frontend/src/features/session/model/contextFrame.ts`
- `frontend/src/features/session/ui/ContextFrameCard.tsx`

相关规格：

- `.trellis/spec/backend/hooks/execution-hook-runtime.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/tasks/05-09-context-frame-consolidation/prd.md`
