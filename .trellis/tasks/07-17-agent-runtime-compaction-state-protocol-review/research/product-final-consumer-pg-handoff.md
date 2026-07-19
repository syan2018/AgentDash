# Product final consumer PostgreSQL handoff

## Runtime command durable claim

Application port: `ProductRuntimeCommandClaimRepository`.

The production adapter must be merged into the single hard-cut migration `0084`. It owns a row
keyed by `(target_run_id, target_agent_id, client_command_id)` with:

- `request_digest` over the Product command payload plus caller-observed Runtime revision;
- the fully resolved `ManagedRuntimeCommandEnvelope` JSON, including operation/idempotency IDs,
  Runtime thread, expected revision, and the resolved SubmitInput-versus-Steer command;
- creation evidence.

`load` and `claim` return `ProductRuntimeCommandClaimError`, not storage strings. A digest mismatch
maps to `RequestDigestConflict { target, client_command_id }`; infrastructure failures map to
`Storage { message }`. The adapter must not encode domain outcomes in message prefixes.

`load` must reject a different digest. `claim` must insert once and return the already committed
envelope on a uniqueness race. A retry checks this claim before reading the latest Runtime
snapshot, so a Runtime-accepted command whose response was lost replays the byte-equivalent
envelope even after Runtime revision or active-turn state advances.

Before resolving a new command, the facade requires matching command-availability evidence from
the same Runtime snapshot revision. Missing evidence is unavailable; stale evidence returns
`StaleAvailabilityEvidence`; unavailable evidence preserves the exact
`ManagedRuntimeUnavailabilityReason` together with `ManagedRuntimeCommandKind`. W8 acceptance tests
must cover missing/unavailable/stale availability, target binding mismatch, Runtime source mismatch,
expected-revision conflict, exact replay after restart, digest conflict, and lost-response replay.

## Product mailbox projection

Application ports: `ProductMailboxReadRepository` and `ProductMailboxCommandRepository`.

`ProductMailboxReadRepository::snapshot` is one transactional read/reconcile boundary. W8 must read
messages and mailbox state from the same database snapshot, compute the canonical digest, reconcile
the Product head/change, and return the cursor and commit evidence matching that exact state before
committing. The facade must not call message/state/projection repositories separately. The returned
snapshot, every message, and mailbox state must all carry the requested target; a mixed-target
result is a typed `TargetMismatch`, never a partially accepted snapshot.

The production schema needs a per-target projection head:

- monotonically increasing `revision`;
- monotonically increasing `latest_change_sequence`;
- canonical snapshot digest;
- typed commit time;
- target primary key.

The ordered change table is keyed by `(target_run_id, target_agent_id, sequence)` and stores a
unique change ID, revision, canonical snapshot digest, typed commit time, and typed origin
(`Command` with client ID/kind or `CanonicalReconcile`). Head, change, terminal receipt, and
snapshot must expose the same `ProductMailboxSnapshotDigest` and commit evidence produced by the
transaction. Changes are never inferred from `MAX(updated_at)`; deletions and equal timestamps
therefore cannot regress or collapse a cursor.

The `changes(after, limit)` contract is strictly ordered and reconnect-safe: sequences are
contiguous from `after + 1`, revisions never regress, and the returned cursor matches the last
change. If W8 applies bounded retention, it returns `ProductMailboxChangeGap` with
requested/earliest/latest sequence, current snapshot revision, current snapshot digest, and typed
detection time; without retention, absence of a gap is mechanically guaranteed. External
Companion/Workflow mailbox mutations are reconciled by the same transactional snapshot boundary
and therefore advance exactly one Product change for the complete state actually observed, never
for a facade-assembled hybrid.

The canonical digest is defined by `canonical_product_mailbox_digest`: schema tag plus the ordered
complete mailbox messages and mailbox state, serialized as deterministic JSON and hashed with
SHA-256. The function owns product ordering: priority descending, order key ascending, and message
UUID ascending as a stable tie-breaker. JSON object keys are recursively normalized. The PG adapter
must map its transaction-local rows into the same domain values and call that function rather than
depending on query order or inventing a SQL/debug-string digest.

All read-port failures are `ProductMailboxReadError` variants (`TargetMismatch`,
`MessageNotFound`, `InvalidContinuity`, or `Storage`). Infrastructure adapters must preserve these
categories directly instead of relying on storage-message prefix inspection.

## Mailbox command transaction

`ProductMailboxCommandRepository::execute` is one durable unit of work. In a single PostgreSQL
transaction it must:

1. target-fence every referenced message and move anchor before mutation;
2. claim `(target, client_command_id, request_digest)`, returning the terminal stored result for
   an exact duplicate and rejecting a different digest;
3. apply Promote/Delete/Move/Resume to the canonical mailbox rows;
4. read the resulting mailbox messages and state from that same transaction snapshot;
5. compute the canonical digest and atomically advance projection revision/change sequence;
6. persist the terminal command result containing the accepted revision/change cursor;
7. commit mutation, projection change, and terminal receipt together.

A non-terminal receipt must never trigger blind side-effect replay. Transaction rollback is the
crash recovery mechanism before commit; after commit the terminal receipt is returned verbatim.
Delete and Move must validate message and anchor target ownership before issuing any update, so a
cross-target ID can never mutate state and be rejected afterward. Command-port failures are typed
as `RequestDigestConflict`, `TargetMismatch`, `MessageNotFound`, `NonTerminalReceipt`, or `Storage`;
the adapter must not collapse them into `Result<_, String>`.

W8's real PostgreSQL behavior fixture must execute Promote/Delete/Move/Resume and assert that
canonical mutation, head, ordered change, and terminal receipt commit as one unit. It must also
cover injected failure before commit with complete rollback, restart replay, same-client digest
conflict with zero mutation, cross-target message/anchor rejection with zero mutation, external
canonical mutation reconciliation, strict sequence paging, retention-gap evidence, and concurrent
claim/reconcile behavior.

## Composition

W8 supplies production implementations of all three ports and injects them into:

- `AgentRunProductCommandFacade`;
- `ProductMailboxFacade`;
- the mounted Product Runtime command and Product mailbox routes.

No Session identifier, host transport field, executor/backend/delivery override, or second Runtime
read model belongs in these tables or adapters.
