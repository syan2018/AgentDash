# W3 — Runtime State & Host Coordination

## Depends On

- W1 Contracts & Crate Skeleton

## Ownership

- `agentdash-agent-runtime`
- `agentdash-agent-runtime-host`
- Runtime/Host persistence ports and adapters
- final schema/constraint specification；正式 migration 由 W8 独占

## Goal

保留统一 Managed Runtime 外层，同时把 Runtime platform facts 与 Host coordination
分开。Runtime 拥有 operation/admission/projection/change；Host 拥有
service/offer/binding/effect/placement/recovery；二者都不拥有外部 Agent history。

## Exit Criteria

- Runtime State 不使用 Session 命名；
- Runtime snapshot + durable platform change 可 reconnect；
- Host stable effect identity + inspect/reconcile 覆盖 unknown outcome；
- stale generation/duplicate/late observation 被 fence；
- in-memory behavior suite 通过，且没有新增会由 W8 重写的正式 migration；
- W8/W9 将在唯一 final migration 上运行 PostgreSQL suite；
- `RuntimeJournalFact` 的 Runtime/Host 生产路径被 owner-specific facts 替代。
