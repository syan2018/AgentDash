# Hook trace 存储策略实现计划

## Ordered Checklist

1. 定义 HookTrace storage disposition。
   - 位置优先考虑 `agentdash-spi::hooks::trace`，让 runtime-session、executor、companion fallback 共用。
   - 覆盖 durable、ephemeral、drop 三类测试。

2. 收敛 trace 发射入口。
   - `HookRuntimeDelegate::record_trace` 使用统一 disposition。
   - `hub/hook_dispatch.rs` 的 `emit_session_hook_trigger` 使用统一 disposition。
   - `companion/tools.rs` 的 subagent trace 使用统一 disposition。
   - 避免只过滤某一条路径导致 live connector 仍写 durable。

3. 让 ephemeral HookTrace 走现有 session ephemeral lane。
   - 不推进 durable cursor。
   - 进入 ephemeral buffer 供 live/reconnect 当前态使用。
   - 不污染 durable `list_event_page`。

4. 增加无有效 hook 的 skip。
   - 找到最小接口判断当前 trigger 是否有有效 rule/preset/provider 行为。
   - 保留 token stats、pending action、context frame 等非 hook-evaluation bookkeeping。
   - Skip 路径不得隐藏 block/approval/rewrite 等可能存在的规则。

5. 调整前端预期。
   - 保留 `systemEventPolicy.ts` 的 silent hook 防线。
   - 更新或新增测试，证明后端减少 durable 后前端聚合逻辑仍稳定。

6. Spec 更新。
   - 只写稳定设计原因，不写过程性废话。
   - 优先更新 hook runtime/event stream 相关条目。

## Validation Commands

- `cargo test -p agentdash-spi hooks::trace`
- `cargo test -p agentdash-application-runtime-session`
- `cargo test -p agentdash-application-agentrun`
- `cargo test -p agentdash-executor`
- `pnpm run contracts:check`
- `pnpm --filter app-web test -- systemEventPolicy useSessionFeed`
- `pnpm --filter app-web typecheck`

## Review Gates

- 确认 `deny/ask/rewrite/continue/step_advanced` 不会被 ephemeral/drop。
- 确认 `matched_rule_keys / diagnostics / completion / block_reason / injections` 任一存在时仍可历史回放。
- 确认普通空 hook 不再出现在 durable lifecycle session events 统计中。
- 确认没有新增兼容性 fallback 或旧字段保留策略。

## Risky Files

- `crates/agentdash-spi/src/hooks/trace.rs`
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs`
- `crates/agentdash-application-runtime-session/src/session/hub/hook_dispatch.rs`
- `crates/agentdash-application-runtime-session/src/session/eventing.rs`
- `crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- `crates/agentdash-application/src/companion/tools.rs`
- `packages/app-web/src/features/session/model/systemEventPolicy.ts`
