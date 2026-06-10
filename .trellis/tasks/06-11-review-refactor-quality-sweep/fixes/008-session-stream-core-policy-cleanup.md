# FIX-008: session-stream 核心流与事件策略收敛

## 模块

`session-stream`

## 来源

- `reviews/003-session-stream.md`
- `research/session-stream-executable-plan.md`
- worker Batch 1: `019eb2c1-af82-7ad1-9be2-3b6d02b7bc1e`
- worker Batch 2: `019eb2c1-f9f5-7b73-873d-0b471a522623`

## 更新

- 删除未生效的 `UseSessionStreamOptions.onEntry`。
- 删除本地 `StreamInputEvent` / `toEventEnvelope`，stream 与历史 hydrate reducer 直接消费 generated `SessionEventResponse`。
- 将 stream reducer 纯逻辑拆到 `sessionStreamReducer.ts`，`useSessionStream` 只保留 hydrate、connect、buffer flush 与 hook 状态装配。
- 新增 `sessionPlatformEventDispatcher.ts`，用 generated `PlatformEvent` union 分发 terminal platform event，不再用 `as unknown as` 裸读字段。
- `context_frame` 在 model 层解析到 `SessionDisplayEntry.contextFrame`，`parseContextFrame` 允许空 `rendered_text`。
- `streamTransport` 严格校验 generated NDJSON `event` 分支，坏包通过 `onError` 暴露并丢弃，不再读取旧 `id` fallback 或合成 `event_seq=0`。
- 将 platform/system/task 事件策略收敛到 `systemEventPolicy.ts`，统一 renderable、task、feed boundary、notification 判断。
- 将 tool burst 资格与 dynamic tool metadata 收敛到 `threadItemKind.ts`，feed 与 registry 不再维护重复 mapper。
- 删除 `useSessionFeed` render 阶段写 ref 的 display item 稳定化逻辑。

## 涉及文件

- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/model/types.ts`
- `packages/app-web/src/features/session/model/systemEventPolicy.ts`
- `packages/app-web/src/features/session/model/systemEventVisibility.ts`
- `packages/app-web/src/features/session/model/threadItemKind.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.test.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventGuard.ts`
- `packages/app-web/src/features/session/ui/SessionTaskEventGuard.ts`
- `packages/app-web/src/features/session/ui/toolCardRegistry.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.test.ts`
- `packages/app-web/src/features/session/model/streamTransport.test.ts`
- `packages/app-web/src/features/session/model/contextFrame.test.ts`
- `packages/app-web/src/features/session/model/threadItemKind.test.ts`

## 验证

- `pnpm --filter app-web run test -- src/features/session/model/streamTransport.test.ts src/features/session/model/sessionStreamReducer.test.ts src/features/session/model/contextFrame.test.ts src/features/session/model/useSessionFeed.test.ts src/features/session/model/threadItemKind.test.ts src/features/session/ui/SessionChatView.test.tsx`：40 tests passed。
- `pnpm --filter app-web run typecheck`：通过。
- `pnpm --filter app-web run lint`：通过；仅剩既有 `SessionChatViewParts.tsx` 两个 `rounded-full` warning。

## Commit

待提交。
