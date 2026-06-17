# 修复 PiAgent 上下文用量与 compact 链路设计

## Architecture Boundary

Backbone `TokenUsageUpdated` 是 session runtime 用量事实的唯一事件入口。Codex app-server 的标准做法是通过 `thread/tokenUsage/updated` 单独推送 token usage，而不是把 usage 写入 item lifecycle；AgentDash external Codex bridge 已经沿用该语义。

native PiAgent 应在 `AgentEvent::MessageEnd` 中提取 assistant message usage，并映射为 Backbone `TokenUsageUpdated`。该事件的 payload 使用 AgentDash 自有 normalized context 扩展，以同时服务前端展示和 compact 判断。

## Data Flow

```text
provider bridge stream usage
  -> AgentMessage::Assistant { usage }
  -> AgentEvent::MessageEnd
  -> pi_agent stream_mapper emits BackboneEvent::TokenUsageUpdated
  -> session event persistence / NDJSON
  -> useSessionStream reducer tokenUsage
  -> ContextUsageRing / SessionProjectionView
```

compact 与 hook stats 使用同一口径：

```text
effective runtime model config
  -> model_context_window
  -> HookRuntime metadata / EvaluateCompactionInput or delegate config
  -> ContextTokenStats.context_window / effective_context_window
  -> before_compact / before_provider_request traces
  -> compact threshold
```

## Contracts

`ThreadTokenUsage` 保持现有字段：

- `total`: session 累计 token usage。
- `last`: 最近一次 provider response usage。
- `model_context_window`: 本轮实际模型窗口。
- `context.provider_context_tokens`: 最近一次 provider 可确认的上下文压力。
- `context.pending_estimate_tokens`: provider usage 后新增内容的本地估算。
- `context.current_context_tokens`: 展示和 compact 判断共同使用的当前压力。
- `context.effective_context_window`: 当前策略可用窗口。
- `context.reserve_tokens`: 输出、工具调用或摘要预留。

PiAgent provider usage 中 `TokenUsage::context_input_tokens()` 已把 `input + cache_read_input + cache_creation_input` 合并，应作为 provider-visible pressure。

## Model Window Source

模型窗口必须来自 PiAgent connector 本轮 resolved provider/model runtime state。优先使用当前已绑定 bridge 的模型 metadata，即 provider registry 的 `context_window`。当用户在同一 session 内切换模型时，connector 重建 bridge 后必须让后续 usage/hook stats 使用新模型窗口。

窗口值不从前端 selector、projection token estimate 或累计 token usage 推导。

## Compact Semantics

`evaluate_compaction` 和 `on_before_provider_request` 都必须看到同一非零 `context_window`。`before_provider_request` 发生在普通请求路径上，即使没有触发 compact，也要传入当前模型窗口，使 trace、状态提示和后续 compact 判断连续。

自动 compact 判断继续使用：

```text
context_pressure = current_context_tokens
threshold = effective_context_window - reserve_tokens
```

## Frontend Boundary

前端继续只消费 `TokenUsageUpdated`。`SessionProjectionView` 的 `context_usage` 是 projection 构成分析，不补齐或覆盖 provider usage。上下文圆环和 projection 浮层里的当前/上限来自 `TokenUsageInfo`。

## Compatibility And Migration

无需数据库 migration。改动属于 runtime event projection 和 in-memory hook stats。历史会话没有 token usage event 时仍展示已有 projection；新事件从修复后的运行开始产生。

## Trade-Offs

- 选择复用 `TokenUsageUpdated`，是因为 Codex 和 AgentDash Backbone 都已经把 token usage 定义为独立 thread-level notification；新增 PlatformEvent 会分裂前端 reducer 和 compact 事实源。
- 不把 window 上限放到 projection API，是因为 projection 解释“模型当前可见内容构成”，而窗口和 provider pressure 属于 runtime/provider facts。
