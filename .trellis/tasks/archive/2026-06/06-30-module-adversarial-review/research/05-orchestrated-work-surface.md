# Research: Orchestrated Work Surface

- Query: 单域对抗性架构审查：Workflow / Lifecycle / Orchestration / Task / Companion / Routine gates 的事实源、状态机、入口 API/tool/frontend、跨模块依赖、路径冗余、概念分叉、重复事实源、模块过厚、抽象泄漏、横向耦合、命名/职责漂移，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Summary

当前 Orchestrated Work Surface 相比 06-14 baseline 已完成几处关键收束：

- Lifecycle cancel 已从直接改 `RuntimeNodeState` 收束为 `OrchestrationRuntimeEvent::NodeCancelled` reducer 路径。
- `/lifecycle-runs` 已拆成 create、continue/drain 和显式 create-and-continue，不再把 start route 隐式等同于 drain。
- Task boot projection 不再写入推断状态；Task execution 主读模型已收束到 `SubjectExecutionView`。
- AgentRun workspace command/control 已明显向 `AgentConversationSnapshot` 收敛，contract 不再暴露顶层 `actions`。

仍值得拆后续实现任务的问题主要不是 reducer 主链路，而是边界周边残留：

1. Companion capability grant payload 仍作为 companion request/result 协议存在，但没有 PermissionGrant broker 闭环；现在不会写授权事实，却仍能创建不可闭环的人类 gate。
2. Routine execution history 暴露 `RoutineExecution.status` dispatch ledger，却不投影 Lifecycle/Agent terminal 状态，用户可见历史会长期停在 `dispatched`。
3. `LifecycleDispatchService` 仍是横跨 run/orchestration/agent/frame/session/association/gate/lineage 的厚 transaction script；06-14 的“过厚 facade”只部分收束。
4. Companion gate control 把 durable gate 状态、parent/child/human delivery、mailbox delivery 和 runtime trace resolution 合在一个 service；目前正确使用 `LifecycleGate`，但模块边界仍偏厚。

### Files Found

- `.trellis/workflow.md` - Trellis task/research workflow and persistence rules.
- `.trellis/tasks/06-30-module-adversarial-review/check.jsonl` - this review's curated spec and baseline manifest.
- `.trellis/tasks/06-30-module-adversarial-review/prd.md` - task requirements and candidate topology.
- `.trellis/tasks/06-30-module-adversarial-review/design.md` - adversarial review lens and evidence contract.
- `.trellis/tasks/06-30-module-adversarial-review/implement.md` - execution and validation plan.
- `.trellis/spec/backend/workflow/architecture.md` - target Workflow/Lifecycle/Orchestration vocabulary and invariants.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - reducer, runtime node, executor launcher, human gate contracts.
- `.trellis/spec/backend/story-task-runtime.md` - Story/Task/LifecycleRun/SubjectExecution ownership contract.
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - frontend lifecycle/runtime read model boundary.
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - historical baseline summary.
- `.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md` - 06-14 lifecycle/workflow/task baseline.
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 06-14 AgentRun/session baseline.
- `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` - 06-14 companion/permission baseline.
- `crates/agentdash-domain/src/workflow/entity.rs` - `LifecycleRun` aggregate, task facts, status aggregation.
- `crates/agentdash-application-workflow/src/orchestration/runtime.rs` - orchestration reducer and runtime node status materialization.
- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` - ready node drain, Function/Agent/Human executor launch.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - lifecycle/subject dispatch use case and graph-backed materialization.
- `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs` - create/continue/create-and-continue command boundary.
- `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs` - subject cancel control.
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs` - `LifecycleRunView` and `SubjectExecutionView` read model builder.
- `crates/agentdash-application/src/task/view_projector.rs` - no-op task boot projection after SubjectExecutionView consolidation.
- `crates/agentdash-application/src/task/fanout.rs` - Task fanout to subject execution dispatch.
- `crates/agentdash-application/src/routine/executor.rs` - Routine trigger admission, dispatch, and reuse mailbox path.
- `crates/agentdash-application/src/routine/dispatch.rs` - Routine dispatch strategy to `SubjectExecutionIntent`.
- `crates/agentdash-application/src/routine/reuse_resolver.rs` - Routine reuse target validation against lifecycle refs and subject association.
- `crates/agentdash-domain/src/routine/entity.rs` - Routine and RoutineExecution ledger facts.
- `crates/agentdash-contracts/src/runtime/routine.rs` - Routine and RoutineExecution frontend contract.
- `packages/app-web/src/features/routine/execution-history-panel.tsx` - Routine execution history UI.
- `crates/agentdash-application/src/companion/tools.rs` - `companion_request` / `companion_respond` runtime tools and platform broker guard.
- `crates/agentdash-application/src/companion/payload_types.rs` - companion payload registry and capability grant JSON validators.
- `crates/agentdash-application/src/companion/gate_control.rs` - durable companion gate control and mailbox delivery.
- `crates/agentdash-api/src/routes/companion_gates.rs` - human companion gate response API.
- `packages/app-web/src/features/session/model/companionRequestViewModel.ts` - companion request view model.
- `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx` - companion request card UI.

### Code Patterns

- Correct reducer path: `apply_orchestration_event_to_run` mutates an `OrchestrationInstance`, then calls `run.refresh_status_from_orchestrations()` and updates run activity timestamps (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:266`).
- Correct cancel path: subject cancel resolves the runtime anchor and submits `OrchestrationRuntimeEvent::NodeCancelled`, then persists the returned run (`crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:221`).
- Correct start/continue split: `create_lifecycle_run` only calls `LifecycleDispatchService::start_lifecycle_run` (`crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs:48`), while `continue_lifecycle_run` owns `drain_ready_nodes` (`crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs:74`).
- Correct Task execution read path: `build_subject_execution_view` starts from `list_by_subject`, resolves runtime attempts, fills `latest_runtime_node` and `artifacts` (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:105`).
- Risk pattern: companion capability grant still has a standalone JSON request/result protocol even though platform broker returns a hard missing-broker error (`crates/agentdash-application/src/companion/payload_types.rs:86`, `crates/agentdash-application/src/companion/tools.rs:1396`).
- Risk pattern: Routine history API maps `RoutineExecution` directly to response status (`crates/agentdash-api/src/routes/routines.rs:244`, `crates/agentdash-contracts/src/runtime/routine.rs:74`), while domain says terminal status should be derived from Lifecycle/Agent projection (`crates/agentdash-domain/src/routine/entity.rs:204`).
- Risk pattern: lifecycle dispatch facade still owns many side effects in one transaction path (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:529`).

### 06-14 Baseline 回归对照

- Resolved: 06-14 P0 "cancel must go through reducer". Current code routes cancel through `NodeCancelled` (`subject_execution_control.rs:243`) and reducer owns cancellation node mutation (`runtime.rs:385`).
- Resolved: 06-14 P1 "lifecycle start API mixes create and drain". Current routes expose `/lifecycle-runs`, `/lifecycle-runs/{id}/continue`, `/lifecycle-runs/{id}/drain`, plus explicitly named `/lifecycle-runs/commands/create-and-continue` (`crates/agentdash-api/src/routes/workflows.rs:136`).
- Resolved: 06-14 P0 "Task boot projection uses wrong association scope and absence -> Failed fallback". Current `project_task_views_on_boot` intentionally no-ops and logs that runtime state is derived through `SubjectExecutionView` (`crates/agentdash-application/src/task/view_projector.rs:45`).
- Resolved: 06-14 P1 "SubjectExecutionView exposes fields but does not fill latest node/artifacts". Current builder fills `runtime_attempts`, `latest_runtime_node`, and `artifacts` from runtime anchors and orchestration node state (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:117`).
- Mostly resolved: 06-14 P1 "AgentRun workspace duplicates top-level actions/mailbox/control". Current `AgentRunWorkspaceView` has no top-level `actions`; API derives `control_plane` from `conversation.execution` (`crates/agentdash-contracts/src/runtime/workflow.rs:1167`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1021`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1459`).
- Partially resolved: 06-14 P1 "RuntimeSession runtime-control is second AgentRun command surface". Current `SessionRuntimeControlView` no longer includes mailbox/actions, but still returns run/agent/frame/subject context under a `runtime-control` name (`crates/agentdash-contracts/src/runtime/workflow.rs:1419`, `crates/agentdash-api/src/routes/sessions.rs:153`).
- Residual: 06-14 P2 "LifecycleDispatchService too thick". It is now in `agentdash-application-lifecycle` and delegates frame construction through ports, but `dispatch_common` still performs graph planning, run/orchestration update, agent creation, association, runtime session, frame, lineage, gate, anchor, and `NodeStarted` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:529`).
- Residual but lower risk: 06-14 P0 "PermissionGrant and companion grant double facts". Current frontend no longer submits authorization from the companion card, and platform target hard-fails without broker, but the companion capability grant request/result payload remains as a valid request type (`crates/agentdash-application/src/companion/payload_types.rs:86`).

### Issue 1: Companion capability grant payload is still a protocol fork without a PermissionGrant broker

- Priority: P1
- Problem type: 概念分叉 / 重复授权入口残留 / gate 死路
- Code evidence:
  - Companion payload registry still registers `capability_grant_request` with `response_type = capability_grant_result` and `ui_hint = capability_grant_card` (`crates/agentdash-application/src/companion/payload_types.rs:86`).
  - The same registry validates grant scopes as `turn | session | workflow_step` (`crates/agentdash-application/src/companion/payload_types.rs:283`), while the current formal permission scope enum is `turn | agent_frame | activity` (`crates/agentdash-contracts/src/system/permission.rs:6`, `crates/agentdash-domain/src/permission/value_objects.rs:8`).
  - `target=platform` explicitly rejects `capability_grant_request` because `PermissionGrantService::request` policy inputs and live runtime capability update handoff are missing (`crates/agentdash-application/src/companion/tools.rs:1396`, `crates/agentdash-application/src/companion/tools.rs:2006`).
  - `target=human` can still create a durable `LifecycleGate` for payload type `capability_grant_request` when `wait=true` (`crates/agentdash-application/src/companion/tools.rs:1213`).
  - Frontend recognizes capability grant cards but intentionally hides response controls and says authorization is owned by PermissionGrant (`packages/app-web/src/features/session/model/companionRequestViewModel.ts:42`, `packages/app-web/src/features/session/model/companionRequestViewModel.ts:83`, `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx:67`).
- Impact:
  - The system no longer writes false authorization facts, which is good, but an agent can still create a human companion gate for a capability grant that the UI will not resolve.
  - Capability grant vocabulary remains split across companion payload, PermissionGrant domain, generated contract, and frontend view model.
  - Future broker implementation may accidentally treat companion response JSON as authorization result instead of creating a `PermissionGrant`.
- Containment boundary:
  - `PermissionGrantService` and generated permission contract should remain the only authorization fact source.
  - Companion should either remove `capability_grant_request/result` from built-in request/response types until broker exists, or make `target=platform capability_grant_request` the only accepted path and return a formal `PermissionGrantResponse` / grant id.
  - Human companion gate should not accept capability grant payloads as ordinary wait gates.

### Issue 2: Routine execution history exposes dispatch ledger status as user-visible execution status

- Priority: P1
- Problem type: 投影不完整 / 重复事实源 / 命名职责漂移
- Code evidence:
  - Domain explicitly documents `RoutineExecution::mark_dispatched` as "submitted to control plane" and says real terminal status is derived from LifecycleRun / Agent projection (`crates/agentdash-domain/src/routine/entity.rs:204`).
  - `RoutineExecutionStatus` only has `pending / dispatched / failed / skipped`; there is no completed/cancelled lifecycle terminal shape (`crates/agentdash-domain/src/routine/entity.rs:225`).
  - API `list_executions` reads `routine_execution_repo` and maps each row directly with `RoutineExecutionResponse::from` (`crates/agentdash-api/src/routes/routines.rs:231`).
  - Contract response exposes `status: RoutineExecutionStatusDto` and `runtime_refs`, but does not attach a lifecycle/agent terminal projection (`crates/agentdash-contracts/src/runtime/routine.rs:57`, `crates/agentdash-contracts/src/runtime/routine.rs:74`).
  - Frontend execution history renders `exec.status` as the visible badge (`packages/app-web/src/features/routine/execution-history-panel.tsx:5`, `packages/app-web/src/features/routine/execution-history-panel.tsx:31`).
- Impact:
  - A successful Routine execution can remain visually `dispatched` forever in history, while the actual run may be completed, failed, cancelled, or blocked.
  - Users must click into Run/Agent to see the real state; history list itself becomes a second, weaker execution-status surface.
  - Reuse strategy makes this more confusing because multiple RoutineExecution rows can point at the same long-lived AgentRun and all show dispatch-level status.
- Containment boundary:
  - Keep `RoutineExecution.status` as a dispatch ledger field, but rename or present it as `dispatch_status`.
  - Add a routine execution read model that derives `runtime_status` from `dispatch_refs.runtime_refs -> LifecycleRun / LifecycleAgent / RuntimeNodeState` using the same lifecycle read model path as `SubjectExecutionView`.
  - Frontend history should display derived runtime status when dispatch refs exist, and keep ledger status only for pending/admission/skipped/dispatch-failed cases.

### Issue 3: LifecycleDispatchService remains an over-thick cross-module transaction script

- Priority: P2
- Problem type: 模块过厚 / 横向耦合 / 职责漂移
- Code evidence:
  - The service struct still owns run, graph, agent, frame, association, gate, lineage, anchor, runtime session creation, frame construction, workflow node frame materialization, and workflow graph planning ports (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:105`).
  - `start_lifecycle_run` correctly only creates a run and root orchestration (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:378`), but `dispatch_common` still resolves graph planning, run/orchestration materialization, agent creation, subject association, runtime session, frame, lineage, gate, anchor, and reducer `NodeStarted` in one method (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:529`).
  - Graph-backed dispatch writes the anchor and immediately binds agent delivery status in the same method before submitting `NodeStarted` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:591`).
- Impact:
  - Workflow graph planning, lifecycle state, subject association, AgentRun identity, runtime session delivery, frame materialization, lineage, and gates all share one modification hotspot.
  - The service is forced to know which dependencies are optional for plain dispatch vs graph-backed dispatch vs workflow node materialization.
  - It increases the chance that a later feature adds another side effect to dispatch instead of introducing a narrower use-case boundary.
- Containment boundary:
  - Keep `LifecycleDispatchService` as the public facade, but split internal owners:
    - run/orchestration starter,
    - subject association writer,
    - agent/frame/runtime session materializer,
    - lineage/gate writer,
    - graph-backed `NodeStarted` bridge.
  - Reducer events must stay in orchestration runtime; the split should expose transaction shape, not bypass reducer.

### Issue 4: CompanionGateControlService mixes durable gate state with delivery mechanisms

- Priority: P2
- Problem type: 模块过厚 / 抽象泄漏 / 横向耦合
- Code evidence:
  - `CompanionGateControlService` owns `LifecycleGateRepository`, `LifecycleRunRepository`, frame/agent/anchor/lineage repos, generic notification delivery, parent mailbox delivery, and human response mailbox delivery (`crates/agentdash-application/src/companion/gate_control.rs:346`).
  - The same service handles direct human response (`respond`, `crates/agentdash-application/src/companion/gate_control.rs:417`), child result completion (`complete_child_result_to_parent`, `crates/agentdash-application/src/companion/gate_control.rs:537`), parent request opening (`open_parent_request`, `crates/agentdash-application/src/companion/gate_control.rs:727`), and parent response resolution (`resolve_parent_request`, `crates/agentdash-application/src/companion/gate_control.rs:905`).
  - API route constructs this full service for a simple human gate response and wires mailbox delivery (`crates/agentdash-api/src/routes/companion_gates.rs:51`).
- Impact:
  - Durable gate lifecycle is coupled to delivery shape: session event notification, parent mailbox delivery, child mailbox delivery, and human response delivery.
  - Parent/child routing rules require runtime trace resolution and lineage lookups in the same module that resolves a gate.
  - The current shape makes it harder to audit "what changes the gate state" separately from "how a response is delivered".
- Containment boundary:
  - Keep `LifecycleGate` as durable gate fact source.
  - Split a pure `LifecycleGateResolver` from delivery adapters:
    - human gate response delivery,
    - parent request delivery,
    - child result delivery.
  - Gate state transitions should return delivery intents; mailbox/session eventing adapters should consume those intents.

## External References

No external references used. This review is based on repository code, Trellis specs, and the 06-14 baseline artifacts only.

## Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. This file uses the user-provided task path `.trellis/tasks/06-30-module-adversarial-review` and the explicitly allowed output path.
- No business code was modified and no full test suite was run.
- The old 06-14 path `crates/agentdash-application/src/workflow/*` no longer exists; current workflow/lifecycle responsibilities are split across `agentdash-application-lifecycle`, `agentdash-application-workflow`, `agentdash-application-agentrun`, and selected `agentdash-application/src/*` modules.
- I did not classify SessionRuntimeInner / AgentRuntimeDelegate as primary issues for this file because they belong more directly to Agent Runtime Session Surface, but they remain relevant cross-module caveats: `SessionRuntimeInner` is still broad (`crates/agentdash-application-runtime-session/src/session/hub/mod.rs:47`) and `AgentRuntimeDelegate` is still a wide trait (`crates/agentdash-agent-types/src/runtime/delegate.rs:25`).
