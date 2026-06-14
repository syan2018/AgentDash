# AgentRun workspace 应用层重构执行计划

## Phase 0: Baseline

- Confirm working tree is clean before implementation.
- Run focused baseline checks when useful:
  - `cargo test -p agentdash-application workflow::project_agent_run_start`
  - `cargo check -p agentdash-api`
  - `pnpm --dir packages/app-web run typecheck`

## Phase 1: ProjectAgent Start Receipt 收束

- Add application types for initial message launch refs.
- Change `ProjectAgentRunInitialMessagePort` to return typed launch refs instead of `AgentRunMailboxCommandResult`.
- Move mailbox outcome/result interpretation into the production initial-message adapter.
- Keep outer `project_agent_start` receipt as the only `ProjectAgentRunStartDispatch.command_receipt` source.
- Store accepted refs from the initial launch on the outer receipt.
- Update tests for:
  - first start accepts outer receipt with launch refs
  - duplicate start replays outer accepted refs
  - initial message not launched fails outer receipt
  - launch refs run/agent/runtime mismatch fails outer receipt
  - missing AgentRun turn id fails outer receipt

Subagent fit: good parallel worker. Suggested ownership:

- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
- `crates/agentdash-api/src/routes/project_agents.rs`
- related application tests in the same module

## Phase 2: Workspace Projection 下沉 Application

- Create `agentdash-application::workflow::agent_run_workspace` module.
- Add projection types and pure derivation tests for execution states:
  - idle
  - running with no turn
  - running with active turn
  - cancelling
  - completed
  - failed
  - interrupted
  - terminal lifecycle agent
  - missing delivery runtime
  - missing frame
- Move state-code, active-turn, last-turn, delivery-status, control-plane, action availability and runtime-command-state derivation into application.
- Replace API-local helpers in `lifecycle_agents.rs` with application projection consumption.

Subagent fit: good parallel worker if scoped to pure projection and tests only. Suggested ownership:

- `crates/agentdash-application/src/workflow/agent_run_workspace/types.rs`
- `crates/agentdash-application/src/workflow/agent_run_workspace/projection.rs`
- module exports and focused projection tests

## Phase 3: Workspace Query Service

- Move workspace read assembly from `build_agent_run_workspace_view` into `AgentRunWorkspaceQueryService`.
- Keep API permission checks before query service invocation.
- Preserve contract response shape through API mapper.
- Move resource/model/mailbox/conversation snapshot input assembly only as far as needed to make application the AgentRun workspace fact owner.
- Keep HTTP DTO and `ApiError` mapping in API.

Subagent fit: integration worker after Phase 2. It overlaps `lifecycle_agents.rs`, so do not run this concurrently with Phase 4 unless file ownership is split very tightly.

## Phase 4: Command Policy 下沉 Application

- Move command precondition enum and stale guard validation into application.
- Return application conflict objects carrying message, code, replacement command and detail.
- Replace route-local `ensure_agent_run_command_allowed` with application policy service.
- Preserve route behavior for composer submit, promote, delete, resume and cancel.

Subagent fit: integration worker after Phase 2. It overlaps `lifecycle_agents.rs`, so it is safer to run after or together with Phase 3 under a single owner.

## Phase 5: API And Frontend Contract Check

- Update API mappers for application read models.
- Regenerate/check contracts if Rust contract shape changes.
- Update frontend generated type consumption only if needed.
- Avoid introducing handwritten frontend AgentRun workspace DTO aliases.

## Commit Plan

Each independently usable phase should be committed separately after its focused validation passes.

- Commit 1: ProjectAgent start receipt 收束。
  - Expected message: `refactor(agentrun): 收束 ProjectAgent start receipt 语义`
  - Validation: `cargo test -p agentdash-application workflow::project_agent_run_start`
- Commit 2: AgentRun workspace projection 纯模型下沉 application。
  - Expected message: `refactor(agentrun): 下沉 workspace 状态投影模型`
  - Validation: focused application projection tests plus `cargo check -p agentdash-application`
- Commit 3: AgentRun workspace query service 下沉 application。
  - Expected message: `refactor(agentrun): 下沉 workspace 查询组装`
  - Validation: `cargo check -p agentdash-api`
- Commit 4: AgentRun workspace command policy 下沉 application。
  - Expected message: `refactor(agentrun): 下沉 workspace command policy`
  - Validation: focused command policy/API tests plus `cargo check -p agentdash-api`
- Commit 5: Contract/frontend follow-through, only if contract output or frontend consumption changes.
  - Expected message: `refactor(agentrun): 对齐 workspace 前端契约消费`
  - Validation: `pnpm run contracts:check` and `pnpm --dir packages/app-web run typecheck`

## Subagent Wave Plan

Wave 0 main session:

- Keep this as one Trellis task.
- Freeze the shared application model names and write ownership before any delegation.
- Dispatch agents with `Active task: .trellis/tasks/06-14-agentrun-workspace-app-refactor` at the top of each prompt.

Wave 1 parallel:

- Worker A: ProjectAgent start receipt and initial launch refs.
- Worker B: AgentRun workspace projection pure model and tests.
- Optional researcher: map `lifecycle_agents.rs` helper call sites and route integration risks into the parent task research notes.

Wave 2 integration:

- Single owner moves workspace query service and command policy out of API.
- Main session reviews, resolves conflicts, updates contracts/frontend types if needed, and runs validation.

Single-task rule:

- Do not create child Trellis tasks for this refactor.
- Subagents, if used, operate under the same active task and return changed file lists for main-session integration.

## Validation Commands

- `cargo test -p agentdash-application workflow::project_agent_run_start`
- `cargo test -p agentdash-application agent_run_workspace`
- `cargo check -p agentdash-api`
- `pnpm run contracts:check` when contract output changes
- `pnpm --dir packages/app-web run typecheck` when generated or consumed frontend types change
- `pnpm run migration:guard` if any database migration is added

## Risk Points

- `lifecycle_agents.rs` currently mixes route concerns with application concerns; migration should keep API endpoints stable while moving one concern at a time.
- Workspace query assembly touches frame/VFS/resource/model/mailbox/conversation snapshot inputs. The read model should remain specific to AgentRun workspace rather than becoming a generic workflow query abstraction.
- ProjectAgent start has two command receipts by design; tests must prove only the outer receipt defines API duplicate replay semantics.

## Review Gates

- Application tests demonstrate the projection truth table before deleting API helper logic.
- API diff shows route handlers becoming thinner rather than moving logic into new route-local helpers.
- Contract/typecheck results show browser wire shape remains generated and aligned.
