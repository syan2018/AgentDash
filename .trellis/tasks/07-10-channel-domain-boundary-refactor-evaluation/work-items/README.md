# Channel Boundary Convergence Work Items

Parent task: `.trellis/tasks/07-10-channel-domain-boundary-refactor-evaluation`

本目录统一追踪父任务内部实施项。根 PRD/design/implement 仍是需求、设计与全局顺序的权威来源。

## Status

`planned -> implementing -> checking -> ready_for_integration -> done`

发现历史决策或目标合同错误时回到 `planned`，先更新父任务规划。

## Tracker

| ID | File | Status | Depends On | Last Evidence |
| --- | --- | --- | --- | --- |
| WI-00 | `WI-00-decision-residual-reconciliation.md` | done | 无 | residual matrix 与 owner evidence gate 已闭合 |
| WI-01 | `WI-01-extension-protocol-rename.md` | done | WI-00、canonical Operation contract | 全链原子改名、qualified identity 与 contract version checks 通过 |
| WI-02 | `WI-02-channel-domain-admission.md` | ready_for_integration | WI-00 | V2 identity、canonical participant、service admission 与 registry-derived capability projection targeted checks 通过 |
| WI-03 | `WI-03-owner-persistence-migration.md` | ready_for_integration | WI-02 | V2 destructive reset、typed row-lock mutation 与并发 create-if-absent repository contract 已落地 |
| WI-04 | `WI-04-binding-provider-delivery.md` | ready_for_integration | WI-02、WI-03 | mutation 自动投影、startup rebuild、跨 owner consistency 与 persistence failure gates 已闭合 |
| WI-05 | `WI-05-integration-spec-verification.md` | planned | WI-01 至 WI-04 | 父任务最终全量 gate |

## Decision Ledger

见 [decisions.md](./decisions.md)。当前 Channel 核心产品方向已有历史证据，不重复询问用户；若实现证据要求推翻，必须先回到 planning 并说明理由。

## Update Contract

- 每项开始前记录实际 write set，避免与另一个父任务重叠修改 shared contracts。
- targeted check 通过进入 `ready_for_integration`；只有 WI-05 全量 gate 后进入 `done`。
- residual closure 必须验证真实 registry identity 和 service admission，不能只验证 typed envelope 存在。
