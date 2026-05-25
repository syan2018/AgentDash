# Session Lineage Projection

## Scope

Session lineage projection applies to AgentDash-owned session branch, fork and rollback flows. It does not replace owner binding, workflow ownership or external connector private state.

## Durable Stores

`session_lineage` records the session tree edge for a child session. A child has one primary parent edge with `relation_kind`, `fork_point_event_seq`, optional `fork_point_ref_json`, optional `fork_point_compaction_id`, `status` and metadata.

`session_projection_heads` records the active model-visible cursor. Rollback moves this cursor and appends an audit event; it does not delete `session_events`.

`session_compactions` and `session_projection_segments` remain the checkpoint surface used by `ContextProjector`. Fork initial projection uses `strategy = "fork_initial_projection"` and a `context_envelope` segment containing serialized `AgentInputMessage` entries from the parent fork point.

## Application Contract

`SessionBranchingService` owns branch use cases:

- `fork_session` resolves an event, message ref or compaction fork point; creates child `SessionMeta`; writes the lineage edge; commits a child initial compaction; and initializes the child model context projection head.
- `rollback_model_projection` appends `session_projection_rolled_back` as a platform event and upserts `session_projection_heads(model_context)` to the requested target head.
- `lineage_view` returns the direct parent edge, ancestors and direct children.

`ContextProjector` builds model input from projection heads. For branch restore it can build at a specific event head and can build from an explicit compaction id. `context_envelope` segments are projection-origin, synthetic model input entries, and keep original message provenance under their segment provenance.

## API Contract

The HTTP surface is exposed through ACP session routes:

- `POST /sessions/{id}/fork`
- `GET /sessions/{id}/lineage`
- `POST /sessions/{id}/projection/rollback`

DTOs live in `agentdash-contracts::session` and are generated to `packages/app-web/src/generated/session-contracts.ts`. Project session list entries include `parent_session_id` and `parent_relation_kind`; the API derives these from `session_lineage` first, with companion context as the narrow companion fallback.

Session detail surfaces lineage through the same generated DTO. The chat view branch panel reads `GET /sessions/{id}/lineage` and displays parent source, relation status, fork point and direct children beside the model context projection view.

## Ownership Boundary

`session_lineage` explains branch topology and restore provenance. Business visibility and project/story/task ownership remain in `session_bindings`. API fork routes copy the parent owner bindings to the child after the runtime branch commit so the new session is immediately visible under the same owner surface.
