# Managed Agent Runtime Context 与 Compaction

本文定义Managed Runtime拥有的模型上下文、checkpoint、activation与managed compaction合同。它与产品Thread transcript分属不同read model；数据库adapter、Driver Host和Agent Adapter必须消费这些对象，不能另建上下文事实源。

## Scenario: Durable Context 与 Managed Compaction Saga

### 1. Scope / Trigger

当实现ContextRecipe、context materialization、checkpoint/head、manual/automatic compaction、driver activation、recovery scan或context read时，适用本合同。目标是让“模型下一轮实际看到什么”拥有可验证revision/digest/fidelity，并让driver side effect与数据库head CAS之间的crash可恢复。

### 2. Signatures

Context command固定业务identity与base：

```rust
RuntimeCommand::ContextCompact {
    thread_id: RuntimeThreadId,
    compaction_id: ContextCompactionId,
    trigger: ContextCompactionTrigger,
    expected_base_checkpoint_id: Option<ContextCheckpointId>,
    expected_context_revision: ContextRevision,
}
```

Thread与Context使用同一个深Gateway的typed query/result：

```rust
enum RuntimeSnapshotQuery {
    Thread { thread_id: RuntimeThreadId, at_revision: Option<RuntimeRevision> },
    Context { thread_id: RuntimeThreadId, at_revision: Option<ContextRevision> },
}

enum RuntimeSnapshotResult {
    Thread(RuntimeSnapshot),
    Context(RuntimeContextView),
}
```

Durable compaction lifecycle：

```text
ContextPreparationWorkItem::Pending
  -> ContextCandidate + immutable ContextCheckpoint
  -> ContextActivation::Prepared + stable activation dispatch intent
  -> ContextActivation::Applied { digest, driver_context_revision }
  -> ActiveContextHead CAS
  -> ContextActivation::Terminal + Operation terminal
```

### 3. Contracts

- `ContextRecipe`记录recipe revision、settings/tool revision provenance与source item IDs；`MaterializedContext`记录typed blocks、digest和真实`ContextFidelity`。
- Thread read返回按canonical Item lifecycle形成的final transcript，fidelity为`EventProjected`；Context read返回active head/checkpoint/materialized blocks及其真实fidelity，两者不互相替代。
- `Opaque` context不能建立或推进platform-managed active head；driver opaque compact只写telemetry。
- ContextCompact acceptance与`ContextPreparationWorkItem::Pending`在同一`RuntimeCommit`持久化，recovery worker通过pending scan发现accepted-but-unprepared operation。
- 同一Thread只允许一个nonterminal managed compaction work；第二条command在任何driver side effect前返回typed `ContextCompactionInProgress`。
- prepare必须验证operation command、thread、compaction ID、trigger、base checkpoint/context revision与durable work item完全一致。
- checkpoint/candidate immutable；activation ID、dispatch intent与driver revision使用typed稳定坐标。NotApplied重投同一activation intent，不追加重复outbox。
- Applied confirmation先durable持久化，head CAS在后续事务完成；crash后由recoverable activation scan继续，不重复apply新identity。
- active head必须引用真实checkpoint，且revision、digest、settings/tool provenance、fidelity完全相同；head revision严格增加1。
- compaction admission在写入自身OperationAccepted前冻结`source_end_event_sequence`。candidate只压缩该边界及以前的source items；之后的durable user/assistant/tool facts作为tail在cold rebind时接到active checkpoint之后。
- transcript投影保留完整tool-call/result配对与typed item；command-owned presentation携带owning operation ID，使当前TurnStart输入不会同时作为历史和本轮prompt重复送入provider。
- pre-apply并发由per-thread durable slot拒绝；post-apply digest/base/observation不可验证时Thread进入`Desynchronized`、Operation进入`Lost`并阻止新Turn。
- activation terminal保留Applied digest与typed driver context revision，duplicate ack只能验证原事实，不能覆盖terminal。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| --- | --- |
| ContextCompact acceptance事务失败 | Operation、event、preparation work全部零落地 |
| 同Thread已有active compaction | `ContextCompactionInProgress`，无candidate/activation/side effect |
| prepare identity/trigger/base与accepted command不一致 | typed context invariant/operation mismatch，零side effect |
| candidate/checkpoint ID复用于不同内容 | `ContextStoreInvariant` |
| activation ID复用于不同payload | `ContextStoreInvariant` |
| Prepared收到NotApplied | 同activation ID重投幂等dispatch intent |
| Applied收到相同ack | 幂等成功，不新增journal |
| Applied/Terminal收到不同digest或driver revision | typed mismatch；不覆盖原事实 |
| Applied后active base/head改变 | `Desynchronized + Lost`，保留现有head且阻止新Turn |
| head CAS事务失败 | Applied事实保留，head不变，recovery可再次finalize |
| Opaque materialization尝试prepare managed checkpoint | typed拒绝 |
| compaction acceptance之后又写入新user/tool facts | checkpoint边界不漂移；新facts只作为durable tail replay |
| cold recovery已有active checkpoint | provision surface、binding descriptor、tool/hook/workspace与provider transcript统一覆盖为active head版本 |
| Driver伪造checkpoint/activation/head/compaction terminal事件 | critical protocol violation + quarantine + Lost收敛 |

### 5. Good/Base/Bad Cases

- Good：manual command acceptance写Pending work；worker prepare candidate并dispatch稳定activation；driver applied ack落库后进程崩溃，recovery扫描Applied记录并完成head CAS。
- Good：automatic与manual只改变trigger provenance，复用相同状态机和事务边界。
- Base：Driver报告opaque native compact，Runtime记录telemetry，active head与context revision保持不变。
- Bad：acceptance后只依靠当前调用栈继续prepare；进程崩溃会留下永远无法发现的active Operation。
- Bad：两个candidate都发给同一Driver后再靠head CAS选赢家；CAS只能决定数据库head，无法证明Driver live context与赢家一致。

### 6. Tests Required

- acceptance failure与pending recovery scan测试，断言accepted-but-unprepared可发现。
- operation/compaction/thread/trigger/base correlation测试，断言任一不一致均在side effect前拒绝。
- per-thread active compaction唯一测试，断言第二条command无candidate/outbox。
- prepare failure injection测试，断言checkpoint/candidate/activation/dispatch/head零部分落地。
- activation retry/duplicate ack/illegal transition测试，断言stable identity与terminal不可回退。
- applied-before-head-CAS crash测试，断言recovery可重入且Operation exactly-once terminal。
- fidelity测试，断言Thread=`EventProjected`、Context使用checkpoint真实fidelity、Opaque不推进head。
- boundary recovery测试断言`source_end_event_sequence`在acceptance前冻结，kept tool pair不拆分，active checkpoint与之后tail在rebind时按原顺序合并。
- production recovery测试从Runtime context snapshot覆盖materialized driver surface、descriptor与callable registry，防止重启后回到启动时旧surface。
- PostgreSQL adapter在02C复用以上behavior suite，并补partial unique、claim/lease/`SKIP LOCKED`与真实并发事务测试。

目标门禁：

```powershell
cargo test -p agentdash-agent-runtime -p agentdash-agent-runtime-contract
cargo clippy -p agentdash-agent-runtime -p agentdash-agent-runtime-contract --all-targets -- -D warnings
pnpm contracts:check
pnpm frontend:check
```

### 7. Wrong vs Correct

#### Wrong

```rust
accept_operation(command).await?;
driver.activate(candidate).await?;
save_head(candidate.checkpoint).await?;
```

调用栈承担了recovery与顺序，一旦任一步crash就无法判断side effect和head事实。

#### Correct

```text
accept + Pending work (transaction)
  -> prepare candidate/checkpoint/activation intent (transaction)
  -> driver apply stable activation ID
  -> persist Applied observation (transaction)
  -> CAS head + activation/operation terminal (transaction)
```

每个跨系统side effect前后都有durable事实和可扫描恢复入口。

## Context presentation projection

Materialized context、tool/workspace surface 与 ContextFrame presentation 必须由同一次 Business Agent Surface 编译产生。ContextFrame 是面向会话流的审计/展示事实，不是 Driver 模型输入，也不能反向成为 tool availability 或 context head 的事实源。

- compiled surface artifact 按 exact binding / surface revision / digest 保存 presentation plan，使 ThreadStart、recovery 与 replay 使用同一版本事实。
- bootstrap frame 随首个 ThreadStart 提交；typed surface delta 随 SurfaceAdopt 提交；managed compaction summary 随 checkpoint/head activation 提交。
- workflow transition phase 是可选 presentation metadata。存在时用于展示 node path；非 Workflow surface update 使用通用 Runtime Surface Update，不得因缺失 phase 拒绝 adoption。
- ContextFrame identity 使用 operation/source frame/revision/ordinal 等稳定坐标；timestamp 与可选 phase 不参与 identity，相同 operation 重放必须产生相同 frame ID 与 digest。
- ContextFrame 与所属 canonical mutation共享 Runtime UoW、revision 与 idempotency；presentation failure 必须阻止对应 surface/head/Hook mutation 接受。
- Native、Codex、Remote adapter 只消费 materialized driver surface，不构造 ContextFrame，也不读取 AgentFrame repository。

这些约束让执行面与展示面保持同源，同时保留 adapter 对 vendor protocol 的纯翻译职责。
