# ContextFrame production family closure matrix

Reference oracle: `D:/Projects/AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`.

This matrix is the execution gate. A protocol type, serde fixture, projector unit test, or frontend parser does **not** count as a connected family. `CONNECTED` requires a real source fact, a production builder call, a canonical Runtime UoW, and an actual-producer stream comparison.

## Current closure status

| Family / dimension | Main source | Current production state | Required restoration |
| --- | --- | --- | --- |
| identity | `session/identity_context_frame.rs` | CONNECTED | Immutable bootstrap facts carry base prompt, identity fragment and executor prompt with main ordering/empty rules |
| user_context | `session/user_context_frame.rs` | CONNECTED | `AuthIdentity` is normalized with main suppression, group fallback and nullable-extra semantics |
| environment | `session/environment_context_frame.rs` | CONNECTED | Operation environment and executor/model/workdir facts are projected through the bootstrap plan |
| system_guidelines | `session/guidelines_context_frame.rs` | CONNECTED | One `derive_launch_context_discovery` pass supplies preferences and project guidelines in main order |
| memory_context snapshot | `session/memory_context_frame.rs`, `memory_inventory_entries.rs` | CONNECTED | Discovery memory sources/diagnostics use main flattening and empty suppression |
| assignment_context bootstrap/live | `session/assignment_context_frame.rs`, `hub/runtime_context_transition.rs` | CONNECTED | Immutable normalized assignment sources drive bootstrap and live projection |
| initial capability_state_delta | `hub/runtime_context_transition.rs`, `dimension/*.rs` | CONNECTED | All eight dimensions are projected in main section order |
| live SurfaceAdopt delta | same as above | CONNECTED | Previous/target normalized state produces capability-first, assignment-second adoption frames |
| Hook model-visible effect | Hook/context transition builders | CONNECTED | Typed semantic Hook facts are projected and committed with HookRun in one Runtime UoW |
| pending action | `pending_action_context_frame.rs`, Hook messages | CONNECTED | Actual Hook source/status/revision/owners/instructions/injections drive TurnStart projection |
| Hook auto-resume / notices | launch preparation system-delivery builders | CONNECTED | HookAutoResume uses `system_delivery`; generic notices use `system_notice` |
| managed compaction | `compaction_context_frame.rs`, eventing | CONNECTED | Preparation persists real compaction facts and activation projects the summary atomically |

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

## Implementation evidence ledger

`context_frames_canonical_roundtrip.json` only proves the AgentDash-owned JSON vocabulary can round-trip and that the Session wrapper removes no payload fields. It is intentionally excluded from every actual-producer column below. Production equality must be asserted from builder output driven through the canonical command/UoW.

| Family | Main builder / trigger | Current typed source / production loader | Runtime builder | Canonical command / UoW | Actual-producer test | Wrapper-neutral oracle | Real AgentRun |
| --- | --- | --- | --- | --- | --- | --- | --- |
| identity | `session/identity_context_frame.rs::build_identity_context_frames`; `launch/preparation.rs` | `BootstrapContextFacts.identity`; `AgentBusinessSurfaceSource::load` via persisted context sources | `context_projection/bootstrap.rs::build_identity_context_frame` | compiled bootstrap plan carried by `ThreadStart` | `runtime_facade::compiled_full_bootstrap_is_committed_by_real_thread_start_in_main_order` | exact frame assertion from compiled artifact + normalized journal payload; protocol fixture is not evidence | OBSERVED event 5, `prepared_for_connector / connector_context / system` |
| user_context | `session/user_context_frame.rs::build_user_context_frame`; `launch/preparation.rs` | `BootstrapContextFacts.user`; `AgentBusinessSurfaceSource::load` | `bootstrap.rs::build_user_context_frame` | `ThreadStart` | same full-bootstrap test | same actual journal assertion | OBSERVED event 6, nullable extras retained |
| environment | `session/environment_context_frame.rs::build_environment_context_frame`; `launch/preparation.rs` | `BootstrapContextFacts.environment`; `AgentBusinessSurfaceSource::load` | `bootstrap.rs::build_environment_context_frame` | `ThreadStart` | same full-bootstrap test | same actual journal assertion | OBSERVED event 7, Windows/model/executor/workdir facts |
| system_guidelines | `session/guidelines_context_frame.rs::build_guidelines_context_frame`; launch discovery | `BootstrapContextFacts.guidelines`; `derive_launch_context_discovery` wired by `AgentBusinessSurfaceSource::load` | `bootstrap.rs::build_guidelines_context_frame` | `ThreadStart` | same full-bootstrap test | same actual journal assertion | OBSERVED event 8, project `AGENTS.md` section |
| memory_context snapshot | `session/memory_context_frame.rs::build_memory_context_frame`; launch discovery | `BootstrapContextFacts.memory`; discovery result loaded by `AgentBusinessSurfaceSource::load` | `bootstrap.rs::build_memory_context_frame` | `ThreadStart` | same full-bootstrap test | same actual journal assertion | PENDING dev observation |
| assignment bootstrap | `session/assignment_context_frame.rs::build_assignment_context_frame`; `launch/preparation.rs` | `BootstrapContextFacts.assignment`; persisted normalized context-source snapshot loaded by `AgentBusinessSurfaceSource::load` | `bootstrap.rs::build_assignment_context_frame` | `ThreadStart` | same full-bootstrap test | exact compiled/journal order assertion | OBSERVED event 4, `project_agent`, context model channel |
| capability_state_delta bootstrap | `session/hub/runtime_context_transition.rs::build_context_frame`; initial transition | `NormalizedContextSurfaceState`; `AgentBusinessSurfaceSource::load` | `context_projection/live.rs::build_initial_capability_frame` plus `dimension/*` | `ThreadStart` | `business_surface::business_facts_order_initial_capability_then_assignment_then_stable_context` + full-bootstrap ThreadStart test | exact eight-dimension section-order assertion | OBSERVED event 3, first durable ContextFrame; non-empty capability sections only |
| capability_state_delta live | `session/hub/runtime_context_transition.rs::build_live_context_frame`; runtime transition | previous/target `NormalizedContextSurfaceState` persisted in `AgentSurfaceSnapshot` | `context_projection/live.rs::project_surface_transition` plus `dimension/*` | `SurfaceAdopt` | `runtime_interface::surface_adopt_is_cas_guarded_idle_only_and_enters_driver_outbox` (must retain exact two-frame assertion) | exact capability-before-assignment journal assertion | PENDING non-tool live transition |
| assignment live | `session/hub/runtime_context_transition.rs::build_workflow_assignment_context_frame` | previous/target normalized assignment in surface snapshots | `context_projection/live.rs::project_surface_transition` | `SurfaceAdopt` | same SurfaceAdopt actual-producer test | exact second-frame assertion | PENDING live transition |
| pending_action | `session/pending_action_context_frame.rs::build_pending_action_context_frame`; hook collection in launch preparation | `AgentRunTurnStartContextFacts.pending_actions` from Hook runtime snapshot | `context_projection/turn_runtime.rs::project_pending_action` | `TurnStart` presentation batch | `runtime_facade::turn_start_pending_and_system_delivery_match_main_stream_family_and_order` | exact actual batch payload/order assertion | PENDING dev Hook action |
| system_delivery | `session/launch/preparation.rs::build_system_delivery_context_frame`; HookAutoResume/system launch | `AgentRunPresentationInput::SystemDelivery` and typed launch source | `turn_runtime.rs::project_system_delivery` | `TurnStart` | same pending/system-delivery test + `runtime_facade_delivery_sources_match_main_delivery_golden_exactly` | exact source/kind/actor/body assertion from actual command | PENDING dev HookAutoResume |
| system_notice | launch queued notice conversion in `session/launch/preparation.rs` | typed Hook/mailbox notice facts in turn-start context | `turn_runtime.rs::project_system_notice` | `TurnStart` / Hook terminal presentation UoW | `hook_orchestration` actual HookRun test and Runtime facade mixed-notice test | exact typed frame assertion; no `auto_resume` substitution | PENDING dev notice |
| compaction_summary | `session/compaction_context_frame.rs::build_compaction_context_frame`; `session/eventing.rs` activation | `CompactionPresentationFacts` persisted on candidate by prepare worker | `context_projection/compaction.rs::project_compaction_summary` | compaction acceptance/activation UoW | `context_compaction::compaction_acceptance_and_recovery_work_are_atomic`; missing facts rejection test | exact activation journal assertion; candidate fixture only supplies typed source facts | PENDING managed compaction |

Rows remain open until their final column is observed in the real dev journal. A serde fixture, projector-only unit test, or enum match cannot close a row.

## Real dev journal evidence (2026-07-15)

- Run / Agent: `17e29e03-2296-46a9-a4df-0fdbe29893b6 / e954a081-0545-4c37-bf7e-36eac7a19fe4`.
- Runtime thread: `thread-17e29e03-2296-46a9-a4df-0fdbe29893b6-e954a081-0545-4c37-bf7e-36eac7a19fe4`；binding target is `pi-agent:PI_AGENT` with `consume` delivery.
- Durable bootstrap frame order is event 3..8: `capability_state_delta -> assignment_context -> identity -> user_context -> environment -> system_guidelines`.
- The dev fixture had no memory inventory, so main's empty suppression rule produced no `memory_context`; the row remains open until a non-empty inventory is constructed.
- The actual user input remained `user_input_submitted`; the reply remained `agentMessage("OK")`; no tool call was synthesized.
- Manual compaction could not reach the Runtime command: migration 65 removed `agent_run_command_receipts` while the product compact route still claims through `PostgresAgentRunCommandReceiptRepository`, so the API returned `500 数据库操作失败: agent_run_command_receipts`. This is an external product-command cutover blocker, not accepted as compaction-frame evidence.
