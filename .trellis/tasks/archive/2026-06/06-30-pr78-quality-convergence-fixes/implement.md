# Implementation Plan

## Parallel Work

- [x] VFS / AgentRun worker:
  - Fix current/effect frame projection in AgentRun effective capability and grant projection.
  - Add launch-frame/current-frame regression tests.
  - Add final policy allow/deny regression around PermissionGrant VFS access.
- [x] WorkspaceModule worker:
  - Preserve `ExecutionContext.session.vfs_access_policy` in runtime bridge.
  - Preserve policy after Canvas expose / VFS replace.
  - Add focused tests.
- [x] RuntimeGateway worker:
  - Make duplicate `action_key` owner behavior deterministic and consistent between Gateway and WorkspaceModule.
  - Prefer resolved descriptor owner; otherwise fail closed on duplicates.
  - Add focused tests.
- [x] Main thread:
  - Fix Trellis markdown EOF blank lines.
  - Integrate worker changes, resolve conflicts, run focused checks, update this checklist.

## Completion Notes

- AgentRun effective capability now resolves runtime-session capability views through the current agent frame and queries active grants by the current/effect frame id. Focused tests cover `launch_frame != current_frame` and confirm launch-frame grants no longer affect runtime admission.
- PermissionGrant VFS contribution now removes whole-mount system projection rules for mounts constrained by PermissionGrant VFS rules, then appends the grant rules. Tests assert final `RuntimeVfsAccessPolicy::admits` allow and deny behavior.
- WorkspaceModule runtime bridge now requires `ExecutionContext.session.vfs_access_policy` and constructs policy-aware `SharedRuntimeVfs`. Canvas runtime surface updates return `RuntimeVfsState`, so expose/replace preserves the effective policy instead of rebuilding whole-mount access.
- RuntimeGateway dynamic extension action catalog now fails closed on duplicate `action_key`. Descriptors carry owner metadata, and WorkspaceModule consumes that resolved owner while marking projection-local duplicates unavailable.
- `git diff --check origin/main` is clean after removing EOF blank lines from Trellis task manifests. `origin/main..HEAD` becomes clean once the whitespace fix is committed, because that range excludes uncommitted work.

## Verification Results

- `cargo test -p agentdash-application-agentrun effective_capability --lib`: passed, 12 tests.
- `cargo test -p agentdash-application-agentrun runtime_surface --lib`: passed, 22 tests.
- `cargo test -p agentdash-workspace-module workspace_module --lib`: passed, 32 tests.
- `cargo test -p agentdash-application-runtime-gateway extension_actions --lib`: passed, 19 tests.
- `cargo check -p agentdash-api`: passed.
- `cargo fmt --check`: passed.
- `git diff --check`: passed.
- `git diff --check origin/main`: passed.
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-pr78-quality-convergence-fixes`: passed.
- `cargo clippy -p agentdash-application-agentrun -p agentdash-workspace-module -p agentdash-application-runtime-gateway -p agentdash-api --all-targets`: passed with existing warnings.
- `cargo clippy -p agentdash-application-agentrun -p agentdash-workspace-module -p agentdash-application-runtime-gateway -p agentdash-api --all-targets -- -D warnings`: failed on pre-existing warning debt outside this quick convergence scope.

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
