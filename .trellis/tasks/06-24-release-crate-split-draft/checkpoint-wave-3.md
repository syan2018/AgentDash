# Checkpoint Wave 3

Date: 2026-06-25

## Summary

Round 3 completed the port-wiring convergence pass after RuntimeGateway extraction:

- Production runtime surface adoption now uses `RuntimeSurfaceAdoptionPort`; the stale `AgentRunActiveRuntimeSurfaceAdopter` contract was deleted.
- Session launch no longer imports old AgentRun launch-envelope provider or accepted-launch commit adapter names; launch/commit contracts are represented in `agentdash-application-ports::frame_launch_envelope`.
- AgentRun and workflow launcher paths no longer construct `LifecycleDispatchService` directly; dispatch is behind a Lifecycle-owned facade.
- AgentRun frame construction no longer imports Lifecycle helper implementation paths; stale `composer_lifecycle_node` was removed and workflow node composition now uses construction-local helpers plus ports DTOs.
- Generic VFS provider registry no longer owns Session/Lifecycle/Canvas provider registration; owner provider registration moved to `vfs::owner_providers`.
- RuntimeGateway extracted crate did not regress.

This checkpoint is green for targeted compile/test gates. It is not a RuntimeSession / VFS physical extraction checkpoint.

## Validation

- `cargo metadata --no-deps --format-version 1`: passed
- `cargo fmt`: passed
- `cargo fmt --check`: passed
- `cargo check -p agentdash-application`: passed
- `cargo check -p agentdash-application-ports`: passed
- `cargo check -p agentdash-application-runtime-gateway -p agentdash-api -p agentdash-local -p agentdash-mcp`: passed
- `cargo test -p agentdash-application-ports --no-run`: passed
- `cargo test -p agentdash-application agent_run::frame::construction --no-run`: passed
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`: passed
- `git diff --check`: passed

Static gates passed with no matches:

```powershell
rg -n "AgentRunActiveRuntimeSurfaceAdopter|ActiveRuntimeSurfaceAdopter" crates -g '*.rs'
rg -n "FrameLaunchEnvelopeProvider|SharedFrameLaunchEnvelopeProvider|AgentRunAcceptedLaunchCommitAdapter|AgentRunAcceptedLaunchCommitInput" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "LifecycleDispatchService" crates/agentdash-application/src/agent_run crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "composer_lifecycle_node|resolve_current_frame_from_delivery_trace_ref|crate::lifecycle" crates/agentdash-application/src/agent_run/frame/construction -g '*.rs'
rg -n "agentdash_application::runtime_gateway" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application::|crate::(mcp_preset|workspace|agent_run|lifecycle|session|vfs|canvas)::" crates/agentdash-application-runtime-gateway/src -g '*.rs'
```

## Check Agent Results

| Worker | Agent id | Result |
| --- | --- | --- |
| `check-session-adoption-port` | `019efb2c-6503-7fa0-a8d8-a5cdb39becd7` | Fixed the last stale adoption trait in ports. Adoption production wiring passed. RuntimeSession extraction remains blocked by non-adoption AgentRun imports: mailbox adapter, concrete launch envelope and effective-capability/surface helper paths. |
| `check-session-launch-commit-port` | `019efb2c-7988-7c22-b8f5-2c87f9e82263` | Old launch/commit adapter names are gone from Session/API bootstrap, but Session launch still binds `FrameLaunchEnvelopePort` to concrete AgentRun `FrameLaunchEnvelope`. RuntimeSession extraction remains blocked. |
| `check-control-dispatch-boundary` | `019efb2c-8e4e-7a70-aaec-c85c00635863` | Passed. AgentRun and workflow launcher no longer directly construct `LifecycleDispatchService`; Lifecycle-owned facade construction is classified as keep. |
| `check-vfs-owner-adapters` | `019efb2c-a2b5-7173-8c80-ca966f6498a6` | Partial. Generic provider registry is clean, but `owner_providers`, lifecycle/canvas providers and application `VfsSurfaceResolver` remain outside generic VFS core readiness. |
| `check-gateway-regression` | `019efb2c-b710-72c1-bed9-ab856ea2e00a` | Passed. Gateway crate did not regain monolithic application dependency and API/local/MCP did not reintroduce old Gateway imports. Temporary application umbrella re-export remains for visibility cleanup. |

## Readiness

Ready:

- RuntimeGateway remains extracted and regression-free.
- Runtime surface adoption boundary is ready.
- Old Session launch/commit adapter-name boundary is retired.
- AgentRun -> Lifecycle dispatch direct construction boundary is retired.
- AgentRun frame construction -> Lifecycle helper implementation boundary is retired.

Not ready:

- RuntimeSession physical extraction: Session launch still consumes concrete AgentRun `FrameLaunchEnvelope`, and Session still has AgentRun implementation imports for mailbox/effective capability/surface helper paths.
- VFS physical extraction: owner-specific Session/Lifecycle/Canvas providers are isolated from generic registry mechanics but still live under `vfs`; `VfsSurfaceResolver` is an application facade and must not be moved with generic VFS core.
- Public visibility cleanup: temporary `agentdash-application` RuntimeGateway umbrella re-export and an application-internal consumer remain.

## P1 Follow-Up Owners

| Owner | Required next work |
| --- | --- |
| `work-items/04-runtime-session-substrate-boundary.md` | Introduce a neutral launch envelope DTO or RuntimeSession-owned launch DTO so Session launch stops binding to AgentRun `FrameLaunchEnvelope`; port mailbox auto-resume, effective capability and hook target resolution. |
| `work-items/03-agentrun-surface-facade.md` | Provide conversion from AgentRun frame construction output into the neutral launch envelope DTO without exposing AgentRun frame internals to RuntimeSession. |
| `work-items/07-vfs-resource-surface-boundary.md` | Split lifecycle/canvas/session owner providers out of generic VFS core placement and classify `VfsSurfaceResolver` as application facade. |
| `work-items/08-public-visibility-cleanup.md` | Remove RuntimeGateway umbrella re-export and update application-internal consumer imports. |
| `work-items/10-physical-crate-extraction-control-plane-vfs.md` | Do not start VFS physical extraction until owner providers are directional; do not start control-plane extraction until the next full import-graph readiness check passes. |

## Next Dispatch Bias

- Do not start RuntimeSession physical extraction yet.
- Do not start VFS physical extraction yet.
- Next implement wave should focus on RuntimeSession neutral launch envelope, mailbox/effective-capability/hook ports, and visibility cleanup.
- Run a control-plane readiness check after the next RuntimeSession substrate pass before starting AgentRun/Lifecycle physical extraction.
