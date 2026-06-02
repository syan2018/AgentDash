# Frontend Session Runtime Frame Query Implement Plan

## Checklist

- [ ] Add contract DTO for session runtime frame view.
- [ ] Add backend route and service call using runtime session anchor.
- [ ] Add `fetchSessionFrameRuntime(runtimeSessionId)` in frontend lifecycle service for `GET /sessions/{runtime_session_id}/frame-runtime`.
- [ ] Update lifecycle store action.
- [ ] Rewrite `useSessionRuntimeState` to call backend endpoint.
- [ ] Remove `findFrameIdForSession` local fallback.
- [ ] Update affected component tests.

## Validation Commands

- [ ] `pnpm run contracts:generate`
- [ ] `pnpm --filter app-web test`
- [ ] `pnpm --filter app-web run typecheck`
- [ ] `cargo test -p agentdash-api lifecycle_views`

## Risk Points

- Existing local store hydration order may hide missing backend data; tests should assert backend query is the source.
- If backend anchor task is not complete, avoid implementing temporary cache fallback.
