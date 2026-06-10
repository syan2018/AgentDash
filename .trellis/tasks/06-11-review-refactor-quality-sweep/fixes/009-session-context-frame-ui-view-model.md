# FIX-009: session context frame UI 消费解析后 view model

## 模块

`session-stream`

## 来源

- `reviews/003-session-stream.md`
- `research/session-stream-executable-plan.md`
- worker: `019eb2cd-f010-7b52-916a-3abc42da0d72`

## 更新

- `SessionEntry` 聚合 context frame 时只读取 `entry.contextFrame`。
- 单帧 `context_frame` 通过 `entry.contextFrame` 传给 `SessionSystemEventCard`。
- `SessionSystemEventCard` 对 `context_frame` 只接受已解析 `ContextFrame`，没有 parsed frame 时返回 `null`。
- `ContextFrameCard` 改为接收 `frame: ContextFrame`，直接委托 `ContextFrameStream`。
- UI 测试改为传入 parsed frame，不再覆盖 parser 行为。

## 涉及文件

- `packages/app-web/src/features/session/ui/SessionEntry.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`
- `packages/app-web/src/features/session/ui/SessionEntry.context-frame.test.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.test.tsx`

## 验证

- `pnpm --filter app-web run test -- src/features/session/ui/SessionEntry.context-frame.test.tsx src/features/session/ui/ContextFrameCard.test.tsx src/features/session/ui/SessionSystemEventCard.test.tsx`：16 tests passed。
- `pnpm --filter app-web run typecheck`：通过。
- `pnpm --filter app-web run lint`：通过；仅剩既有 `SessionChatViewParts.tsx` 两个 `rounded-full` warning。

## Commit

待提交。
