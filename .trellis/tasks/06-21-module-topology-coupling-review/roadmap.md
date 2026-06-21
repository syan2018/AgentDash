# Coupling Convergence Roadmap

本文件是本轮多模块耦合收敛的总入口。它追踪从 review 发现到父任务、工作项、阻塞关系和下一步执行顺序的状态，避免后续只记得局部 task 而丢失整体路线。

## Current Snapshot

| Cluster | Task | Status | Current Focus | Blocks / Unlocks |
| --- | --- | --- | --- | --- |
| Mechanical refactors | `.trellis/tasks/06-21-architecture-review-mechanical-refactors/` | completed | 已完成低风险机械整理 | 不阻塞后续 |
| Contract Boundary | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | in_progress, closeout check passed | A/C/E/F/G 已完成；B/D 等待上游 | CB04-B waits RC02；CB04-D waits CE02-04 |
| Runtime Coordinate | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | RC02 completed | 已落地 LifecycleAgent current delivery binding、selection service 与 Ready/Running 写入点 | 解锁 CB04-B、RC04 workspace、RC05/RC06 control target、RC07 SubjectExecutionView history |
| Capability Exposure | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | CE05 completed, CE02 implementation-ready | 已建立 AgentRun effective capability/admission 边界，resolver 不再直读 Grant；下一步收敛 PermissionGrant 分类投影 | 解锁 CE02；后续解锁 CB04-D、Canvas expose、WorkspaceModule visibility |
| Control Surface | `.trellis/tasks/06-21-control-surface-command-boundary/` | planning | 已定 create-only run、continue/drain、terminal outbox、session extension binding | 依赖 RC selection service 承接 cancel/command target |
| Runtime Failure / Placement | `.trellis/tasks/06-21-runtime-failure-placement-convergence/` | planning | 已定 backend disconnect -> `turn_lost` / `lost`，session MCP no fallback | 与 Control Surface / RC delivery status 对齐 |

## Execution Order

1. Finish Contract Boundary closeout check.
   - Confirm no additional low-risk incoming DTO reverse conversion remains.
   - Keep CB04-B and CB04-D explicitly blocked by RC/CE.
   - Commit any final closeout-only documentation.

2. Implement Runtime Coordinate RC02. `[completed]`
   - `LifecycleAgent` current delivery binding fields, migration and repository mapping are implemented.
   - `DeliveryRuntimeSelectionService` policies, output model and error semantics are implemented.
   - Dispatch and accepted-turn write Ready/Running binding; workspace/cancel/mailbox consumers remain next-step migrations.

3. Lock CE05, then implement Capability Exposure CE02-CE04.
   - Define AgentRun effective capability/admission service as the only runtime capability access path. `[completed]`
   - Treat AgentFrame as AgentRun model-visible surface revision.
   - Treat PermissionGrant as AgentRun-scoped Grant system: tool-internal permission becomes admission projection; toolset expansion becomes AgentFrame surface revision.
   - Fold replaced direct paths into the AgentRun resolver after it exists.
   - Next executable item: CE02 PermissionGrant classification and AgentRun projection.

4. Return to blocked Contract Boundary items.
   - CB04-B: split AgentRun workspace snapshot from generated contract DTO after RC02 stabilizes.
   - CB04-D: split Capability catalog read model after CE exposure fact source stabilizes.

5. Continue Control Surface and Runtime Failure implementation.
   - Align continue/drain/cancel/mailbox/terminal outbox with RC selection.
   - Align backend disconnect lost projection with delivery status semantics.

## Dependency Map

```text
Runtime Coordinate RC02
  -> CB04-B AgentRun workspace snapshot split
  -> RC04 workspace target convergence
  -> RC05 / RC06 cancel and mailbox target convergence
  -> RC07 SubjectExecutionView history/latest convergence

Capability Exposure CE02-CE04
  -> CB04-D Capability catalog read model split
  -> PermissionGrant AgentRun admission/toolset expansion convergence
  -> Canvas expose recovery
  -> WorkspaceModule visibility resolver

Contract Boundary closeout
  -> keeps DTO ownership work bounded
  -> leaves only RC/CE-dependent items for later return
```

## Active Parallel Work

| Date | Work | Owner | Output |
| --- | --- | --- | --- |
| 2026-06-21 | Contract Boundary closeout check | `trellis-check` subagent | passed; only B/D remain blocked by RC/CE |
| 2026-06-21 | RC02 implementation-scope research | `trellis-research` subagent | `.trellis/tasks/06-21-runtime-coordinate-convergence/research/rc02-implementation-scope.md` |
| 2026-06-21 | CE02-CE04 implementation-scope research | `trellis-research` subagent | `.trellis/tasks/06-21-capability-exposure-fact-convergence/research/ce02-ce04-implementation-scope.md` |
| 2026-06-21 | RC02 current delivery binding implementation | `trellis-implement` subagent + main session integration | LifecycleAgent binding, migration, selection service, dispatch Ready / accepted-turn Running writes |
| 2026-06-21 | CE05 AgentRun capability boundary implementation | `trellis-implement` subagent | AgentRun effective capability/admission service; CapabilityResolver no longer consumes direct Grant override |

## Return Checklist

When resuming this roadmap:

- Read this file first, then each cluster `work-items/index.md`.
- Check RC04/CE02 before reopening CB04-B/D; RC02 and CE05 are now complete, but CB04-B/D still depend on consumer/read-model migration semantics.
- Do not reopen CB04-D until CE02-CE04 stabilize; CB04-B can be revisited after RC04 workspace migration clarifies the generated DTO split surface.
- Keep implementation work inside the owning parent task; use this roadmap only as the cross-cluster index.
