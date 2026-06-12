# 实施计划

## Phase 0: Planning Review

- [ ] 用户审阅 `prd.md`、`design.md`、`implement.md`。
- [ ] 决定 `AgentLaunchPlan` 第一阶段是 transient contract 还是 persisted read model。
- [ ] 通过 `task.py start 06-12-agent-run-lifecycle-convergence` 进入实现阶段。

## Phase 1: Launch Contract Skeleton

- [ ] 新增 `workflow/launch_plan.rs` 或等价模块，定义 `AgentLaunchPlan`、`LaunchOwnerKind`、`FrameSurfaceKind`、`LaunchCleanupPolicy`。
- [ ] 为 ProjectAgent start、AgentRun message、Workflow AgentCall、Routine、Companion、pending drain 列出 adapter function。
- [ ] 保持 public API 不变，先让 plan 在 application 层传递或可从现有 facts 确定生成。
- [ ] 单测覆盖 plan builder：
  - ProjectAgent graphless -> owner bootstrap
  - ProjectAgent explicit lifecycle -> owner bootstrap + active workflow binding
  - workflow AgentCall -> lifecycle node
  - companion -> companion surface

## Phase 2: Dispatch And Frame Construction Convergence

- [ ] 调整 `LifecycleDispatchService` 返回的 facts/refs，使 launch plan 能稳定关联 run / agent / frame / runtime session / orchestration binding。
- [ ] 让 `ProjectAgentRunStartService` 在 materialization 前确定 ProjectAgent owner plan，减少“dispatch 后补写 project_agent_id”的时序风险。
- [ ] 让 `FrameConstructionService` 优先消费 launch plan 或 launch-plan projection。
- [ ] 收紧 `classify.rs`：分类函数只作为 plan projection 兼容层存在，最终由 owner/surface kind 决策。
- [ ] 确认 ProjectAgent explicit lifecycle 仍通过 owner bootstrap composer 挂 lifecycle mount。
- [ ] 确认 Workflow AgentCall 仍通过 lifecycle node composer 挂 node-scoped lifecycle mount。

## Phase 3: Workspace Projection And Frontend Contract

- [ ] 扩展 `AgentRunWorkspaceControlPlaneView` 或增加 launch readiness 子对象，表达 surface composing / surface failed / cleanup pending / delivery failed 等状态。
- [ ] Workspace projection 从统一 launch/control facts 生成 `control_plane` 和 `actions`。
- [ ] 前端 `deriveAgentRunWorkspaceChatControlState` 只消费 workspace status/actions，不补后端启动推断。
- [ ] `useAgentRunWorkspaceState` 将 runtime surface 解析错误纳入 workspace panel 状态展示，不影响 command readiness 的后端权威判断。
- [ ] Draft start transport failure 复用 command id，并通过 receipt/workspace refresh 恢复 accepted refs。

## Phase 4: Tests And Regression Matrix

### Backend Application

- [ ] `ProjectAgentRunStartService` 使用真实 `AgentRunMessageLaunchDeliveryPort` 或等价 integration harness 覆盖 `SessionLaunchService -> FrameConstructionService`。
- [ ] ProjectAgent graphless start produces owner surface frame.
- [ ] ProjectAgent explicit lifecycle start produces owner surface frame and lifecycle mount.
- [ ] Workflow AgentCall produces lifecycle node surface and lifecycle mount.
- [ ] AgentRun send_next reuses current surface or rehydrates by plan.
- [ ] First message surface failure cleans empty runtime/run/anchor per cleanup policy.
- [ ] Duplicate `project_agent_start` returns same accepted refs.

### Backend API / Contracts

- [ ] `cargo check -p agentdash-contracts -p agentdash-api`
- [ ] `pnpm run contracts:check`
- [ ] Workspace projection tests cover ready/running/cancelling/terminal/frame missing/delivery missing/surface failure if added.

### Frontend

- [ ] `pnpm --filter app-web run typecheck`
- [ ] Focused vitest:
  - draft -> accepted -> route `/agent-runs/:runId/:agentId`
  - ready -> send_next
  - running -> enqueue + optional steer
  - cancelling -> input disabled
  - surface failed / frame missing -> readonly with backend reason
  - transport retry reuses `client_command_id`

### Grep Checks

- [ ] `rg -n "SessionPage|/session/new|/session/:sessionId" packages/app-web/src`
- [ ] `rg -n "FrameConstructionService|route_and_compose|RuntimeSessionExecutionAnchor" crates/agentdash-application/src/workflow`
- [ ] `rg -n "created_by_kind|dispatch_launch_anchor" crates/agentdash-application/src crates/agentdash-domain/src`

## Phase 5: Spec And Cleanup

- [ ] Update `.trellis/spec/backend/workflow/architecture.md` with launch plan / composer ownership contract.
- [ ] Update `.trellis/spec/backend/session/runtime-execution-state.md` if workspace readiness grows new states.
- [ ] Update frontend spec if AgentRun workspace control state gets new generated fields.
- [ ] Remove temporary compatibility-only helpers after every entry point consumes launch plan.

## Risky Files

- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
- `crates/agentdash-application/src/workflow/dispatch_service.rs`
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs`
- `crates/agentdash-application/src/workflow/frame_construction/classify.rs`
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs`
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs`
- `crates/agentdash-application/src/session/launch/orchestrator.rs`
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts`
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`

## Validation Commands

```powershell
cargo test -p agentdash-application project_agent_run_start
cargo test -p agentdash-application workflow::frame_construction
cargo test -p agentdash-application orchestration
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-contracts
pnpm run contracts:check
pnpm --filter app-web run typecheck
pnpm --filter app-web run lint
git diff --check
```

## Rollback Points

- After Phase 1, launch plan types can be removed without behavior changes.
- After Phase 2, keep old classification tests green before deleting compatibility classification.
- After Phase 3, generated contract changes should be isolated in one commit with frontend consumption.
- Before any migration, run `pnpm run migration:guard` and verify migration history.
