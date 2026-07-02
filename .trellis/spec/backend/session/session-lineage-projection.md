# Session Lineage Projection

## Scope

Session lineage projection applies to AgentDash-owned cross-session fork and rollback flows. It records how independently resumable runtime traces relate to each other; lifecycle subject association、agent lineage and external connector private state stay in their own stores.

## Durable Stores

`session_lineage` records the session tree edge for a child session. A child has one primary parent edge with `relation_kind`, `fork_point_event_seq`, optional `fork_point_ref_json`, optional `fork_point_compaction_id`, `status` and metadata.

`session_projection_heads` records the active model-visible cursor for `(session_id, projection_kind)`. Rollback moves this cursor and appends an audit event; append-only `session_events` remain the factual timeline.

`session_compactions` and `session_projection_segments` remain the checkpoint surface used by `ContextProjector`. Fork initial projection uses `strategy = "fork_initial_projection"` and a `context_envelope` segment containing serialized `AgentInputMessage` entries from the parent fork point.

## Application Contract

`SessionBranchingService` owns branch use cases:

- `fork_session` resolves a message ref, compaction fork point or the current projection head; creates child `SessionMeta`; writes a `Fork` lineage edge; commits a child initial compaction; and initializes the child model context projection head.
- `fork_session` accepts an explicit compaction fork point only when that compaction has committed projection facts and covers the requested fork event head, because the child initial projection must not inherit model context that is outside the parent boundary.
- Explicit message refs are resolved against the current projected transcript and must land on a complete model-input boundary. A ref gives the fork service both a stable user-facing coordinate and the persisted source range needed to materialize the child context.
- `rollback_model_projection` appends `session_projection_rolled_back` as a platform event and upserts `session_projection_heads(model_context)` to the requested target head. The target is bounded by the current model-visible projection head, because append-only `session_events` can contain facts that rollback has already hidden from model input.
- `lineage_view` returns the direct parent edge, ancestors and direct children. List surfaces that only need parent grouping read the direct parent edge instead of the full lineage view.

`ContextProjector` builds model input from projection heads. For fork materialization it can build at a specific event head and can build from an explicit compaction id. `context_envelope` segments are projection-origin, synthetic model input entries, and keep original message provenance under their segment provenance.

## Diagnostic API Contract

The retained HTTP routes are internal diagnostics for runtime trace inspection:

- `POST /sessions/{id}/fork`
- `GET /sessions/{id}/lineage`
- `POST /sessions/{id}/projection/rollback`

These routes must resolve the `RuntimeSessionExecutionAnchor` and apply Project `Use` permission before returning trace facts or mutating projection state. RuntimeSession lineage is not a product interaction surface because it does not materialize `LifecycleRun`, `LifecycleAgent`, `AgentFrame`, AgentRun mailbox, or cross-run AgentRun lineage facts.

DTOs live in `agentdash-contracts::session` and are generated to `packages/app-web/src/generated/session-contracts.ts`. Runtime trace list entries include `parent_session_id` and `parent_relation_kind`; diagnostic APIs derive these from direct `session_lineage` parent edges. Product control trees use AgentRun workspace projections and AgentRun scoped runtime endpoints, while session lineage stays a trace/debug projection.

`SessionBranchingService::fork_session` always creates `SessionLineageRelationKind::Fork`. Other relation kinds remain trace facts of the lineage model; companion, spawned-agent and rollback-branch semantics are owned by lifecycle / agent services because they imply different lifecycle policy, visibility and restore behavior from an ordinary trace fork.

## Ownership Boundary

`session_lineage` explains runtime branch topology and restore provenance. Business visibility is projected through `LifecycleSubjectAssociation` and `AgentLineage`; runtime fork routes return trace refs, while product surfaces decide visibility through subject / agent / run views.
