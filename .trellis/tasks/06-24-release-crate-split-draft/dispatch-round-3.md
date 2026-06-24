# Dispatch Round 3

## Start Condition

Round 3 starts after commit `14fed2af` completed the first physical crate extraction:

- `agentdash-application-runtime-gateway` is extracted and checked.
- API/local/MCP no longer import `agentdash_application::runtime_gateway`.
- RuntimeSession, AgentRun/Lifecycle and VFS physical extraction are explicitly not ready.
- Checkpoint notes live in `checkpoint-wave-2.md`.

Round 3 is a port-wiring convergence round. Do not start RuntimeSession, AgentRun, Lifecycle or VFS physical crate moves in this round.

## Shared Mode

- High concurrency is allowed, but each worker owns a narrow write set.
- Workers are not alone in the codebase. They must preserve parallel edits, adapt to them, and report owner conflicts instead of reverting.
- Prefer command-driven mechanical work: `rg`, targeted move/rename commands, batch import rewrite, `cargo metadata`, narrow `cargo check -p`, and controlled `cargo fix` when the compiler output is decisive.
- Do not hand-edit imports one by one when a mechanical rewrite can do the job safely.
- Run only the minimal gate owned by the work item. Broad readiness, stale-path review and cross-module architecture checks belong to checkpoint check agents.
- Delete obsolete paths and matching tests when they only preserve the old implementation chain or a business-unrelated stale behavior.
- Do not add compatibility shells for old module paths.

## Implement Worker Lanes

| Worker | Work item | Ownership | Must not touch |
| --- | --- | --- | --- |
| `session-adoption-port-impl` | `work-items/04-runtime-session-substrate-boundary.md` | `crates/agentdash-application/src/session/hub/tool_builder.rs`, `session/runtime_builder.rs`, `session/hub/facade.rs`, active runtime adoption wiring, API session bootstrap injection as needed | Session launch/commit files, Lifecycle dispatch files, physical crate moves |
| `session-launch-commit-port-impl` | `work-items/04-runtime-session-substrate-boundary.md` | `crates/agentdash-application/src/session/launch/**`, `agent_run/frame/launch_envelope_provider.rs`, `agent_run/frame/launch_commit.rs`, `crates/agentdash-api/src/bootstrap/frame_launch_envelope_provider.rs`, launch/commit port DTOs | Active adoption wiring, Lifecycle dispatch files, physical crate moves |
| `control-dispatch-facade-impl` | `work-items/03-agentrun-surface-facade.md` + `work-items/05-agentrun-lifecycle-boundary.md` | `agent_run/project_agent_start.rs`, Lifecycle dispatch facade/port, workflow orchestration launcher seams, runtime session creator placement if required by dispatch ownership | Session launch internals, VFS providers, physical crate moves |
| `frame-construction-helper-port-impl` | `work-items/05-agentrun-lifecycle-boundary.md` | `agent_run/frame/construction/**`, frame materialization/projection port DTOs, remaining lifecycle helper imports in AgentRun frame construction | Lifecycle dispatch service body unless a trait call site is required, Session hub/launch internals |
| `vfs-owner-adapter-prep-impl` | `work-items/07-vfs-resource-surface-boundary.md` | Classify and remove/relocate generic VFS dependencies on session/lifecycle/canvas owner providers where this is mechanically clear | Generic VFS physical crate move, API VFS route behavior rewrite unless required by adapter extraction |

If two workers need the same file, the later worker reports the conflict in handoff and stops at the nearest compile boundary instead of broadening ownership.

## Started

| Worker | Agent id | Nickname |
| --- | --- | --- |
| `session-adoption-port-impl` | `019efb0d-bbd6-7260-873d-9ec7d6c8230c` | Godel |
| `session-launch-commit-port-impl` | `019efb0d-d02c-7122-a725-fd77504fb7ab` | Noether |
| `control-dispatch-facade-impl` | `019efb0d-e4d8-7c02-9522-65e73ce11a8e` | Locke |
| `frame-construction-helper-port-impl` | `019efb0d-f927-7330-a339-3fbd39f01d69` | Volta |
| `vfs-owner-adapter-prep-impl` | `019efb0e-0e11-7ad0-b394-b11030327031` | Linnaeus |

## Implement Prompt Injection

Every implement worker prompt must start with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
Round: 3 port-wiring convergence
Work item: <path>
Checkpoint: .trellis/tasks/06-24-release-crate-split-draft/checkpoint-wave-2.md
```

Then inject the shared mode plus one worker-specific bias:

- `session-adoption-port-impl`: production wiring must consume `RuntimeSurfaceAdoptionPort` instead of the old `AgentRunActiveRuntimeSurfaceAdopter` path where the direction crosses Session/AgentRun. Keep live Session runtime cache/tool refresh behavior inside Session.
- `session-launch-commit-port-impl`: Session launch should depend on launch envelope and accepted launch commit contracts, not AgentRun implementation adapters. If a stale launch test only asserts the old adapter path, delete it with the path it anchors.
- `control-dispatch-facade-impl`: AgentRun must stop directly constructing `LifecycleDispatchService`. Introduce or use a port/facade that lets composition roots or Lifecycle-owned adapters provide dispatch behavior.
- `frame-construction-helper-port-impl`: AgentRun frame construction must stop importing Lifecycle helper implementation paths. Move shared DTO/trait shape to ports when needed; keep frame internals private to AgentRun.
- `vfs-owner-adapter-prep-impl`: Generic VFS should not own session/lifecycle/canvas-specific provider wiring. Split owner adapters where obvious and report remaining owner-specific paths that block physical VFS extraction.

Each handoff must include:

- changed files
- commands run
- commands not run and why
- stale paths/tests deleted
- unresolved imports or compile errors with owner assignment
- whether the next checkpoint check should classify the result as ready, blocked, or partial

## Current Static Blockers

These are the Round 3 blockers to retire or assign:

```powershell
rg -n "AgentRunActiveRuntimeSurfaceAdopter|RuntimeSurfaceAdoptionPort|ActiveRuntimeSurfaceAdopter" crates/agentdash-application/src crates/agentdash-api/src -g '*.rs'
rg -n "FrameLaunchEnvelopeProvider|AgentRunAcceptedLaunchCommitAdapter|AgentRunAcceptedLaunchCommitInput" crates/agentdash-application/src/session crates/agentdash-application/src/agent_run crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "LifecycleDispatchService" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "composer_lifecycle_node|resolve_current_frame_from_delivery_trace_ref|LifecycleLaunch" crates/agentdash-application/src/agent_run/frame/construction crates/agentdash-application/src/lifecycle -g '*.rs'
```

## VFS Owner Adapter Classification

`vfs-owner-adapter-prep-impl` 已把 `MountProviderRegistryBuilder` 的 owner provider 注册从 generic `vfs/provider.rs` 移到 `vfs/owner_providers.rs`，并停止通过 generic VFS public facade 暴露 concrete `CanvasFsMountProvider` / `LifecycleMountProvider`。

剩余 owner-specific VFS 路径分类：

| Path | Classification | Owner assignment |
| --- | --- | --- |
| `crates/agentdash-application/src/vfs/owner_providers.rs` | adapter | VFS/API composition root 后续决定是否迁到 owner adapter crate；当前仍需要 Session persistence、Lifecycle provider 和 Canvas provider 一次性注册。 |
| `crates/agentdash-application/src/vfs/provider_lifecycle.rs` | move | Lifecycle owner；依赖 `LifecycleJourneyProjection`、execution log、Session persistence/tool result cache，不能在本轮不触碰 Lifecycle/RuntimeSession 的前提下安全移动。 |
| `crates/agentdash-application/src/vfs/mount_lifecycle.rs` | move | Lifecycle owner；mount builder 已去掉纯编码 helper 的 lifecycle import，但业务语义仍是 lifecycle runtime / AgentRun session mount。 |
| `crates/agentdash-application/src/vfs/provider_canvas.rs` | move | Canvas owner；依赖 Canvas repository 与 binding projection，不能归入 generic VFS core。 |
| `crates/agentdash-application/src/vfs/mount_canvas.rs` | move | Canvas owner；模块已降为 crate-private，仍通过 VFS facade 暴露 mount builder 给 Canvas/AgentRun surface update。 |

## Implementation Completed

All five implement workers completed and were closed after handoff.

| Worker | Result |
| --- | --- |
| `session-adoption-port-impl` | Production adoption wiring now consumes `RuntimeSurfaceAdoptionPort`; old `AgentRunActiveRuntimeSurfaceAdopter` paths are removed from Session/API bootstrap. |
| `session-launch-commit-port-impl` | Session launch now consumes ports-level frame launch envelope and accepted launch commit contracts; old AgentRun commit adapter imports are removed from Session/API bootstrap. |
| `control-dispatch-facade-impl` | AgentRun/workflow launcher paths call a Lifecycle-owned dispatch facade instead of constructing `LifecycleDispatchService` directly. |
| `frame-construction-helper-port-impl` | AgentRun frame construction no longer imports Lifecycle helper implementation paths; stale `composer_lifecycle_node` was deleted. |
| `vfs-owner-adapter-prep-impl` | Generic VFS registry builder no longer owns Session/Lifecycle/Canvas provider registration; owner providers remain classified blockers for physical VFS extraction. |

Integration validation passed:

- `cargo fmt`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application-runtime-gateway -p agentdash-api -p agentdash-local -p agentdash-mcp`
- `cargo test -p agentdash-application-ports --no-run`
- `cargo test -p agentdash-application agent_run::frame::construction --no-run`
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`
- `git diff --check`

Static gates passed with no matches:

```powershell
rg -n "AgentRunActiveRuntimeSurfaceAdopter|ActiveRuntimeSurfaceAdopter" crates/agentdash-application/src crates/agentdash-api/src -g '*.rs'
rg -n "FrameLaunchEnvelopeProvider|AgentRunAcceptedLaunchCommitAdapter|AgentRunAcceptedLaunchCommitInput" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "LifecycleDispatchService" crates/agentdash-application/src/agent_run crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "composer_lifecycle_node|resolve_current_frame_from_delivery_trace_ref|crate::lifecycle" crates/agentdash-application/src/agent_run/frame/construction -g '*.rs'
rg -n "agentdash_application::runtime_gateway" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application::|crate::(mcp_preset|workspace|agent_run|lifecycle|session|vfs|canvas)::" crates/agentdash-application-runtime-gateway/src -g '*.rs'
```

VFS owner adapter gate still has expected owner-specific matches in `vfs/owner_providers.rs`, `provider_lifecycle.rs`, `mount_lifecycle.rs`, `provider_canvas.rs` and `mount_canvas.rs`; this keeps VFS physical extraction classified as partial.

## Checkpoint Check Waves

Run checkpoint checks after the implement workers complete, not during the first mechanical move pass.

| Check worker | Focus | Required output |
| --- | --- | --- |
| `check-session-adoption-port` | Session/AgentRun live adoption direction | P0/P1 findings, remaining concrete adopter paths, readiness for RuntimeSession extraction |
| `check-session-launch-commit-port` | Launch envelope and accepted launch commit contracts | Whether Session launch still imports AgentRun implementation adapters; stale tests to delete |
| `check-control-dispatch-boundary` | AgentRun/Lifecycle dispatch and frame construction | Whether AgentRun still constructs Lifecycle or imports Lifecycle helper paths |
| `check-vfs-owner-adapters` | VFS generic/owner split | Remaining session/lifecycle/canvas owner dependencies and physical VFS extraction readiness |
| `check-gateway-regression` | Gateway extracted crate regression | Confirm no new monolithic application dependency or old `agentdash_application::runtime_gateway` consumer import |

Check worker prompt bias:

- Findings first, ordered by severity.
- Classify every finding as `delete`, `move`, `port`, or `keep`.
- Assign each finding to a work item owner.
- Treat tests as evidence only when they encode target architecture.
- Recommend deleting tests that only preserve stale chains.
- Do not run large workspace tests unless a narrow gate first passes and the check explicitly needs broader confidence.

## Round 3 Commit Rule

Use small checkpoint commits:

1. Commit dispatch docs before spawning workers.
2. Commit implement integration after each coherent lane or compatible group of lanes.
3. Commit checkpoint findings separately when they update task state.

Commits may be partially tested if the handoff clearly records failing commands and owner assignment.
