# 修复 PiAgent 上下文用量与 compact 链路

## Goal

补齐 native PiAgent 会话的上下文用量事实链路，使前端输入栏上下文圆环能够展示当前模型上下文占用比例，并让自动 compact 使用同一份 provider-visible pressure 与 effective context window 进行判断。

本任务同时确认 Codex app-server 的事件流语义：token usage 是标准 `thread/tokenUsage/updated` notification，独立于 `turn/*` 与 `item/*` lifecycle；AgentDash 应继续在 Backbone 中使用 `TokenUsageUpdated`，而不是新增平台私有用量事件。

## Confirmed Facts

- `references/codex/codex-rs/app-server/README.md` 说明 token usage events 通过 `thread/tokenUsage/updated` 单独推送，item lifecycle 仍保持 `item/started -> deltas -> item/completed`。
- Codex `ThreadTokenUsage` 包含 `total`、`last` 和 `modelContextWindow`，`model_context_window` 当前仍为 optional。
- AgentDash Backbone 已有 `TokenUsageUpdated(ThreadTokenUsageUpdatedNotification)`，并在项目 spec 中定义了 normalized `context` 字段：`provider_context_tokens`、`pending_estimate_tokens`、`current_context_tokens`、`model_context_window`、`effective_context_window`、`reserve_tokens`。
- external Codex bridge 已把 Codex `thread/tokenUsage/updated` 映射到 Backbone `TokenUsageUpdated`。
- native PiAgent `stream_mapper.rs` 当前在 `AgentEvent::MessageEnd` 中只输出 message/tool/reasoning 事件，没有输出 Backbone `TokenUsageUpdated`。
- 前端 `tokenUsage` 只从 `TokenUsageUpdated` reducer 链路进入 `SessionChatView`，上下文圆环和 `SessionProjectionView` 的当前/上限展示都依赖该状态。
- compact hook delegate 当前从 runtime metadata 的 `model_context_window` 读取窗口上限；普通 `before_provider_request` 观测路径在没有 compact 参数时会传 `context_window = 0`。

## Requirements

- native PiAgent 每轮 provider response 产生 usage 后，必须向 Backbone 主事件流输出 `TokenUsageUpdated`。
- `TokenUsageUpdated` payload 必须使用 AgentDash normalized context 语义：当前压力使用 provider-visible input pressure，Anthropic cache read / cache creation input 计入压力。
- 当前模型上下文上限必须来自本轮实际生效的 provider/model runtime 配置，而不是前端模型选择器本地状态或 projection token estimate。
- compact 评估、`before_provider_request` hook stats、前端上下文圆环必须使用同一 effective window 口径。
- `SessionProjectionView.context_usage` 继续负责解释 projection 构成；它不成为 provider usage 或窗口上限的事实源。
- 保持 Codex 对齐：不新增独立 platform usage event，不把 usage 塞入 message item lifecycle。
- 不做兼容性回退；本项目预研阶段直接修正为正确事实流。

## Acceptance Criteria

- [ ] PiAgent `MessageEnd` 带 usage 时，session NDJSON 中出现 `token_usage_updated` 事件。
- [ ] 该事件包含非零 `currentContextTokens`、`providerContextTokens`，并包含当前模型的 `modelContextWindow` / `effectiveContextWindow`。
- [ ] 前端 `ContextUsageRing` 能从 PiAgent 会话显示占用百分比和当前/上限。
- [ ] `SessionProjectionView` 浮层中的“当前 / 上限”和“剩余空间”基于同一 `tokenUsage` 状态。
- [ ] `before_compact` 与 `before_provider_request` hook trace 的 token stats 使用非零 context window。
- [ ] 自动 compact 的触发判断使用 `current_context_tokens > effective_context_window - reserve_tokens`。
- [ ] external Codex bridge 继续按现有 `thread/tokenUsage/updated -> TokenUsageUpdated` 路径工作。
- [ ] 覆盖 Rust 单元测试或集成测试，证明 PiAgent mapper 会发 usage event，且 compact/hook stats 能拿到窗口上限。
- [ ] 必要的前端 model/reducer 测试继续通过；若协议生成文件变化，运行 contract check。

## Notes

- 关键参考：`.trellis/spec/cross-layer/backbone-protocol.md`、`.trellis/spec/backend/session/context-compaction-projection.md`、`.trellis/spec/backend/capability/llm-model-config.md`、`references/codex/codex-rs/app-server/README.md`。
- 本任务是跨层修复，需要 `design.md` 与 `implement.md`。
