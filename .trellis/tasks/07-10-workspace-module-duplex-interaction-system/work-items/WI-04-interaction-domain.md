# WI-04 Interaction Domain

Status: done

Depends On: WI-00

## Scope

- immutable V1 InteractionDefinitionRevision + SourceBundle/lineage/VFS changeset CAS、InteractionInstance、Attachment、RuntimeBinding。
- `state_patch_v1` path allowlist/schema/limits、平台 versioned state transition、event/state revision、idempotency 与 audit。
- replay-safe single OperationEffectIntent transaction/outbox/dispatcher。
- definition scope 派生 owner；User/Agent role/capability projection、subscription 与 explicit close/retention。
- `direct` / `human_only` Agent command policy；Channel suggestion 不写 canonical state。
- instance definition revision 与 exact Extension artifact digest pinning。
- PostgreSQL migrations/repositories/application use cases/contracts。

## Exit Criteria

- canonical state 只有一个事实源。
- Human/Agent 使用同一 typed command use case；`human_only` 明确拒绝 Agent write。
- state transition 由有限平台 command/typed handler 确定性执行并可重建 state。
- `state_patch_v1` 与 event/state 在单事务提交，Component binding 只做 payload pass-through。
- durable effect 只接受 replay-safe 单 Operation，复杂多步执行进入 Workflow。
- 不存在 generic reducer registry、proposal aggregate、durable draft 或通用 state migration engine。
- owner、attachment、presentation、renderer lease 生命周期互不替代。

## Validation

- SourceBundle/changeset CAS、`state_patch_v1` path/schema/size、expected revision/conflict/idempotency/event ordering/state rebuild tests。
- OperationEffectIntent atomic write/claim/replay/idempotency tests。
- direct/human_only 与 Channel suggestion boundary tests。
- ownership/permission/secret projection tests。
- migration/repository concurrency tests。

## Evidence

- `776349dd`：V1 definition/source/instance/command/event/effect 与 repository/transaction ports。
- `cf01f178`：pinned definition command resolution、canonical SourceBundle/patch、typed binding/audit 与唯一 OperationRef。
- `e806d312`：PostgreSQL definition/instance/event/effect repository、原子 command transaction 与 effect claim 实现。
- Stage B（本提交）：server-owned caller admission、确定性 command/state rebuild、typed close 与 lease-aware effect dispatcher。
- Domain Interaction 13 tests、Application Interaction 6 tests、Infrastructure transaction validation test 与三层 production checks 已通过。

## Remaining Integration Gates

- WI-05 在最终 `0062` 中同时创建 Interaction schema、删除旧 Canvas 五表，并运行真实 PostgreSQL transaction/concurrency tests。
- API/AppState、canonical OperationExecutionCore adapter、subscription/frontend projection 与 Channel suggestion 边界尚待对应 work item 接线。
- 完成上述 gate 前保持 `in_progress`，不标记 `ready_for_integration`。
