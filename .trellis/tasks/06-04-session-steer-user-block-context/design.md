# 设计：Steer 用户输入协议事实

## Architecture Boundary

本任务把运行中 steer 定义为“同一 active turn 内的一次用户输入提交”。控制请求仍是 lifecycle runtime control：前端调用 lifecycle agent steering API，应用层校验 runtime anchor、agent/frame/run、active turn 和 connector steering capability，然后投递到 session control。

用户输入事实不属于 lifecycle 控制面，也不属于前端乐观态。它属于 session Backbone Protocol，并在后端接收成功后持久化为 session event。

## Protocol Shape

新增协议级用户输入事件，建议命名为：

```rust
BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification)
```

通知字段建议：

- `thread_id: String`
- `turn_id: String`
- `item_id: String`
- `submission_kind: UserInputSubmissionKind`
- `content: Vec<codex_app_server_protocol::UserInput>`

`UserInputSubmissionKind` 至少包含：

- `Prompt`：turn 起始用户 prompt
- `Steer`：运行中 steer 输入

这个 shape 对齐 Codex app-server 的 `ThreadItem::UserMessage { id, content: Vec<UserInput> }`，但增加 AgentDash 需要的来源标注。它比 `Platform(SessionMetaUpdate key="user_message_chunk")` 更正确，因为字段由协议类型表达，前端、后端、relay 和 projection 不再依赖字符串 key 猜语义。

## Data Flow

```text
SessionChatView submit
  -> lifecycle steering API
  -> LifecycleAgentSteeringService
  -> inspect active turn + supports_session_steering
  -> session_control.steer_session(expected active turn)
  -> persist BackboneEvent::UserInputSubmitted(submission_kind=Steer)
  -> NDJSON session stream
  -> useSessionStream / useSessionFeed
  -> SessionEntry renders user block with Steer marker
  -> ContextProjector consumes same event as user role
```

普通 prompt 的 launch commit 也应改为写入 `UserInputSubmitted(submission_kind=Prompt)`。这样 prompt 与 steer 使用同一 transcript 入口，差异只存在于 `submission_kind`。

## Ordering

steer 事件应在 connector 接受成功后持久化。原因是 session event 表达后端已接收的事实，而不是浏览器尝试发送的草稿。若 connector 返回 no active turn / expected turn mismatch / non-steerable，API 返回错误，前端输入保留或按现有错误提示处理，不写 transcript。

事件落库位置应尽量靠近 application service。`LifecycleAgentSteeringService` 已经拥有 anchor、active turn 和 prompt blocks，上游 API 不应重复理解 session event 语义。

## Content Conversion

浏览器 API 当前发送 ACP `ContentBlock`。协议事件和 Codex connector 需要 Codex app-server `UserInput`。实现应引入共享转换 helper：

```text
ContentBlock -> UserInput
UserInput -> model-visible ContentPart / rendered text
```

转换应覆盖当前已支持的 text / image / resource 输入；不支持的块在应用层显式报错，避免静默丢上下文。

## Frontend Rendering

前端不再把 `session_meta_update.key === "user_message_chunk"` 当作用户输入事实。`useSessionStream` 应按 `user_input_submitted` 构造稳定 entry id：

```text
user-input:{turn_id}:{item_id}
```

`useSessionFeed` 将 `user_input_submitted` 视为 hard boundary。`SessionEntry` 根据 `submission_kind` 渲染普通 user prompt 或带 Steer 标记的 user block。资源、图片、文本仍复用现有 `ContentBlockCard` / `SessionMessageCard` 的视觉语言，但输入内容来源应来自协议 `UserInput`。

## Runtime Control State

Agent 页输入栏在 running 状态下的可发送性应由以下事实共同决定：

- session execution state 是 running，且有 active turn id；
- lifecycle runtime anchor 能反查当前 agent/frame/run；
- connector 支持 `supports_session_steering`；
- 当前 session 已连接 runtime control channel。

“当前 Session 未连接到 Agent dispatch”不能作为已运行 session 的 steer 阻塞文案。该文案只适用于无法定位 runtime control surface 的真实断链状态。

## Relay And Local Runtime

relay command 继续承载 steer control payload；session event 事实由接收侧 application 写入 Backbone。远端或本机 runtime 不应各自发明用户消息 meta key。跨端转发如果需要 ACP 兼容输出，应在 `compat` 层由 `UserInputSubmitted` 转换成标准 ACP update，并在 AgentDash meta 中保留 submission kind。

## Trade-offs

最小改法是在 steer 成功后追加 `user_message_chunk`。这能修 UI 刷新，但会把 prompt 与 steer 混成同一个字符串 meta key，projection、relay 和前端都无法从协议类型上知道输入来源。协议级事件更改范围更大，但它把“用户输入事实”提升为 Backbone 一等语义，符合 AgentDash 作为 Codex app-server protocol 扩展的定位。

## Spec Updates

实现完成后应同步更新：

- `.trellis/spec/cross-layer/backbone-protocol.md`：记录 `UserInputSubmitted` 的协议定位和 `submission_kind`。
- `.trellis/spec/frontend/hook-guidelines.md`：更新 feed hard boundary 表。
- `.trellis/spec/backend/session/context-compaction-projection.md`：说明用户输入 projection 消费协议事件。
