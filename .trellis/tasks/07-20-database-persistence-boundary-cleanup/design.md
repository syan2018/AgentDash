# 数据库持久化边界清理设计

## 1. Design Goal

以 owner-owned JSONB aggregate 作为 Runtime 与 Complete Agent coordination 的唯一 durable
authority，删除同一事实的 normalized 关系镜像。PostgreSQL 负责 revision CAS、事务原子性和
JSONB durability；领域类型与 validator 负责 aggregate 内部不变量。

目标不是减少 JSONB，而是让每份 durable state 只有一个 owner、一个 canonical representation
和一个更新入口。

## 2. Authority Model

| Owner | Canonical durable representation | Process-local state |
| --- | --- | --- |
| Managed Runtime | `agent_runtime_state_revision`，每个 thread 一份 revisioned facts JSONB | gateway/worker handles |
| Complete Agent Host | `agent_runtime_host_revision`，一份 revisioned coordination facts JSONB | live catalog、attachment handle、availability |
| Complete Agent Callback | `agent_runtime_callback_revision`，一份 revisioned reservation/outcome JSONB | callback handler objects |
| Product / AgentRun | Product binding、saga、receipt 与 projection owner rows；复杂状态优先 JSONB | API/composition observers |
| Concrete Complete Agent | Agent-native store | live Agent process state |

跨 owner reference 使用 typed identity、digest、revision、generation、source coordinate 与
inspection evidence。不能为了建立数据库 FK 把另一个 owner 的 JSONB 内部事实复制成关系锚点。

## 3. Managed Runtime Persistence

`ManagedRuntimeStateRepository` 继续通过完整 `ManagedRuntimeStateCommit` 做 per-thread CAS。
PostgreSQL adapter只读写：

```text
agent_runtime_state_revision(thread_id, revision, facts)
```

删除 Runtime normalized 镜像：

```text
agent_runtime_source_projection
agent_runtime_source_identity
agent_runtime_source_change
agent_runtime_projection
agent_runtime_thread_binding
agent_runtime_operation
agent_runtime_idempotency
agent_runtime_pending_command
agent_runtime_change
agent_runtime_outbox
agent_runtime_surface_snapshot
```

`load` 解码 canonical facts 并调用完整 domain validator。`commit` 在锁定 thread row 后应用
domain transition，只更新 revision/facts。change page 从 `facts.changes` 读取，Product consumer
从 `facts.outbox` 读取。

JSONB aggregate 已经原子包含 operation、idempotency、pending intent、projection、change 与
outbox；不再实现第二套 SQL 状态推进、prefix 或 drift invariant。

## 4. Product Change Delivery

删除 `agent_runtime_product_change_delivery`。在
`agent_run_product_runtime_binding` 增加独立的 `change_delivery_state JSONB`，其内容按
consumer name保存：

```json
{
  "consumer": {
    "delivered_sequence": 12,
    "claim": {
      "owner": "worker",
      "token": "uuid",
      "expires_at_ms": 1
    },
    "attempt_count": 2,
    "last_error": null
  }
}
```

该字段不参与 immutable Product binding digest。Product delivery repository 以 Product binding
row lock/CAS claim单个 consumer，随后从对应 Runtime facts outbox选择下一条 sequence。成功只推进
该 consumer；失败只release该 consumer。

worker不扫描无 Product binding 的 Runtime threads。Create 期间早于 Product binding 的 change
保留在 Runtime outbox，binding提交后自然补消费。

### 4.1 Product Runtime Binding

Product binding row 保存一份 canonical `binding JSONB`。target、project、RuntimeThread 与 launch
frame scalar columns 只服务 owner lookup、unique/FK 与索引，并以数据库约束证明其坐标与文档
相同；repository 不使用这些列重建 binding。

immutable digest 使用带 schema identity 的递归 canonical JSON，object key 字典序稳定、array
保持业务顺序。binding 只包含 Product 拥有的 target、RuntimeThread、精确 AgentFrame 与
execution profile；Runtime source/applied/activation evidence、Host binding identity/generation、
compiled surface 与 callback route 都不复制进 Product row。delivery state 是 Product owner
自己的可变消费进度，不进入 immutable digest。

`commit_product_binding` 和 frame replacement 返回 committed receipt。Runtime Create/Activate
只推进 Managed Runtime owner document；Product launch/recovery 验证 Runtime applied surface
是否匹配 binding-pinned AgentFrame，不产生第二次 Product 写入。

## 5. Complete Agent Host and Callback

Host repository只读写 `agent_runtime_host_revision`；Callback repository只读写
`agent_runtime_callback_revision`。删除：

```text
agent_runtime_lifecycle_target
agent_runtime_lifecycle_effect
agent_runtime_binding
agent_runtime_source_coordinate
agent_runtime_callback_route
agent_runtime_callback_route_tombstone
agent_runtime_effect
agent_runtime_effect_attempt_history
agent_runtime_lease
agent_runtime_lease_epoch
agent_runtime_callback_reservation
agent_runtime_callback_outcome
```

现有 Host/Callback domain validator、revision conflict、idempotent replay、effect monotonicity、
lease fencing和callback reservation/outcome不变量继续生效。删除的只是第二 representation。

Product recovery和其它跨 owner调用不再直接 JOIN这些表，改读Host公开的typed
snapshot/inspection接口。Infrastructure adapter不能把JSON path查询暴露成新的隐式Host read API。

## 6. Migration

新增下一序号 forward migration：

1. 为 Product Runtime binding 增加 `change_delivery_state JSONB NOT NULL DEFAULT '{}'`及object
   shape约束。
2. 通过删除 normalized anchor 移除 Product/recovery schema 指向 Runtime/Host 内部集合的外键。
3. 保留三个 owner 的 canonical revision facts；它们的 domain schema 未改变，Product binding
   原位获得空 delivery state。
4. 删除 Runtime、Product delivery、Host和Callback normalized镜像表。
5. `0092_product_host_pin_boundary.sql` 从已经执行过 `0091` 的开发库 forward 删除 Product row
   上的 Host binding id/generation。
6. `0093_agent_frame_runtime_authority.sql` 删除 Product resource surface 表、snapshot pin，并
   清理旧 recovery phase。
7. `0094_runtime_evidence_single_writer.sql` 清理携带 Runtime evidence 副本的 Product binding、
   recovery saga 与 Workspace presentation 文档，并删除 presentation source revision 拆分列。
8. readiness 将删除表纳入 retired 集合，并校验 Product delivery JSONB column。

Provider、credential、Product execution profile与其它owner facts保持原有authority。

## 7. Failure and Concurrency

- Runtime仍以每thread revision CAS隔离并发。
- Host/Callback继续使用现有revision CAS；本轮不改变其domain transaction范围。
- Product binding 与 Managed Runtime 是单向意图/结果链：Product 写 binding intent，Runtime
  写 applied evidence。Host callback 使用 Host-owned generation/source/compiled-surface evidence
  拒绝 stale 调用；Product command 只把 AgentFrame revision 与 Runtime applied surface 做派生校验。
- Product consumer claim使用数据库时钟、owner/token/expiry；stale token不能ack新claim。
- observer外部副作用与cursor ack无法跨事务原子，因此保持at-least-once；每个observer必须是
  idempotent/convergent，独立cursor避免跨observer重放。
- canonical JSONB decode、validation或revision drift继续fail-fast；optional live adapter
  availability仍按live catalog策略隔离。

### 7.1 Revision ownership

Revision 只在它能证明一个明确并发命题时存在：

- repository aggregate revision：owner transaction 内部 CAS baseline，不跨 repository seam；
- snapshot/change revision：观察版本、durable cursor 与诊断 evidence，不作为所有命令的总锁；
- command-specific coordinate：active turn、interaction、fork cutoff、binding source、generation
  等直接进入对应命令并表达真实业务前置条件。

因此 Product/API 不再执行“读取 snapshot revision 后原样回传”的 read-before-write。Runtime
admission 以 current canonical facts 判定 availability，并把 command-specific coordinate
冻结进 durable operation；repository commit 仍用本次 load 的 aggregate revision 保证原子性。
availability、receipt、outbox 或其它无关事实推进不会制造命令冲突。

### 7.2 Product runtime authority

Product binding 已经冻结精确 AgentFrame identity，因此 VFS/capability authority 可由该 frame
确定性重建。Task grant 则是当前 Product subject association 的访问策略，必须在授权时读取才能
保证撤权即时生效。这两部分都没有独立写入命令或生命周期，不构成 aggregate。

最终读取链路是 `Product binding -> exact AgentFrame + current Product associations -> transient
authority`。API surface、VFS resolver、workspace module 和 runtime tool authorizer 共享这一
resolver；launch、recovery 与 surface update 只推进 binding/Runtime/Host 事实，不物化资源快照。
Host compiled/applied surface 仍由 Host owner 保存并负责 callback fencing。

### 7.3 AgentFrame owner-local document

AgentFrame 是 LifecycleAgent 内部的 revision history，而不是跨 Agent 共享的全局实体。最终形态
应把 frame history 收进 LifecycleAgent owner JSONB（或等价的严格 agent-scoped document），
每个 revision 内以单一 surface document 原子保存 capability、context、VFS、MCP、execution
profile 与 hook plan。Product binding 只引用 owner 内的精确 frame identity。

实施顺序是先把 repository seam 从全局 `get(frame_id)` 收口为 agent-scoped exact lookup，再迁移
现有 frame rows，最后删除没有独立业务生命周期的 transition/split storage。这样删除 Agent 时只
删除 owner document，不需要跨大量局部事实表设计级联技巧。

## 8. Rejected Designs

- 保留normalized表并补齐upsert：继续维护第二套状态机，不能消除drift根因。
- claim Runtime outbox时增加Product binding EXISTS：只隐藏合法启动顺序，仍保留全局共同delivery。
- 将JSONB拆成更多关系canonical表：与项目持久化取向和现有aggregate validator重复，增加接口与
  迁移复杂度。
- 为跨owner reference保留空壳anchor表：anchor仍会成为第二authority，并诱导直接SQL JOIN。

## 9. Validation

- Runtime operation status advancement、source projection、change/outbox、restart replay通过
  canonical repository interface验证。
- Product delivery覆盖binding前outbox、binding后catch-up、每consumer独立cursor、claim expiry和
  stale token。
- Host/Callback覆盖exact target、effect/lease、callback reservation/outcome的restart replay。
- Product binding 覆盖多键/嵌套 JSONB roundtrip、commit receipt、activation replay 和
  repository restart digest复验。
- migration从当前开发schema顺序升级，空库得到相同最终schema。
- 负向搜索确认镜像表名、`replace_*_projection`、`verify_*_projection`与直接Host JOIN生产引用为零。
