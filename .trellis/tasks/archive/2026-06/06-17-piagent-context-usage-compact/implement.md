# 修复 PiAgent 上下文用量与 compact 链路实施计划

## Checklist

1. 定位 PiAgent 当前生效模型窗口来源。
   - 检查 `PiAgentConnector` bridge 构建与模型切换状态。
   - 找到可传给 mapper / delegate 的 resolved `context_window`。

2. 补齐 native PiAgent usage event。
   - 在 `stream_mapper.rs` 的 `AgentEvent::MessageEnd` 分支提取 assistant `usage`。
   - 构造 Backbone `ThreadTokenUsageUpdatedNotification`。
   - 使用 `TokenUsage::context_input_tokens()` 计算 provider pressure。
   - 写入 `model_context_window`、`effective_context_window`、`reserve_tokens`。

3. 统一 hook / compact stats 窗口。
   - 让 `HookRuntimeDelegate::update_token_stats_from_messages` 能拿到当前模型窗口。
   - 让 `evaluate_compaction` 缺少 provider usage 时仍保留当前模型窗口。
   - 让 `on_before_provider_request` 普通路径传入非零 `context_window`。

4. 检查前端 reducer 与 UI。
   - 确认现有 `extractTokenUsageFromEvent` 能消费补齐后的 payload。
   - 若无需协议字段变化，不改 generated TS。
   - 补充或更新针对 PiAgent usage payload 的前端 model 测试。

5. 更新测试。
   - PiAgent stream mapper 测试：`MessageEnd` with usage 会产生 `TokenUsageUpdated`。
   - hook delegate 测试：token stats 包含模型窗口，`before_provider_request` 不再写入 0 window。
   - 如涉及 protocol generation，运行 contract drift check。

6. 必要时更新 spec。
   - 若实现确认了新的不变量，更新 Backbone Protocol 或 context compaction projection 文档，只记录为什么采用该事实源。

## Validation Commands

```powershell
cargo test -p agentdash-executor pi_agent --lib
cargo test -p agentdash-application session::hook_delegate
cargo test -p agentdash-agent-protocol usage
pnpm run frontend:check
pnpm run contracts:check
```

根据实际改动范围收窄执行，避免无关长测阻塞小规模迭代。

## Risky Files

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent-protocol/src/backbone/usage.rs`
- `packages/app-web/src/features/session/model/types.ts`

## Review Gate

实施前确认以下结论：

- Codex 标准 usage 事件是 `thread/tokenUsage/updated`。
- AgentDash 继续使用 Backbone `TokenUsageUpdated`。
- 不新增 PlatformEvent 表达上下文用量。
- projection API 不承载 provider window 事实。
