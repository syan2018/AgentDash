# W7 — Recovery、Conformance、旧架构删除与最终集成

## Depends On

- W1–W6 全部完成并通过各自定向检查。

## Goal

在真实故障、重启、重复、cursor gap与并发下证明Hosted Agent会收敛；删除旧Runtime journal/state/schema/protocol全链路，并通过最终cross-layer门禁确认仓库只有一个会话状态机。

## Scope

- dispatch/settlement fault injection；
- process restart、lease reclaim、generation replacement；
- repository/driver conformance；
- Journal no-op/delete test；
- compaction端到端tracer bullet；
- negative search与legacy deletion；
- migration/generated/Rust/frontend最终门禁；
- 最终spec更新。

## Ownership

主要负责：

- cross-crate integration/fault/conformance tests
- 旧Runtime journal/state/interface/schema/protocol残留文件
- 最终generated output与migration guard修复
- `.trellis/spec/` 最终架构合同

本包是唯一允许做跨工作包旧路径最终删除与全仓残留清点的包；不得碰无关并行修改。

## Deliverables

- crash/restart/duplicate/stale/unknown recovery证据；
- in-memory/PostgreSQL behavior parity；
- Native/Codex/Remote conformance；
- Journal deletion证据；
- negative gate零业务残留；
- final compaction/reconnect UI tracer bullet；
- 更新后的可执行spec。

## Acceptance Criteria

- [ ] dispatch前/后崩溃、observation前后崩溃、lease reclaim均不复制effect/entity。
- [ ] Applied/NotApplied/Unknown在重启后得到同一terminal决策。
- [ ] generation stale observation不可写Agent state。
- [ ] Journal完全停用后read/resume/fork/compact/context/recovery/protocol仍成立。
- [ ] `RuntimeJournalFact|RuntimeJournalRecord|journal_records_after|append_presentation|ContextActivationDispatch`无业务路径残留。
- [ ] authoritative `agent_runtime_event` schema已删除。
- [ ] manual、queued、automatic A/B/C、clean failure、cancel、Lost与reconnect端到端通过。
- [ ] migration、contracts、Rust定向测试、frontend check与`pnpm check:quick`通过。
- [ ] 最终spec只记录正确ownership与原因，不记录临时任务过程。

## Non-Goals

- 不通过compatibility wrapper让negative gate“看起来通过”。
- 不在小步中反复运行全workspace重测试。
- 不扩展未在父PRD中的新driver或产品功能。

## Validation

执行父任务 `implement.md` W7 的完整negative gates与最终门禁。
