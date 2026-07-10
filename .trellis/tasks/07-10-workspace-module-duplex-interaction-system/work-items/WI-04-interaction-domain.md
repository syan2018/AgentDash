# WI-04 Interaction Domain

Status: planned

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
