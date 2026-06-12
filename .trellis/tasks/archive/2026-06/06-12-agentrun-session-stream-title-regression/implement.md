# Implementation Plan

## Steps

1. Read relevant specs:
   - frontend state/hooks and testing guidance
   - backend route/contract testing guidance
   - cross-layer runtime projection guide
2. Fix frontend refresh semantics in `useAgentRunWorkspaceState`:
   - Split initial load and refresh behavior.
   - Preserve committed state during same-identity refresh.
   - Preserve runtime identity on refresh failure.
3. Add focused frontend tests for refresh stability.
4. Fix backend AgentRun workspace shell title selection:
   - Prefer delivery `SessionMeta.title/title_source`.
   - Keep workspace title resolver only for missing delivery meta.
   - Ensure list entry inherits the same shell.
5. Add focused backend test for title selection or route projection.
6. Run targeted validation:
   - relevant frontend unit tests
   - relevant backend test/clippy subset as time permits
   - contract generation only if generated contracts change
7. Inspect `git diff` for accidental unrelated churn.
8. Commit with required format.

## Validation Commands

- `pnpm --filter app-web test -- useAgentRunWorkspaceState`
- `pnpm --filter app-web test -- AgentRunWorkspacePage`
- `cargo test -p agentdash-api lifecycle_agents`
- `pnpm run backend:clippy` if backend changes are non-trivial or tests do not compile the touched route.

## Rollback Points

- Frontend refresh behavior is isolated to `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`.
- Backend title behavior is isolated to `crates/agentdash-api/src/routes/lifecycle_agents.rs` unless a pure helper/test requires minor contract-module exposure.

## Review Gates

- No `sessionId` placeholder/disabled transition during same AgentRun refresh.
- No frontend-specific title workaround in header/list components.
- No compatibility layer for old Session-first naming.
