# Dispatch Round 4

## Start Condition

Round 4 starts after commit `a5fb8e34` recorded `checkpoint-wave-3.md`.

Ready from Round 3:

- RuntimeGateway remains extracted.
- Runtime surface adoption uses `RuntimeSurfaceAdoptionPort`.
- Old Session launch/commit adapter-name boundary is retired.
- AgentRun no longer constructs Lifecycle dispatch directly.
- AgentRun frame construction no longer imports Lifecycle helper implementation paths.
- Generic VFS registry no longer owns owner provider registration.

Not ready:

- RuntimeSession physical extraction is blocked by concrete AgentRun `FrameLaunchEnvelope`, mailbox adapter and effective-capability/surface helper imports.
- VFS physical extraction is blocked by owner-specific providers and application `VfsSurfaceResolver`.
- Gateway visibility cleanup is blocked by the temporary application umbrella re-export and application-internal consumers.

Round 4 is still a port-wiring / visibility convergence round. Do not start RuntimeSession, AgentRun, Lifecycle or VFS physical crate moves in this round.

## Shared Mode

- High concurrency is allowed, but each worker owns a narrow write set.
- Workers must preserve parallel edits, adapt to them, and report owner conflicts instead of reverting.
- Prefer command-driven mechanical work: `rg`, batch import rewrite, whole-module moves when scoped, narrow `cargo check -p`, and compiler-driven fixes.
- Do not hand-edit imports one by one when a mechanical rewrite is safer.
- Run only work-item minimal gates; broad readiness checks belong to checkpoint check agents.
- Delete obsolete paths/tests that only preserve stale behavior or old chains.
- Do not add compatibility shells for old module paths.

## Implement Worker Lanes

| Worker | Work item | Ownership | Must not touch |
| --- | --- | --- | --- |
| `session-neutral-envelope-impl` | `work-items/04-runtime-session-substrate-boundary.md` + `work-items/03-agentrun-surface-facade.md` | Neutral launch envelope DTO/port shape, `crates/agentdash-application-ports/src/frame_launch_envelope.rs`, `session/launch/**`, `session/hub/{mod,factory,facade,tests}.rs`, AgentRun frame construction producer conversion, API `frame_launch_envelope_provider.rs` | mailbox/effective capability wiring, VFS provider files, physical crate moves |
| `session-mailbox-capability-ports-impl` | `work-items/04-runtime-session-substrate-boundary.md` | `session/runtime_builder.rs`, `session/hub/tool_builder.rs`, `session/hub/hook_dispatch.rs`, mailbox auto-resume/effective capability/hook target ports, AgentRun mailbox/effective-capability adapters only where needed | launch envelope DTO migration, Gateway visibility, VFS providers |
| `gateway-visibility-cleanup-impl` | `work-items/08-public-visibility-cleanup.md` | Remove `agentdash-application` RuntimeGateway umbrella re-export, update application-internal consumers to `agentdash_application_runtime_gateway` or narrower ports | Session launch/mailbox internals, VFS owner providers |
| `vfs-owner-adapter-split-impl` | `work-items/07-vfs-resource-surface-boundary.md` | Split VFS owner-specific providers/facades into explicit owner modules or classify unmoved files; keep generic VFS core free of owner imports | Session runtime substrate, Gateway visibility, physical VFS crate move |

If a worker needs another lane's file, stop at the nearest compile boundary and report the conflict instead of widening ownership.

## Started

| Worker | Agent id | Nickname |
| --- | --- | --- |
| `session-neutral-envelope-impl` | `019efb37-1afa-7f42-8a89-4f6d46b2bf86` | Plato |
| `session-mailbox-capability-ports-impl` | `019efb37-2f70-7b23-a659-633a99ccbceb` | Fermat |
| `gateway-visibility-cleanup-impl` | `019efb37-443f-7f92-8d2e-44396d9a5bd1` | Gibbs |
| `vfs-owner-adapter-split-impl` | `019efb37-58f8-7b20-9ab1-8ad2d918300c` | Feynman |

## Implement Prompt Injection

Every implement worker prompt must start with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
Round: 4 substrate convergence
Work item: <path>
Checkpoint: .trellis/tasks/06-24-release-crate-split-draft/checkpoint-wave-3.md
```

Worker-specific bias:

- `session-neutral-envelope-impl`: Session launch must stop binding to AgentRun `FrameLaunchEnvelope`. Introduce a neutral DTO or RuntimeSession-owned launch DTO with the fields Session actually reads, then adapt AgentRun frame construction output into it. Do not hide the dependency behind a generic parameter that still names the AgentRun concrete type in Session.
- `session-mailbox-capability-ports-impl`: Session must stop depending directly on `AgentRunMailboxRuntimeAdapter`, `AgentRunEffectiveCapabilityService`, `AgentFrameSurfaceExt` and `project_capability_state_from_frame` in production paths. Use ports or composition-root adapters. Do not touch launch envelope migration.
- `gateway-visibility-cleanup-impl`: Remove the umbrella re-export and update consumers directly. Do not leave an alias module or compatibility shell.
- `vfs-owner-adapter-split-impl`: Generic VFS core must not import Session/Lifecycle/Canvas owners. Move owner-specific files behind owner modules where mechanically clear; if a file cannot move safely, document why and keep it out of future generic VFS crate scope.

Each handoff must include:

- changed files
- commands run
- commands not run and why
- stale paths/tests deleted
- unresolved imports or compile errors with owner assignment
- readiness classification for the next checkpoint

## Current Static Blockers

```powershell
rg -n "crate::agent_run|agentdash_application::agent_run" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "crate::runtime_gateway|pub use agentdash_application_runtime_gateway as runtime_gateway|agentdash_application::runtime_gateway" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "crate::session|crate::lifecycle|crate::canvas|provider_lifecycle|provider_canvas|mount_canvas|owner_providers|VfsSurfaceResolver" crates/agentdash-application/src/vfs crates/agentdash-api/src -g '*.rs'
```

## VFS Owner Split Classification

Round 4 `vfs-owner-adapter-split-impl` moved the mechanically clear owner files out of the generic VFS module expression:

| File | Classification | Future scope |
| --- | --- | --- |
| `crates/agentdash-application/src/canvas/vfs_mount.rs` | `move` from `vfs/mount_canvas.rs`; Canvas mount identity and access are derived from Canvas owner facts. | Canvas owner module. |
| `crates/agentdash-application/src/canvas/vfs_provider.rs` | `move` from `vfs/provider_canvas.rs`; provider mutates Canvas files and resolves Canvas binding projections. | Canvas owner module. |
| `crates/agentdash-application/src/lifecycle/vfs_provider.rs` | `move` from `vfs/provider_lifecycle.rs`; provider reads LifecycleRun, node artifact scope, journey/session evidence, and tool-result cache. | Lifecycle owner module. |
| `crates/agentdash-application/src/session/vfs_owner_providers.rs` | `move` from `vfs/owner_providers.rs`; registry owner composition needs Session persistence/tool-result cache plus Lifecycle and Canvas providers. | Session/application composition adapter. |
| `crates/agentdash-application/src/vfs_surface_resolver.rs` | `move` from `vfs/surface_resolver.rs`; resolver is an application facade over RepositorySet, AgentRun resource surface query, project workspace, and VFS summary. | Application facade outside generic VFS core. |

The future generic VFS crate scope is the remaining `crates/agentdash-application/src/vfs/**` core: provider registry mechanics, paths, service, summary, materialization, mutation, search, rewrite, inline/routine/skill/project/workspace mount mechanics, and tools. API `AppState` consumes `VfsSurfaceResolver` from the application facade, so the `VfsSurfaceResolver` grep hit is an application consumer reference rather than generic VFS core membership.

## Implementation Completed

All four implement workers completed and were integrated.

| Worker | Result |
| --- | --- |
| `session-neutral-envelope-impl` | `agentdash-application-ports::frame_launch_envelope::FrameLaunchEnvelope` is now the RuntimeSession-neutral launch DTO. Session launch/hub signatures no longer bind to AgentRun concrete `FrameLaunchEnvelope`; AgentRun frame construction converts its concrete output at the producer boundary. |
| `session-mailbox-capability-ports-impl` | Session production paths now consume `RuntimeSessionMailboxRuntimePort`, `RuntimeSessionEffectiveCapabilityPort` and resource-surface query ports instead of `AgentRunMailboxRuntimeAdapter`, `AgentRunEffectiveCapabilityService`, `AgentFrameSurfaceExt` or `project_capability_state_from_frame`. |
| `gateway-visibility-cleanup-impl` | Removed the `agentdash-application` RuntimeGateway umbrella re-export and updated application consumers to import `agentdash_application_runtime_gateway` directly. |
| `vfs-owner-adapter-split-impl` | Moved owner-specific VFS providers/facades out of generic `vfs/**` into Canvas, Lifecycle, Session/application adapter and application facade modules. |

Integration validation passed:

- `cargo fmt`
- `cargo fmt --check`
- `cargo metadata --no-deps --format-version 1`
- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-application-runtime-gateway -p agentdash-local -p agentdash-mcp`
- `cargo test -p agentdash-application-ports --no-run`
- `cargo test -p agentdash-application session:: --no-run`
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`
- `git diff --check`

Static gates passed with no matches:

```powershell
rg -n "crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope|FrameLaunchEnvelopePort<FrameLaunchEnvelope>|SharedFrameLaunchEnvelopePort<FrameLaunchEnvelope>" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "AgentRunMailboxRuntimeAdapter|AgentRunEffectiveCapabilityService|AgentFrameSurfaceExt|project_capability_state_from_frame" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "crate::runtime_gateway|pub use agentdash_application_runtime_gateway as runtime_gateway|agentdash_application::runtime_gateway" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

VFS owner gate now has only the expected API application-facade consumer reference:

```powershell
rg -n "crate::session|crate::lifecycle|crate::canvas|provider_lifecycle|provider_canvas|mount_canvas|owner_providers|VfsSurfaceResolver" crates/agentdash-application/src/vfs crates/agentdash-api/src -g '*.rs'
```

## Checkpoint Check Waves

Dispatch after implement workers complete:

| Check worker | Focus |
| --- | --- |
| `check-runtime-session-envelope` | Session launch no longer binds to concrete AgentRun `FrameLaunchEnvelope`; neutral envelope fields are sufficient and not a compatibility shell. |
| `check-runtime-session-live-ports` | Session mailbox/effective capability/hook target production paths consume ports/adapters instead of AgentRun implementation imports. |
| `check-gateway-visibility` | RuntimeGateway umbrella re-export removed; API/local/MCP/application consumers import the extracted crate or ports directly. |
| `check-vfs-owner-split` | Generic VFS core has no owner imports; remaining owner files are classified and excluded from physical VFS extraction. |
| `check-round-4-readiness` | Decide whether RuntimeSession physical extraction or AgentRun/Lifecycle physical extraction can start next. |

## Checkpoint Checks Started

| Worker | Agent id | Nickname |
| --- | --- | --- |
| `check-runtime-session-envelope` | `019efca4-f84c-7802-b12a-b50096240d95` | Singer |
| `check-runtime-session-live-ports` | `019efca5-0c65-7522-9ce3-d3b764eac68c` | Bernoulli |
| `check-gateway-visibility` | `019efca5-20f9-7b90-8899-f0d96d94af0c` | Rawls |
| `check-vfs-owner-split` | `019efca5-352a-7980-88ed-1f8ab7e5fb90` | Parfit |
| `check-round-4-readiness` | `019efca5-4a73-78e1-bba3-7a680d892402` | Galileo |

Check agents classify findings as `delete`, `move`, `port`, or `keep`, assign each finding to a work item owner, and avoid broad workspace tests unless narrow gates pass first.

## Round 4 Commit Rule

1. Commit dispatch docs before spawning workers.
2. Commit implement integration by compatible lanes if possible.
3. Commit checkpoint findings separately.
