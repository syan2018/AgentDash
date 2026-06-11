# REVIEW-003: session-stream

## 范围

- `packages/app-web/src/features/session/model/useSessionStream.ts`
- session stream transport / feed / context frame / UI registry 直接边界

## 实现级可修复问题

### SESSION-IMPL-001: `UseSessionStreamOptions.onEntry` 未被调用

- 证据：`packages/app-web/src/features/session/model/useSessionStream.ts:25` 定义 `onEntry` 并进入 `callbackRefs`，但全文件没有调用。
- 影响：hook API 表面积虚胖，调用方会误以为能监听单条 entry，实际无效。
- 建议：删除 `onEntry` 选项；如果确实需要事件回调，应在 `enqueueEvent` 或 reducer 输出差量处明确调用并补测试。

### SESSION-IMPL-002: stream transport 宽松解析非当前 contract 字段

- 证据：`packages/app-web/src/features/session/model/streamTransport.ts:72` `parseSessionEventEnvelopePayload` 在 generated `SessionNdjsonEnvelope` 后又手写宽松解析；`streamTransport.ts:90` 仍读取 `record.id` 并默认 `event_seq=0`。
- 影响：DTO 字段事实源被 transport 重新解释，`id` 这种非当前 contract 字段继续存活；默认 0 会让坏包被 reducer 的 seq 去重静默吞掉。
- 建议：transport 只接受 generated `SessionNdjsonEnvelope` 的 `event` 分支，移除 `id` fallback 和 `0` 默认；缺少 `event_seq` 直接丢弃并报错。

### SESSION-IMPL-003: 本地 stream input 类型重复表达 wire shape

- 证据：`packages/app-web/src/features/session/model/useSessionStream.ts:61` 本地 `StreamInputEvent` 与 `types.ts:253` `SessionEventEnvelope = SessionEventResponse` 重复表达同一 wire shape。
- 影响：历史事件、stream event、reducer input 三处类型可分叉，后续改 DTO 时容易只改 generated contract，却漏掉本地裸字段。
- 建议：删除 `StreamInputEvent`，`reduceStreamState` 直接消费 `SessionEventResponse[]` 或明确 `SessionStreamEvent` alias，转换只保留在 transport 边界。

### SESSION-IMPL-004: terminal platform event 副作用嵌在 stream hook 内

- 证据：`packages/app-web/src/features/session/model/useSessionStream.ts:386` `interceptTerminalEvent` 在 session stream hook 内直接写 `useTerminalStore`，并用 `as unknown as` 读取 `terminal_id/state/exit_code`。
- 影响：session stream reducer 前插入跨 feature 副作用，rawEvents 中不会保留 terminal 事件；同时绕过 generated 类型，字段错误只能运行时暴露。
- 建议：把 terminal 平台事件分流移到独立 dispatcher，例如 `sessionPlatformEventDispatch`；先用 typed guard 验证 payload，再由外层 composition 注入 terminal handler。

### SESSION-IMPL-005: `applyEventToEntries` 过宽

- 证据：`packages/app-web/src/features/session/model/useSessionStream.ts:164` 同时处理文本 delta 累积、工具生命周期、审批、platform 可见性、token 静默策略。
- 影响：新增 BackboneEvent 时必须同时理解 reducer、feed aggregation、UI guard，局部改动风险高。
- 建议：抽出纯 reducer 模块，按 `messageDelta / toolItem / platformEvent / usage` 分小函数；`useSessionStream` 只负责 hydrate、连接、flush 和 state 装配。

### SESSION-IMPL-006: tool 聚合与展示元数据有三套 mapper

- 证据：`useSessionFeed.ts:57` `getToolAggregationType` 硬编码 tool item 类型；`threadItemKind.ts:60` 维护 `resolveKind`；`toolCardRegistry.ts:195` 又按 dynamic tool 名二次分支。
- 影响：工具类型、聚合资格、badge/header 三套 mapper 会漂移；新增 `ThreadItem` 或 dynamic tool 需要多处同步。
- 建议：在 `threadItemKind.ts` 增加 `isToolBurstEligible(item)` / `resolveDynamicToolMeta(tool)`，feed 和 registry 都消费同一元数据。

### SESSION-IMPL-007: render 阶段修改 ref

- 证据：`packages/app-web/src/features/session/model/useSessionFeed.ts:401` `useMemo` 内修改 `prevDisplayItemsRef.current`，并通过 eslint disable 绕过 refs 规则。
- 影响：render 阶段产生副作用，React 并发/重放渲染下可能出现稳定化缓存与真实输出不同步。
- 建议：把稳定化逻辑改成纯函数返回 `{items, cache}`，用 effect 或自定义 hook 的受控路径更新；或优先删除手写稳定化。

### SESSION-IMPL-008: context frame 在 UI 层重复解析

- 证据：`SessionEntry.tsx:180` 聚合后的 `context_frame` 在 UI render 时逐条 `extractPlatformEventData -> parseContextFrame`；单帧路径在 `SessionSystemEventCard.tsx:198` 再解析一次。
- 影响：model 已经把 `context_frame` 作为 soft boundary 聚合，但 typed frame 仍在 UI 层临时解析，model/UI 边界不清，解析失败只能表现为卡片消失。
- 建议：在 session model 层生成 `ContextFrameEntry` 或 `AggregatedContextFrameGroup.frames`，UI 只接收已解析的 `ContextFrame[]`。

### SESSION-IMPL-009: context frame DTO 手写且错误丢弃空文本帧

- 证据：`contextFrame.ts:195` `parseContextFrame` 手写完整 DTO 和默认值，`contextFrame.ts:205` 要求 `rendered_text` 非空，否则整帧丢弃。
- 影响：后端 context frame contract 变化不会被 generated 类型捕获；空可见文本但有结构化 sections 的 frame 会被静默丢掉。
- 建议：为 context frame 进入 generated contract 或定义单一 route-local wire type；`rendered_text` 允许空字符串，必填校验以 `id/kind/source/sections/created_at_ms` 为主。

### SESSION-IMPL-010: UI 通知未复用 system event visibility guard

- 证据：`SessionChatViewModel.ts:38` `collectNewSystemEvents` 收集所有 platform event type，未复用 `systemEventVisibility.ts:104` 的可渲染/显著性判断。
- 影响：UI 不展示的 hook trace 或静默 meta update 仍可能触发 `onSystemEvent`，造成调用方处理不可见事件的隐式副作用。
- 建议：明确分成 `collectRenderableSystemEvents` 与 `collectAllPlatformEvents`；当前聊天视图若只服务 UI 通知，应复用 visibility guard。

## 架构 backlog 候选

### SESSION-ARCH-001: session running/control 状态事实源分散

- 证据：`SessionChatView.tsx:269` 运行态由 `fetchSessionExecutionState` 轮询、raw stream event 扫描、`optimisticRunning` 三者共同推导。
- 影响：`turn_completed/turn_failed/turn_interrupted` 的前端判断会和后端 `SessionRuntimeControlView.actions`、execution state 产生竞态。
- 建议：收敛到后端 runtime-control / execution projection 作为唯一控制事实源，stream event 只做失效触发，不直接决定 action running。

### SESSION-ARCH-002: session stream hook 同时暴露数据流、控制命令和 UI 派生输入

- 证据：`useSessionStream.ts:674` hook 返回 `entries/rawEvents/tokenUsage/reconnect/close/sendCancel`，同时负责数据流、控制命令和 UI 派生输入。
- 影响：任何消费会话 feed 的页面都会拿到过宽能力，session stream 与 session control 生命周期耦合。
- 建议：拆成 `useSessionEventStream`、`useSessionFeedProjection`、`useSessionControlActions` 三层；聊天页在 composition root 组合。

### SESSION-ARCH-003: UI 直接消费完整 BackboneEvent

- 证据：`SessionDisplayEntry` 直接携带完整 `BackboneEvent` 穿透到 `SessionEntry.tsx` 和工具卡 registry。
- 影响：UI 层长期依赖 wire event 细节，导致 `event.payload.*` 裸字段跨多层传播；model 层难以成为稳定 view model 边界。
- 建议：定义 session feed view model union，例如 `MessageEntry / ToolEntry / SystemEventEntry / ContextFrameEntry`，UI 不直接 switch generated event。

### SESSION-ARCH-004: platform/session_meta_update 策略分散

- 证据：`systemEventVisibility.ts`、`useSessionFeed.ts`、`SessionChatViewModel.ts` 三处都在解释 platform/session_meta_update 语义。
- 影响：`context_frame`、`hook_event`、`context_compacted` 这类事件的“可见、聚合、刷新 projection、触发 side effect”规则分散。
- 建议：建立 `sessionEventPolicy` 单一策略模块，输出 `visibility / aggregationLane / projectionInvalidation / sideEffectTopic`。
