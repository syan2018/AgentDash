# Fix 026: session capability model helper

## 问题

`SessionCapabilityCard` 同时负责识别 `agentdash://session-capabilities/*` resource、解析 JSON、归一化 capabilities payload、筛选可见 provider cluster 和默认暴露 skill。UI 组件因此混入了 session resource 协议与 model 解析规则。

## 改动

- 新增 `sessionCapabilitiesBlock` model helper，集中处理 capability resource URI、block 识别、JSON parse、capabilities normalize、visible cluster、flat visible skill 和默认暴露 skill 统计。
- `SessionCapabilityCard` 改为消费 model helper 返回的 view model，并只保留展开状态与渲染逻辑。
- 将 `isSessionCapabilitiesBlock` 移到 model helper，并实现为 TypeScript type predicate，供解析逻辑获得 resource block 收窄。

## 涉及文件

- `packages/app-web/src/features/session/model/sessionCapabilitiesBlock.ts`
- `packages/app-web/src/features/session/ui/SessionCapabilityCard.tsx`

## 验证

- `pnpm --filter app-web test -- SessionCapabilityCard.test.tsx`：通过，1 file passed，3 tests passed。
- `pnpm --filter app-web run typecheck`：最终通过。
- 中间失败：首次 `pnpm --filter app-web run typecheck` 报 `src/features/session/model/sessionCapabilitiesBlock.ts(33,32)` 与 `(33,49)` 上 `Property 'resource' does not exist on type 'ContentBlock'`；已通过把 `isSessionCapabilitiesBlock` 改为 type predicate 修复。

## Commit

- hash: `cb9cb9ab`
