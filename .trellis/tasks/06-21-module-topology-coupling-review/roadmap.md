# Coupling Convergence Roadmap

本文件是本轮多模块耦合收敛的总入口。它追踪从 review 发现到父任务、工作项、阻塞关系和下一步执行顺序的状态，避免后续只记得局部 task 而丢失整体路线。

## Current Snapshot

| Cluster | Task | Status | Current Focus | Blocks / Unlocks |
| --- | --- | --- | --- | --- |
| Mechanical refactors | `.trellis/tasks/06-21-architecture-review-mechanical-refactors/` | completed | 已完成低风险机械整理 | 不阻塞后续 |
| Contract Boundary | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | in_progress, closeout check passed | A/C/E/F/G 已完成；B/D 等待上游 | CB04-B waits RC02；CB04-D waits CE02-04 |
| Runtime Coordinate | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | planning, RC02 implementation-ready | RC02 current delivery binding / selection service 已补齐实现级规划 | 解锁 CB04-B、workspace/cancel/mailbox/SubjectExecutionView 迁移 |
| Capability Exposure | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | planning, CE05 first | 收束为 AgentRun effective capability/admission 唯一路径；PermissionGrant 是 AgentRun 独立授权/护栏系统，由 AgentRun 投影为 final capability 或 admission decision | 解锁 CB04-D、Canvas expose、WorkspaceModule visibility；清理 grant/live-cache/local-helper 旁路 |
| Control Surface | `.trellis/tasks/06-21-control-surface-command-boundary/` | planning | 已定 create-only run、continue/drain、terminal outbox、session extension binding | 依赖 RC selection service 承接 cancel/command target |
| Runtime Failure / Placement | `.trellis/tasks/06-21-runtime-failure-placement-convergence/` | planning | 已定 backend disconnect -> `turn_lost` / `lost`，session MCP no fallback | 与 Control Surface / RC delivery status 对齐 |

## Execution Order

1. Finish Contract Boundary closeout check.
   - Confirm no additional low-risk incoming DTO reverse conversion remains.
   - Keep CB04-B and CB04-D explicitly blocked by RC/CE.
   - Commit any final closeout-only documentation.

2. Implement Runtime Coordinate RC02.
   - Define `LifecycleAgent` current delivery binding fields, migration and repository roundtrip.
   - Define `DeliveryRuntimeSelectionService` policies, output model and error semantics.
   - Dispatch RC02 implementation before migrating workspace/cancel/mailbox consumers.

3. Lock CE05, then implement Capability Exposure CE02-CE04.
   - Define AgentRun effective capability/admission service as the only runtime capability access path.
   - Treat AgentFrame as AgentRun model-visible surface revision.
   - Treat PermissionGrant as AgentRun-scoped Grant system: tool-internal permission becomes admission projection; toolset expansion becomes AgentFrame surface revision.
   - Fold replaced direct paths into the AgentRun resolver after it exists.

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
  -> workspace / cancel / mailbox target convergence
  -> SubjectExecutionView history/latest convergence

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

## Return Checklist

When resuming this roadmap:

- Read this file first, then each cluster `work-items/index.md`.
- Check whether RC02 and CE02-CE04 research files exist and whether their parent `implement.md` files were updated from them.
- Do not reopen CB04-B/D until their resume conditions in `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/cb04/README.md` are satisfied.
- Keep implementation work inside the owning parent task; use this roadmap only as the cross-cluster index.
