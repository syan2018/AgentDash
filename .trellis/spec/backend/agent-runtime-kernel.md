# Managed Agent Runtime Kernel

本文定义`agentdash-agent-runtime`持久状态内核的可执行合同。Context/Compaction、Hook orchestration与PostgreSQL concrete adapter在后续章节扩展，但不得改变这里的operation、journal、projection、outbox与terminal原子性。

## Scenario: Runtime mutation、driver event 与 durable cursor

### 1. Scope / Trigger

当实现Runtime command、Driver event ingestion、repository/unit-of-work adapter、journal projection、outbox、snapshot或event subscription时，适用本合同。Managed Runtime拥有状态转换；Infrastructure只原子保存`RuntimeCommit`，Driver Host只投递outbox并回送source event。

### 2. Signatures

Runtime-owned read与transaction ports：

```rust
trait RuntimeRepository {
    async fn load_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeThreadState>, RuntimeStoreError>;

    async fn find_operation(
        &self,
        operation_id: &RuntimeOperationId,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError>;

    async fn find_idempotency(
        &self,
        thread_id: &RuntimeThreadId,
        key: &IdempotencyKey,
    ) -> Result<Option<RuntimeOperationRecord>, RuntimeStoreError>;

    async fn events_after(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<EventSequence>,
    ) -> Result<RuntimeEventBatch, RuntimeStoreError>;
}

trait RuntimeUnitOfWork {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError>;
    async fn quarantine(
        &self,
        event: QuarantinedDriverEvent,
    ) -> Result<(), RuntimeStoreError>;
}

enum DriverEventAdmission {
    Durable { sequence: EventSequence },
    Terminalized { sequence: EventSequence },
    Transient,
    Observed,
    Quarantined,
}
```

完整write-set：

```rust
struct RuntimeCommit {
    expected_projection_revision: Option<RuntimeRevision>,
    projection: RuntimeThreadState,
    operation: Option<RuntimeOperationRecord>,
    operation_terminals: Vec<(RuntimeOperationId, RuntimeOperationTerminal)>,
    events: Vec<RuntimeEventEnvelope>,
    outbox: Vec<RuntimeOutboxEntry>,
    quarantine: Vec<QuarantinedDriverEvent>,
}
```

`expected_projection_revision=None`表示create-if-absent；`Some(revision)`表示事务内精确CAS。

### 3. Contracts

- mutation先在单个`RuntimeCommit`中持久化Operation acceptance、canonical projection、authoritative journal与outbox；commit成功后Driver Host才可执行side effect。
- per-thread `OperationSequence`、`EventSequence`与`RuntimeRevision`单调分配；CAS loser不消费任何序号。
- idempotency唯一域是`(RuntimeThreadId, IdempotencyKey)`；record持久化actor与完整typed command。只有operation ID、key、actor、command全部一致才返回duplicate receipt。
- `RuntimeUnitOfWork::commit`必须在一个数据库事务内校验projection CAS和operation/idempotency唯一约束，并写入全部write-set。
- authoritative event推进durable cursor；transient conversation delta使用`stream_generation + transient_sequence + event_id`，不进入durable cursor或projection revision。Gateway在读取replay前建立per-thread broadcast订阅，再输出durable/transient replay并持续等待live事件，避免replay/live交界丢失。
- transient replay只保存当前active turn且上限512条；binding/turn terminal先广播authoritative durable terminal，再清理buffer、重置generation并回收per-thread sender。已持有receiver仍能收到terminal，新订阅从durable replay恢复。
- `InteractionRequested`直接携带generated owned request params；approval、user input、MCP elicitation与dynamic tool请求不得压缩成`kind + prompt`或从裸JSON摘取展示字段。
- `RuntimeEventBatch`携带`earliest_available`与`latest_available`；subscription区分future cursor和retention gap。
- canonical/source坐标与binding generation在任何state transition前校验。stale generation只进入typed quarantine，不推进canonical state。
- BindingLost、critical protocol violation与非法critical lifecycle在一个`RuntimeCommit`内持久事实、quarantine原事件，并将所有active Item、Interaction、Turn、Operation收敛为typed Lost/terminal。
- Driver envelope归约同时保留immutable committed base与in-memory staged projection。只有完整fact batch、terminal projection与write-set校验全部成功后才以committed base revision提交staged projection；任一fact失败都从committed base重新构造critical violation commit，不能复用已推进的staged state。
- critical violation若终结active Turn，必须在同一commit中写入canonical Lost、唯一durable terminal presentation、terminal application effect与quarantine，并返回`DriverEventAdmission::Terminalized`。event sink把该admission作为停止producer pump的flow-control；pump不得再追加第二份`BindingLost`。
- presentation-only transient只通过transient publication进入live session stream，不生成语义重复的internal fact，也不推进`EventSequence`或`RuntimeRevision`。Driver adapter新增internal fact时必须与Runtime admission规则成对验证。
- Completed Item携带authoritative final content；Failed、Cancelled、Lost不伪造final content。
- composition root提供真实Thread/Binding/source mapping；测试用defaults不成为production ID allocation或binding admission事实源。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| --- | --- |
| expected projection revision不匹配 | `ProjectionConflict`，Gateway映射`RevisionConflict`，write-set零落地 |
| operation ID复用于不同请求 | `OperationConflictKind::OperationIdReused` |
| 同Thread idempotency key换actor或command | `OperationConflictKind::IdempotencyKeyReused` |
| 完全相同的operation ID/key/actor/command重试 | 返回原receipt且`duplicate=true`，不新增event/outbox |
| store任一write stage失败 | projection/operation/idempotency/journal/outbox/quarantine均保持事务前状态 |
| cursor高于latest durable sequence | `RuntimeSubscribeError::InvalidCursor` |
| requested cursor早于retained prefix | `RuntimeSubscribeError::CursorGap { requested, earliest_available, latest_available }` |
| snapshot请求非current revision且无历史snapshot | `RuntimeSnapshotError::RevisionUnavailable` |
| stale binding/generation/source coordinate | typed quarantine，不推进revision/cursor |
| Driver发送runtime-owned OperationAccepted | durable critical protocol violation + typed quarantine + active状态Lost收敛 |
| terminal重复、parent改变、terminal后delta | typed transition violation；critical入口按同一事务收敛 |
| 同一Driver batch前置fact有效、后置fact非法 | 前置staged mutation零落地；从原committed revision原子写critical violation并返回`Terminalized` |
| presentation-only Provider status/delta | 仅transient publication；不进入internal journal、revision或durable cursor |

### 5. Good/Base/Bad Cases

- Good：Command在revision 7被接受，事务同时写operation、revision 8 projection、连续events与outbox；Driver worker只在commit后消费outbox。
- Good：两个并发Command都声明expected revision 7，仅一条commit成功；失败方不占用operation/event sequence。
- Good：Driver batch在revision 18归约到临时revision 19后发现非法后置fact；Runtime丢弃整份staged projection，以expected revision 18提交唯一violation/Lost终态并停止pump。
- Base：客户端携带durable cursor与当前transient generation/sequence重连；Gateway去重replay后保持live连接，final durable item覆盖过程delta。
- Bad：先写operation再尝试写projection/outbox；数据库中间失败会留下无法完成或错误重放的acceptance。
- Bad：把actor放进idempotency namespace；攻击者或另一主体可换actor复用同一Thread key绕过冲突检查。

### 6. Tests Required

- Interface test通过`AgentRuntimeGateway`验证acceptance、snapshot、events，不绕过public seam测试内部map。
- 五个transaction failure stage分别断言projection、operation、idempotency、journal、outbox全部零部分落地。
- 并发CAS测试断言唯一成功、连续operation/event sequence与projection/cursor一致。
- idempotency测试覆盖exact duplicate、same key/different actor、same key/different command与operation ID复用。
- Driver ingress测试覆盖stale generation、source mismatch、duplicate terminal、runtime-owned event与critical violation Lost收敛。
- Driver ingress组合测试覆盖`valid prefix -> invalid suffix`，断言prefix不入journal、column/projection/journal revision一致、唯一terminal presentation/effect/quarantine同事务落地，并使用真实PostgreSQL注入末阶段失败验证全写集回滚。
- 每个Driver event pump（Native、Codex、Remote与durable worker）都必须覆盖`Terminalized`后停止且不补`BindingLost`；普通nonterminal sink error保持原有重试/保留语义。
- Cursor测试覆盖normal tail、future cursor、retention gap、空retained journal、subscribe-before-replay race、generation切换、transient重复去重、Lagged重连、terminal reset与transient不推进durable cursor。
- PostgreSQL adapter落地时必须复用以上behavior suite并增加真实并发transaction/migration测试；in-memory通过不代表数据库原子性已证明。

目标门禁：

```powershell
cargo test -p agentdash-agent-runtime
cargo clippy -p agentdash-agent-runtime --all-targets -- -D warnings
pnpm contracts:check
```

### 7. Wrong vs Correct

#### Wrong

```rust
repository.insert_operation(operation).await?;
repository.append_events(events).await?;
repository.save_projection(projection).await?;
outbox.enqueue(command).await?;
```

这些调用即使逐个成功，也没有表达共享CAS与失败回滚合同。

#### Correct

```rust
unit_of_work
    .commit(RuntimeCommit {
        expected_projection_revision: Some(expected),
        projection,
        operation: Some(operation),
        operation_terminals,
        events,
        outbox,
        quarantine,
    })
    .await?;
```

所有状态变化共享一个事务入口，Infrastructure可以用同一CAS和唯一约束实现真实原子性。

```rust
// Wrong: batch失败后把已推进的staged projection当作CAS基线。
let mut state = repository.load_thread(thread_id).await?;
reduce_prefix(&mut state)?;
persist_protocol_violation(state, invalid_suffix).await?;

// Correct: committed base不可变；成功提交staged，失败从base构造violation。
let committed = repository.load_thread(thread_id).await?;
let mut staged = committed.clone();
match reduce_all(&mut staged, facts) {
    Ok(write_set) => commit(committed.revision, staged, write_set).await?,
    Err(violation) => commit_violation(committed, violation).await?,
}
```
