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

```rust
pub trait DashConversationNamer {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError>;
}

pub enum HistoryPayload {
    ThreadNameChanged { thread_name: String },
    // other native history facts
}

pub enum ContextFrameSection {
    ToolSchemaDelta {
        added_tools: Vec<RuntimeToolSchemaEntry>,
        removed_tools: Vec<String>,
        changed_tools: Vec<RuntimeToolSchemaEntry>,
    },
    // other platform presentation sections
}
```

## 3. Contracts

- 一个 Dash source 使用一个 canonical repository document 保存 history、context、branch、
  command/effect state 与 compaction facts。document 内部 CAS 可以使用 owner revision；数据库
  不再拆出 branch/history/command/effect/change 关系镜像。
- `DashSurface` 以 `SurfaceApplied/SurfaceRevoked` native history entry 表达，当前 surface 从
  history fold 得出。repository root 不保存第二个 `surface` 字段。
- `DashSurface` 保存按应用顺序排列的materialized instructions
  `[{ key, channel, text }]`与callable tools。provider system prompt通过拼接该instruction列表
  得出，`ContextFrameChanged`按同一列表的key/channel逐项投影；source history不另存一份扁平
  prompt字符串，presentation也不从prompt正文猜测分片。
- callable tools在accepted surface中保存名称、description、input schema与owner projector。provider
  `tools[]`携带完整机器契约；Dash system prompt中的工具参数摘要从同一列表按需渲染，帮助模型读取
  用途、类型、必填性与关键嵌套字段。`tool_schema_delta`是Native Adapter面向平台UI的变化投影，
  不作为另一条Agent输入通道。
- Dash Core只理解`DashSurface { instructions, tools }`。`ContextFrame`是Native Adapter从
  `SurfaceApplied` history生成的平台展示协议，不进入Dash领域模型，也不作为prompt的另一份输入。
  Adapter以`state_at(sequence - 1)`与当前surface比较：instruction只发布新增/修改项，tool发布
  added/removed/changed真增量；authoritative read、changes与live callback共用同一个projector。
- Product的skill、memory、MCP、workspace与context requirement必须先物化为Agent实际接纳的
  instruction/tool surface，再由Native Adapter按channel映射为Identity、Environment、
  SystemGuidelines、AssignmentContext、CapabilityStateDelta、MemoryContext或UserContext。
  Adapter不读取Product表反补ContextFrame，也不从展示文本反猜Agent输入。
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
- 会话标题是 Dash Agent 从已接纳 user input 与已完成 Agent output 生成的原生展示事实。
  首个成功回合后，Dash 通过 `DashConversationNamer` 生成非空标题，并以
  `ThreadNameChanged` 提交到同一 source history；`read`、`changes` 与 durable live event
  从该 entry 分别投影 `AgentThreadNameSnapshot`、`AgentChangePayload::ThreadNameChanged` 与
  canonical `ThreadNameUpdated`。Product 只消费 Agent snapshot 中的标题，不另存或推导标题。
- 标题生成不是回合 terminal 的组成部分：生成失败不能把已经成功提交的 Agent 回合改写成失败；
  未命名 source 可在后续成功回合再次尝试。history 一旦已有标题，自动命名不再重复调用。
- Core 只拥有 provider-neutral inference/stream/tool loop，不依赖 Product workflow、
  Lifecycle、PostgreSQL repository、Codex DTO 或 Runtime persistence。
- Tool/Hook 通过 Host callback route调用真实 handler。Dash 在 callback identity 上重试；
  handler owner负责副作用幂等。
- callback result以typed text/image/reasoning content parts与structured details穿过Core、Dash event和
  folded state。provider transcript只选择其支持的content，Native Adapter按owner projector生成
  AgentDash ThreadItem；两者不解析序列化callback envelope。
- owner projector固定展示family，不决定工具执行准入。presentation所需的path/command等参数缺失时，
  callback仍到达实际工具owner，由owner的typed validation result形成同一ToolResult。
- compaction/fork/context state 属于 Dash source。Product只保存 fork lineage与 source
  association，不复制 checkpoint/history digest作为执行 authority。
- provider protocol terminal决定一次 model response是否完成。transport EOF不能冒充 terminal；
  解码/断流保留 provider分类。

## 4. Validation & Error Matrix

| 场景 | 必须结果 |
| --- | --- |
| source document CAS conflict | reload owner document并按 typed command重试/冲突 |
| 同surface digest重放但instruction列表不同 | typed idempotency conflict；迁移必须从已提交callback surface恢复精确列表 |
| 连续surface只增加一个tool | 只发布该tool的`added_tools`，不重放完整当前schema |
| tool同名但description/schema变化 | 发布到`changed_tools`，不同时出现在`added_tools` |
| tool从surface消失 | tool name进入`removed_tools` |
| surface revision变化但instruction/tool语义未变 | 不发布伪ContextFrame delta |
| effect identity相同且request相同 | 返回原 receipt |
| effect identity相同但request不同 | typed idempotency conflict |
| unsupported input family | Core side effect 前 typed unsupported |
| 空输入 | side effect 前 rejected |
| provider失败 | terminal history保留真实 code/message/retryable |
| source callback未绑定 | composition/configuration error |
| 没有 live subscriber | execution/history commit 正常完成 |
| live subscriber lagged | retryable unavailable；重新 read |
| history commit成功但live通知失败 | commit保持成功；subscriber重新read authoritative history |
| 命名输入缺少非空user input或Agent output | 不生成标题，不提交history entry |
| namer返回空标题 | 拒绝标题提交；已成功回合保持成功 |
| namer/provider失败 | 不提交标题；已成功回合保持成功，后续成功回合可重试 |
| history已有标题 | 不调用namer，不重复提交自动标题 |
| fork cutoff/context digest不匹配 | typed reject；source document不变 |
| transport在provider terminal前EOF | retryable `stream_disconnected` |
| provider terminal后transport继续开放 | 逻辑 response立即完成且只完成一次 |

## 5. Good / Base / Bad Cases

- Good：Dash command 原子更新 source document，成功后把同一 committed suffix 发布为 durable
  live record；Core callback只在其间发布partial delta，重连从同一document read得到完整终态。
- Good：首个成功回合提交 terminal 后，Dash 根据该 source 的原生对话生成标题并追加
  `ThreadNameChanged`；列表标题、snapshot与live notification均来自这一个 entry。
- Good：Product提交skills/MCP/memory instructions与tool surface，Dash原样保存；Native Adapter从
  同一`SurfaceApplied` entry生成多种typed ContextFrame和真实tool delta，前端只展示变化摘要。
- Base：live subscriber掉线，Core继续执行并提交 history；新 subscriber先 read再订阅。
- Base：同一surface幂等重放不产生新的history entry，因此不产生重复ContextFrame。
- Base：标题生成暂时失败，回合仍保持成功且source保持未命名；下一成功回合可以再次生成。
- Bad：Dash 同时写 repository JSONB 与 history/effect镜像，再逐次校验相等。镜像没有独立
  owner，只会制造 drift。
- Bad：Product 从首条用户消息截断标题并写入另一张表。该标题无法证明是 Agent 接纳的事实，
  并会与 Agent snapshot 形成双写。
- Bad：生产 composition 注入 Noop execution callback。输入可能执行成功，但用户永远看不到
  live delta。
- Bad：把`ContextFrame`塞进Dash或在每个`SurfaceApplied`上把完整tools数组写成`added_tools`；前者
  让展示协议反向污染Agent领域，后者让snapshot历史看起来像能力不断重复新增。

## 6. Tests Required

- Dash repository测试覆盖首次 create、CAS、restart read、fork、compaction与 owner document
  原子性。
- Complete Agent tests覆盖 create/resume/fork/execute/read/changes/inspect/apply surface及
  stable effect replay/conflict。
- failure tests断言 provider/Core code、message、retryable 在 terminal history与 Agent snapshot
  一致。
- live composition test在执行前订阅，断言顺序为 durable user input、durable `TurnStarted`、
  ephemeral text/reasoning/tool delta 与 durable terminal；执行后从 read断言同 turn 已终态。
- surface/context tests断言 native history保存实际instruction边界与tools，provider prompt由
  instructions派生，snapshot与live逐项投影`ContextFrameChanged`，repository root不存在平行
  surface字段。
- surface delta tests连续应用三版surface，覆盖tool新增、修改、删除，断言read重放只返回真实
  变化且`rendered_text`不包含原始JSON schema；context channel矩阵覆盖system、identity、
  workspace、workflow、skills、MCP、memory与user context的typed frame映射。
- naming test断言成功回合把 accepted user input与最终Agent output交给namer，只提交一次非空
  `ThreadNameChanged`，且 `read/changes/live` 均投影同一标题；命名失败不改变回合terminal。
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
// Wrong: Product根据输入另建标题事实。
product_thread_names.upsert(run_id, infer_title(prompt)).await?;

// Correct: Dash保存原生标题，平台只投影Agent证据。
store.commit(HistoryPayload::ThreadNameChanged { thread_name })?;
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

```rust
// Wrong: 把当前完整snapshot伪装成每次都新增的tool delta。
let added_tools = surface.tools.iter().map(runtime_tool_schema_entry).collect();

// Correct: Native Adapter比较同一source history中的相邻surface事实。
let previous = history.state_at(entry.sequence - 1)?.surface;
let (added_tools, removed_tools, changed_tools) = diff_tools(previous, &surface);
```
