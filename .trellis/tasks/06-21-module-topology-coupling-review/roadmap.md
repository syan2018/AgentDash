# Coupling Convergence Roadmap

本文件是本轮多模块耦合收敛的总入口。它追踪从 review 发现到父任务、工作项、阻塞关系和下一步执行顺序的状态，避免后续只记得局部 task 而丢失整体路线。

## Current Snapshot

| Cluster | Task | Status | Current Focus | Blocks / Unlocks |
| --- | --- | --- | --- | --- |
| Mechanical refactors | `.trellis/tasks/06-21-architecture-review-mechanical-refactors/` | completed | 已完成低风险机械整理 | 不阻塞后续 |
| Contract Boundary | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | in_progress, closeout check passed | A/C/E/F/G 已完成；B/D 等待 RC/CE read-model 收口 | CB04-B waits RC07/RC08；CB04-D waits CE03/CE04 |
| Runtime Coordinate | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | RC02-RC06 completed | current delivery binding、selection service、workspace/cancel/mailbox consumers 已统一到 CurrentDelivery | 解锁 RC07 SubjectExecutionView history；RC08 waits RC07 |
| Capability Exposure | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | CE05 completed, CE02 first slice landed | AgentRun effective capability/admission 与 Grant 分类投影已落地；剩余 bulk expiry owner path 与 active-runtime adoption helper | 后续解锁 CE03 Canvas expose、CE04 WorkspaceModule visibility、CB04-D |
| Control Surface | `.trellis/tasks/06-21-control-surface-command-boundary/` | CS02/CS05 completed | lifecycle create/continue/drain 命令拆分完成；extension session backend target 统一到 session route | CS04 command availability core resolver 仍待处理 |
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
   - Treat PermissionGrant as AgentRun-scoped Grant system: tool-internal permission becomes admission projection; toolset expansion becomes AgentFrame surface revision. `[first slice landed]`
   - Next executable item: CE02 bulk expiry owner path / active-runtime adoption helper, then CE03 Canvas expose recovery.

4. Return to blocked Contract Boundary items.
   - CB04-B: split AgentRun workspace snapshot from generated contract DTO after RC07/RC08 defines execution history/resource coordinate DTO.
   - CB04-D: split Capability catalog read model after CE03/CE04 stabilizes exposure/visibility read models.

5. Continue Control Surface and Runtime Failure implementation.
   - CS02 lifecycle create/continue/drain and CS05 extension session backend target are complete.
   - RF02 backend disconnect lost projection and RF04 session MCP no-fallback are complete.
   - Next executable control item: CS04 command availability core resolver.

## Dependency Map

```text
Runtime Coordinate RC02-RC06
  -> RC07 SubjectExecutionView history/latest convergence
  -> RC08 AgentRun resource surface coordinate contract
  -> CB04-B AgentRun workspace snapshot split

Capability Exposure CE02 first slice
  -> CE02 bulk expiry / active-runtime adoption helper
  -> Canvas expose recovery
  -> WorkspaceModule visibility resolver
  -> CB04-D Capability catalog read model split

Control / Failure convergence
  -> lifecycle create/continue/drain command path
  -> extension session route backend target
  -> backend disconnect lost terminal projection
  -> session MCP route-bound no-fallback
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

## Return Checklist

When resuming this roadmap:

- Read this file first, then each cluster `work-items/index.md`.
- RC07/RC08 are the next Runtime Coordinate items before reopening CB04-B.
- CE02 still has bulk expiry / active-runtime adoption helper remaining before CE03/CE04 and CB04-D.
- CS04 is the next Control Surface item if command availability needs UI/API unification.
- `turn_lost` backend projection is implemented; frontend-specific wording can be tracked separately if product presentation needs it.
- Keep implementation work inside the owning parent task; use this roadmap only as the cross-cluster index.
