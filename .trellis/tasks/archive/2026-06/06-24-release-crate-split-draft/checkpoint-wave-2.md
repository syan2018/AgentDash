# Checkpoint Wave 2

Date: 2026-06-25

## Summary

Round 2 completed the first physical crate extraction and retired several direct implementation edges:

- Added `agentdash-application-runtime-gateway` and moved RuntimeGateway implementation into it.
- API Gateway consumers now import `agentdash_application_runtime_gateway` directly.
- `agentdash-application` keeps a temporary `runtime_gateway` umbrella re-export only for application-internal transition paths.
- Session no longer imports Lifecycle directly and no longer calls `resolve_current_frame_from_delivery_trace_ref` in session scope.
- AgentRun runtime/resource surface paths now consume Lifecycle projection through ports DTOs and `LifecycleSurfaceProjectionPort`.
- Lifecycle dispatch consumes `RuntimeSessionCreationPort` and `AgentRunFrameConstructionPort`.
- API VFS routes no longer directly consume `agentdash_application::vfs::ResolvedVfsSurfaceSource` or `build_surface_summary`; route-local assembly moved behind `VfsSurfaceResolver`.

This checkpoint is green for targeted compile/test gates. It is a clean Gateway extraction checkpoint, not a RuntimeSession / AgentRun / Lifecycle / VFS physical extraction checkpoint.

## Validation

- `cargo metadata --no-deps --format-version 1`: passed
- `cargo fmt --check`: passed
- `cargo check -p agentdash-application-runtime-gateway`: passed
- `cargo test -p agentdash-application-runtime-gateway --no-run`: passed
- `cargo check -p agentdash-application-ports`: passed
- `cargo check -p agentdash-application`: passed
- `cargo check -p agentdash-api`: passed
- `cargo check -p agentdash-local -p agentdash-mcp`: passed
- `cargo test -p agentdash-application agent_run::runtime_surface`: passed, 13 tests
- `cargo test -p agentdash-application-ports vfs_surface_runtime`: passed, 5 tests
- `git diff --check`: passed

Checkpoint check agents reported:

- `check-runtime-gateway-crate` (`019efafd-3959-7b32-9da1-fc9d9e860c28`): Gateway crate extraction passes. No P0/P1/P2 findings. The temporary umbrella re-export is acceptable until a later visibility cleanup checkpoint.
- `check-session-port-wiring` (`019efafd-4db5-7971-9453-125757ec0ba8`): Session no longer imports Lifecycle, but RuntimeSession extraction is not ready because adoption, launch envelope, accepted launch commit, mailbox/effective capability and hook target resolution still need production port wiring.
- `check-control-plane-port-wiring` (`019efafd-6277-7b53-9d58-d4633c50658c`): Old AgentFrameBuilder/current-frame resolver gates pass, but AgentRun/Lifecycle extraction is not ready because AgentRun still constructs Lifecycle dispatch service and frame construction still imports lifecycle helper paths.
- `check-api-vfs-facade` (`019efafd-76c5-75a1-b79f-b43289097343`): API/VFS facade cleanup passes for Round 2. VFS physical extraction remains blocked by owner-specific provider dependencies.

## Readiness

Ready:

- RuntimeGateway crate extraction is complete enough to keep as a committed checkpoint.

Not ready:

- RuntimeSession crate extraction: `RuntimeSurfaceAdoptionPort` exists but production injection still uses the old AgentRun active adopter chain; launch envelope and accepted launch commit are still implementation-coupled.
- AgentRun/Lifecycle crate extraction: AgentRun still constructs `LifecycleDispatchService`, Lifecycle still owns concrete RuntimeSession creator placement, and AgentRun frame construction still consumes lifecycle implementation helpers.
- VFS core extraction: API direct helper use is fixed, but generic VFS still contains session/lifecycle/canvas owner providers.

## P1 Follow-Up Owners

| Owner | Required next work |
| --- | --- |
| `work-items/04-runtime-session-substrate-boundary.md` | Replace production adoption injection with `RuntimeSurfaceAdoptionPort`; move launch envelope, accepted launch commit/bootstrap status, mailbox auto-resume, effective capability and hook target resolution behind ports or composition-root adapters. |
| `work-items/03-agentrun-surface-facade.md` + `work-items/05-agentrun-lifecycle-boundary.md` | Stop AgentRun from constructing `LifecycleDispatchService` directly; replace remaining lifecycle helper imports in AgentRun frame construction with a port/facade. |
| `work-items/05-agentrun-lifecycle-boundary.md` | Move concrete `SessionPersistenceRuntimeSessionCreator` out of Lifecycle ownership or make it composition-root wiring before Lifecycle extraction. |
| `work-items/07-vfs-resource-surface-boundary.md` + `work-items/10-physical-crate-extraction-control-plane-vfs.md` | Split VFS owner-specific providers into owner adapters before generic VFS physical extraction. |
| `work-items/08-public-visibility-cleanup.md` | Remove temporary RuntimeGateway umbrella re-export and old AgentRun adoption facade once consumers use direct crates/ports. |

## Next Dispatch Bias

- Do not start RuntimeSession physical extraction yet.
- Do not start AgentRun/Lifecycle physical extraction yet.
- Do not start VFS physical extraction yet.
- Next implement wave should focus on production port wiring and public visibility cleanup, with Gateway only checked for regression.
