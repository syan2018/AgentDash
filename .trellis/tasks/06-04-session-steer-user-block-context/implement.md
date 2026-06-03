# 实施计划：Steer 用户输入上下文

## Phase 0：开始前检查

- 先提交 planning commit；不要 stage 现有 `.trellis/config.yaml` 变更。
- 确认用户已审阅本任务规划并同意进入实现。
- 进入实现时运行 `python ./.trellis/scripts/task.py start 06-04-session-steer-user-block-context`。

## Phase 1：协议与生成

- 在 `agentdash-agent-protocol` 中新增 `UserInputSubmittedNotification` 与 `UserInputSubmissionKind`。
- 让 `BackboneEvent` 新增 `UserInputSubmitted` 变体，payload 使用 Codex app-server `UserInput`。
- 更新 Backbone TS 生成并运行 contract drift check。
- 在 compat 层支持 `UserInputSubmitted` 与 ACP session notification 的互转，保留 AgentDash meta 中的 submission kind。

## Phase 2：后端事件写入与投影

- 把 launch commit 的普通 prompt 用户输入从 `user_message_chunk` 收敛到 `UserInputSubmitted(submission_kind=Prompt)`。
- 在 `LifecycleAgentSteeringService` 中，steer control 成功后持久化 `UserInputSubmitted(submission_kind=Steer)`。
- 确保 event trace 使用 active turn id，并为每次用户输入生成稳定 item id。
- 更新 `continuation` / `ContextProjector` 原始 transcript 重建逻辑，消费 `UserInputSubmitted` 作为 user role。
- 删除或停止新写入 `user_message_chunk`，避免新协议和旧 key 双写。

## Phase 3：前端流式聚合与展示

- 更新 generated Backbone types 后，调整 `useSessionStream` 的 entry id 与 reducer 分支。
- 更新 `useSessionFeed` hard boundary 判断，把 `user_input_submitted` 作为用户可见产出。
- 更新 `SessionEntry`：普通 prompt 显示为 user message，steer 显示为带 Steer 标记的 user message。
- 统一前端内容解析，必要时增加 `UserInput -> renderable content` helper，不再依赖 `session_meta_update.value` 解析用户块。
- 修正 Agent 页运行中输入栏 gating，让可 steer 状态来自后端 control surface，而不是误用 dispatch 连接缺失文案。

## Phase 4：验证

- Rust：
  - `cargo check -p agentdash-agent-protocol`
  - `cargo check -p agentdash-application`
  - 针对 session continuation / hub / steering service 的相关测试
- Frontend：
  - `pnpm run contracts:check`
  - `pnpm --filter app-web run typecheck`
  - 针对 session stream/feed/entry 的相关测试
- Browser：
  - 重启 `pnpm dev`。
  - 在 Agent 页打开一个运行中 session，提交 steer。
  - 验证提交后立即出现带 Steer 标记的用户输入块。
  - 验证输入栏在 running + steerable 时可继续发送，不再显示错误的 dispatch 文案。

## Risk Points

- `user_message_chunk` 当前被 projection、frontend feed、branching tests 多处消费；替换时必须一次性更新所有事实入口。
- Codex `UserInput` 与 ACP `ContentBlock` 形态不同；转换 helper 需要放在共享边界，避免 connector、projection、frontend 各写一套。
- 事件写入顺序必须代表已接收事实；不要在 connector 拒绝之前持久化 steer 输入。

## Review Gate

进入实现前确认：

- 是否接受 `BackboneEvent::UserInputSubmitted` 作为协议级一等事件。
- 是否接受本任务不做旧事件兼容迁移。
