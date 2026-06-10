# Research: session-stream executable plan

- Query: 基于 `reviews/003-session-stream.md`，把 session-stream 模块收敛为可执行、可并行的模块级修复批次。
- Scope: internal
- Date: 2026-06-11

## Findings

### Inputs Read

- `.trellis/workflow.md`：研究结果必须持久化到 task `research/`，Phase 2 通过 sub-agent 执行实现与检查。
- `.trellis/tasks/06-11-review-refactor-quality-sweep/reviews/003-session-stream.md`：本次 review 的直接输入。
- `.trellis/spec/frontend/index.md`：前端改动先读 architecture/type/state/hook/component/quality 规范。
- `.trellis/spec/frontend/type-safety.md`：generated wire 是单源，禁止前端重声明跨层 DTO、字段别名双读和逐字段 identity rebuild。
- `.trellis/spec/frontend/hook-guidelines.md`：NDJSON envelope 属于 cross-layer contract，hook 只消费解析后的 envelope，业务聚合交 reducer。
- `.trellis/spec/frontend/state-management.md`：Session control action 状态来自后端 runtime-control，stream 事件只适合触发刷新。
- `.trellis/spec/frontend/component-guidelines.md`：model 层和 UI 层分离。
- `.trellis/spec/frontend/quality-guidelines.md`：检查命令与禁止 `any` / 无约束断言。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：NDJSON stream envelope 必须来自 Rust contract -> generated TS。
- `.trellis/spec/backend/session/streaming-protocol.md`：Session stream `connected` / `event` / `heartbeat` 字段契约。
- `.trellis/spec/backend/session/pi-agent-streaming.md`：前端 feed 聚合消费优先级。
- `.trellis/spec/backend/session/execution-context-frames.md`：ContextFrame 是 connector 边界的动态上下文投影。

### Files Found

| Path | Description |
| --- | --- |
| `packages/app-web/src/features/session/model/useSessionStream.ts` | hydrate + NDJSON stream + reducer + terminal side effect 当前都在同一 hook 内。 |
| `packages/app-web/src/features/session/model/streamTransport.ts` | fetch NDJSON transport，当前对 generated envelope 做了宽松重解析。 |
| `packages/app-web/src/features/session/model/types.ts` | session feature view model 类型；`SessionEventEnvelope` 当前直接 alias generated `SessionEventResponse`。 |
| `packages/app-web/src/features/session/model/contextFrame.ts` | 前端手写 ContextFrame DTO 与 parser。 |
| `packages/app-web/src/features/session/model/platformEvent.ts` | Backbone platform event 的 event type / data / message 提取工具。 |
| `packages/app-web/src/features/session/model/systemEventVisibility.ts` | 当前 system/task/platform 可见性 guard。 |
| `packages/app-web/src/features/session/model/threadItemKind.ts` | ThreadItem kind / badge / label 的主要元数据来源。 |
| `packages/app-web/src/features/session/model/useSessionFeed.ts` | feed 聚合、工具 burst、context_frame soft boundary、display item 稳定化。 |
| `packages/app-web/src/features/session/model/useSessionFeed.test.ts` | 工具 burst 与 context_frame soft boundary 已有覆盖。 |
| `packages/app-web/src/features/session/model/useTerminalStore.ts` | terminal side effect 的目标 Zustand store。 |
| `packages/app-web/src/features/session/ui/SessionEntry.tsx` | display item 渲染入口，当前聚合 context_frame 时在 UI 解析。 |
| `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx` | 单个 system event 卡片，当前单帧 context_frame 再解析。 |
| `packages/app-web/src/features/session/ui/ContextFrameCard.tsx` | 当前 raw `Record<string, unknown>` -> `ContextFrame` 的 UI 兼容层。 |
| `packages/app-web/src/features/session/ui/ContextFrameStream.tsx` | 已经声明只接收解析后的 `ContextFrame[]`。 |
| `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` | `collectNewSystemEvents` 与 projection refresh key 计算。 |
| `packages/app-web/src/features/session/ui/SessionChatView.tsx` | rawEvents 驱动 execution refresh、turn end、system event callback。 |
| `packages/app-web/src/features/session/ui/*.test.tsx` | context frame、system event、chat view 局部测试。 |
| `packages/app-web/src/generated/session-contracts.ts` | generated `SessionEventResponse` / `SessionNdjsonEnvelope`。 |
| `packages/app-web/src/generated/backbone-protocol.ts` | generated `BackboneEvent` / `PlatformEvent`，含 terminal platform event union。 |
| `crates/agentdash-contracts/src/session.rs` | Rust source of generated SessionEventResponse / SessionNdjsonEnvelope。 |
| `crates/agentdash-spi/src/hooks/mod.rs` | Rust source of ContextFrame shape，当前未生成到 `session-contracts.ts`。 |

### Code Patterns

- `UseSessionStreamOptions.onEntry` 在 `useSessionStream.ts:31` 定义，`callbackRefs` 在 `useSessionStream.ts:447` 保存，但仅 `onConnectionChange` / `onError` 在 `useSessionStream.ts:597`、`useSessionStream.ts:617` 等处调用；`onEntry` 没有调用方。全 repo 搜索也未找到业务传入 `onEntry`。
- `StreamInputEvent` 在 `useSessionStream.ts:61` 重声明了 session event wire shape；`types.ts:253` 已把 `SessionEventEnvelope` alias 到 generated `SessionEventResponse`。
- `toEventEnvelope` 在 `useSessionStream.ts:111` 对本地 input 重新补默认值；`reduceStreamState` 在 `useSessionStream.ts:344` 消费 `StreamInputEvent[]`，而历史页 `fetchSessionEvents` 已返回 `SessionEventResponse[]`（`services/session.ts:46`）。
- `applyEventToEntries` 从文本 delta、tool lifecycle、approval、platform visibility 到 token 静默都集中在 `useSessionStream.ts:164`，是核心 reducer 拆分点。
- terminal side effect 在 `useSessionStream.ts:386` 直接读取 `useTerminalStore`，并通过 `as unknown as` 与字段断言读取 `terminal_id/state/exit_code`。
- generated `PlatformEvent` 已包含 terminal union：`backbone-protocol.ts:269` 有 `terminal_output` 和 `terminal_state_changed`，可以用 discriminated union 替代 `as unknown as`。
- `streamTransport.ts:72` 的 `parseSessionEventEnvelopePayload` 接收 `unknown`；`streamTransport.ts:90` 仍读 `record.id` 和 `fallbackEventSeq`，最终 `?? 0`；这违背 generated envelope 单源。
- Rust contract 中 `SessionNdjsonEnvelope::Event` flatten `SessionEventResponse`，见 `crates/agentdash-contracts/src/session.rs:68`；generated TS 在 `session-contracts.ts:47` 已表达 `connected/event/heartbeat` union。
- `systemEventVisibility.ts:7` 定义可见 system event 白名单，`systemEventVisibility.ts:104` 输出 `isRenderableSystemEventUpdate`，`systemEventVisibility.ts:120` 输出 `isRenderablePlatformEvent`。
- `SessionChatViewModel.ts:38` 的 `collectNewSystemEvents` 收集所有 platform event type，没有复用 `isRenderableSystemEventUpdate`。
- `useSessionFeed.ts:57` 的 `getToolAggregationType` 手写 tool burst 资格；`threadItemKind.ts:60` 的 `resolveKind` 和 `toolCardRegistry.ts:195` 的 `getDynamicToolHeader` 是另外两套 tool/dynamic tool 映射。
- `useSessionFeed.ts:399` 在 render/useMemo 路径写 `prevDisplayItemsRef.current`，并用 `react-hooks/refs` disable 包裹 `useSessionFeed.ts:401` 到 `useSessionFeed.ts:430`。
- `contextFrame.ts:195` 手写 ContextFrame parser；`contextFrame.ts:205` 当前把空 `rendered_text` 当作整帧无效。
- `SessionEntry.tsx:185` 聚合 context_frame 时 `extractPlatformEventData -> parseContextFrame`；`SessionSystemEventCard.tsx:198` 单帧 context_frame 又走 `ContextFrameCard`；`ContextFrameCard.tsx:18` 再解析一次。
- `ContextFrameStream.tsx:20` 的 props 已经是 `frames: ContextFrame[]`，UI 下层不需要 raw event。

### Immediate Implementation Batches

#### Batch 1: Stream Core Contract And Side Effects

并行性：可与 Batch 2 并行；Batch 3 依赖本批约定的 `SessionDisplayEntry.contextFrame?: ContextFrame` 接口，但不写同一批核心文件。

Write scope:

- `packages/app-web/src/features/session/model/useSessionStream.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts`（新增）
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts`（新增）
- `packages/app-web/src/features/session/model/streamTransport.ts`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/model/types.ts`
- `packages/app-web/src/features/session/model/sessionStreamReducer.test.ts`（新增）
- `packages/app-web/src/features/session/model/streamTransport.test.ts`（新增）
- `packages/app-web/src/features/session/model/contextFrame.test.ts`（新增）

Core changes:

- 删除 `UseSessionStreamOptions.onEntry`，移除 `callbackRefs` 中的 `onEntry` 和 options 解构；`useSessionFeed` 调用点本来没有传该字段。
- 删除本地 `StreamInputEvent` 和 `toEventEnvelope`。`reduceStreamState` 直接消费 `SessionEventEnvelope[]` / `SessionEventResponse[]`，历史页与 transport 输出走同一 generated type。
- 把 `applyEventToEntries`、`reduceStreamState`、`makeDisplayEntry`、entry id 生成等纯逻辑移到 `sessionStreamReducer.ts`；`useSessionStream.ts` 只保留 hydrate、connect、buffer flush、状态装配。
- 在 `makeDisplayEntry` 内对 `platform/session_meta_update key=context_frame` 调 `parseContextFrame`，成功时写入 `SessionDisplayEntry.contextFrame`。这让 UI 不再解析 raw platform payload。
- `parseContextFrame` 允许 `rendered_text === ""`，必填校验以 `id/kind/source/delivery_status/delivery_channel/message_role/created_at_ms/sections` 为主；新增空文本但有 sections 的单元测试。
- 新增 `sessionPlatformEventDispatcher.ts`，用 generated `PlatformEvent` discriminated union 分发 `terminal_output` / `terminal_state_changed`，只在这里接触 `useTerminalStore`。`terminal_state_changed.state` 先经 `isTerminalProcessState` guard，再调用 store；无效 state 报错或跳过，不用 `as unknown as`。
- `useSessionStream.enqueueEvent` 调 dispatcher；被 dispatcher 消费的 terminal event 不进入 reducer，保持当前 rawEvents 行为不变。
- `streamTransport.ts` 只接受 `record.type === "event"` 的 generated envelope。缺少 `notification`、缺少/非法 `event_seq`、缺少必要 generated fields 时 `onError(new Error(...))` 并丢弃该行；移除 `record.id`、`fallbackEventSeq`、`?? 0`。
- 对 `connected.last_event_id` 继续只更新 `sinceId`；`heartbeat` 只保活；未知 `type` 安全忽略或报错后忽略，但不合成 event。

Risks:

- 严格 transport 会暴露后端坏包，以前被 `event_seq=0` 静默吞掉的问题会变成 `onError`。这是预期收敛，但调试时会看到更多错误。
- terminal event 不进入 rawEvents 的行为保持不变；如果后续希望审计 terminal event，需要另起产品决策，不混入本批。
- `contextFrame` 字段增加在 display entry view model 上，不改变 generated wire contract。

Validation commands:

```powershell
pnpm --filter app-web run test -- src/features/session/model/streamTransport.test.ts src/features/session/model/sessionStreamReducer.test.ts src/features/session/model/contextFrame.test.ts
pnpm --filter app-web run typecheck
```

#### Batch 2: Feed Policy, Tool Metadata, And Render Purity

并行性：可与 Batch 1 并行；不写 Batch 1 的 stream/reducer/transport/contextFrame 文件，不写 Batch 3 的 context-frame UI files。

Write scope:

- `packages/app-web/src/features/session/model/systemEventPolicy.ts`（新增，或由 `systemEventVisibility.ts` 直接改名后全量更新 import）
- `packages/app-web/src/features/session/model/systemEventVisibility.ts`（若改名则删除；不要保留长期兼容 wrapper）
- `packages/app-web/src/features/session/model/threadItemKind.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.ts`
- `packages/app-web/src/features/session/model/useSessionFeed.test.ts`
- `packages/app-web/src/features/session/model/threadItemKind.test.ts`（新增）
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts`
- `packages/app-web/src/features/session/ui/SessionChatView.test.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventGuard.ts`
- `packages/app-web/src/features/session/ui/SessionTaskEventGuard.ts`
- `packages/app-web/src/features/session/ui/toolCardRegistry.ts`

Core changes:

- 抽出 `systemEventPolicy` 单一策略模块，输出类似 `getPlatformEventPolicy(event)`：
  - `eventType`
  - `isTaskEvent`
  - `isRenderableSystemEvent`
  - `isRenderablePlatformEvent`
  - `feedBoundary: "hard" | "soft" | "neutral"`
  - `notificationVisibility: "renderable" | "all"` 或明确的 `shouldNotifyRenderableSystemEvent`
- `useSessionFeed.classifyEntry` 不再直接调用 visibility guard；改为消费 policy 的 `feedBoundary`。`context_frame` 仍是 soft boundary，silent hook trace 仍 neutral。
- `SessionChatViewModel.collectNewSystemEvents` 改为 `collectRenderableSystemEvents`，复用 policy 的 renderable/system 判断；如果调用方确实要全部 platform event，再额外导出 `collectAllPlatformEvents`，但当前 `SessionChatView` 的 `onSystemEvent` 应走 renderable。
- `SessionSystemEventGuard.ts` / `SessionTaskEventGuard.ts` 从 `systemEventPolicy` re-export；更新 import 后可以删除旧 `systemEventVisibility.ts`。
- `threadItemKind.ts` 增加 `isToolBurstEligible(item)`，`useSessionFeed.ts:57` 的 switch 改为调用它。保持 `contextCompaction` 是否进入 tool burst 的现状：当前 `getToolAggregationType` 未纳入 `contextCompaction`，不要在本批顺手改语义，除非先补测试并确认产品期望。
- `threadItemKind.ts` 增加 dynamic tool metadata（例如 `resolveDynamicToolMeta(tool)` 返回 kind 与 tool family），`toolCardRegistry.ts` 继续负责 React header 节点，但不要另起一套 dynamic tool family 判断。
- 移除 `useSessionFeed.ts:399` 的 render 阶段 ref mutation。建议优先删除手写稳定化；如果必须保留，写成纯 `stabilizeDisplayItems(prev, next)`，再通过 effect/受控状态更新 cache，不能在 `useMemo` 写 ref。

Risks:

- `collectRenderableSystemEvents` 会让 UI callback 不再收到 silent platform meta/hook trace；这是 review 要求，但如果上层曾依赖不可见事件副作用，需要显式改用 `collectAllPlatformEvents`。
- 删除 display item 稳定化可能增加少量重渲染；先用现有 tests 和手动观察确认，不要为了微优化保留 render side effect。
- dynamic tool metadata 不能下沉 ReactNode；model 输出分类/文案/字段选择，UI 仍负责 `FilePathPill` 等 React rendering。

Validation commands:

```powershell
pnpm --filter app-web run test -- src/features/session/model/useSessionFeed.test.ts src/features/session/model/threadItemKind.test.ts src/features/session/ui/SessionChatView.test.tsx
pnpm --filter app-web run typecheck
pnpm --filter app-web run lint
```

#### Batch 3: Context Frame UI Consumes Parsed View Model

并行性：文件范围与 Batch 1/2 不冲突；实现前需固定 Batch 1 的 `SessionDisplayEntry.contextFrame?: ContextFrame` 接口。

Write scope:

- `packages/app-web/src/features/session/ui/SessionEntry.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`（仅更新注释/props 说明时需要）
- `packages/app-web/src/features/session/ui/SessionEntry.context-frame.test.tsx`
- `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx`
- `packages/app-web/src/features/session/ui/SessionSystemEventCard.test.tsx`

Core changes:

- `SessionEntry.AggregatedContextFrameGroupEntry` 改为从 `group.entries.map(entry => entry.contextFrame)` 取 frames；移除 UI 层 `extractPlatformEventData` / `parseContextFrame` import。
- `SingleEntry` 渲染 platform context_frame 时，把 `entry.contextFrame` 传给 `SessionSystemEventCard`。
- `SessionSystemEventCard` 对 `context_frame` 只接受已解析 `contextFrame` prop；没有 parsed frame 时返回 `null`，不在 card 内解析 raw event。
- `ContextFrameCard` props 改为 `{ frame: ContextFrame; defaultExpanded?: boolean }`，直接委托 `ContextFrameStream frames={[frame]}`。
- 测试 fixture 改为构造带 `contextFrame` 的 `SessionDisplayEntry` 或显式先调用 model parser，再传 `frame`。UI 测试不再断言 parser 行为，parser 行为由 Batch 1 的 model test 覆盖。

Risks:

- `SessionSystemEventCard` 作为裸组件直接传 raw context_frame event 时将不再渲染；当前产品路径经 `SessionEntry`，这是把解析责任前移到 model 的预期结果。
- `ContextFrameStream.tsx` 注释当前写着 `SessionEntry` 负责解析，Batch 3 后应改成 model 负责解析，避免误导后续维护。

Validation commands:

```powershell
pnpm --filter app-web run test -- src/features/session/ui/SessionEntry.context-frame.test.tsx src/features/session/ui/ContextFrameCard.test.tsx src/features/session/ui/SessionSystemEventCard.test.tsx
pnpm --filter app-web run typecheck
```

### Non-Deferred Review Items

这些项不应被误判为大架构，建议纳入上面的模块级批次：

- SESSION-IMPL-001 `onEntry`：Batch 1 删除。
- SESSION-IMPL-002 strict stream transport contract：Batch 1。
- SESSION-IMPL-003 `StreamInputEvent` 重复类型：Batch 1。
- SESSION-IMPL-004 terminal platform event dispatcher：Batch 1。
- SESSION-IMPL-005 `applyEventToEntries` 过宽：Batch 1 做纯 reducer 抽离即可，不需要跨层设计。
- SESSION-IMPL-006 tool mapper 三套来源：Batch 2。
- SESSION-IMPL-007 render 阶段 ref mutation：Batch 2。
- SESSION-IMPL-008 context frame UI 重复解析：Batch 1 + Batch 3。
- SESSION-IMPL-009 空 `rendered_text` 被前端 parser 丢弃：Batch 1 前端 parser 修复；generated contract 化另见延后项。
- SESSION-IMPL-010 UI 通知未复用 visibility guard：Batch 2。

### Deferred / Separate Work

- `ContextFrame` 进入 generated contract：应单独排 cross-layer contract 批次，涉及 `agentdash-spi`/`agentdash-contracts` 类型归属、TS 生成、前端 parser 替换和 `pnpm run contracts:check`，属于 contract change，不应混进本次 frontend module 收敛。
- SESSION-ARCH-001 running/control 状态事实源：涉及后端 runtime-control / execution projection 与 `SessionChatView.tsx` 控制面，跨层 contract 与行为面更大，应延后。
- SESSION-ARCH-002 拆 `useSessionEventStream` / `useSessionFeedProjection` / `useSessionControlActions`：会改变 hook API 与多个消费入口，预计超过模块级修复范围，应延后。
- SESSION-ARCH-003 UI 不直接消费完整 `BackboneEvent` 的 view model union：会影响多数 `SessionEntry`/tool/system/task card 渲染路径，预计超过 10 个文件，应延后。
- SESSION-ARCH-004 platform/session_meta_update 全策略总线：Batch 2 只抽 `systemEventPolicy` 覆盖 visibility/feed boundary/notification；projection invalidation、sideEffectTopic、control state 等更宽策略等后续有跨层需求时再做。

### Related Specs

- `.trellis/spec/frontend/type-safety.md`：generated wire 单源，禁止 DTO 重声明与字段别名兼容。
- `.trellis/spec/frontend/hook-guidelines.md`：NDJSON transport、buffer flush、feed 聚合契约、platform event 可见性、context_frame soft boundary。
- `.trellis/spec/frontend/state-management.md`：Session control action 状态来自后端 runtime-control。
- `.trellis/spec/frontend/component-guidelines.md`：model/UI 分离。
- `.trellis/spec/frontend/quality-guidelines.md`：`pnpm --filter app-web run check`、Vitest、lint/typecheck 约束。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：NDJSON envelope 与 generated contract。
- `.trellis/spec/backend/session/streaming-protocol.md`：Session stream envelope 字段。
- `.trellis/spec/backend/session/pi-agent-streaming.md`：feed 聚合消费优先级。
- `.trellis/spec/backend/session/execution-context-frames.md`：ContextFrame 的运行时语义。

### External References

- No web references used. This research is based on local code, generated contracts, and Trellis specs.
- Local package versions from `packages/app-web/package.json`: React `^19.2.0`, TypeScript `~5.9.3`, Vitest `^4.0.18`, ESLint `^9.39.1`.
- Validation scripts from `packages/app-web/package.json`: `typecheck`, `lint`, `test`, `check`.
- Root validation scripts from `package.json`: `contracts:check`, `frontend:check`, `frontend:lint`, `check`.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)`, so this research used the explicit task path supplied in the prompt: `.trellis/tasks/06-11-review-refactor-quality-sweep`.
- No business caller of `UseSessionStreamOptions.onEntry` was found under `packages/app-web/src`; removal should be localized to the type and callback ref cleanup.
- No existing `streamTransport.test.ts` or `useSessionStream` reducer test was found; Batch 1 should add focused model tests before relying on stricter malformed-line behavior.
- `ContextFrame` is not currently generated into `packages/app-web/src/generated/session-contracts.ts`; frontend-only parser cleanup can proceed, but generated contract adoption is a separate cross-layer task.
- Backend paths such as `crates/agentdash-application/src/session/context_frame.rs:51` also skip empty `rendered_text` for turn-start enqueue. The frontend parser should still allow empty text because persisted/session_meta_update context_frame values can arrive through other paths, but changing backend emission policy is outside this research scope.
