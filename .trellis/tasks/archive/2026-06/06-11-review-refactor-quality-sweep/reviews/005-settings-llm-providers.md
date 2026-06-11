# REVIEW-005: settings-llm-providers

## 范围

- `packages/app-web/src/features/settings/ui/LlmProvidersSection.tsx`
- 直接相关的 settings model/service 调用边界

## 实现级可修复问题

### SETTINGS-LLM-IMPL-001: `LlmProviderForm` 职责过宽

- 证据：`LlmProvidersSection.tsx:406` `LlmProviderForm` 持续到约 `760` 行，内部同时管理基础字段、凭据、OAuth、模型探测、默认模型候选、blocked models、删除确认和保存 request 组装。
- 影响：表单逻辑、API 调用和渲染高度耦合，后续改任一子能力都容易误伤其它状态。
- 建议：拆成 `useLlmProviderFormState`、`ProviderBasicFields`、`ProviderCredentialFields`、`ProviderModelManagement`、`ProviderDangerZone`，先做文件内拆分也能快速降低复杂度。

### SETTINGS-LLM-IMPL-002: UI 直接调用 service/API，绕过 settings model 边界

- 证据：`LlmProvidersSection.tsx:5` UI 直接 import `llmProvidersApi`；`handleProbeModels` 直接调用 `llmProvidersApi.probeModels`；OAuth wizard 直接传 `start/getStatus/cancel` API。
- 影响：`features/settings/model/llmProviderQueries.ts` 只承载部分 CRUD，probe/OAuth 的失效刷新和错误处理散落在 UI，model/ui 分离不完整。
- 建议：在 `features/settings/model` 增加 `useProbeLlmProviderModelsMutation`、`useAdminCodexOAuthActions` 或窄 action factory，UI 只消费 hook/action。

### SETTINGS-LLM-IMPL-003: DTO/view-model mapper 放在 UI 文件顶部

- 证据：`LlmProvidersSection.tsx:18` 的 `parseModelConfigs`、`parseStringList`、`modelConfigsToJsonValue` 与 `ModelConfig` 都在 UI 文件中，保存时再塞回 `UpdateLlmProviderRequest`。
- 影响：`models` / `blocked_models` 的 JSON 形态解释被 UI 持有，违反 model/ui 分离，也让测试只能通过组件间接覆盖。
- 建议：提到 `features/settings/model/llmProviderModels.ts`，导出 `LlmProviderModelConfig`、parse/serialize、默认值和单测。

### SETTINGS-LLM-IMPL-004: `onSave` 类型与实际 async 行为不一致

- 证据：`LlmProvidersSection.tsx:342` / `421` 的 `onSave: (req) => void`，父组件实际传入 async 回调；`handleSave` 调用后立即清 touched 状态，没有等待保存成功。
- 影响：保存失败时本地 dirty 标记已被清掉，用户可能以为已保存；错误也没有在该表单内收口。
- 建议：把 `onSave` 类型改为 `Promise<void>`，`handleSave` 改 async，成功后再 reset touched，并增加局部 error 展示。

### SETTINGS-LLM-IMPL-005: `provider_id` 字段实际用 slug 匹配

- 证据：`LlmProvidersSection.tsx:206` 用 `(m.provider_id ?? "") === provider.slug`；probe 结果手动写 `provider_id: provider.slug`；后端 DTO 组装也填 `provider.slug.clone()`。
- 影响：字段名像 UUID/DB id，但语义是 provider slug，调用侧容易误传 `provider.id`。
- 建议：短期在 settings model 封装 `modelBelongsToProviderSlug(model, provider)`，避免 UI 直接解释混淆字段。

### SETTINGS-LLM-IMPL-006: Provider preset 是 UI 内硬编码事实源

- 证据：`LlmProvidersSection.tsx:76` `PROVIDER_PRESETS` 包含 provider slug、env var、base_url、默认模型；创建请求直接把这些字段写入 API。
- 影响：创建默认 provider 的业务事实隐藏在 UI，后端只接收持久化结果；后续 CLI、种子数据或其它入口无法复用。
- 建议：快速修复先移到 settings model 常量；如果目标是后端 provider catalog/seed，则另行设计。

## 模块级 refactor 候选

- 将 `LlmProviderForm` 拆成状态 hook 与小组件。
- 将 model config parse/serialize 移到 `features/settings/model` 并补单元测试。
- 将 probe/OAuth action 从 UI 移到 model action/hook。
- 将 `provider_id` 即 provider slug 的判断封装为命名明确的 helper。

## 架构 backlog 候选

### SETTINGS-LLM-ARCH-CANDIDATE-001: Provider preset/catalog 事实源

- 证据：Provider preset/catalog 目前是前端 UI 常量。
- 影响面：若改成后端/数据库事实源，会触及 seed/migration/API surface/generated contract。
- 建议：当前不进入 `architecture-backlog.md`，等确认要跨层迁移 provider catalog 时再升级。
