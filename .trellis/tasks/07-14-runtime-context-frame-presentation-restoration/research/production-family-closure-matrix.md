# ContextFrame production family closure matrix

Reference oracle: `D:/Projects/AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`.

This matrix is the execution gate. A protocol type, serde fixture, projector unit test, or frontend parser does **not** count as a connected family. `CONNECTED` requires a real source fact, a production builder call, a canonical Runtime UoW, and an actual-producer stream comparison.

## Current correction baseline

| Family / dimension | Main source | Current production state | Required restoration |
| --- | --- | --- | --- |
| identity | `session/identity_context_frame.rs` | MISSING; identity exists only in a projector test | Persist/load base prompt, agent identity fragment, executor system prompt; port trim/empty/order rules |
| user_context | `session/user_context_frame.rs` | MISSING | Adapt `AuthIdentity`; port `system:` suppression, group fallback, `extra` null/value semantics |
| environment | `session/environment_context_frame.rs` | MISSING | Inject operation date/platform/arch plus executor/model/workdir; port Windows note and optional fields |
| system_guidelines | `session/guidelines_context_frame.rs` | MISSING | Reuse `derive_launch_context_discovery`; inject `SettingsRepository` for `agent.pi.user_preferences`; preserve preferences-before-project order |
| memory_context snapshot | `session/memory_context_frame.rs`, `memory_inventory_entries.rs` | MISSING | Reuse discovery output; port source/diagnostic flattening and empty suppression |
| assignment_context bootstrap/live | `session/assignment_context_frame.rs`, `hub/runtime_context_transition.rs` | MISSING | Persist immutable full assignment source snapshot; immediately support exact Hook injection fallback; never infer from `FrameContextBundleSummary` |
| initial capability_state_delta | `hub/runtime_context_transition.rs`, `dimension/*.rs` | PARTIAL/WRONG; only `ToolSchemaDelta` | Port complete state projection and exact section order: capability key -> tool path -> MCP -> companion -> VFS -> memory -> skill -> tool schema |
| live SurfaceAdopt delta | same as above | PARTIAL/WRONG; compares tools only, no deletion semantics | Diff previous/target normalized state for every dimension; capability frame first, optional assignment frame second |
| Hook model-visible effect | Hook/context transition builders | PARTIAL/DORMANT; arbitrary `effect_type=context_frame` JSON passthrough | Produce typed facts/builders at Hook boundary; Runtime validates typed effect and commits with HookRun |
| pending action | `pending_action_context_frame.rs`, Hook messages | PARTIAL/WRONG | Use actual source/status/revision/owners/instructions/injections/usage kinds and exact empty rules |
| Hook auto-resume / notices | launch preparation system-delivery builders | WRONG FAMILY | HookAutoResume must produce `system_delivery`; generic notices produce `system_notice`; do not substitute reserved `auto_resume` |
| managed compaction | `compaction_context_frame.rs`, eventing | PARTIAL/WRONG | Persist real summary/token/count/strategy/trigger/phase/boundaries/ref/time facts with candidate; project only at activation |

## Contract corrections required before porting builders

- Add production vocabulary missing from owned enums: `system_delivery`, `system_notice`, `applied_to_compacted_context`, and `continuation`.
- Keep `auto_resume` only if it remains a deliberate protocol extension; it must not represent main HookAutoResume behavior.
- Replace the current `context_frames_main_957fa9d.json` as an acceptance oracle. Its top-level delivery values are not the main builders' production values for identity, assignment, memory, and compaction.
- Treat delivery-plan order and durable eventstream order as different contracts:
  - delivery plan sorts by delivery phase/order/frame id;
  - main durable launch stream emits pending transitions first, then accepted insertion order: initial capability, initial assignment, system delivery, identity, user, environment, guidelines, memory, pending actions.

## Existing current-code facts to reuse

- `AgentBusinessSurfaceSource::load` is the only Application source adapter for the compiled surface and already owns AgentFrame, runtime surface, executor, callable tools, and Hook snapshot loading.
- `derive_launch_context_discovery` already implements the single VFS-based discovery pass for guidelines, memory, and Skill baseline. Its dependencies already exist in the same `AppState` composition scope; wire it rather than reimplementing discovery.
- `AgentRunRuntimeSurface` already carries capability state, VFS, MCP servers, identity, workflow provenance, and runtime coordinates.
- `hook_snapshot.injections` can implement main's exact assignment fallback mapping immediately.
- Full assignment fragments are not recoverable from `FrameContextBundleSummary`. Frame construction must persist an immutable normalized context-source snapshot keyed to the AgentFrame revision.

## Mandatory proof columns

Every row in the implementation review must contain all of the following and may not be marked done with blanks:

1. main builder and trigger source anchor;
2. current typed source fact and production loader callsite;
3. Runtime builder callsite;
4. canonical command/UoW carrying the frame;
5. actual-producer test that drives that command/UoW;
6. wrapper-neutral stream oracle result;
7. real AgentRun observation when the family is constructible in the dev fixture.
