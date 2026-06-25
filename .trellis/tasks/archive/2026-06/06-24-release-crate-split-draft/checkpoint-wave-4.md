# Checkpoint Wave 4

Date: 2026-06-25

## Summary

Round 4 completed the RuntimeSession substrate convergence pass after RuntimeGateway extraction:

- RuntimeSession launch now consumes the neutral `agentdash-application-ports::frame_launch_envelope::FrameLaunchEnvelope` DTO instead of naming AgentRun concrete launch envelope types in Session/API signatures.
- The stale launch-envelope provider shell was deleted from both ports and AgentRun; AgentRun keeps only a construction-local input for frame composition.
- RuntimeSession live production paths now use `RuntimeSessionMailboxRuntimePort`, `RuntimeSessionEffectiveCapabilityPort` and `RuntimeSessionHookTargetPort`.
- RuntimeGateway visibility cleanup is complete: `agentdash-application` no longer exposes the extracted Gateway as an umbrella re-export.
- Generic VFS core no longer declares Lifecycle/Canvas/Session owner providers or Lifecycle catalog/mount helpers. Lifecycle VFS catalog and mount construction now live under `lifecycle`.

Code checkpoint commit:

- `193e2022 refactor(crate-split): 收束第四轮 checkpoint fixes`

This checkpoint is green for the targeted compile/no-run/static gates below. It is not a RuntimeSession or AgentRun/Lifecycle physical extraction checkpoint.

## Validation

- `cargo fmt`: passed
- `cargo fmt --check`: passed
- `cargo metadata --no-deps --format-version 1`: passed
- `cargo check -p agentdash-application-ports`: passed
- `cargo check -p agentdash-application`: passed
- `cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp`: passed
- `cargo test -p agentdash-application-ports --no-run`: passed
- `cargo test -p agentdash-application session:: --no-run`: passed
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`: passed
- `git diff --check`: passed

Static gates passed with no matches:

```powershell
rg -n "FrameLaunchEnvelopeProvider|FrameLaunchEnvelopeProviderInput|SharedFrameLaunchEnvelopeProvider|RuntimeDeliveryCommandRef" crates -g '*.rs'
rg -n "crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope|FrameLaunchEnvelopePort<FrameLaunchEnvelope>|SharedFrameLaunchEnvelopePort<FrameLaunchEnvelope>" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "AgentRunMailboxRuntimeAdapter|AgentRunEffectiveCapabilityService|AgentFrameSurfaceExt|project_capability_state_from_frame" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "crate::agent_run::frame::hook_runtime::AgentFrameHookRuntime|AgentFrameHookRuntime::" crates/agentdash-application/src/session -g '*.rs' -g '!hook_delegate.rs'
```

VFS owner split gate now has only the expected API application-facade consumer reference:

```powershell
rg -n "crate::session|crate::lifecycle|crate::canvas|provider_lifecycle|provider_canvas|mount_canvas|owner_providers|VfsSurfaceResolver|lifecycle_catalog|mount_lifecycle" crates/agentdash-application/src/vfs crates/agentdash-api/src -g '*.rs'
```

Expected hit:

- `crates/agentdash-api/src/app_state.rs` consumes `agentdash_application::vfs_surface_resolver::VfsSurfaceResolver`.

## Check Agent Results

| Worker | Agent id | Result |
| --- | --- | --- |
| `check-runtime-session-envelope` | `019efca4-f84c-7802-b12a-b50096240d95` | Passed after deleting the stale provider shell. Session launch signatures no longer bind to AgentRun concrete envelope types. |
| `check-runtime-session-live-ports` | `019efca5-0c65-7522-9ce3-d3b764eac68c` | Passed after adding `RuntimeSessionHookTargetPort`. Production hook runtime creation is behind an AgentRun adapter; remaining `AgentFrameHookRuntime` hits are test-only in `session/hook_delegate.rs`. |
| `check-gateway-visibility` | `019efca5-20f9-7b90-8899-f0d96d94af0c` | Passed. Consumers import the extracted Gateway crate or ports directly. |
| `check-vfs-owner-split` | `019efca5-352a-7980-88ed-1f8ab7e5fb90` | Passed after moving Lifecycle catalog/mount helpers into `lifecycle`. Generic VFS core has no owner module declarations. |
| `check-round-4-readiness` | `019efca5-4a73-78e1-bba3-7a680d892402` | RuntimeSession is still partial; AgentRun/Lifecycle remains blocked; generic VFS core is ready for physical extraction with owner adapters excluded. |

## Readiness

Ready:

- RuntimeGateway extracted crate remains clean.
- RuntimeSession neutral launch envelope boundary is ready.
- RuntimeSession mailbox/effective capability/hook target live production paths are port-mediated.
- Generic VFS core is ready for a physical extraction attempt, with `canvas`, `lifecycle`, `session/vfs_owner_providers.rs` and `vfs_surface_resolver.rs` kept outside the new generic crate.

Not ready:

- RuntimeSession physical extraction: remaining production imports still include AgentRun frame close/capability-delta helpers such as `session/hub/facade.rs` and `session/runtime_transition_service.rs`.
- AgentRun/Lifecycle physical extraction: cross-control links remain in AgentRun read models/dispatch paths and Lifecycle materialization/projection paths. These require a dedicated control-plane convergence wave before crate moves.
- Final workspace check is intentionally deferred; this checkpoint uses targeted gates because the branch is still mid-extraction.

## P1 Follow-Up Owners

| Owner | Required next work |
| --- | --- |
| `work-items/10-physical-crate-extraction-control-plane-vfs.md` | Start with VFS core physical extraction. Move only generic VFS mechanics; leave owner providers, owner mounts and `VfsSurfaceResolver` outside. |
| `work-items/04-runtime-session-substrate-boundary.md` | Port or relocate remaining frame close and capability delta helpers so RuntimeSession stops importing AgentRun implementation modules. |
| `work-items/03-agentrun-surface-facade.md` | Provide AgentRun-side adapters for the remaining frame close/capability transition facts without leaking builder/surface implementation to RuntimeSession. |
| `work-items/05-agentrun-lifecycle-boundary.md` | Continue AgentRun/Lifecycle control-plane convergence before physical crate extraction. |

## Next Dispatch Bias

- Prefer Round 5 as VFS core physical extraction, because generic VFS core is the only extraction-ready large boundary after Round 4.
- Do not move owner-specific providers into `agentdash-application-vfs`.
- Keep implement workers on narrow file ownership and minimal gates; use check agents for VFS crate purity and compile blocker assignment.
- After the main crate-split task completes, create a separate follow-up task to evaluate isolated large modules such as Canvas and Marketplace for extraction. Do not mix that follow-up into the current VFS core move.
