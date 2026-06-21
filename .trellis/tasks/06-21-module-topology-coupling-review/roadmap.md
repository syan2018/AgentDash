# Coupling Convergence Roadmap

本文件是本轮多模块耦合收敛的总入口。它追踪从 review 发现到父任务、工作项、阻塞关系和下一步执行顺序的状态，避免后续只记得局部 task 而丢失整体路线。

## Current Snapshot

| Cluster | Task | Status | Current Focus | Blocks / Unlocks |
| --- | --- | --- | --- | --- |
| Mechanical refactors | `.trellis/tasks/06-21-architecture-review-mechanical-refactors/` | completed | 已完成低风险机械整理 | 不阻塞后续 |
| Contract Boundary | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | in_progress, closeout check passed | A/C/E/F/G 已完成；B/D 等待 RC/CE read-model 收口 | CB04-B waits RC08；CB04-D waits CE03/CE04 |
| Runtime Coordinate | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | RC02-RC07 completed | current delivery consumers 与 SubjectExecutionView runtime history 已收束 | 解锁 RC08 AgentRun resource surface coordinate contract |
| Capability Exposure | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | CE02/CE05 completed | AgentRun capability/admission、Grant 分类投影、bulk expiry 与 active-runtime adoption helper 已落地 | 解锁 CE03 Canvas expose；CE04 waits CE03 |
| Control Surface | `.trellis/tasks/06-21-control-surface-command-boundary/` | CS02/CS04/CS05 completed | lifecycle command、extension backend target、command availability core resolver 已收束 | 当前无本轮阻塞项 |
| Runtime Failure / Placement | `.trellis/tasks/06-21-runtime-failure-placement-convergence/` | RF02/RF04 completed | backend disconnect 已投影 `turn_lost` / `lost`；session MCP no fallback 已落地 | 前端 `turn_lost` 展示文案可后续补齐 |

## Execution Order

1. Finish Contract Boundary closeout check.
   - Confirm no additional low-risk incoming DTO reverse conversion remains.
   - Keep CB04-B and CB04-D explicitly blocked by RC/CE.
   - Commit any final closeout-only documentation.

2. Implement Runtime Coordinate current delivery consumer convergence. `[completed]`
   - `LifecycleAgent` current delivery binding fields, migration and repository mapping are implemented.
   - `DeliveryRuntimeSelectionService` policies, output model and error semantics are implemented.
   - Dispatch and accepted-turn write Ready/Running binding; workspace/cancel/mailbox consumers now use CurrentDelivery selection.

3. Lock CE05, then implement Capability Exposure CE02-CE04.
   - Define AgentRun effective capability/admission service as the only runtime capability access path. `[completed]`
   - Treat AgentFrame as AgentRun model-visible surface revision.
   - Treat PermissionGrant as AgentRun-scoped Grant system: tool-internal permission becomes admission projection; toolset expansion becomes AgentFrame surface revision. `[completed]`
   - Bulk overdue expiry now applies the same AgentRun Grant classification path, and persisted AgentFrame revisions can be adopted into active runtime without writing another revision. `[completed]`
   - Next executable item: CE03 Canvas expose recovery.

4. Return to blocked Contract Boundary items.
   - CB04-B: split AgentRun workspace snapshot from generated contract DTO after RC08 defines resource coordinate DTO.
   - CB04-D: split Capability catalog read model after CE03/CE04 stabilizes exposure/visibility read models.

5. Continue Control Surface and Runtime Failure implementation.
   - CS02 lifecycle create/continue/drain and CS05 extension session backend target are complete.
   - RF02 backend disconnect lost projection and RF04 session MCP no-fallback are complete.
   - CS04 command availability core resolver is complete.

## Dependency Map

```text
Runtime Coordinate RC02-RC06
  -> RC08 AgentRun resource surface coordinate contract
  -> CB04-B AgentRun workspace snapshot split

Runtime Coordinate RC07
  -> SubjectExecutionView runtime_attempts history/latest convergence
  -> RC08 AgentRun resource surface coordinate contract

Capability Exposure CE02
  -> Canvas expose recovery
  -> WorkspaceModule visibility resolver
  -> CB04-D Capability catalog read model split

Control / Failure convergence
  -> lifecycle create/continue/drain command path
  -> extension session route backend target
  -> backend disconnect lost terminal projection
  -> session MCP route-bound no-fallback
  -> command availability core resolver
```

## Active Parallel Work

| Date | Work | Owner | Output |
| --- | --- | --- | --- |
| 2026-06-21 | Contract Boundary closeout check | `trellis-check` subagent | passed; only B/D remain blocked by RC/CE |
| 2026-06-21 | RC02 implementation-scope research | `trellis-research` subagent | `.trellis/tasks/06-21-runtime-coordinate-convergence/research/rc02-implementation-scope.md` |
| 2026-06-21 | CE02-CE04 implementation-scope research | `trellis-research` subagent | `.trellis/tasks/06-21-capability-exposure-fact-convergence/research/ce02-ce04-implementation-scope.md` |
| 2026-06-21 | RC02 current delivery binding implementation | `trellis-implement` subagent + main session integration | LifecycleAgent binding, migration, selection service, dispatch Ready / accepted-turn Running writes |
| 2026-06-21 | CE05 AgentRun capability boundary implementation | `trellis-implement` subagent | AgentRun effective capability/admission service; CapabilityResolver no longer consumes direct Grant override |
| 2026-06-21 | RC04-RC06 current delivery consumer migration | `trellis-implement` subagents | workspace query, subject/cancel control and mailbox target consume CurrentDelivery selection |
| 2026-06-21 | CE02 Grant classification first slice | `trellis-implement` subagent | admission-only Grant projection and toolset Grant AgentFrame surface revision path |
| 2026-06-21 | CS02/CS05 command and extension target convergence | `trellis-implement` subagents | lifecycle create/continue/drain commands; extension action/channel backend from session route |
| 2026-06-21 | RF02/RF04 failure placement convergence | `trellis-implement` subagents | backend disconnect `turn_lost`/`lost`; session MCP route-bound no-fallback |
| 2026-06-21 | RC07 SubjectExecutionView history convergence | `trellis-implement` subagent | `runtime_attempts` history with latest runtime node/artifacts derived from first item |
| 2026-06-21 | CE02 Grant expiry/adoption convergence | `trellis-implement` subagent | application-owned overdue expiry and persisted AgentFrame active-runtime adoption helper |
| 2026-06-21 | CS04 command availability convergence | `trellis-implement` subagent | route policy and UI snapshot share `ConversationCommandAvailabilityResolver` |

## Return Checklist

When resuming this roadmap:

- Read this file first, then each cluster `work-items/index.md`.
- RC08 is the next Runtime Coordinate item before reopening CB04-B.
- CE03 Canvas expose recovery is next in Capability Exposure; CE04 and CB04-D wait on it.
- Control Surface core items in this convergence batch are complete unless new command-surface coupling appears.
- `turn_lost` backend projection is implemented; frontend-specific wording can be tracked separately if product presentation needs it.
- Keep implementation work inside the owning parent task; use this roadmap only as the cross-cluster index.
