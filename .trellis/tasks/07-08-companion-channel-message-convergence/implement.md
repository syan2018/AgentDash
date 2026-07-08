# Companion 与 Channel message 语义收束实施计划

## Phase 0: 准备

- [ ] 重新读取相关 spec：
  - `.trellis/spec/cross-layer/backbone-protocol.md`
  - `.trellis/spec/backend/session/agentrun-mailbox.md`
  - `.trellis/spec/backend/session/execution-context-frames.md`
  - `.trellis/spec/frontend/type-safety.md`
- [ ] 确认当前工作区已有未提交修改，实施时不得碰无关改动。

## Phase 1: 协议与来源模型

- [ ] 在 `agentdash-agent-protocol` 中为 `UserInputSubmittedNotification` 增加 source/channel provenance 类型。
- [ ] 更新构造器和 re-export。
- [ ] 运行 Backbone TS 生成：

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
```

- [ ] 更新前端 generated 文件，并保证 TS 类型使用 generated source 字段，不手写并行 DTO。

## Phase 2: Launch 主链路

- [ ] 在 `agentdash-application-ports::launch` 增加中立 `LaunchInputSource`。
- [ ] 为 `LaunchCommand` 增加 input source 携带能力。
- [ ] 在 HTTP / lifecycle user / local relay / companion / mailbox delivery 入口设置明确 source。
- [ ] 修改 `TurnPreparer`：
  - Companion 不生成 `system_delivery` context frame。
  - Companion 不替换 `resolved_payload.prompt_payload`。
  - 真正 system/control delivery 继续走 context/system projection。
- [ ] 修改 `TurnCommitter`：
  - Companion 发 `UserInputSubmitted`。
  - source 写入 `UserInputSubmittedNotification`。
  - 命名从 human-only 改为 user-role input。

## Phase 3: AgentRun Mailbox Scheduler

- [ ] 扩展 `RuntimeSessionEventingPort::emit_user_input_submitted` 和 `SessionEventingService`，传入 source/channel provenance。
- [ ] 修改 `AgentRunMailboxService`：
  - Companion launch delivery emit source-aware `UserInputSubmitted`。
  - Companion steer delivery emit source-aware `UserInputSubmitted`。
  - system/hook/workflow 控制投影保留 `system_message`，但命名避免把 Companion 混入。
- [ ] 更新 mailbox scheduler tests，覆盖 launch 与 steer 两条路径。

## Phase 4: Transcript 与 Projection

- [ ] 补 `transcript_restore` 测试，确认 Companion source 的 `UserInputSubmitted` 恢复为 `AgentMessage::User`。
- [ ] 检查 `context_projector` / branching / fork prefix 中手工构造 `UserInputSubmittedNotification` 的调用点，补 source。
- [ ] 检查 AgentRun journal projection 不依赖 `system_message.kind=companion_delivery`。

## Phase 5: Frontend

- [ ] 更新 `packages/app-web/src/features/session/model/types.ts` 中的 source view helper。
- [ ] 更新 `SessionEntry.tsx` / `SessionMessageCard.tsx`，支持 Companion/channel 输入样式。
- [ ] 更新 `useSessionFeed.ts` 相关测试，确认 Companion input 仍是 hard boundary，但不是普通用户气泡。
- [ ] 更新 `systemEventPolicy.ts` 测试，确认不再依赖 companion delivery system event。

## Phase 6: Spec 与验证

- [ ] 更新 `.trellis/spec/cross-layer/backbone-protocol.md`。
- [ ] 更新 `.trellis/spec/backend/session/agentrun-mailbox.md` 中 non-user/system projection 旧结论。
- [ ] 更新 `.trellis/spec/backend/session/execution-context-frames.md` 中 system context frame 边界。
- [ ] 运行建议验证：

```powershell
cargo test -p agentdash-agent-protocol user_input
cargo test -p agentdash-application-runtime-session launch
cargo test -p agentdash-application-agentrun mailbox
pnpm --filter app-web test -- sessionStreamReducer useSessionFeed SessionMessageCard
```

- [ ] 根据实际改动补充 `cargo check` / `pnpm typecheck`。

## Risk Notes

- `UserInputSubmittedNotification::new` 调用点较多，协议字段变更会触发一批编译错误，这是可接受的正确迁移路径。
- `SessionEventingService::emit_user_input_submitted` 是核心接口，实施时应优先让编译器暴露所有缺 source 的调用点。
- Mailbox scheduler 和 runtime-session commit 都有 system projection 分支，必须一起改，否则 Companion 仍会在某些运行状态下落入 system timeline。
- 前端 feed 当前把所有 `user_input_submitted` 渲染为普通用户消息，必须新增 source-aware presentation，避免把 Companion 输入画成用户本人说的话。
