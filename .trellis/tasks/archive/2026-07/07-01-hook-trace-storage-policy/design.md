# Hook trace 存储策略设计

## Problem Statement

Hook evaluation 的存在不等于 session audit fact。当前系统把大量“评估过但没有改变任何行为”的 HookTrace 包装成 durable `PlatformEvent::HookTrace`，导致 session event log 膨胀，并把 UI 已判定为 silent 的 hook 噪音留在仓储层。

## Boundaries

- `agentdash-spi::hooks` 负责定义 `HookTraceEntry` 及 storage disposition 的可复用判断。
- `agentdash-application-runtime-session` 负责在 Hub fallback、eventing durable/ephemeral 判定中使用同一策略。
- `agentdash-application-agentrun` / executor connector 负责避免 live trace 广播绕过后端 disposition。
- `app-web` 继续负责展示策略，但不再承担仓储降噪职责。

## Disposition Model

新增一个后端统一分类，名称可由实现时确定：

```text
HookTraceDisposition =
  Durable      # audit / behavior / lifecycle fact
  Ephemeral    # live/debug only, no durable append
  Drop         # no event
```

推荐判定：

| Condition | Disposition |
| --- | --- |
| `block_reason` 非空 | Durable |
| decision in `deny/ask/rewrite/continue/step_advanced` | Durable |
| `refresh_snapshot = true` | Durable |
| `completion` 非空 | Durable |
| `diagnostics` 有实质内容 | Durable |
| `matched_rule_keys` 非空但最终不改变行为 | Ephemeral |
| `matched_rule_keys` 非空且伴随行为变化、diagnostic、completion、block、refresh 或 injection | Durable |
| `injections` 非空 | Durable |
| 完全空 `noop/allow/observed/effects_applied/stop/terminal_observed` | Drop |
| 命中规则但最终不改变行为的 silent trace | Ephemeral |

## Data Flow

1. Hook evaluation 产出 `HookResolution`。
2. 调用点构造 `HookTraceEntry` 前先判断是否需要 trace；完全空决策直接不构造或构造后 drop。
3. 需要保留的 trace 进入 runtime memory ring。
4. 只有 `Durable` trace 被转换成 durable `PlatformEvent::HookTrace`。
5. `Ephemeral` trace 使用现有 ephemeral event lane，live 可见，不写 durable session_events。

## Runtime Skip Strategy

Skip 是性能优化，不是存储优化，应该独立建模：

- 没有 `hook_runtime` 时，不 attach hook delegates/facets；现有 `Option<SharedHookRuntime>` 路径应保持这个语义。
- 有 hook runtime 但 provider/snapshot 能证明某 trigger 无有效规则时，不调用 Rhai/script evaluation。
- `BeforeProviderRequest` 的 token stats 更新、`UserPromptSubmit` 的 pending action / notice 消费、context frame delivery 这类 hook 之外的 bookkeeping 不随 skip 消失。
- Provider 层可暴露轻量 query，例如 `has_effective_trigger(trigger, target)` 或在 snapshot 上预计算 trigger bitmap。实现时优先选择能被测试稳定证明的最小接口。

## Spec Hygiene

本任务完成后只在 spec 中记录稳定原因：

- HookTrace durable log 只保存审计/行为事实，因为 RuntimeSession event log 是长期回放和追责材料。
- Empty/silent hook 属于运行时诊断，因为它不解释用户可见行为或 lifecycle 变化。
- 无有效 hook 的 trigger 应短路，因为脚本评估和 trace 包装不应成为普通会话固定开销。

不记录“以前怎么错”“本任务删了哪些噪音”这类一次性描述。

## Risks

- 过度 drop 会影响调试。缓解方式是把“命中规则但没有改变行为”的 trace 先放 ephemeral，而不是直接删除。
- 如果只改 Hub fallback，不改 live connector 广播，仍会 durable append。必须覆盖 `append_trace -> trace_broadcast -> connector -> eventing` 这条路径。
- 如果直接把所有 HookTrace 加入 `is_ephemeral_event`，会误伤 block/ask/rewrite 等历史审计。必须先分类。

## Validation Focus

- Storage disposition unit tests。
- Session eventing durable vs ephemeral backlog tests。
- Connector/live stream 不绕过 disposition 的测试。
- Frontend silent hook aggregation tests 回归。
