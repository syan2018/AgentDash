# Research: Codex session/history/runtime boundaries

- Query: Codex Thread/Session/Turn/Item/history/context 的真实权威边界是什么；统一 Agent Runtime 应如何在保留 canonical contract/snapshot/change 的同时接入完整 Codex Agent；07-17 将 Codex 视为 `AgentExecutionPort` stateful replica、并把全部状态收进单个 `AgentSession` aggregate 的假设是否成立。
- Scope: internal
- Date: 2026-07-17

## Findings

### 1. 结论

07-10 定义的统一 Agent Runtime 外层应保留，但必须区分两种实现形态：

1. AgentDash 自有 Agent：Runtime 内部可以使用自有 transition kernel、normalized entities、typed `ContextRevision`、effect ledger 与 transactional `AgentChange` outbox。
2. Codex、pi-coding-agent 一类完整 Agent：Integration/Driver seam 接入的是完整 Agent service，不是只负责模型/tool loop 的执行副本。平台仍维护自己的 canonical contract、snapshot/change、AgentRun 映射和产品策略，但不能接管完整 Agent 内部的 conversation admission、model context、rollout reconstruction、fork、compaction 安装与 recovery truth。

因此，07-17 的 `HostedAgentGateway::{execute, read, changes}` 外层形状可以保留；问题在于其下游把所有实现都降为 `AgentExecutionPort`，并断言 `AgentSession` 对所有 Thread/Turn/Item/Context 都是唯一权威。该假设适用于 AgentDash 自有 Agent 实现，不适用于 Codex 这种已经联合实现了中层 Agent 与自有宿主能力的完整 Agent。设计原文把 Native、Codex、Remote 全部定义为只返回 receipt/observation 的 adapter（`.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:16`），并把 Codex native session 定义成平台 transcript 的 replica（同文件 `:774-779`、`:907-909`）；这会迫使平台复制 Codex 已经拥有的状态机、context 与恢复语义，形成双重权威。

正确修正不是删除统一 Runtime，而是把 seam 分成：

- **统一外层 Runtime**：拥有 AgentDash canonical vocabulary、产品级 command gate、offer/profile admission、AgentRun/source identity mapping、binding generation、平台 operation/idempotency、canonical snapshot/change feed 与 UI/protocol projection。
- **完整 Agent service adapter**：拥有其 native session/history/context/turn/task/tool/hook/compaction/fork/recovery 语义；通过有限 typed command 精确改变，通过 read/notification 映射为平台 canonical snapshot/change。
- **AgentDash 自有 Agent kernel**：只是完整 Agent service 的一种实现；其内部才使用 07-17 的 aggregate、normalized tables、`ContextRevision`、effect/observation coordination。

统一的是语义词汇和平台接口，不是所有 Agent 实现的存储模型与内部 transition kernel。

### 2. Codex 的权威链

#### 2.1 ThreadStore、rollout JSONL 与 SQLite

**Codex 事实**

- `ThreadStore` 是 Codex 的 thread persistence boundary；`append_items` 是 raw canonical history append API，metadata 只能通过独立的 `update_thread_metadata` 写入（`references/codex/codex-rs/thread-store/README.md:9-18`）。
- 本地实现把 canonical history 写入 rollout JSONL，把可查询 metadata 写入 SQLite（`references/codex/codex-rs/thread-store/README.md:22-30`）。
- writer 明确先 durable-write JSONL，再 materialize SQLite；注释将 SQLite 定义成 rebuildable view，允许落后但不能领先 canonical history（`references/codex/codex-rs/thread-store/src/local/live_writer.rs:301-307`）。
- `ThreadStore::load_history` 服务于 resume/fork/rollback/memory；`load_latest_model_context` 允许实现只读取重建最新 model context 所需的 suffix，也允许不支持 targeted read 的实现回退为全量历史（`references/codex/codex-rs/thread-store/src/store.rs:74-92`）。
- paginated rollout 的最新 model context 通过反向扫描找到 replacement-history checkpoint 与恢复 metadata；legacy/compressed rollout 才走全量历史路径（`references/codex/codex-rs/thread-store/src/local/model_context.rs:26-35`、`:61-75`）。

**可迁移原则**

- canonical recovery log 与查询 projection 可以分离。
- projection 必须可重建，且 durability ordering 不能让 projection 超前于 canonical log。
- “展示历史”“可查询 metadata”“模型恢复输入”是三个不同读模型，不应由一个表或 DTO 假装等价。

**AgentDash 建议**

- 对 AgentDash 自有 Agent，normalized tables + outbox 可以成为 canonical aggregate storage。
- 对 Codex，平台 normalized tables 只能保存平台 canonical projection、source identity 与 integration state；不能替代 Codex rollout/ThreadStore，也不能成为 Codex resume/fork/compaction 的恢复依据。

**不可直接推论**

- Codex 使用 JSONL canonical log 不意味着 AgentDash 自有 Agent 也必须使用 JSONL。
- `ThreadStore` 是 storage-neutral trait 不意味着平台应实现或接管 Codex `ThreadStore`。
- SQLite 是 Codex 内部 rebuildable view，不意味着平台 projection 与 Codex SQLite 是同一数据库或同一事实源。

#### 2.2 LiveThread、Session、ContextManager 与 ActiveTurn

**Codex 事实**

- `LiveThread` 是 active thread persistence handle；session 只依赖该 handle，不需要知道 local file 或 remote store（`references/codex/codex-rs/thread-store/src/live_thread.rs:29-38`）。
- `LiveThread::append_items` 先按 persistence policy 持久化 canonical items，再由上层派生并同步 metadata（`references/codex/codex-rs/thread-store/src/live_thread.rs:184-240`）。
- `Session` 是“initialized model agent”，同一 session 至多一个 running task，持有 `thread_id`、state、realtime conversation、active turn、input queue、guardian 与 services（`references/codex/codex-rs/core/src/session/session.rs:25-48`）。
- `SessionConfiguration` 同时持有 provider、instructions、compact prompt、approval/permission、source/history mode、fork provenance、parent、dynamic tools 等配置（`references/codex/codex-rs/core/src/session/session.rs:51-108`）。
- `SessionState` 持有 `ContextManager` history、previous turn settings、compaction window、connector selections、permissions 与其他 session-scoped 状态（`references/codex/codex-rs/core/src/state/session.rs:25-46`）。
- `ContextManager` 保存 model-visible transcript；compaction 或 rollback 重写 history 时递增 `history_version`（`references/codex/codex-rs/core/src/context_manager/history.rs:36-57`、`:199-204`）。`for_prompt` 还会做模型输入归一化、call/output 配对修复和不支持图片过滤（同文件 `:120-154`、`:355-368`），所以 presentation transcript 不等于 exact model input。
- `ActiveTurn` 持有 running task 与 `TurnState`；task kind 包含 Regular、Review、Compact；turn state 还持有 approvals、permissions、user input、MCP elicitation、dynamic tools、mailbox 与 token/tool 状态（`references/codex/codex-rs/core/src/state/turn.rs:29-100`）。
- `SessionServices` 聚合 MCP runtime、exec manager、hooks、auth、skills/plugins、AgentControl、state DB、`LiveThread`、`ThreadStore`、`ModelClient` 等服务（`references/codex/codex-rs/core/src/state/service.rs:50-104`）。

**可迁移原则**

- Agent session truth 不只是 transcript；还包括 active task、pending interactions、configuration、tool/hook services、model-visible normalization baseline 与 recovery metadata。
- exact model context 必须由实际执行 Agent 的 context manager 定义，不能从 App Server 展示 Item 反向解析。

**AgentDash 建议**

- Codex adapter 的 context profile 应声明 “Agent-owned exact / platform-observed opaque or event-projected”。除非 Codex 明确导出 exact context read/digest/apply API，否则平台不能声称自己的 typed `ContextRevision` 与 Codex 模型实际看到的输入完全相同。
- 平台可把 Thread/Turn/Item/Interaction 映射为 canonical read model，但 source IDs 保持 opaque/stable，另建 AgentRun ↔ source coordinate 映射。

**不可直接推论**

- Codex 内部存在 `history_version` 不等于 App Server 对外提供了稳定、可持久化、可用于 gap replay 的 context revision。
- Codex Session 中包含宿主服务，不代表这些服务都应纳入 AgentDash 平台 aggregate；它只证明 Codex 是完整 Agent 实现，而不是纯 execution core。

### 3. Resume、fork、rollback 与 recovery

#### 3.1 Resume

**Codex 事实**

- resume 从 stored rollout 形成 `InitialHistory::Resumed`，再由 `apply_rollout_reconstruction` 恢复 history、previous settings、reference context、world state 与 compaction window（`references/codex/codex-rs/core/src/thread_manager.rs:778-845`、`references/codex/codex-rs/core/src/session/mod.rs:1345-1384`）。
- reconstruction 反向扫描 rollout；找到最新 surviving replacement checkpoint 与 resume metadata 后，旧 prefix 不再需要（`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:112-190`、`:286-324`）。
- replay 对 `Compacted` replacement 执行 history replace，对 `ThreadRolledBack` 执行逻辑丢弃 turn，对 WorldState/TurnContext 恢复对应 baseline（同文件 `:325-421`）。
- paginated thread resume 使用 `load_latest_model_context` suffix，legacy history 才读取全量（`references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:3448-3488`）。

**AgentDash 建议**

- Runtime host 可以拥有进程重连、binding generation、transport retry 与 source reattachment；Codex session 的 recovery 必须调用 Codex read/resume，由 Codex 自己重建 rollout，不应由平台 normalized transcript replay 成 Codex 内部状态。
- 平台 projection gap 应重新读取 Codex thread snapshot/resume bootstrap，并重建平台 snapshot；这不是用平台 snapshot 恢复 Codex。

#### 3.2 Fork

**Codex 事实**

- fork 从 store history 或已加载 history 构造 `ForkSnapshot`，生成全新 thread，并保留 source lineage（`references/codex/codex-rs/core/src/thread_manager.rs:978-1090`）。
- child 的 inherited model context 会在创建 `LiveThread` 时直接持久化（`references/codex/codex-rs/thread-store/src/live_thread.rs:110-143`）；forked thread 也会在启动后立即 materialize rollout（`references/codex/codex-rs/core/src/session/mod.rs:1320-1331`）。
- protocol 支持按 `last_turn_id` inclusive 或 `before_turn_id` exclusive 截断；最后 turn 仍进行中时不能使用普通 `last_turn_id` fork（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:495-590`）。
- interrupted snapshot 会给 child 附加明确的 interrupted/TurnAborted boundary，避免把未完成 suffix 当作正常完成历史（`references/codex/codex-rs/core/src/thread_manager.rs:1915-1989`）。

**可迁移原则**

- fork 是创建新 Agent session/thread 的一等命令，不是复制展示消息。
- fork contract 必须描述 source、cutoff、lineage、mid-turn policy、new source identity、accepted 与 terminal。

**AgentDash 建议**

- fork 保持 P0 capability。外层 Runtime 做产品授权、profile/cutoff gate、platform operation 与 AgentRun/source mapping；Codex adapter 把命令精确翻译为 Codex fork，并将新 source thread 绑定为新 canonical session。
- 不应由平台先在 `AgentSession` normalized tables 中复制 transcript，再把复制结果 `ApplyContextRevision` 给 Codex replica；这会丢失 replacement checkpoint、world/context baseline、mid-turn marker 与 Codex 自有 lineage 语义。

#### 3.3 Rollback

**Codex 事实**

- rollback 要求没有 active turn；先读取 persisted rollout，把 `ThreadRolledBack` marker 追加到 replay，运行 reconstruction，再持久化 marker（`references/codex/codex-rs/core/src/session/handlers.rs:461-563`）。
- reconstruction 对 marker 执行 `drop_last_n_user_turns`（`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:365-367`）。
- protocol 明确 rollback 只改 history，不回滚本地文件变化；响应中的 turns 也是 lossy（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:1071-1090`）。

**可迁移原则**

- append-only history 可以通过逻辑 marker 表达 rollback/rewrite，无需物理改写历史。
- “会话历史 rollback”与“工作区副作用 rollback”必须分开。

**不可直接推论**

- Codex rollback 当前被标记 deprecated，不能据此把 rollback 放进统一 P0 contract；fork 的证据和稳定性更强。

### 4. Compaction：生成与安装的边界

**Codex 事实**

- compaction 是一个正式 `SessionTask`，task kind 为 `Compact`；实现会按 provider/feature 选择 token-budget、remote-v2、remote 或 local compact 路径（`references/codex/codex-rs/core/src/tasks/compact.rs:15-81`）。
- local compaction 创建 `ContextCompactionItem`、发出 item started，把 synthetic summarization prompt 加入克隆 history 后调用 model（`references/codex/codex-rs/core/src/compact.rs:221-269`）。
- model 返回 summary 后，Codex 构造 replacement history 与 `CompactedItem { replacement_history }`，调用 `replace_compacted_history`，重算 token usage，再发 item completed（同文件 `:323-377`）。
- remote compaction 复用 `ContextCompactionItem` ID 作为 protocol lifecycle、endpoint attempt 与 installed checkpoint 的 join key（`references/codex/codex-rs/core/src/compact_remote.rs:194-215`）。
- remote path 明确把 “install” 定义成 endpoint output 成为 live thread history 的 semantic boundary，然后才调用 `replace_compacted_history`（同文件 `:275-300`）。
- remote output 还会过滤过期 developer/context wrapper，并重新注入当前 session 的 canonical context（同文件 `:305-353`）。
- `replace_compacted_history` 先替换内存 state，再持久化带完整 replacement history 的 `RolloutItem::Compacted`，随后持久化 WorldState 与 TurnContext baseline（`references/codex/codex-rs/core/src/session/mod.rs:2979-3023`）。
- persistence error 仅记录日志，不向调用者传播（同文件 `:3461-3465`）。因此 Codex 当前“内存安装后持久化”的顺序不是可直接复制的 transactional aggregate 范本。

**可迁移原则**

- summary generation 只产生候选；context install 才是影响未来模型输入的 semantic boundary。
- compaction 应有 task/turn/item lifecycle、稳定 join identity、replacement checkpoint 和恢复规则。
- 展示历史与 future model context 可分离：已发生业务历史不必被物理删除，新的 context head 决定未来模型输入。

**AgentDash 建议**

- 对 AgentDash 自有 Agent，07-17 的 `ContextRevision` candidate/install、transactional state + outbox、continuation queue 可以采用。
- 对 Codex，`Compact` 是完整 Agent command：Runtime 记录 platform operation，adapter 提交 `thread/compact/start`，再从 Codex item/turn/read 映射 accepted/running/terminal 与新的 observed context state。summary generation、replacement history 和 apply 都由 Codex 内部完成。
- 对外 context snapshot 应保存 fidelity；没有 exact context API 时，只能记录 `Opaque`/`EventProjected` 和 compaction/window observation，不能伪造平台可重放的 exact `ContextRevision`。当前 contract 已有 `Opaque`、`EventProjected`、`AgentReplay`、`DriverExact` 梯度（`crates/agentdash-agent-runtime-contract/src/profile.rs:92-96`），可继续用于分级。
- automatic overflow continuation 对完整 Codex 默认是 Agent-native policy；平台只观察结果。只有实现明确提供可控 capability 时，外层 Runtime 才把它提升为 canonical queue policy。

**不可直接推论**

- Codex 注释中的 install semantic boundary 不证明其持久化具备 AgentDash 07-17 要求的同事务原子性；当前源码恰好显示 state mutation 先于 best-effort persistence。
- `CompactedItem.replacement_history` 可用于 Codex 自己的 reconstruction，不等于 App Server 向平台公开了 exact context revision 的读写接口。

### 5. Codex layering 不是可直接拆成 `AgentExecutionPort` 的层次

观察到的实际层次是：

1. **Provider/model client**：TurnContext 持有 provider/model/tool/inference 配置。
2. **Core session/task/context**：Session、ActiveTurn、SessionTask、ContextManager、compaction、rollout reconstruction。
3. **ThreadStore/LiveThread**：canonical rollout、metadata、resume/fork storage boundary。
4. **App Server protocol/service**：Thread/Turn/Item read model、commands、interactions、notifications、live connection。
5. **Codex product/runtime services**：MCP、tools、hooks、permissions、skills/plugins、auth、AgentControl 等均已在 SessionServices 内联合运行。

这不是一个只有 provider/tool loop 的“低层 driver”。`SessionServices` 的范围（`references/codex/codex-rs/core/src/state/service.rs:50-104`）与 Session/Turn state（`references/codex/codex-rs/core/src/session/session.rs:25-108`、`references/codex/codex-rs/core/src/state/turn.rs:29-100`）说明 Codex 对外应被视作完整 Agent service。

07-10 的外层依赖方向仍成立：Application 不理解 vendor protocol，Runtime contract 使用 AgentDash-owned vocabulary，Integration 负责 binding/source mapping/adapter。需要修正的是 “Driver” 的能力层级：

- `AgentExecutionPort` 仅适合 AgentDash 自有 Agent kernel 内部的 execution provider。
- Codex Integration adapter 应实现更高层的 complete Agent service port；它可以仍位于 Integration/Driver host seam 下，但不能被要求接受平台生成的每个 model-context revision 或只返回低层 execution observation。

### 6. 外层 complete Agent service seam

建议把 07-17 的 `HostedAgentGateway` 作为 Application-facing outer seam 保留，并在 Integration 层定义可分级的完整 Agent service contract：

```text
describe() -> AgentOffer / AgentCapabilityProfile / fidelity
execute(AgentCommandEnvelope) -> AgentCommandReceipt
read(AgentReadQuery) -> AgentReadResult
changes(AgentChangeSubscription) -> optional AgentChangeStream
respond(AgentInteractionResponse) -> AgentInteractionReceipt
```

关键语义：

- `execute` 至少包含 start/resume/fork、turn start/steer/interrupt、compact；management command 如 archive/delete 可作为独立 capability。
- receipt 必须区分 platform accepted、source accepted 与 terminal result，不能把 RPC success 当成 turn/compact/fork 完成。
- `read` 返回 canonical mapped snapshot，同时标注 source、fidelity、snapshot consistency/cursor；presentation transcript 与 exact model context 是不同 query。
- `changes` 是能力分级项，不应假设所有完整 Agent 都提供 durable revision tail。gap 或 reconnect 后必须重新 `read` source snapshot。
- tool/approval/permission/user-input/MCP elicitation/dynamic-tool callback 使用稳定 correlation ID 与 typed response；同一 interaction 只能 terminalize 一次。
- source Thread/Turn/Item ID 保持 opaque，平台 canonical ID 与 source ID 映射单独持久化，不能让 adapter 重编号后再作为 fork/context identity。
- capability profile 必须明确 fork cutoff modes、context fidelity、compaction semantics、interaction kinds、tool surface、hook contribution、change replay/cursor 和 settings mutability。

#### 6.1 只读映射、有限命令与平台独立状态

| 类别 | Codex 内部状态/能力 | 平台处理 |
| --- | --- | --- |
| 只读映射 | Thread status、forked-from/parent、turn status、typed items、error/timestamp、hook started/completed、live notifications | 映射为 canonical snapshot/change；source 仍是 Codex |
| 只读但非 exact context | App Server transcript、turn/item history | 标注 presentation/event-projected fidelity；不可宣称等于 model prompt |
| Agent-owned exact | `ContextManager`、history normalization、replacement checkpoint、world/reference baseline、active task/input queue、rollout reconstruction | 不镜像成可写副本；通过 Agent read/resume/commands 间接观察 |
| 精确 command | thread start/resume/fork、turn start/steer/interrupt、thread compact、archive/unarchive（若 profile 声明） | platform policy gate 后提交 source；记录 accepted 与 terminal |
| 精确 interaction | approval、permission、user input、MCP elicitation、dynamic tool response | typed correlation channel；不直接写 TurnState |
| tool surface | dynamic tool specs 与 callback 是显式协议能力；普通内置/MCP tools 由 Codex session 管理 | binding/start 时提供 finite contribution；运行期变更必须由 profile 声明 |
| hooks | Codex 内部持有 hooks；App Server 发 hook started/completed notification（`references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:1025-1042`），managed hooks 通过 config requirements 描述（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/config.rs:423-455`） | 默认是 binding/config contribution + read notification，不定义任意 live “改写 hook state” command |
| 平台独立持久化 | AgentRun/source mapping、binding/service instance/generation、offer/profile digest、product authorization、mailbox/availability、operation/idempotency、dispatch receipt、projection revision/cursor/gap、canonical change outbox、tool/hook contribution revision | 平台事实；不能写回 Codex rollout 冒充 source truth |

App Server 的 Thread read model 本身也说明其是映射层：Thread 含 `session_id`、`forked_from`、`parent`、preview、history mode、provider、status，turns 只在特定 read/resume/fork 场景填充（`references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:170-225`）；Turn 包含 items/status/error/timestamps（同文件 `:231-250`）。它适合平台 snapshot projection，不是 exact provider context API。

### 7. 对 07-17 机制的逐项评估

| 机制 | AgentDash 自有 Agent | Codex 完整 Agent adapter | 结论 |
| --- | --- | --- | --- |
| normalized `agent_session/turn/item/context` tables | 可作为 canonical aggregate storage | 只存平台 canonical projection、source mapping 与 integration metadata；不可替代 Codex rollout | 保留，但限定 owner/scope |
| 单一 `AgentSession` aggregate | 可拥有本实现的 command admission、context、compaction、recovery | 不能拥有 Codex 内部 context/task/history/recovery；只能拥有平台映射与产品约束 | 07-17 对所有实现统一套用属于过度 |
| transactional `AgentChange` outbox | state mutation 同事务产出，适合作为 authoritative platform feed | 可对“已映射的 Codex observation + 平台事实”生成 durable canonical change，但不是 Codex source recovery log | 保留外层 feed，禁止反向推断 source truth |
| typed `ContextRevision` | 可作为 exact model input contract | Codex 未公开 exact context read/apply 时只能记录 fidelity + opaque source checkpoint/window observation | profile 分级，不能强制 |
| driver observation → transition | 适合低层 execution driver | Codex notification/read 是完整 Agent observation；平台 reducer 可生成 canonical projection，但不能重新裁决 Codex internal terminal/context transition | 把 observation seam 提升到 complete Agent 层 |
| snapshot + committed tail | 对自有 aggregate 可同时 authoritative | Codex App Server 有 snapshot-like read/resume bootstrap + live notifications，但未发现 durable revisioned `changes(after_revision)` | snapshot mandatory；tail capability-graded；gap reread |
| effect ledger / inspect / generation fence | 对平台 dispatch、binding、transport recovery有价值 | 只能协调“是否成功把 command 提交给 Codex/是否需要重连”，不能替代 Codex session recovery | 保留 host coordination，缩小语义 |
| `ApplyContextRevision` stateful replica | 可用于真正接受 external context 的 driver | Codex 没有证据支持平台精确 apply 任意 typed context；fork/compact/resume 都有 native 命令 | 不得作为 Codex 的默认实现路径 |

07-17 的 snapshot + tail 目标（`.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:650-653`、`:792-796`）仍适合 AgentDash canonical client reducer。需要把 “authoritative” 拆成：

- 对平台产品/映射事实，平台 snapshot/change 是权威；
- 对 Codex session/history/context/recovery 事实，Codex source 是权威，平台 snapshot/change 是 canonical mapped projection；
- projection gap 通过 source read 重新收敛，不能用 platform tail 重放并恢复 Codex 内部 session。

### 8. 最小非回归契约

1. **统一 Runtime 不删除**：Application 仍只依赖 AgentDash canonical gateway，不直接依赖 Codex DTO、ThreadStore 或 native handle。
2. **完整 Agent 不降格**：Codex adapter 实现 complete Agent service port；不得把 Codex session 当作平台 `ContextRevision` 的 stateful replica。
3. **双层 admission 明确**：平台拥有产品授权、offer/profile、idempotency 与 availability gate；Codex 保留 native active-turn/task/context admission。receipt 明确 platform accepted、source accepted、terminal。
4. **读 fidelity 明确**：Thread/Turn/Item transcript、model-visible context、platform business snapshot 分开；任何 exact 声明都需要 source API/revision/digest 证据。
5. **fork 为 P0**：source、cutoff、lineage、mid-turn policy、new identity、accepted/terminal 均为 typed contract；source IDs 不重编号。
6. **resume/recovery 归实现**：平台只恢复 binding/transport/projection，Codex 通过自己的 rollout reconstruction 恢复 session。
7. **compaction apply 归实现**：平台可以命令并观察 compact task/item/terminal；不能直接安装 Codex replacement history。只有 source 导出 exact capability 后才升级 fidelity。
8. **interaction 有稳定关联**：tool、approval、permission、user input、elicitation response 必须带 source correlation，duplicate/stale response 不可二次终结。
9. **projection 不冒充 source log**：AgentChange outbox 可以保证平台 delivery/reducer 收敛，但不能驱动 Codex fork、resume 或 model context reconstruction。
10. **gap 必须 reread**：无 durable source cursor 或检测到 retention/generation gap 时，丢弃局部 projection并从 source snapshot 重建。
11. **平台状态独立持久化**：binding/profile/operation/idempotency/source mapping/cursor/product policy 不写进 Codex transcript。
12. **conformance 按 profile 测试**：每个 adapter 只承诺真实支持的 command、cutoff、context fidelity、change replay、tool/hook 和 settings mutability。

### 9. 尚需用户决定

1. **tail 基线**：完整 Agent common seam 是否只强制 `read snapshot`，把 durable `changes(after_cursor)` 设为可选 capability；还是平台必须为 Codex observation 再建 durable projection tail。后者能保证客户端 delivery，但仍不能成为 Codex recovery truth。
2. **fork cutoff P0 范围**：是否以 Codex 已验证的 `last_turn_id` inclusive / `before_turn_id` exclusive 为最小集合，还是首版就要求 item/cursor cutoff。当前 Codex 证据只支持 turn-level cutoff 与 interrupted snapshot。
3. **Codex context fidelity**：接受平台视角 `Opaque/EventProjected`，还是修改/扩展 Codex 以导出 exact model-context digest/revision/read。未扩展前不能把 `DriverExact` 写进 offer。
4. **AgentRun 映射基准**：一个平台 AgentRun 对应 Codex thread，还是对应 Codex `session_id` 下的 thread tree；fork 后是新 AgentRun、新 session，还是同一产品 run 的 child。
5. **automatic compaction continuation**：对 Codex 保持 agent-native policy，还是要求所有实现都暴露平台可控 continuation capability。若统一成平台 queue，会重新引入双重 admission。
6. **hook 变更时机**：首版仅允许 binding/start/resume 时应用 managed hook contribution，还是要求 live hook reconfiguration。当前证据支持 config + lifecycle notification，不足以承诺任意运行期变更。
7. **管理/保留接口**：archive/delete/retention 是否属于 complete Agent P0，平台删除 mapping 与 source thread 删除失败时采用何种可见终态。

## Files Found

- `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md` — 当前 aggregate、`AgentExecutionPort`、`ContextRevision`、outbox、snapshot+tail 方案及其 replica 假设。
- `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/design.md` — 统一 Runtime、Application/Managed Runtime/Integration Host 分层及 AgentDash-owned canonical vocabulary。
- `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/target-crate-shape.md` — 目标 crate 依赖方向、offer/profile/binding 与 adapter seam。
- `references/codex/codex-rs/thread-store/README.md` — ThreadStore、LiveThread、JSONL canonical history 与 SQLite metadata 职责。
- `references/codex/codex-rs/thread-store/src/store.rs` — create/resume/append/load history/model context 的 storage-neutral trait。
- `references/codex/codex-rs/thread-store/src/live_thread.rs` — active thread persistence、fork inherited context 与 metadata sync。
- `references/codex/codex-rs/thread-store/src/local/live_writer.rs` — JSONL durability 在前、SQLite projection 在后的 ordering。
- `references/codex/codex-rs/thread-store/src/local/model_context.rs` — replacement checkpoint + suffix 的 latest model-context targeted read。
- `references/codex/codex-rs/core/src/session/session.rs` — Codex Session 与 configuration ownership。
- `references/codex/codex-rs/core/src/state/session.rs` — session-scoped history/context/config state。
- `references/codex/codex-rs/core/src/state/service.rs` — MCP/tools/hooks/auth/store/model 等联合宿主服务。
- `references/codex/codex-rs/core/src/state/turn.rs` — active task、turn state 与 pending interactions。
- `references/codex/codex-rs/core/src/context_manager/history.rs` — model-visible context normalization、version、replace/rollback。
- `references/codex/codex-rs/core/src/session/rollout_reconstruction.rs` — resume、compaction checkpoint、rollback marker 的 replay/reconstruction。
- `references/codex/codex-rs/core/src/thread_manager.rs` — resume/fork、cutoff、lineage 与 interrupted snapshot。
- `references/codex/codex-rs/core/src/compact.rs` — local compaction candidate generation、replacement history 与 item lifecycle。
- `references/codex/codex-rs/core/src/compact_remote.rs` — remote compaction install semantic boundary。
- `references/codex/codex-rs/core/src/session/mod.rs` — state install、rollout persistence 与 compaction replacement ordering。
- `references/codex/codex-rs/core/src/session/handlers.rs` — append-only logical rollback full path。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs` — thread read/resume/fork/compact/rollback public command shapes。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs` — Thread/Turn read model。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs` — turn start/steer/interrupt command shapes。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/item.rs` — typed item、dynamic tool、hook prompt vocabulary。
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/config.rs` — managed hooks 与 configuration requirements。
- `references/codex/codex-rs/app-server/src/thread_state.rs` — process-local listener/live connection state。
- `references/codex/codex-rs/app-server/src/request_processors/thread_lifecycle.rs` — snapshot bootstrap、listener 与 live event forwarding。
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs` — typed interaction、dynamic tool 与 hook notification bridge。
- `crates/agentdash-agent-runtime-contract/src/profile.rs` — 当前 context fidelity 梯度。
- `crates/agentdash-agent-runtime-contract/src/driver.rs` — 当前低层 `AgentRuntimeDriver` 形状。
- `crates/agentdash-agent-runtime-contract/src/gateway.rs` — 当前 AgentDash outer gateway。

## Code Patterns

- **canonical log + rebuildable projection**：JSONL 先 durable，SQLite 后 materialize（`references/codex/codex-rs/thread-store/src/local/live_writer.rs:301-307`）。
- **active handle hides storage backend**：Session 使用 `LiveThread`，不感知 local/remote store（`references/codex/codex-rs/thread-store/src/live_thread.rs:29-38`）。
- **checkpoint + tail reconstruction**：最新 replacement checkpoint 成为 base，再 replay surviving suffix（`references/codex/codex-rs/core/src/session/rollout_reconstruction.rs:286-373`）。
- **append-only logical rewrite**：compaction 追加带 replacement history 的 `Compacted`；rollback 追加 `ThreadRolledBack` marker（`references/codex/codex-rs/core/src/session/mod.rs:2992-3016`、`references/codex/codex-rs/core/src/session/handlers.rs:529-536`）。
- **candidate vs install**：compaction endpoint/model 输出先是候选，install 才成为 live history（`references/codex/codex-rs/core/src/compact_remote.rs:275-300`）。
- **full Agent state is wider than transcript**：Session/Turn/Services 同时拥有 active task、interactions、tools、hooks、provider、store 与 context（`references/codex/codex-rs/core/src/session/session.rs:25-108`、`references/codex/codex-rs/core/src/state/service.rs:50-104`）。
- **snapshot + notification, not durable source tail**：App Server 的 live listener/connection state 位于进程内（`references/codex/codex-rs/app-server/src/thread_state.rs:70-92`、`:280-298`），未发现公开的 durable revisioned `changes(after_revision)`。

## External References

- 未使用外部网页或文档；结论仅基于仓库内 `references/codex` 当前源码与本项目 Trellis 设计/规范。
- 未独立验证 `references/codex` checkout 的发布版本号；本报告只对所引用源码快照负责。

## Related Specs

- `.trellis/spec/backend/agent-runtime-kernel.md` — 自有 Runtime transition/kernel invariants。
- `.trellis/spec/backend/agent-runtime-persistence.md` — journal/snapshot/outbox durability 与 recovery。
- `.trellis/spec/backend/agent-runtime-context.md` — context fidelity、checkpoint、compaction 与 replay。
- `.trellis/spec/backend/agent-runtime-driver-host.md` — service/binding/generation/placement/driver lifecycle。
- `.trellis/spec/backend/agent-runtime-codex-adapter.md` — Codex App Server mapping 与 adapter fidelity。
- `.trellis/spec/backend/agent-runtime-surface-tool-broker.md` — capability/tool contribution 与 admission。
- `.trellis/spec/backend/agent-runtime-agentrun-facade.md` — AgentRun/Application facade 与 product ownership。
- `.trellis/spec/cross-layer/agent-runtime-wire-relay.md` — snapshot/event/interaction relay。
- `.trellis/spec/cross-layer/backbone-protocol.md` — accepted/terminal、typed interaction 与 protocol vocabulary。

## Caveats / Not Found

- 未发现 Codex App Server 对外提供 exact model-context read、arbitrary context apply、stable context digest/revision 或 durable `changes(after_revision)` API；因此不能证明平台可把 Codex 当作 `ApplyContextRevision` replica。
- App Server Thread/Turn/Item DTO 是 presentation/protocol read model，部分路径明确 lossy；不能由其反推 exact provider prompt。
- Codex 当前 compaction 先改内存 state、后 best-effort persist，错误只记日志；这是事实描述，不应作为 AgentDash 自有 transactional aggregate 的 durability 模板。
- hooks 有 config requirements 与 started/completed notifications，但未找到与 thread/compact/fork 同等级的任意 live hook-state mutation command。
- 当前 07-17 的 `AgentExecutionPort`、normalized aggregate 与 `AgentChange` 主要仍是设计文本；现有代码仍是旧 `AgentRuntimeGateway` / `AgentRuntimeDriver` contract，不能把提案当作已实现行为。
- 本结论不否定统一 Runtime 的 canonical contract/snapshot/change。它只限制这些平台事实不能越界成为 Codex 内部 session/history/context/recovery 的第二事实源。
