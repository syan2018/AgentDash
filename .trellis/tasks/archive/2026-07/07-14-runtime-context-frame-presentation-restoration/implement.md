# ContextFrame production restoration — corrective execution plan

## Outcome

The existing owned protocol, immutable artifact, canonical Runtime presentation lane, Session wrapper normalization, and frontend consumer remain the skeleton. The remaining work is to restore the **complete main production logic**, not to create more protocol scaffolding.

The task is complete only when every row in `research/production-family-closure-matrix.md` has a real production source, builder, canonical UoW, actual-producer golden, and (where constructible) real-run observation.

## Parallel execution model

Use one shared branch/workspace with explicit file ownership. Agents announce parallel edits and do not commit. The main agent owns shared exports/contracts, integration resolution, staging, commits, and final validation.

### Main coordinator — shared contract and integration owner

Exclusive responsibilities:

- lock the common facts interfaces and module exports before workers touch callsites;
- own `context_projection/mod.rs`, cross-worker enum/contract changes, API composition wiring, migrations, generated contracts, and task matrices;
- resolve overlaps; workers must not independently redesign shared facts;
- run the production-callsite matrix after each wave;
- stage and commit only after all three branches meet their actual-producer gates.

The coordinator first corrects the owned vocabulary (`system_delivery`, `system_notice`, `applied_to_compacted_context`, `continuation`) and defines three disjoint handoffs:

```text
BootstrapContextFacts
NormalizedContextSurfaceState + ContextSurfaceDelta
TurnRuntimePresentationFacts + CompactionPresentationFacts
```

This contract-lock step is deliberately small; it does not create another standalone skeleton work item or commit.

## Parallel branch A — complete bootstrap facts and ThreadStart stream

### Exclusive file area

- Application AgentRun frame/context source snapshot and `context_sources` adapter;
- new Runtime `context_projection/bootstrap*.rs` builders;
- bootstrap-specific actual producer tests.

Do not edit live-delta/compaction files owned by B/C or shared `mod.rs` owned by the coordinator.

### Move from main-reference

- `session/identity_context_frame.rs`
- `session/user_context_frame.rs`
- `session/environment_context_frame.rs`
- `session/guidelines_context_frame.rs`
- `session/assignment_context_frame.rs`
- `session/memory_context_frame.rs`
- `session/memory_inventory_entries.rs`
- launch preparation rules for startup predicate, user preferences, system delivery/notices, dedupe, delivery metadata, and accepted event insertion order
- Application-owned default/base identity prompt resolution formerly under executor/session composition

### Wire into current architecture

1. Extend frame construction to persist an immutable normalized `AgentContextSourceSnapshot` with the AgentFrame revision. It contains the full assignment fragments/source coordinates; `FrameContextBundleSummary` remains control metadata and is never treated as content.
2. Expand `AgentBusinessSurfaceSource` dependencies with the already-available VFS/discovery providers, `SettingsRepository`, base identity source, explicit projection clock/platform facts.
3. Call existing `derive_launch_context_discovery` exactly once for guidelines, memory, and Skill baseline.
4. Build complete `BootstrapContextFacts` from request/surface/frame/executor/discovery/Hook facts.
5. Port main builders and exact empty/null/order/delivery rules.
6. Make `AgentSurfaceCompiler::compile_business_facts` emit the full bootstrap plan, not a fixed tool-only frame.
7. Drive the real surface compiler and real `ThreadStart`; compare the durable stream in main insertion order.

### Branch A gate

- full fixture yields the expected non-empty identity, user, environment, guidelines, memory, capability, and assignment frames;
- independent empty fixtures prove each main suppression rule;
- driver instructions/context and ContextFrame presentation derive from the same facts without turning ContextFrame into model input;
- actual ThreadStart stream, not a projector-only test, matches main payload and event order.

## Parallel branch B — complete normalized surface state and SurfaceAdopt stream

### Exclusive file area

- new Runtime `context_projection/surface_state*.rs`, `dimension/*.rs`, and live projector files;
- `RuntimeSurfacePresentationPlan::for_adoption` replacement;
- SurfaceAdopt actual command/journal tests and live fixtures.

Do not edit Application bootstrap sources or turn-runtime/compaction producers.

### Move from main-reference

- pure builder/diff portions of `session/hub/runtime_context_transition.rs`
- `session/dimension/capability_key.rs`
- `tool_path.rs`
- `mcp_server.rs`
- `companion_agent.rs`
- `vfs.rs`
- `memory.rs`
- `skill.rs`
- `tool_schema.rs`
- main `compute_capability_state_delta` semantics and exact render/section order

### Wire into current architecture

1. Persist a complete `NormalizedContextSurfaceState` inside the compiled immutable artifact: capability keys/state, tool paths/schemas, MCP identity/readiness, companions, VFS, memory inventory, Skills, and assignment revision/fragments.
2. Compute one typed previous/target `ContextSurfaceDelta`; do not derive dimensions from Driver DTOs or bootstrap frames.
3. Replace the tool-only `for_adoption` with the complete main projector.
4. Emit at most one capability-state frame with non-empty sections in exact order, followed by an independent assignment frame when applicable.
5. Cover added/removed/changed semantics for every dimension and main's tool-schema selection rules.
6. Preserve empty-delta, assignment-only, replay, CAS failure, and exact-artifact semantics already present in the Runtime UoW.

### Branch B gate

- one actual SurfaceAdopt scenario exercises all eight sections plus assignment and matches main;
- one actual scenario per dimension proves isolated add/remove/change behavior;
- a state change with unchanged tools still emits the required non-tool frame;
- empty delta emits nothing; assignment-only emits only assignment; replay does not duplicate; failure does not adopt surface.

## Parallel branch C — Hook, pending, system delivery/notices, compaction

### Exclusive file area

- protocol ContextFrame production enum/section additions after coordinator contract lock;
- new Runtime `context_projection/turn_runtime*.rs` and `compaction*.rs` builders;
- Runtime Hook/context state and Application facade callsites for these producers;
- their actual-producer tests.

Do not edit bootstrap/live surface projectors owned by A/B.

### Move from main-reference

- pending-action builder and Hook message formatting/owner/usage-kind rules;
- `system_delivery` and `system_notice` builders used by HookAutoResume and queued notices;
- Hook injection-to-assignment mapping where it belongs to turn-runtime facts;
- `session/compaction_context_frame.rs` and the eventing extraction of real compaction facts;
- Hook model-visible context effect rules, represented as typed facts rather than arbitrary presentation JSON.

### Wire into current architecture

1. Replace reserved `auto_resume` production with main-equivalent `system_delivery`; support `system_notice` for generic notices.
2. Rebuild pending frames from actual Hook source/status/revision/owners/instructions/injections and exact empty rules.
3. Change Hook context effects from stringly arbitrary JSON to typed presentation facts; keep HookRun/effect/frame in one Runtime commit.
4. Add `CompactionPresentationFacts` to the candidate/checkpoint path so activation has real summary, token/message counts, strategy, trigger, phase, source boundaries, compacted reference, and timestamp.
5. Keep peek/ack, replay, and canonical UoW behavior, but compare the actual produced payload against main.

### Branch C gate

- actual turn-start stream proves system delivery/notice and pending payload/order;
- Hook actual completion proves typed effect, rollback, and replay;
- managed compaction actual activation proves the full main payload; opaque driver compaction emits no frame;
- no hard-coded revision, placeholder summary, fabricated token count, or arbitrary ContextFrame JSON remains.

## Integration wave — coordinator only

After A/B/C pass their local gates:

1. integrate shared contracts and remove obsolete tool-only and wrong-family code paths;
2. rebuild the oracle from main production builders; delete/replace fixtures with incorrect delivery status/channel/role;
3. generate a machine-readable closure report with the seven mandatory proof columns for every family;
4. run cross-layer checks and inspect production callsites with `rg`; enum/fixture-only matches never satisfy the report;
5. use a real dev Agent with a full context fixture and assert the required bootstrap family set, not merely “at least one ContextFrame”;
6. trigger a live surface transition that changes non-tool dimensions and verify capability frame + assignment ordering;
7. trigger Hook/pending/system-delivery and managed compaction scenarios, then inspect journal payload/revision/replay;
8. confirm the Session boundary is the only wrapper change and frontend session source remains behavior-identical to main.

## Commit plan

Parallel work does not imply parallel commits. Commit only integrated, independently reviewable outcomes:

1. `refactor(context): 补齐 ContextFrame 全量业务事实与投影`
   - full source snapshot, bootstrap builders, normalized surface state/dimensions, typed turn-runtime/compaction facts.
2. `feat(runtime): 接通全部 ContextFrame production producer`
   - ThreadStart, SurfaceAdopt, Hook/pending/system-delivery, compaction actual UoWs with exact payloads.
3. `fix(session): 验证 main 等价 ContextFrame 全链路`
   - corrected production goldens, closure report, generated contracts/specs, real multi-family AgentRun evidence.

No branch may commit independently. No task archive or completion statement is allowed until the real run observes the complete required family set and the closure matrix has no MISSING/PARTIAL/WRONG row.

## Validation commands

Run targeted suites in parallel where they do not compete for the Cargo target lock, then one final serialized full gate:

```powershell
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-api journal_projection
cargo test -p agentdash-infrastructure agent_runtime_composition
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-codex
pnpm contracts:check
pnpm --dir packages/app-web test
cargo check --workspace --all-targets
pnpm dev
```

Final manual/API evidence must enumerate actual frame kinds/sections/order from the new journal and compare them to the corrected main production oracle.
