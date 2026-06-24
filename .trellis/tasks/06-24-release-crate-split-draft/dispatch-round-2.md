# Dispatch Round 2

## Start Condition

Round 2 starts after commit `47004415` fixed the Wave 1/2 checkpoint:

- `agentdash-application-ports` purity passed.
- RuntimeGateway MCP/setup dependencies are port-mediated.
- Targeted compile/test gates passed.
- Check agents agreed that only RuntimeGateway-only extraction is ready.

## Worker Lanes

| Worker | Work item | Ownership | Do not touch |
| --- | --- | --- | --- |
| `runtime-gateway-crates-impl` | `work-items/09-physical-crate-extraction-runtime.md` Gateway subset | workspace manifests, `crates/agentdash-application-runtime-gateway/**`, `runtime_gateway/**` move, API/bootstrap Gateway dependency wiring | RuntimeSession, AgentRun, Lifecycle, VFS physical moves |
| `session-port-wiring-impl` | `work-items/04-runtime-session-substrate-boundary.md` | `crates/agentdash-application/src/session/**`, session bootstrap wiring, stale SessionConstruction tests | RuntimeGateway crate move, AgentRun/Lifecycle implementation rewrites outside adapter seams |
| `control-plane-port-wiring-impl` | `work-items/03-agentrun-surface-facade.md` + `work-items/05-agentrun-lifecycle-boundary.md` | AgentRun/Lifecycle projection/materialization port wiring, workflow orchestration materializer seams | RuntimeSession physical crate move, API VFS route cleanup |
| `api-vfs-facade-impl` | `work-items/06-api-consumer-facade-cleanup.md` + `work-items/07-vfs-resource-surface-boundary.md` | API VFS surface route/helper cleanup and application VFS preview/resource facade wiring | Generic VFS physical crate extraction, RuntimeGateway/Session crate moves |

## Started

| Worker | Agent id | Nickname |
| --- | --- | --- |
| `runtime-gateway-crates-impl` | `019efad7-d8f0-73e2-8881-901a3615746e` | Franklin |
| `session-port-wiring-impl` | `019efad7-ed62-74a3-9454-7b096193ff63` | Pascal |
| `control-plane-port-wiring-impl` | `019efad8-01bc-7da0-bd80-a39d68cd481b` | Descartes |
| `api-vfs-facade-impl` | `019efad8-1605-7bb1-bac2-1043db14f0b6` | Avicenna |

## Shared Bias

- Start every worker prompt with `Active task: .trellis/tasks/06-24-release-crate-split-draft`.
- Prefer mechanical move/replace commands over hand-editing imports one by one.
- Run only minimal gates owned by the work item; broad readiness checks go to checkpoint check agents.
- Delete stale path/test pairs when they only preserve the old chain.
- Do not add compatibility shells for old module paths.
- Preserve parallel worker edits and report conflicts by owner.

## Current Blockers To Retire

| Blocker | Owner |
| --- | --- |
| RuntimeSession direct imports of AgentRun/Lifecycle for launch/adoption/mailbox/effective capability | `session-port-wiring-impl` |
| AgentRun/Session direct consumption of Lifecycle projector/current-frame resolver | `control-plane-port-wiring-impl` |
| Lifecycle-local `RuntimeSessionCreator` contract instead of `runtime_session_delivery` port | `control-plane-port-wiring-impl` + `session-port-wiring-impl` |
| API VFS route-local parsing/summary assembly using application VFS internals | `api-vfs-facade-impl` |
| VFS owner-specific provider dependencies blocking generic VFS extraction | `api-vfs-facade-impl` reports, no physical extraction yet |

## Checkpoint Plan

After these workers finish, dispatch:

- `check-runtime-gateway-crate`: verify the new Gateway crate does not depend on monolithic application or owner implementations.
- `check-session-port-wiring`: verify Session direct imports have become ports or documented live-runtime internals.
- `check-control-plane-port-wiring`: verify AgentRun/Lifecycle mutual links use ports/facades.
- `check-api-vfs-facade`: verify API route/helper VFS direct imports are removed or classified as presentation/debug read-model.

## Expected Stage Commit

Use a checkpoint commit after worker integration, even if some port-wiring gates remain red, as long as failures are owner-attributed.
