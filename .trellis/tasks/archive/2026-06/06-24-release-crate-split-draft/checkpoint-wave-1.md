# Checkpoint Wave 1

Date: 2026-06-25

## Summary

Round 1 fixed the first useful boundary checkpoint:

- `agentdash-application-ports` now exposes the planned DTO / trait / error modules for AgentRun, RuntimeSession, Lifecycle, RuntimeGateway and VFS boundaries.
- RuntimeGateway setup actions consume `runtime_gateway_setup` ports; concrete MCP/workspace adapters live in API composition root.
- API current-surface helpers consume AppState-owned facade handles instead of rebuilding the query per request.
- Terminal launch target derivation moved behind the AgentRun runtime surface facade.
- Lifecycle no longer passes `AgentFrameBuilder` across the workflow AgentCall materialization edge; builder usage is kept inside AgentRun frame materialization.

This checkpoint is green for targeted compile/test gates, but it is not a full extraction readiness point for RuntimeSession, AgentRun/Lifecycle or VFS.

## Validation

- `cargo metadata --no-deps --format-version 1`: passed
- `cargo check -p agentdash-application-ports`: passed
- `cargo check -p agentdash-application`: passed
- `cargo check -p agentdash-api`: passed
- `cargo check -p agentdash-local -p agentdash-mcp`: passed
- `cargo test -p agentdash-application runtime_gateway::setup_actions`: passed, 11 tests
- `cargo fmt --check`: passed
- `git diff --check`: passed
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`: passed, `implement.jsonl` 29 entries, `check.jsonl` 13 entries

Checkpoint check agents also reported:

- `check-boundary-ports` (`019efaca-c269-7612-a546-c45ecfaf4c62`): ports crate is pure DTO / trait / error; no boundary leakage found.
- `check-import-graph` (`019efaca-d6d0-7562-a14b-3bf94228d735`): RuntimeGateway setup and Lifecycle `AgentFrameBuilder` gates are clean; AgentRun/Session/Lifecycle and API/VFS direct imports still block broader extraction.
- `check-dead-paths` (`019efaca-ebbb-71d1-a04a-b16c749c42b9`): stale SessionConstruction test fixture paths and unanchored RuntimeSession fallback tests should be removed during RuntimeSession cleanup.
- `check-wave-readiness` (`019efacb-00a9-7253-bf96-5a9b7c3e9c87`): RuntimeGateway-only physical extraction can start; RuntimeSession, AgentRun/Lifecycle and VFS physical extraction must wait for port wiring cleanup.

## Extraction Readiness

Ready:

- RuntimeGateway-only extraction from `work-items/09-physical-crate-extraction-runtime.md`.

Not ready:

- RuntimeSession extraction: session launch/adoption/mailbox/effective capability still directly import AgentRun/Lifecycle implementation.
- AgentRun/Lifecycle extraction: mutual implementation imports remain through lifecycle projector/current-frame resolver and AgentRun frame materialization helper.
- VFS extraction: generic VFS core still mixes owner providers and API routes still consume application VFS internals.

## P0 / P1 Follow-Up Owners

| Owner | Required next work |
| --- | --- |
| `work-items/04-runtime-session-substrate-boundary.md` | Move Session launch/adoption/mailbox/effective capability consumption to ports; delete stale SessionConstruction test fixture paths. |
| `work-items/03-agentrun-surface-facade.md` + `work-items/05-agentrun-lifecycle-boundary.md` | Replace direct lifecycle projector/current-frame resolver imports with ports; replace AgentRun frame materialization helper coupling with `agent_frame_materialization` port. |
| `work-items/05-agentrun-lifecycle-boundary.md` | Make Lifecycle consume `runtime_session_delivery` port instead of lifecycle-local `RuntimeSessionCreator` contract. |
| `work-items/06-api-consumer-facade-cleanup.md` + `work-items/07-vfs-resource-surface-boundary.md` | Move API VFS surface parsing/summary building behind an application facade or VFS port. |
| `work-items/08-public-visibility-cleanup.md` | Contract public re-exports after consumers move to ports/facades. |

## Next Dispatch Bias

- `runtime-gateway-crates-impl`: only move RuntimeGateway into a new crate; do not touch RuntimeSession, AgentRun, Lifecycle or VFS.
- `session-port-wiring-impl`: no physical crate move; replace direct AgentRun/Lifecycle imports with ports and remove stale tests.
- `control-plane-port-wiring-impl`: wire AgentRun/Lifecycle through lifecycle projection and frame materialization ports.
- `api-vfs-facade-impl`: move API VFS route-local business assembly behind VFS/AgentRun facades.
- `visibility-impl`: run only after consumers have moved; use compiler errors to remove obsolete public exports and tests.
