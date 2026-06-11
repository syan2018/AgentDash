# FIX-003: settings-llm-providers model 边界收敛

## 模块

`settings-llm-providers`

## 来源

- `reviews/005-settings-llm-providers.md`
- worker: `019eb2ab-1cf3-7ba2-bd4b-c750407ff858`

## 更新

- 将 `models` / `blocked_models` 的 parse/serialize 和模型配置类型移到 settings model 层。
- 将 Provider preset 常量移到 settings model 层，创建行为保持不变。
- 新增 `modelBelongsToProviderSlug`，避免 UI 直接写 `provider_id` 与 `provider.slug` 的混淆匹配。
- `LlmProviderForm.onSave` 改为 `Promise<void>`；保存成功后才清 touched，失败时保留 dirty 状态并展示局部保存错误。
- 将 probe models 与 admin Codex OAuth 调用包装为 settings model action，UI 不再直接 import `llmProvidersApi`。

## 涉及文件

- `packages/app-web/src/features/settings/ui/LlmProvidersSection.tsx`
- `packages/app-web/src/features/settings/model/llmProviderActions.ts`
- `packages/app-web/src/features/settings/model/llmProviderModels.ts`
- `packages/app-web/src/features/settings/model/llmProviderPresets.ts`
- `packages/app-web/src/features/settings/model/llmProviderModels.test.ts`

## 验证

- `pnpm --filter app-web exec vitest run src/features/settings/model/llmProviderModels.test.ts`：4 tests passed。
- `pnpm --filter app-web run typecheck`：通过。
- `pnpm --filter app-web run lint`：通过；仅剩既有 `SessionChatViewParts.tsx` 两个 warning。

## Commit

`205d2a91 refactor(settings): 收敛 LLM Provider 模型边界`
