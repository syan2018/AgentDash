# Hook trace 存储策略收口

## Goal

降低 RuntimeSession durable event log 中由空 hook、no-op hook、纯观测 hook 造成的事件膨胀。HookTrace 只有在解释执行行为、审计事实或 lifecycle 变化时才进入 durable session_events；仅服务当前调试的 hook 决策应走 live/ephemeral 或直接不发。

## User Value

- 会话事件仓储更小，历史回放和 lifecycle 事件视图不被 hook 心跳污染。
- Hook 审计保留真正有价值的行为解释，例如 block、ask、rewrite、continue、completion、diagnostic、context injection。
- 没有加载 hook 或没有有效规则的触发点不进入完整 hook evaluation，减少普通会话运行时开销。

## Confirmed Facts

- 截图中的 lifecycle session events 有 1087 条，其中 `platform` 598 条，已超过总事件的一半。
- `HookRuntimeDelegate::record_trace` 会把 `noop`、`allow`、`effects_applied`、`observed`、普通 `stop` 等决策写入 `HookTraceEntry`。
- `AgentFrameHookRuntime::append_trace` 同时写入 200 条内存 ring 并广播 trace；Pi Agent connector 会把广播转换成 `PlatformEvent::HookTrace`。
- session eventing 当前只把 delta/progress/provider attempt status 等事件视为 ephemeral；`PlatformEvent::HookTrace` 默认 durable append。
- 前端 `systemEventPolicy.ts` 已把 `noop/allow/observed/effects_applied/stop` 等 hook decision 当作 silent，只是不渲染，不减少后端仓储。
- 后端 hook spec 已要求纯噪音 trace 不强制发入事件流；带 `matched_rule_keys / diagnostics / completion / block_reason` 的 trace 必须发。
- 代码证据见 `research/hook-trace-storage-evidence.md`。

## Requirements

1. HookTrace 必须有统一的 storage disposition，而不是由每个调用点各自决定是否包装成 platform event。
2. Durable HookTrace 只保存具备审计价值或行为解释价值的决策：
   - `block_reason` 非空。
   - decision 改变执行行为，例如 `deny`、`ask`、`rewrite`、`continue`、`step_advanced`。
   - `refresh_snapshot = true`。
   - `completion` 非空。
   - `diagnostics` 有实质内容。
   - `matched_rule_keys` 非空。
   - `injections` 非空，或同步产生 context frame / pending action / turn-start notice。
3. 完全空的 hook 决策不进入 durable session_events。空决策指没有 matched rule、diagnostic、completion、block、refresh、injection、effect，也没有改变执行流。
4. 仅用于当前会话调试但不应长期保存的 hook trace 可走 ephemeral event，不推进 durable cursor，不进入 durable backlog。
5. 没有 hook runtime、没有有效 hook provider、或 provider 能证明当前 trigger 没有有效规则时，应跳过完整 hook evaluation。
6. Skip 不得跳过 hook 之外的必要 bookkeeping，例如 token stats 更新、pending action 消费、context frame delivery。
7. 前端保留静默 hook 展示策略，但后端是 durable/ephemeral/drop 的权威来源。
8. Spec 更新只记录稳定设计原因，不记录本任务过程、旧错误实现或自动生成式流水账。

## Out Of Scope

- custom hook preset API 的持久化、展示和编辑能力。
- 重新设计 hook rule DSL 或 Rhai 脚本能力。
- 改动 session event 表结构或做存量数据清理。
- 调整 token usage、item lifecycle、turn lifecycle 等非 HookTrace 事件。

## Acceptance Criteria

- [ ] 普通会话中空 `before_provider_request:observed` 不进入 durable session_events。
- [ ] 空 `before_tool:allow`、空 `after_tool:effects_applied`、空 `after_turn:noop`、无 gate 的 `before_stop:stop` 不进入 durable session_events。
- [ ] 带 `matched_rule_keys`、`diagnostics`、`completion`、`block_reason`、`refresh_snapshot` 或 `injections` 的 HookTrace 仍进入 durable session_events。
- [ ] `deny`、`ask`、`rewrite`、`continue`、`step_advanced` 仍作为 durable HookTrace 可被前端历史回放看到。
- [ ] ephemeral HookTrace 如被保留，必须使用现有 `ephemeral_event` lane，不推进 durable resume cursor。
- [ ] 无 hook runtime 或无有效 trigger rule 的路径不会调用脚本引擎进行空评估。
- [ ] 相关 Rust tests 覆盖 durable/ephemeral/drop 三类 disposition。
- [ ] 相关前端 tests 仍证明静默 hook 不切断工具 burst，且显著 hook 仍可展示。
- [ ] `pnpm run contracts:check` 和相关 Rust/frontend 测试通过。

## Decision

采用推荐策略：完全空 hook 直接 drop；命中过规则但最终是 `allow/noop/observed` 等不改变行为的 silent trace 走 ephemeral；所有 actionful/auditable trace durable。
