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
- Fork initial projection commit persists the child `session_branch_forked` Backbone platform event as the child RuntimeSession durable event. Product AgentRun journal reuses that event as the single fork marker and maps it into AgentRun journal sequence.
- Explicit message refs are resolved against the current projected transcript and must land on a complete model-input boundary. Turn completion is judged from the persisted `BackboneEnvelope` terminal event (`TurnCompleted` or platform `turn_terminal`), while `session_update_type` and session summary status remain query/projection fields. A ref gives the fork service both a stable user-facing coordinate and the persisted source range needed to materialize the child context.
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

## Scenario: AgentRun Product Fork Over RuntimeSession Projection

### 1. Scope / Trigger

- Trigger: product fork / fork-submit creates a user-visible AgentRun from a stable RuntimeSession projection boundary.
- Scope: `AgentRunForkService`, `SessionBranchingService::fork_session`, `agent_run_lineages`, `RuntimeSessionExecutionAnchor`, AgentRun command receipts, mailbox delivery, workspace projection, and generated workflow contracts.

RuntimeSession lineage remains the projection provenance layer. AgentRun fork is the product use case because it also creates control-plane ownership, workspace navigation, command idempotency, mailbox intake, and cross-run lineage.

### 2. Signatures

Product HTTP surface:

```text
POST /agent-runs/{run_id}/agents/{agent_id}/fork
POST /agent-runs/{run_id}/agents/{agent_id}/fork-submit
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
```

Runtime primitive:

```rust
SessionBranchingService::fork_session(parent_session_id, fork_point_ref, options)
```

Durable product lineage:

```sql
agent_run_lineages.parent_run_id
agent_run_lineages.parent_agent_id
agent_run_lineages.child_run_id
agent_run_lineages.child_agent_id
agent_run_lineages.relation_kind = 'fork'
agent_run_lineages.parent_frame_id
agent_run_lineages.parent_frame_revision
agent_run_lineages.child_frame_id
agent_run_lineages.child_frame_revision
agent_run_lineages.fork_point_event_seq
agent_run_lineages.fork_point_ref_json
agent_run_lineages.forked_by_user_id
agent_run_lineages.metadata_json
```

Ownership facts:

```sql
lifecycle_runs.created_by_user_id
lifecycle_agents.created_by_user_id
```

### 3. Contracts

- `AgentRunForkService` claims the outer AgentRun command receipt before creating a child RuntimeSession. The request digest includes current user, parent run / agent refs, fork point, optional input, and executor/backend selection.
- `SessionBranchingService::fork_session` validates the model-visible boundary and materializes child RuntimeSession projection only. It does not create LifecycleRun, LifecycleAgent, AgentFrame, mailbox messages, or product lineage.
- `AgentRunForkMaterializationPort` adopts the child RuntimeSession into a new LifecycleRun / LifecycleAgent / AgentFrame and writes `agent_run_lineages` in one persistence transaction.
- `fork-submit` writes the submitted `Vec<UserInputBlock>` into the child AgentRun mailbox. Parent AgentRun mailbox and parent RuntimeSession event stream remain unchanged.
- `AgentRunForkOutcomeView` returns `outcome="forked"`, parent refs, child refs, lineage ref, optional child mailbox result, and `redirect={run_id, agent_id}`.
- Project `Use` allows reading visible parent AgentRun and creating a current-user fork. Project `Configure` is not required for AgentRun participation.
- AgentRun owner is `created_by_user_id == current_user.user_id` unless a future explicit control grant says otherwise. Project owner/editor does not silently become owner of another user's AgentRun.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Caller lacks Project `Use` on parent project | reject before runtime fork |
| Parent AgentRun has no current delivery RuntimeSession | return conflict / delivery_missing |
| Fork point ref is outside current projection head | return invalid fork point |
| Fork point lands on incomplete user/assistant/tool-result boundary | return unstable fork point; no child RuntimeSession |
| Duplicate command receipt accepted | replay stored child refs, mailbox result, and redirect |
| Duplicate command receipt pending with no accepted refs | return retryable conflict; do not create another child RuntimeSession |
| Runtime fork succeeds, AgentRun materialization fails | best-effort delete child RuntimeSession and mark receipt terminal failed with diagnostics |
| Materialization succeeds, fork-submit mailbox write fails | preserve child AgentRun refs in diagnostics; response must distinguish mailbox failure from explicit fork success |
| Non-owner submits to a visible AgentRun | create child AgentRun owned by current user and deliver input to child mailbox |
| Owner explicitly forks own AgentRun | create child AgentRun as exploration branch; parent remains unchanged |

### 5. Good/Base/Bad Cases

- Good: member opens another user's visible AgentRun, submits input, receives `outcome="forked"` and navigates to a child AgentRun whose mailbox contains the input.
- Good: owner clicks fork on a stable assistant round, receives child refs, and continues from that boundary without mutating parent mailbox.
- Base: repeated explicit fork with the same `client_command_id` replays the same child refs.
- Boundary mismatch: returning a child RuntimeSession id alone leaves the browser without AgentRun ownership, mailbox, or navigation facts.
- Canonical flow: RuntimeSession projection fork is immediately adopted into an AgentRun and exposed through AgentRun scoped contracts.

### 6. Tests Required

- RuntimeSession branching tests cover stable message refs, unfinished turns, assistant tool-call boundaries, incomplete tool-result groups, compaction fork points, and projection cleanup.
- Application tests cover explicit fork, fork-submit, current-user ownership, cross-run lineage, duplicate replay, pending duplicate conflict, terminal failure replay, and parent immutability.
- API tests cover Project `Use` permission, no Project `Configure` requirement for participation, `composer-submit` fork outcome, and retained diagnostic Session route permission.
- Frontend tests cover fork redirect navigation, round action disabled reasons, and no product caller using raw Session fork / lineage / rollback services.
- AgentRun journal tests cover the child `session_branch_forked` event appearing exactly once in the visible journal after parent lineage prefix.

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```text
diagnostic runtime trace fork -> child runtime trace id -> browser continues without AgentRun ownership
```

#### Canonical

```text
POST /agent-runs/{run}/agents/{agent}/fork
  -> AgentRunForkService
  -> SessionBranchingService::fork_session
  -> AgentRunForkMaterializationPort
  -> AgentRunForkOutcomeView { child_run_id, child_agent_id, redirect }
```
