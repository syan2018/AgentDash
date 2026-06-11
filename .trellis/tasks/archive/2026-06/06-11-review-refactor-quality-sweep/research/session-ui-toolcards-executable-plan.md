# Session UI Tool Cards Executable Plan

## 模块边界

本轮只看 session UI 中 tool / companion / capability 卡片展示链路，不重新定义 backend event 协议。当前问题集中在少量前端组件内，均属于模块级快速修复范畴。

## 证据

- `SessionCompanionRequestCard.tsx` 同时解包 `BackboneEvent`、解释 `payload_type` / `ui_hint`、判断 `capability_grant_request`、组装响应 payload 并渲染按钮。
- `SessionSystemEventCard.tsx` 对 `companion_human_request` 特判转交 companion 卡，同时在 generic detail 中解释其它 `companion_*` 状态字段。
- `toolCardRegistry.ts` 的 `getItemDisplayStatus` 与 `features/session/model/types.ts` 的 `getThreadItemStatus` 存在状态读取重复。
- `ToolCallCardShell.tsx` 持有 approval API、提交状态、错误状态、状态文案和最小展示时间，职责超过 shell 命名。
- `SessionCapabilityCard.tsx` 在 UI 组件内处理 resource URI 判定、JSON parse、normalize、展示筛选和渲染。

## 可执行批次

### Batch A: capability resource 解析从组件移出

- 写入：`SessionCapabilityCard.tsx`，新增 `features/session/model/sessionCapabilitiesBlock.ts`。
- 内容：提取 capability URI、block 识别、parse/normalize、默认 exposed skills 与 visible clusters。
- 风险：低；已有 `SessionCapabilityCard.test.tsx` 覆盖 cluster/flat 解析。
- 验证：`pnpm --filter app-web test -- SessionCapabilityCard.test.tsx`；`pnpm --filter app-web run typecheck`。

### Batch B: companion request view model 收口

- 写入：`SessionCompanionRequestCard.tsx`，可新增 `features/session/model/companionRequestViewModel.ts`。
- 内容：提取 `parseCompanionRequest(event)`，组件只保留状态提交和渲染。
- 风险：中；需要确认 capability grant 请求和普通 options/text 请求按钮行为不变。
- 验证：`pnpm --filter app-web test -- SessionSystemEventCard.test.tsx`；`pnpm --filter app-web run typecheck`。

### Batch C: tool status 单一来源

- 写入：`features/session/model/types.ts`、`toolCardRegistry.ts`、`SessionEntry.tsx`。
- 内容：删除 registry 私有 status resolver，统一使用 session model status helper；保留 `contextCompaction` 的明确特殊语义。
- 风险：低。
- 验证：`pnpm --filter app-web run typecheck`。

### Batch D: tool card header/body 责任拆分

- 写入：`toolCardRegistry.ts`，可新增 `toolCardHeaders.tsx`。
- 内容：`renderToolCallCard` 只做 item kind 分发，摘要/header 构造移出。
- 风险：中；涉及多类工具卡标题显示。
- 验证：`pnpm --filter app-web run typecheck`，必要时补轻量 header 测试。

## 架构项

无。问题影响面集中在前端 3-5 个文件内，不需要超过十个文件的跨层事实源重定。
