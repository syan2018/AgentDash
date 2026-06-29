# Implementation Plan

## Operating Rules

- Follow Trellis workflow and start this task before code edits.
- Subagent prompts must start with `Active task: .trellis/tasks/06-30-command-availability-cleanup`.
- Cleanup-first constraint: this review exists to converge architecture from first principles. Removing old parallel command status/gates is more important than adding feature surface.
- Do not add compatibility paths or local mirrors of command availability.
- Implementation subagents must avoid large Rust builds and broad suites. Use scoped `rg`, format, small targeted tests/typecheck only.

## Work Items

1. Backend projection cleanup
   - Remove runtime-command-state types and helper from AgentRun workspace projection if they remain internal-only.
   - Update projection tests to assert display projection only.
   - Keep command policy/resolver tests intact; add a focused assertion if old runtime state was the only terminal display coverage.

2. Frontend command handler cleanup
   - Remove workspaceStatus semantic gates from `useAgentRunWorkspaceCommands`.
   - Remove `workspaceStatus` from hook options/dependencies if no longer needed.
   - Update tests if any assert workspace status blocks command execution.

3. Specs and checks
   - Update backend session spec and frontend/application notes if needed.
   - Static check for `runtime_command_state` and `workspaceStatus !== "ready"` in command handlers.

## Suggested Subagent Split

- Implement A: backend AgentRun workspace projection cleanup.
- Implement B: frontend `useAgentRunWorkspaceCommands` cleanup and focused TS checks.
- Check: targeted review that no parallel command authority remains.

## Validation Commands

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-command-availability-cleanup
git diff --check
cargo fmt --check --package agentdash-application-agentrun --package agentdash-api
cargo test -p agentdash-application-agentrun workspace::projection --lib
cargo test -p agentdash-application-agentrun command_policy --lib
pnpm --filter app-web run typecheck
pnpm --filter app-web run lint
pnpm --filter app-web exec vitest run src/pages/AgentRunWorkspacePage.workspace-module.test.ts
rg -n "runtime_command_state|RuntimeCommandState" crates/agentdash-application-agentrun crates/agentdash-contracts packages/app-web/src/generated packages/app-web/src
rg -n "workspaceStatus !== \"ready\"|workspaceStatus !== 'ready'" packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts
```

## Verification Result

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-command-availability-cleanup`: passed.
- `git diff --check`: passed.
- Static search for `runtime_command_state|RuntimeCommandState|AgentRunWorkspaceRuntimeCommand`: no production matches.
- Static search for `workspaceStatus` in `useAgentRunWorkspaceCommands`, `AgentRunWorkspacePage.tsx`, and conversation command state: no command-handler matches.
- `cargo fmt --check --package agentdash-application-agentrun --package agentdash-api`: passed.
- `cargo test -p agentdash-application-agentrun workspace::projection --lib`: passed.
- `cargo test -p agentdash-application-agentrun command_policy --lib`: passed.
- `pnpm --filter app-web run typecheck`: passed.
- `pnpm --filter app-web run lint`: passed.
- `pnpm --filter app-web exec vitest run src/pages/AgentRunWorkspacePage.workspace-module.test.ts`: passed.

Known unrelated warning: `agentdash-workspace-module::workspace_module::tools` has an unused `resolve_workspace_module_visibility` import.
