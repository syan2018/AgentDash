# Lifecycle 控制面长链路收敛与 Frame 化

## Goal

作为 6/2 Frame 化收敛的父任务，统一追踪已经完成和仍待派发的 child tasks。当前 RuntimeSession anchor、FrameLaunchEnvelope、Frontend runtime frame query 已归档完成；剩余收口点集中在 scoped lifecycle artifacts 和 structured active projection。

目标状态仍是：Frame 层锚定 runtime session 的可执行事实，Assignment 层锚定 Activity attempt 的执行事实，Session 层只承担 runtime trace / turn supervision / connector delivery 等运行边界事实。

## Current Status

Completed children:

- `06-02-runtime-session-frame-assignment-anchor`
- `06-02-frame-launch-envelope-session-boundary`
- `06-02-frontend-session-runtime-frame-query`

Remaining children:

- `06-02-scoped-lifecycle-artifacts`
- `06-02-lifecycle-run-active-projection-structure`

Known remaining code evidence:

- `provider_lifecycle.rs` still reads run-level `list_port_outputs(run_id)`.
- `orchestrator.rs` still derives activity outputs from run-level port maps.
- `lifecycle_runs.active_node_keys` still exists in migration, repository and domain entity.
- `advance_node` still outputs string `active_node_keys`.

## Requirements

- Dispatch `06-02-scoped-lifecycle-artifacts` to make output port, completion policy, hook gate and artifact binding use scoped artifact facts.
- Dispatch `06-02-lifecycle-run-active-projection-structure` to remove string active projection as business fact source.
- After both children complete, run parent final integration review for contracts, specs, migration and residual scans.
- Do not reopen archived children unless a regression is found in their exact acceptance scope.

## Acceptance Criteria

- [x] RuntimeSession can resolve Frame / Assignment / Activity attempt through direct anchor.
- [x] Session planner consumes `FrameLaunchEnvelope`.
- [x] Frontend runtime state query uses backend frame/runtime read model.
- [ ] Output port write/read, completion gate, hook gate and artifact binding use graph/activity/attempt scoped facts.
- [ ] Active Activity projection is structured and derived from graph instance state.
- [ ] Parent final scan shows no session-first / run-level string fallback in lifecycle control-plane runtime paths.

## Out Of Scope

- No new lifecycle branching/fork-join feature.
- No compatibility bridge for old artifact paths or active key strings.
