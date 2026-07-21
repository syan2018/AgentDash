# Dash Complete Agent 与 Clean Agent Core

## 1. Scope / Trigger

本规范适用于 first-party Dash Complete Agent、`DashAgentRepositoryState`、Agent Core、
provider bridge、execution callbacks、native history/context/fork/compaction 与 live event。
修改 Dash source document、Core callback、terminal evidence 或 Complete Agent adapter 时必须复核。

Dash 是 concrete Agent owner：它保存自己的 source/history/effect，并实现平台中立
`CompleteAgentService`。Runtime 与 Product 不复制这些事实。

## 2. Signatures

```rust
pub struct DashAgentRepositoryState {
    pub store: DashAgentStore,
    // source-owned execution/context/surface facts live in native history
}

pub trait DashAgentRepository {
    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError>;
    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError>;
}
```

```rust
pub struct DashAgentCompleteService { /* store + bridge + live channels */ }

impl CompleteAgentService for DashAgentCompleteService {
    async fn create(...);
    async fn resume(...);
    async fn fork(...);
    async fn execute(...);
    async fn read(...);
    async fn changes(...);
    async fn live_events(...);
    async fn inspect(...);
    async fn apply_surface(...);
}
```

```rust
pub trait DashExecutionCallbacks {
    async fn on_event(&self, event: DashExecutionEvent) -> Result<(), DashCoreError>;
}

pub trait DashHistoryCallbacks {
    async fn on_committed(&self, commit: DashHistoryCommit) -> Result<(), DashServiceError>;
}
```

## 3. Contracts

- 一个 Dash source 使用一个 canonical repository document 保存 history、context、branch、
  command/effect state 与 compaction facts。document 内部 CAS 可以使用 owner revision；数据库
  不再拆出 branch/history/command/effect/change 关系镜像。
- `DashSurface` 以 `SurfaceApplied/SurfaceRevoked` native history entry 表达，当前 surface 从
  history fold 得出。repository root 不保存第二个 `surface` 字段。
- source metadata 与 repository描述同一个 concrete Agent source，必须在同一 Dash source
  document/atomic commit 中更新。
- Create 前还没有 source coordinate 的 effect 可以保留独立 `effect_id` lookup receipt。
  receipt 属于 Dash Complete Agent，用于 `inspect`，不进入 Runtime/Host/Product。
- `create/fork/execute/apply_surface` 使用稳定 effect identity。相同 identity + 相同 request
  返回原 receipt；不同 request 返回 typed conflict。
- `read` 从 Dash document 投影 authoritative history/context/lifecycle。`changes` 只发布 Dash
  自己真正保存的 change evidence；平台不能替 Dash 发明 durable cursor。
- Provider/Core 失败以真实 `code + message + retryable` 写入 Dash terminal history并通过
  `read/changes` 原样投影。通用错误文案不能替代 owner evidence。
- source service 打开时必须把真实 `DashExecutionCallbacks` 与 `DashHistoryCallbacks` 绑定到
  source-scoped Complete Agent live sink。未绑定 callback 是 composition error；当前没有
  subscriber 不是错误。
- 成功 CAS 后，`DashHistoryCallbacks` 把本次提交的 exact history suffix 经 canonical projector
  发布为 durable live record；外层原子事务只能在事务提交成功后发布。live 通知失败不改变已提交
  history 的真值，消费者通过重新 `read` 恢复。
- `DashExecutionCallbacks` 只发布 provider/Core 尚未提交的 ephemeral delta。它不补造
  `TurnStarted`、user input 或 terminal lifecycle。
- `AgentLiveEvent.sequence` 只在当前 service process + source 内单调。broadcast lag 返回
  retryable unavailable；消费者重新 `read`，不从 Runtime DB replay。
- `InitialContextInstalled`、`SurfaceApplied` 与 `SurfaceRevoked` 必须从 Agent 实际保存的 native
  history 投影 `Platform(ContextFrameChanged)`；Product intent 或 repository metadata 不能直接
  冒充 Agent 已接纳 context。
- Core 只拥有 provider-neutral inference/stream/tool loop，不依赖 Product workflow、
  Lifecycle、PostgreSQL repository、Codex DTO 或 Runtime persistence。
- Tool/Hook 通过 Host callback route调用真实 handler。Dash 在 callback identity 上重试；
  handler owner负责副作用幂等。
- compaction/fork/context state 属于 Dash source。Product只保存 fork lineage与 source
  association，不复制 checkpoint/history digest作为执行 authority。
- provider protocol terminal决定一次 model response是否完成。transport EOF不能冒充 terminal；
  解码/断流保留 provider分类。

## 4. Validation & Error Matrix

| 场景 | 必须结果 |
| --- | --- |
| source document CAS conflict | reload owner document并按 typed command重试/冲突 |
| effect identity相同且request相同 | 返回原 receipt |
| effect identity相同但request不同 | typed idempotency conflict |
| unsupported input family | Core side effect 前 typed unsupported |
| 空输入 | side effect 前 rejected |
| provider失败 | terminal history保留真实 code/message/retryable |
| source callback未绑定 | composition/configuration error |
| 没有 live subscriber | execution/history commit 正常完成 |
| live subscriber lagged | retryable unavailable；重新 read |
| history commit成功但live通知失败 | commit保持成功；subscriber重新read authoritative history |
| fork cutoff/context digest不匹配 | typed reject；source document不变 |
| transport在provider terminal前EOF | retryable `stream_disconnected` |
| provider terminal后transport继续开放 | 逻辑 response立即完成且只完成一次 |

## 5. Good / Base / Bad Cases

- Good：Dash command 原子更新 source document，成功后把同一 committed suffix 发布为 durable
  live record；Core callback只在其间发布partial delta，重连从同一document read得到完整终态。
- Base：live subscriber掉线，Core继续执行并提交 history；新 subscriber先 read再订阅。
- Bad：Dash 同时写 repository JSONB 与 history/effect镜像，再逐次校验相等。镜像没有独立
  owner，只会制造 drift。
- Bad：生产 composition 注入 Noop execution callback。输入可能执行成功，但用户永远看不到
  live delta。

## 6. Tests Required

- Dash repository测试覆盖首次 create、CAS、restart read、fork、compaction与 owner document
  原子性。
- Complete Agent tests覆盖 create/resume/fork/execute/read/changes/inspect/apply surface及
  stable effect replay/conflict。
- failure tests断言 provider/Core code、message、retryable 在 terminal history与 Agent snapshot
  一致。
- live composition test在执行前订阅，断言顺序为 durable user input、durable `TurnStarted`、
  ephemeral text/reasoning/tool delta 与 durable terminal；执行后从 read断言同 turn 已终态。
- surface/context tests断言 native history保存实际 surface/context，snapshot与live均投影
  `ContextFrameChanged`，repository root不存在平行surface字段。
- lag/no-subscriber tests区分临时没有观察者与 callback未装配。
- source scan/migration test断言 Dash关系镜像表和 production Noop callback缺席。
- Core dependency test断言不依赖 Application/Domain workflow/vendor DTO/repository。

## 7. Wrong vs Correct

```rust
// Wrong: 同一个 source 写两份执行事实。
save_dash_document(&state).await?;
replace_history_rows(&state.store.history).await?;

// Correct: Dash owner document是唯一原子事实。
repository.compare_and_swap(expected, replacement).await?;
```

```rust
// Wrong: live delta缺失时从平台 durable projection恢复。
let replay = runtime_projection.load_changes(source).await?;

// Correct: partial stream失效后读取 concrete Agent authority。
let snapshot = dash_complete_agent.read(AgentReadQuery {
    source,
    at_revision: None,
}).await?;
```
