# Agent Runtime Owner-Document Persistence

## 1. Scope / Trigger

本规范适用于 Managed Runtime、Complete Agent Host、Complete Agent callback 与
AgentRun Product binding 的 PostgreSQL 持久化。修改这些 owner 的 snapshot、revision、
outbox 消费或跨 owner binding evidence 时必须复核本规范。

选择 owner document 的原因是这些聚合本身以完整 fact graph 做 CAS、单调性与幂等校验。
把同一图再次拆成全局关系表不会产生新的查询权威，只会产生第二份需要同步和验真的状态。

## 2. Signatures

```rust
pub trait ManagedRuntimeStateRepository: Send + Sync {
    async fn load(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError>;

    async fn commit(
        &self,
        commit: ManagedRuntimeStateCommit,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError>;
}

pub trait CompleteAgentHostRepository: Send + Sync {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;
    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError>;
}

pub trait CompleteAgentCallbackRepository: Send + Sync {
    async fn load(
        &self,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;
    async fn commit(
        &self,
        commit: CompleteAgentCallbackCommit,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;
}

pub struct AgentRunCommittedProductRuntimeBinding {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_digest: String,
}

pub trait AgentRunProductRuntimeBindingStore:
    AgentRunProductRuntimeBindingRepository + Send + Sync
{
    async fn commit_product_binding(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String>;

    async fn prepare_product_binding_recovery(
        &self,
        expected_previous_binding_digest: &str,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String>;
}
```

最终数据库签名：

```text
agent_runtime_state_revision(
  thread_id text primary key,
  revision numeric(20,0),
  facts jsonb
)

agent_runtime_host_revision(
  singleton boolean primary key,
  revision numeric(20,0),
  facts jsonb
)

agent_runtime_callback_revision(
  singleton boolean primary key,
  revision numeric(20,0),
  facts jsonb
)

agent_run_product_runtime_binding(
  target_run_id text,
  target_agent_id text,
  project_id text,
  runtime_thread_id text,
  launch_frame_id text,
  binding_digest text,
  binding jsonb,
  change_delivery_state jsonb
)
```

`change_delivery_state` 是以 `consumer_name` 为 key 的 Product-owned JSON object：

```json
{
  "terminal_projection": {
    "delivered_sequence": 12,
    "claim": {
      "owner": "api-instance",
      "token": "uuid",
      "expires_at_ms": 1780000000000
    },
    "attempt_count": 3,
    "last_error": null
  }
}
```

## 3. Contracts

- 每个 owner 只持久化一个 canonical document：Managed Runtime 按 `RuntimeThreadId`
  保存 revision + facts；Host 与 callback 各保存 singleton revision + facts。
- commit 在事务内锁定 canonical row，解码 typed snapshot，执行 domain commit/CAS，
  再原子更新 revision 与完整 facts。load 只解码 owner document，不读取关系镜像来“复验”自己。
- Runtime facts 内的 projection、binding、source projection/identity/change、operation、
  idempotency、pending command、change 与 outbox 都属于同一 Runtime 聚合，不各建全局表。
- Host facts 内的 runtime target、binding、source coordinate、callback route、effect、
  lease、provisioning 与 recovery 属于同一 Host 聚合；callback reservation/outcome 属于
  callback 聚合。
- 跨 owner 只持久化使用方真正拥有的事实。Product binding 保存 RuntimeThread、精确 AgentFrame
  identity 与 execution profile intent；Runtime source/applied/activation evidence 只存在于
  Managed Runtime document，Complete-Agent binding identity、generation、compiled/applied
  surface 和 callback route 只存在于 Host owner document。
- Product activation 不读取或复制 Host aggregate。Host callback admission 使用自己的
  binding generation、source 与 applied-surface digest fence stale callback；Product tool
  authorization 使用 RuntimeThread 与 AgentFrame surface revision 对 Runtime 当前 applied
  evidence 做派生校验。两个 owner 的 digest 各自覆盖不同 schema，不能要求字符串相等。
- Product runtime resource authority 是 binding-pinned AgentFrame 与当前 Product 关系的纯派生值：
  VFS/capability surface 从精确 frame 读取，Task grant 从当前 subject association 读取。它没有
  独立生命周期或写入语义，因此按请求即时编译，不建立 snapshot/current 表，也不参与 activation
  pin。关系撤销应立即缩小授权面，无需等待 rematerialize。
- Runtime Product change delivery 从已激活的 Product binding 出发，读取对应 Runtime
  document 的 canonical outbox。没有 Product binding 的 Runtime thread 不是待消费工作，
  因此不会产生错误或占用重试队列。
- 每个 Product consumer 在 `change_delivery_state` 中拥有独立 cursor、claim、attempt 和
  last error。一个 consumer 失败只释放自己的 claim，不重放其他 consumer 已确认的 change。
- Product binding 的 immutable digest 不包含 delivery state；delivery state 是同一 Product
  owner 下的可变消费进度，不改变执行绑定身份。
- `agent_run_product_runtime_binding.binding` 是 Product Runtime binding 的唯一 canonical
  文档。`runtime_thread_id`、`launch_frame_id` 与 target/project columns 只承担唯一性、owner
  lookup 或 Product-local FK，并由数据库约束与文档坐标相等；repository 只能从 `binding`
  解码领域对象，不能从这些 scalar columns 重建第二份 binding。
- Product binding digest 使用带 schema identity 的递归 canonical JSON：object key
  逐层按字典序排序，array 保持业务顺序。digest 表达 immutable binding 语义，不受
  `serde_json` map backend、插入顺序或 PostgreSQL JSONB key order 影响。
- `commit_product_binding` 与 frame replacement 返回 repository 确认的 committed receipt；
  调用方不以写前内存对象自行声明数据库提交证据。
- repository 每次读取 Product binding 都必须解码 canonical document，复算 digest，并验证
  target/runtime thread/launch frame scalar coordinates。任一不一致表示 durable evidence
  损坏，不能通过 split columns 补齐。
- Runtime Create/Activate 与 Host recovery 不修改 Product binding。Product 只验证 Runtime
  当前 applied surface 与 binding-pinned AgentFrame 一致；Runtime evidence 不进入 Product digest。
- Complete Agent descriptor、verification、effective offer、placement availability 与 callable
  handle 属于当前进程的 live catalog，不进入上述 owner documents。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| commit expected revision 与 canonical row 不一致 | typed revision conflict；事务不更新 |
| document 无法解码或 fact graph 不满足 domain invariant | typed invariant error |
| Runtime operation 从 Accepted 推进到 Running | 更新同一 Runtime JSONB document；重启后读取 Running |
| runtime authority 的 binding digest 或 AgentFrame revision 不匹配 | Product tool authorization typed reject |
| Host callback generation/source/applied surface 不匹配 Host current facts | Host callback admission typed reject；Product repository 不参与 |
| Product surface-facts digest 与 Host compiled/applied-surface digest 不同 | 合法；两者分别在自己的 schema namespace 内复验 |
| Runtime 尚未应用或激活 Product AgentFrame | command/delivery 不推进；Product binding 不被改写 |
| Product binding JSONB 经 PostgreSQL 往返后 object key 顺序变化 | 复算 digest 与 committed receipt 相同 |
| Product binding document 与 scalar coordinate 或 stored digest 不一致 | repository load/commit 显式失败；不从 scalar columns 重建 |
| launch replay 命中相同 Product binding | 返回原 committed receipt；Runtime 幂等命令决定 applied state |
| authority resolver 找不到 binding-pinned AgentFrame | typed missing facts；不改用 latest frame |
| Runtime thread 没有 Product binding | 不构造 Product work；不产生 warning/retry row |
| consumer claim 未过期 | 其他 worker 不得接管该 consumer |
| claim 到期后接管 | 新 token、attempt + 1；旧 token ack 失败 |
| 一个 consumer observe 失败 | 只记录该 consumer 的 last error 并释放其 claim |
| Product receipt 保存 Runtime operation identity | 保存 opaque typed coordinate；不对 Runtime 内部集合建外键 |
| schema readiness 发现任一 retired mirror table | readiness fail，明确列出残留表 |

## 5. Good / Base / Bad Cases

- Good：Runtime 在一次 commit 中推进 operation、projection 与 outbox；进程重启后从
  `agent_runtime_state_revision.facts` 恢复完全相同的 snapshot。
- Good：terminal consumer 已确认 sequence 8，routine consumer 仍停在 sequence 5；
  两者在同一 Product binding document 中独立推进。
- Good：execution profile configuration 经过不同插入顺序和 PostgreSQL JSONB roundtrip，
  Product binding receipt 仍使用同一 canonical digest。
- Good：Task association 被删除后，下一次工具授权即时收窄 Task grant，无需推进 snapshot。
- Base：Runtime 尚未完成 activation 时已经产生 change；activation 完成后
  consumer 从自己的 cursor 读取 canonical outbox，不需要预先复制 delivery row。
- Base：launch retry 提交同一 Product binding；repository 返回相同 receipt，Runtime activation
  evidence 仍只由 Runtime owner 保存。
- Bad：把 `facts.operations` 展开到 `agent_runtime_operation`，再要求每次状态变化同时
  upsert 两份事实。两份写入只有一个业务含义，任何冲突策略都会制造漂移窗口。
- Bad：用 `serde_json::to_vec` 的当前 map iteration order 计算 durable digest，或从
  `execution_profile`、source revision 等拆分列重新拼装 Product binding。
- Bad：Product worker先扫描全局 Runtime outbox，再为没有 Product binding 的 thread 报错；
  这把“尚无消费者”误建模成失败工作。

## 6. Tests Required

- 真实 PostgreSQL 测试覆盖 Runtime/Host/callback document 的首次提交、CAS conflict、精确
  replay 与 repository restart reload。
- Runtime operation 测试必须覆盖 `Accepted -> Running` 后重建 repository，断言 canonical
  document 中 status 为 `Running`。
- Product binding 测试只提交 Product intent，重启 Product repository 后断言 canonical
  binding 不变，且 Product repository 不读取 Host aggregate。
- Product tool authorization 测试使用不同的 Product surface-facts digest 与 Host
  compiled/applied-surface digest，断言共享 coordinate 匹配时授权成功；generation/source
  失配继续由 Host callback contract 拒绝。
- Product binding digest 单元测试必须用递归对象键置换断言 digest 相同；真实 PostgreSQL 测试
  必须覆盖多键 execution profile JSONB roundtrip、commit receipt、idempotent replay 和
  repository restart 后复算 digest。
- migration/readiness 断言 Product binding 只有 canonical `binding`、必要 coordinate/index
  columns 与 delivery state；不得恢复 execution profile/source revision 或 resource snapshot
  pin 镜像列。
- Product delivery 单元测试覆盖 per-consumer cursor 独立序列化；PostgreSQL 测试覆盖
  claim/ack/release、lease takeover、stale token，以及未激活/无 binding thread 不被 claim。
- Migration 测试运行最新 schema，断言三个 canonical revision table 与 Product delivery
  JSONB column 存在，所有 Runtime/Host/callback mirror table 和全局 delivery table 不存在。
- Product receipt 测试断言 Runtime operation coordinate 可作为 opaque evidence 保存，并由
  Product 自身约束拒绝“operation ID 存在但 thread ID 缺失”。

## 7. Wrong vs Correct

```rust
// Wrong: 一个 owner graph 同时写 JSONB 与逐实体镜像。
persist_runtime_document(&snapshot).await?;
replace_runtime_operation_rows(&snapshot.facts.operations).await?;
verify_mirrors_equal_document().await?;

// Correct: domain 校验完整 graph，repository 只提交 owner document。
let committed = apply_managed_runtime_state_commit(&mut current, commit)?;
persist_runtime_snapshot(&mut tx, &thread_id, &committed).await?;
```

```rust
// Wrong: serialization order becomes durable identity, then split columns rebuild another binding.
let digest = sha256(serde_json::to_vec(&binding)?);
let binding = map_binding_from_execution_profile_and_source_columns(row)?;

// Correct: canonical document is decoded and attested; runtime authority resolves exact frame.
let binding: AgentRunProductRuntimeBinding = decode(row.binding)?;
let digest = binding.calculated_digest()?;
ensure_eq!(digest, row.binding_digest)?;
bindings.commit_product_binding(&binding).await?;
let frame = frames.get(binding.launch_frame.frame_id).await?;
let authority = product_authority.resolve(&binding, &frame).await?;
```

```rust
// Wrong: Product activation copies Host generation and compares unrelated digest schemas.
ensure_eq!(product_surface.digest, host_applied_surface.digest)?;
product_binding.host_generation = host_binding.generation;

// Correct: each owner validates its own evidence; the bridge compares shared coordinates only.
host_callbacks.admit(callback_generation, callback_source, host_surface_digest)?;
product_authorizer.authorize(runtime_thread_id, source, agent_frame_surface_revision)?;
```

```rust
// Wrong: 一个全局 delivery row 串行调用所有 observer。
for observer in observers {
    observer.observe(&change).await?;
}
ack_shared_delivery(change.sequence).await?;

// Correct: 每个 consumer 只 claim/ack 自己的 Product-owned cursor。
let claims = delivery.claim(observer.consumer_name(), limit).await?;
for claim in claims {
    observer.observe(&claim.input).await?;
    delivery.ack(&claim).await?;
}
```
