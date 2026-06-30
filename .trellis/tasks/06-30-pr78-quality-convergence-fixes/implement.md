# Implementation Plan

## Parallel Work

- [ ] VFS / AgentRun worker:
  - Fix current/effect frame projection in AgentRun effective capability and grant projection.
  - Add launch-frame/current-frame regression tests.
  - Add final policy allow/deny regression around PermissionGrant VFS access.
- [ ] WorkspaceModule worker:
  - Preserve `ExecutionContext.session.vfs_access_policy` in runtime bridge.
  - Preserve policy after Canvas expose / VFS replace.
  - Add focused tests.
- [ ] RuntimeGateway worker:
  - Make duplicate `action_key` owner behavior deterministic and consistent between Gateway and WorkspaceModule.
  - Prefer resolved descriptor owner; otherwise fail closed on duplicates.
  - Add focused tests.
- [ ] Main thread:
  - Fix Trellis markdown EOF blank lines.
  - Integrate worker changes, resolve conflicts, run focused checks, update this checklist.

## Validation Commands

- `cargo test -p agentdash-application-agentrun effective_capability --lib`
- `cargo test -p agentdash-application-agentrun runtime_surface --lib`
- `cargo test -p agentdash-workspace-module workspace_module --lib`
- `cargo test -p agentdash-application-runtime-gateway extension_actions --lib`
- `git diff --check origin/main..HEAD`

## Rollback Points

- If current/effect frame change reveals missing persisted anchors, keep existing launch-frame behavior only behind a named helper and document the residual.
- If Gateway descriptor owner propagation expands contract surface too much, switch to duplicate `action_key` rejection in Gateway dynamic provider as the quick fix.
- If VFS PermissionGrant effective policy requires broader model work, keep runtime bridge policy preservation in scope and record PermissionGrant policy semantics as an explicit remaining decision.
