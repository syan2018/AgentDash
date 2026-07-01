# Implementation Plan

## Current Evidence

- Task evidence and prior injection map: `.trellis/tasks/archive/2026-05/04-29-session-context-builder-unification/research/context-injection-map.md`
- ContextFrame protocol: `crates/agentdash-spi/src/hooks/mod.rs`
- Turn preparation: `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`
- PiAgent system assembly: `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- Memory frame: `crates/agentdash-application-runtime-session/src/session/memory_context_frame.rs`
- continuation frame: `crates/agentdash-application-runtime-session/src/session/continuation.rs`
- compaction frame: `crates/agentdash-application-runtime-session/src/session/compaction_context_frame.rs`
- Frontend frame model/render: `packages/app-web/src/features/session/model/contextFrame.ts`, `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`

## Ordered Work

### W1. Protocol and Planner Skeleton

- Add delivery metadata types near `ContextFrame` or introduce `ContextDeliveryPlan`.
- Define phase/order/cache/channel/consumption enums.
- Add serde support and frontend parser shape.
- Add unit tests for stable ordering and cache metadata.

### W2. Runtime-Session Planning

- Introduce `ContextDeliveryPlanner` in runtime-session or a closely scoped module.
- Convert current `TurnPreparer` push-order semantics into planner entries.
- Preserve existing accepted context frame emission.
- Make dedupe happen before final planning or explicitly document where it sits.

### W3. PiAgent Consumption

- Replace `assemble_system_prompt` kind list with plan-driven filtering.
- Keep PiAgent system prompt limited to `stable_system` / `session_policy` entries marked for system/developer consumption.
- Update connector tests so memory no longer appears in system prompt.

### W4. Memory Reclassification

- Change `memory_context` delivery metadata to discovered inventory.
- Remove memory from system prompt consumption.
- Ensure memory inventory/index remains visible through model context when applicable.
- Add tests for digest/cache policy and PiAgent visibility.

### W5. continuation_context Cleanup

- Remove or rename `continuation_context` after confirming no producer.
- Default path: remove builder, envelope field, launch plan field, preparation consumption, frontend parser/render/test fixture.
- Keep `compaction_summary` tests green and add guard that compaction summary remains visible.

### W6. Frontend Official Order

- Parse delivery metadata.
- Group/sort by phase/order in `ContextFrameStream`.
- Display phase/cache/channel/consumption in compact debug-visible form.
- Update ContextFrame tests and fixtures.

### W7. Validation

- Rust targeted tests:
  - `cargo test -p agentdash-application-runtime-session context`
  - `cargo test -p agentdash-executor assemble_system_prompt`
  - exact package/module names may be adjusted after implementation.
- Frontend targeted tests:
  - `pnpm --filter app-web test -- ContextFrame`
- Final broader checks follow project norms once implementation scope is known.

## Parallel Execution

Recommended sub-agent split:

| Agent | Work | Output |
| --- | --- | --- |
| Implement-A | SPI + delivery plan type skeleton | protocol/types/tests |
| Implement-B | runtime-session planner and frame ordering | planner integration/tests |
| Implement-C | PiAgent + memory reclassification | connector and memory tests |
| Implement-D | frontend parser/render | UI/model tests |
| Implement-E | continuation cleanup + compaction guard | cleanup patch/tests |
| Check | cross-layer review after patches land | check report |

Efficient sequence:

1. Main session finalizes PRD/design decision.
2. Implement-A creates the smallest compiling protocol/planner interface.
3. B/C/D start after A's shape is stable.
4. E starts immediately because continuation cleanup is independent.
5. Check agent runs after B/C/D/E converge.

## Files With High Conflict Risk

- `crates/agentdash-spi/src/hooks/mod.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`

## Pre-Implementation Gate

- Confirm the first connector profile shape for non-PiAgent system consumption: system override, system append, connector-native, ignore, or audit-only.
- Confirm deletion path for `continuation_context`.
- Decide whether `ContextDeliveryPlan` is embedded in `ContextFrame` metadata or emitted as a distinct plan object.
