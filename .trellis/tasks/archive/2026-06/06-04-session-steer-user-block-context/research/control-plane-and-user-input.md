# Research: control-plane-and-user-input

- Query: session steer 用户输入上下文；user_message_chunk 写入/读取路径；browser API -> workflow -> session_control -> relay/local -> connector 控制面；旧路径替换点；优先测试点
- Scope: mixed
- Date: 2026-06-04

## Findings

### Files found

- `.trellis/tasks/06-04-session-steer-user-block-context/prd.md` - 任务目标要求把运行中 steer 作为协议级用户输入事实，并用 Codex app-server protocol 收敛 session 控制面。
- `.trellis/tasks/06-04-session-steer-user-block-context/design.md` - 设计建议新增 `BackboneEvent::UserInputSubmitted`、`submission_kind` 和 Codex-aligned control command。
- `.trellis/tasks/06-04-session-steer-user-block-context/implement.md` - 实施计划明确替换 `user_message_chunk`、收敛 relay/local/control payload、更新前端 stream/feed/entry。
- `.trellis/spec/cross-layer/backbone-protocol.md` - BackboneEvent 是持久化、NDJSON 和前端消费的统一协议；PlatformEvent 只补 Codex 未覆盖能力。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - 浏览器消费 DTO 必须由 Rust contract / protocol 生成，前端不手写长期 wire shape。
- `.trellis/spec/backend/session/streaming-protocol.md` - session NDJSON event envelope 暴露 `session_update_type`、`turn_id`、`entry_index` 和 `notification`。
- `.trellis/spec/backend/session/context-compaction-projection.md` - ContextProjector 从 durable `session_events` 构建模型输入，用户输入必须是事实事件。
- `.trellis/spec/frontend/hook-guidelines.md` - 当前 `useSessionFeed` 把 `user_message_chunk` 作为 hard boundary。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs` - Codex `TurnStartParams` / `TurnSteerParams` 使用 `Vec<UserInput>`，`turn/steer` 要求 `expected_turn_id`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs` - Codex request method 声明包含 `turn/start`、`turn/steer`、`turn/interrupt`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs` - Codex 依赖显式 turn boundary 表达同 turn 内 prompt 与 mid-turn steer。
- `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs` - Codex 服务端校验 `expected_turn_id` 非空、active turn 和 steerable turn。
- `crates/agentdash-agent-protocol/src/compat/mod.rs` - ACP `SessionUpdate::UserMessageChunk` 兼容入口当前转换为 Platform `user_message_chunk`。
- `crates/agentdash-application/src/session/hub_support.rs` - 普通 prompt 当前写入 Platform `SessionMetaUpdate(key="user_message_chunk")`。
- `crates/agentdash-application/src/session/launch/commit.rs` - launch accepted 后持久化 user message envelopes，再写 TurnStarted。
- `crates/agentdash-application/src/session/continuation.rs` - raw transcript / projection 当前读取 `user_message_chunk`。
- `crates/agentdash-application/src/workflow/lifecycle/journey/session_items.rs` - lifecycle VFS/session item projection 当前读取 `user_message_chunk`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - 浏览器 steer endpoint 入口。
- `crates/agentdash-application/src/workflow/agent_steering.rs` - workflow steering service 校验 anchor、agent/frame、active turn 和 steering capability。
- `crates/agentdash-application/src/session/control.rs` - session control 当前只把 `Vec<ContentBlock>` 转交 connector。
- `crates/agentdash-relay/src/protocol/prompt.rs` - relay `command.steer` 当前 payload 是 `session_id + prompt_blocks`。
- `crates/agentdash-application-ports/src/backend_transport.rs` - cloud -> backend relay steer request 当前也是 `session_id + prompt_blocks`。
- `crates/agentdash-local/src/handlers/prompt.rs` - 本机 relay `handle_steer` 解析 prompt blocks 并调用 local session control。
- `crates/agentdash-application/src/relay_connector.rs` - relay connector 把 `ContentBlock` 序列化成 JSON 发送 `RelaySteerRequest`。
- `crates/agentdash-executor/src/connectors/codex_bridge.rs` - Codex connector 最终发送 `turn/steer`，但 `expected_turn_id` 来自 connector 内部 live active turn。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` - Pi connector 把 prompt blocks 降成文本并塞入 `runtime.agent.steer`。
- `packages/app-web/src/services/lifecycle.ts` - 前端 service 调用 `/steering-messages` endpoint。
- `packages/app-web/src/pages/SessionPage.tsx` - 前端输入栏 primary action 和 steer 提交逻辑。
- `packages/app-web/src/features/session/model/useSessionStream.ts` - 前端 stream 当前把 `user_message_chunk` 构造成 user entry 并累积文本。
- `packages/app-web/src/features/session/model/useSessionFeed.ts` - 前端 feed 当前把 `user_message_chunk` 视为 hard boundary。
- `packages/app-web/src/features/session/ui/SessionEntry.tsx` - 前端 UI 当前把 `user_message_chunk` 渲染为 user message。

### Current user_message_chunk write/read paths

1. ACP compatibility write path:
   - `crates/agentdash-agent-protocol/src/compat/mod.rs:389` 将 `SessionUpdate::UserMessageChunk(chunk)` 转为 `BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key: "user_message_chunk", value })`。
   - 这条路径是 ACP passthrough 兼容入口，不携带 `submission_kind`、Codex `UserInput` 或 item id。

2. Normal prompt launch write path:
   - `crates/agentdash-application/src/session/launch/commit.rs:113` 调用 `build_user_message_envelopes(...)`。
   - `crates/agentdash-application/src/session/hub_support.rs:14` 遍历 `user_blocks`。
   - `crates/agentdash-application/src/session/hub_support.rs:26` 写 `PlatformEvent::SessionMetaUpdate`。
   - `crates/agentdash-application/src/session/hub_support.rs:27` 使用 key `"user_message_chunk"`。
   - `crates/agentdash-application/src/session/hub_support.rs:33` trace 只设置 `turn_id` 和 `entry_index`。
   - `crates/agentdash-application/src/session/launch/commit.rs:119` 逐条 `persist_notification`。
   - `crates/agentdash-application/src/session/launch/commit.rs:127` 之后才写 `TurnStarted`。这与 Codex thread history 的 explicit turn boundary 语义需要实现时重新确认。

3. Persistence/index behavior:
   - `crates/agentdash-infrastructure/src/persistence/session_core.rs:736` 将 event type 映射成 `session_update_type`。
   - `crates/agentdash-infrastructure/src/persistence/session_core.rs:756` 当前所有 Platform event 都索引为 `"platform"`，所以 `user_message_chunk` 不是一等 session update type。
   - 新增 Backbone event 时需要给 `backbone_event_type_name` 增加 `"user_input_submitted"`，否则 NDJSON 便利字段仍无法表达用户输入事实。

4. Projection / transcript read path:
   - `crates/agentdash-application/src/session/continuation.rs:247` 匹配 `PlatformEvent::SessionMetaUpdate`。
   - `crates/agentdash-application/src/session/continuation.rs:248` 只在 key 为 `"user_message_chunk"` 时投影用户消息。
   - `crates/agentdash-application/src/session/continuation.rs:250` 将 value 反序列化为 `ContentBlock`。
   - `crates/agentdash-application/src/session/continuation.rs:251` 转为模型可见 message part。
   - `crates/agentdash-application/src/session/continuation.rs:252` 用 `restored_user_key(event)` 合并，来源是 event turn/index 而不是协议 item id。

5. Lifecycle session item / recall read path:
   - `crates/agentdash-application/src/workflow/lifecycle/journey/session_items.rs:154` 匹配 Platform session meta。
   - `crates/agentdash-application/src/workflow/lifecycle/journey/session_items.rs:157` key 为 `"user_message_chunk"` 时生成 `user_message` projection。
   - `crates/agentdash-application/src/workflow/lifecycle/journey/session_items.rs:164` raw blocks 直接保存旧 value。

6. Branching tests / test helper path:
   - `crates/agentdash-application/src/session/branching.rs:799` 测试 helper `user_message(...)` 构造 Platform `user_message_chunk`。
   - `crates/agentdash-application/src/session/branching.rs:825` 等测试通过旧事件验证 fork/rollback 基线。
   - `crates/agentdash-application/src/session/hub/tests.rs:2432`、`:2921` 也直接构造或查找旧 key。

7. Frontend stream read path:
   - `packages/app-web/src/features/session/model/useSessionStream.ts:145` 识别 Platform event。
   - `packages/app-web/src/features/session/model/useSessionStream.ts:147` 判断 `session_meta_update.key === "user_message_chunk"`。
   - `packages/app-web/src/features/session/model/useSessionStream.ts:149` entry id 为 `user:{turnId}:{entryIndex}`。
   - `packages/app-web/src/features/session/model/useSessionStream.ts:314` 将旧 key 作为用户消息累积入口。
   - `packages/app-web/src/features/session/model/useSessionStream.ts:317` value 解析为 `ContentBlock`。
   - 这不能表达同 turn 内多次 steer 的稳定 item id，也不能表达 `submission_kind`。

8. Frontend feed / UI read path:
   - `packages/app-web/src/features/session/model/useSessionFeed.ts:108` 定义 `isUserMessageChunk`。
   - `packages/app-web/src/features/session/model/useSessionFeed.ts:161` 把旧 key 当作 hard boundary。
   - `packages/app-web/src/features/session/ui/SessionEntry.tsx:10` 注释说明 `user_message_chunk -> SessionMessageCard(user)`。
   - `packages/app-web/src/features/session/ui/SessionEntry.tsx:161` 运行时渲染仍以旧 key 判断 user message。

9. E2E read path:
   - `tests/e2e/story-context-injection.spec.ts:434` 和 `:444` 读取 ACP/session update 的 `"user_message_chunk"`。
   - 新协议替换后这些测试要改成 `user_input_submitted` 或新 Backbone event。

### Current steer control-plane path

1. Browser service and page:
   - `packages/app-web/src/pages/SessionPage.tsx:361` 读取 `runtimeControl.actions`。
   - `packages/app-web/src/pages/SessionPage.tsx:362` 优先选择 `actions.steer.enabled` 作为 primary action。
   - `packages/app-web/src/pages/SessionPage.tsx:464` action 为 `"steer"` 时调用 service。
   - `packages/app-web/src/pages/SessionPage.tsx:465` 调用 `sendLifecycleAgentSteeringMessageByRuntimeSession(sessionId, { prompt_blocks: [{ type: "text", text }] })`。
   - `packages/app-web/src/services/lifecycle.ts:77` POST `/lifecycle-agents/by-runtime-session/{runtimeSessionId}/steering-messages`。

2. Browser API route:
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:32` 注册 `steering-messages` route。
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:133` 进入 `steer_lifecycle_agent_message`。
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:139` 仍校验 `prompt_blocks`。
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:143` 通过 runtime session 解析 `RuntimeSessionExecutionAnchor`。
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:169` 构造 `LifecycleAgentSteeringService`。
   - `crates/agentdash-api/src/routes/lifecycle_agents.rs:178` 下发 `LifecycleAgentSteeringCommand { delivery_runtime_session_id, prompt_blocks }`。

3. Workflow application service:
   - `crates/agentdash-application/src/workflow/agent_steering.rs:13` `LifecycleAgentSteeringCommand` 只有 `delivery_runtime_session_id` 和 `Vec<serde_json::Value> prompt_blocks`。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:70` 解析 execution anchor。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:80` 校验 lifecycle agent。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:96` terminal agent 拒绝。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:111` 获取 frame。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:129` inspect session execution state。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:135` 只有 `Running { turn_id: Some(turn_id) }` 可 steer。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:149` 调用 `supports_session_steering`。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:158` 解析 prompt blocks。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:159` 调用 `session_control.steer_session(session_id, prompt_blocks)`。
   - `crates/agentdash-application/src/workflow/agent_steering.rs:168` 返回的 `active_turn_id` 只在 dispatch response 中出现，没有进入 session_control command。

4. Session control boundary:
   - `crates/agentdash-application/src/session/control.rs:26` `steer_session` 入参仍是 `session_id + Vec<ContentBlock>`。
   - `crates/agentdash-application/src/session/control.rs:31` 直接转发到 connector。
   - `crates/agentdash-spi/src/connector/mod.rs:727` 默认 `supports_session_steering` 是 `supports_steering && has_live_session(session_id)`。
   - `crates/agentdash-spi/src/connector/mod.rs:744` connector trait 的 `steer_session` 仍只接收 `Vec<ContentBlock>`，没有 `expected_turn_id`、`Vec<UserInput>`、submission metadata。

5. Relay/cloud route:
   - `crates/agentdash-application/src/relay_connector.rs:273` relay connector 实现 `steer_session(session_id, Vec<ContentBlock>)`。
   - `crates/agentdash-application/src/relay_connector.rs:287` 把 prompt blocks 序列化为 JSON。
   - `crates/agentdash-application/src/relay_connector.rs:291` 调用 backend transport `relay_steer`。
   - `crates/agentdash-application-ports/src/backend_transport.rs:125` `RelaySteerRequest` 只有 `session_id` 和 `prompt_blocks`。
   - `crates/agentdash-api/src/workspace_resolution.rs:260` 转为 `RelayMessage::CommandSteer`。
   - `crates/agentdash-relay/src/protocol/prompt.rs:60` `CommandSteerPayload` 只有 `session_id` 和 `prompt_blocks`。

6. Local handler route:
   - `crates/agentdash-local/src/handlers/prompt.rs:222` `handle_steer` 接收 `CommandSteerPayload`。
   - `crates/agentdash-local/src/handlers/prompt.rs:238` parse prompt blocks。
   - `crates/agentdash-local/src/handlers/prompt.rs:250` 调用 local `session_runtime.control.steer_session(&payload.session_id, prompt_blocks)`。
   - 远端/本机都没有保留 `expected_turn_id` 或 `UserInput` command shape。

7. Connector bridge route:
   - Composite connector：`crates/agentdash-executor/src/connectors/composite.rs:479` 找到持有 live session 的 child 后转发 `steer_session(session_id, prompt_blocks)`。
   - Codex connector：`crates/agentdash-executor/src/connectors/codex_bridge.rs:989` 实现 `steer_session`。
   - Codex connector：`crates/agentdash-executor/src/connectors/codex_bridge.rs:1011` 从内部 live session 读取 `active_turn_id` 作为 `expected_turn_id`。
   - Codex connector：`crates/agentdash-executor/src/connectors/codex_bridge.rs:1017` 将 `ContentBlock` 转为 Codex `UserInput`，但 helper 只抽取文本。
   - Codex connector：`crates/agentdash-executor/src/connectors/codex_bridge.rs:1018` 发送 `ClientRequest::TurnSteer`。
   - Codex connector：`crates/agentdash-executor/src/connectors/codex_bridge.rs:1020` 构造 `TurnSteerParams { thread_id, input, expected_turn_id }`。
   - Pi connector：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:885` 实现 `steer_session`。
   - Pi connector：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:890` 把 blocks 降成纯文本。
   - Pi connector：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:897` 调用 `runtime.agent.steer(AgentMessage::user(message))`。

8. Runtime control view:
   - `crates/agentdash-api/src/routes/sessions.rs:152` 提供 `get_session_runtime_control`。
   - `crates/agentdash-api/src/routes/sessions.rs:169` 无 anchor 时返回 `UnboundTrace`，并禁用 steer。
   - `crates/agentdash-api/src/routes/sessions.rs:251` running 时调用 `session_control.supports_session_steering`。
   - `crates/agentdash-api/src/routes/sessions.rs:270` running 时返回 `AnchoredRunning`。
   - `crates/agentdash-api/src/routes/sessions.rs:290` `has_frame && !terminal_agent && delivery_running && supports_steering` 才启用 steer。
   - `crates/agentdash-contracts/src/workflow.rs:918` 定义 `SessionRuntimeControlPlaneStatus`。
   - `crates/agentdash-contracts/src/workflow.rs:946` 定义 `SessionRuntimeActionSetView { send_next, steer, cancel }`。

### Codex protocol references

- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:745` method `TurnStart => "turn/start"`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:751` method `TurnSteer => "turn/steer"`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:757` method `TurnInterrupt => "turn/interrupt"`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:49` `TurnStartParams` 包含 `thread_id` 和 `input: Vec<UserInput>`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:137` `TurnSteerParams`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:138` `thread_id`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:139` `input: Vec<UserInput>`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:144` `expected_turn_id` 是 required active turn id precondition。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:242` `UserInput` enum 支持 Text / Image / LocalImage / Skill / Mention。
- `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:729` Codex 拒绝空 `expected_turn_id`。
- `references/codex/codex-rs/app-server/src/request_processors/turn_processor.rs:750` Codex 调用 `thread.steer_input(mapped_items, Some(&params.expected_turn_id), ...)`。
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs:1807` 测试名说明 mid-turn steering 依赖 explicit turn boundaries。
- `references/codex/codex-rs/app-server-protocol/src/protocol/thread_history.rs:1816` 和 `:1823` 在同一 turn boundary 内连续两个 `UserMessage`，分别表示 Start 与 Steer。

### Must delete or replace old incorrect paths

- Replace `BackboneEvent::Platform(SessionMetaUpdate key="user_message_chunk")` as the write format for new prompt and steer user inputs with a protocol-level `BackboneEvent::UserInputSubmitted` carrying `Vec<UserInput>`, `item_id`, `turn_id/thread_id`, and `submission_kind`.
- Replace `hub_support::build_user_message_envelopes` or change it to build the new event. It currently creates one Platform event per `ContentBlock`, which loses item-level user message identity.
- Stop using ACP compat `SessionUpdate::UserMessageChunk -> user_message_chunk` as the main internal representation. Keep any external ACP conversion behind compat only if needed, but do not let it be the AgentDash Backbone fact.
- Replace `SessionControlService::steer_session(session_id, Vec<ContentBlock>)` with a Codex-aligned turn control command that carries `expected_turn_id` and `Vec<UserInput>`.
- Replace `AgentConnector::steer_session(session_id, Vec<ContentBlock>)` with executor-specific delivery of the same command or a narrowed `Steer` command containing `thread/session id`, `expected_turn_id`, `input`, and metadata.
- Replace relay `RelaySteerRequest` and `CommandSteerPayload` shape (`session_id + prompt_blocks`) with the same command shape; otherwise cloud/local will keep a separate steer protocol.
- Replace local `handle_steer` parsing from JSON prompt blocks with deserializing the control command and passing it through unchanged.
- Replace Codex bridge helper `prompt_blocks_to_codex_user_input` with shared conversion used before connector dispatch. Current helper only preserves text and drops image/resource semantics.
- Replace Pi connector `prompt_blocks_to_user_text` as the control-plane boundary. Pi/native may still need text conversion internally, but only after the unified command has been accepted and event fact has preserved `UserInput`.
- Replace projection reads in `continuation.rs` from `user_message_chunk` to new `UserInputSubmitted` event.
- Replace journey/session item reads in `session_items.rs` from `user_message_chunk` to new event.
- Replace frontend `useSessionStream` entry id from `user:{turn_id}:{entry_index}` to `user-input:{turn_id}:{item_id}` for the new event.
- Replace frontend `useSessionFeed` hard boundary predicate from `user_message_chunk` to `user_input_submitted`.
- Replace `SessionEntry` user message rendering source from Platform meta key to typed Backbone event, with `submission_kind=steer` display marker.
- Replace tests that construct or assert `user_message_chunk` (`branching.rs`, `hub/tests.rs`, `story-context-injection.spec.ts`) with new event helpers/assertions.
- Add new event type mapping in `backbone_event_type_name`; leaving it as Platform would keep `session_update_type` unhelpful for stream/event page consumers.

### Suggested priority test points

1. Application steer success persistence:
   - Unit/integration test `LifecycleAgentSteeringService` with running session and active turn.
   - Assert connector accepts command, then one `UserInputSubmitted(submission_kind=Steer)` event is persisted with same active turn id and stable item id.
   - Assert failed steer paths do not persist user input: no active turn, terminal agent, unsupported steering, connector error, expected turn mismatch.

2. Codex connector command shape:
   - Test Codex bridge sends `turn/steer` with the command's `expected_turn_id`, not a silently re-derived or stale connector-only value.
   - Include failure path when command expected turn differs from connector active turn and ensure no user input event is written upstream.

3. Relay/local parity:
   - Test cloud relay connector serializes the unified turn control command.
   - Test `CommandSteerPayload` round-trips through relay protocol with `expected_turn_id`, `input`, and metadata.
   - Test local `handle_steer` passes the same command to session control without reconstructing prompt blocks.

4. Prompt and steer transcript projection:
   - Update `continuation.rs` tests so raw projected transcript includes both initial prompt and mid-turn steer as role user.
   - Verify same turn can contain multiple `UserInputSubmitted` events and order remains event order / item id stable.
   - Verify resume/fork/rollback baseline tests consume new user input event.

5. Frontend stream/feed/render:
   - Test `useSessionStream` builds `user-input:{turn_id}:{item_id}` entries for `user_input_submitted`.
   - Test `useSessionFeed` treats `user_input_submitted` as hard boundary.
   - Test `SessionEntry` renders prompt and steer differently using `submission_kind`, not string meta keys.

6. Runtime control UI:
   - Test running + anchored + supports steering returns `actions.steer.enabled=true` and `SessionPage` primary action is `steer`.
   - Test running + anchored + not steerable displays `actions.steer.unavailable_reason`.
   - Test `UnboundTrace` is the only path showing missing control surface style messaging.

7. Contract drift:
   - Run protocol TS generation/check for new Backbone event.
   - Run workflow/session contract generation/check for any new control DTO.
   - Ensure `pnpm run contracts:check` catches generated TS drift.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task. The user explicitly provided `.trellis/tasks/06-04-session-steer-user-block-context` and the required research output path, so this research used that path instead of guessing.
- I did not modify code, specs, generated files, or git state.
- I did not find any current steer success path that writes a user input fact after connector acceptance. Existing steer path only dispatches to connector and returns dispatch metadata.
- Current Codex bridge already calls `turn/steer`, but expected turn id is derived inside the connector from live state, not carried through the AgentDash browser/workflow/session_control/relay command plane.
- Current ordinary prompt write order persists `user_message_chunk` before `TurnStarted`; Codex thread history reference emphasizes explicit turn boundaries for mid-turn steering. Implementation should verify intended ordering when replacing with `UserInputSubmitted`.
- Resource/image conversion is not fully traced here beyond noting current Codex steer helper only preserves text. Implementation should inspect `ContentBlock` definitions and existing render/conversion helpers before deciding the shared `ContentBlock -> UserInput` conversion surface.
