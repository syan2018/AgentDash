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

## Phase 3: Workspace Query Service

- Move workspace read assembly from `build_agent_run_workspace_view` into `AgentRunWorkspaceQueryService`.
- Keep API permission checks before query service invocation.
- Preserve contract response shape through API mapper.
- Move resource/model/mailbox/conversation snapshot input assembly only as far as needed to make application the AgentRun workspace fact owner.
- Keep HTTP DTO and `ApiError` mapping in API.

## Phase 4: Command Policy 下沉 Application

- Move command precondition enum and stale guard validation into application.
- Return application conflict objects carrying message, code, replacement command and detail.
- Replace route-local `ensure_agent_run_command_allowed` with application policy service.
- Preserve route behavior for composer submit, promote, delete, resume and cancel.

## Phase 5: API And Frontend Contract Check

- Update API mappers for application read models.
- Regenerate/check contracts if Rust contract shape changes.
- Update frontend generated type consumption only if needed.
- Avoid introducing handwritten frontend AgentRun workspace DTO aliases.

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
