# Product final consumer PostgreSQL handoff

## Runtime command durable claim

Application port: `ProductRuntimeCommandClaimRepository`.

The production adapter must be merged into the single hard-cut migration `0084`. It owns a row
keyed by `(target_run_id, target_agent_id, client_command_id)` with:

- `request_digest` over the Product command payload plus caller-observed Runtime revision;
- the fully resolved `ManagedRuntimeCommandEnvelope` JSON, including operation/idempotency IDs,
  Runtime thread, expected revision, and the resolved SubmitInput-versus-Steer command;
- creation evidence.

`load` must reject a different digest. `claim` must insert once and return the already committed
envelope on a uniqueness race. A retry checks this claim before reading the latest Runtime
snapshot, so a Runtime-accepted command whose response was lost replays the byte-equivalent
envelope even after Runtime revision or active-turn state advances.

## Product mailbox projection

Application ports: `ProductMailboxReadRepository` and `ProductMailboxCommandRepository`.

`ProductMailboxReadRepository::snapshot` is one transactional read/reconcile boundary. W8 must read
messages and mailbox state from the same database snapshot, compute the canonical digest, reconcile
the Product head/change, and return the cursor matching that exact state before committing. The
facade must not call message/state/projection repositories separately.

The production schema needs a per-target projection head:

- monotonically increasing `revision`;
- monotonically increasing `latest_change_sequence`;
- canonical snapshot digest;
- target primary key.

The ordered change table is keyed by `(target_run_id, target_agent_id, sequence)` and stores a
unique change ID, revision, canonical snapshot digest, and commit time. Changes are never inferred
from `MAX(updated_at)`; deletions and equal timestamps therefore cannot regress or collapse a
cursor. The `changes(after, limit)` contract is ordered and reconnect-safe. If W8 applies bounded
retention, it returns `ProductMailboxChangeGap` with requested/earliest/latest/snapshot revision;
without retention, absence of a gap is mechanically guaranteed. External Companion/Workflow
mailbox mutations are reconciled by the same transactional snapshot boundary and therefore advance
one Product change for the complete state actually observed, never for a facade-assembled hybrid.

The canonical digest is defined by `canonical_product_mailbox_digest`: schema tag plus the ordered
complete mailbox messages and mailbox state, serialized as deterministic JSON and hashed with
SHA-256. The PG adapter must map its transaction-local rows into the same domain values and call
that function rather than inventing a SQL- or debug-string digest.

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
Delete must validate target ownership before issuing the update, so a cross-target message ID can
never be deleted and rejected afterward.

## Composition

W8 supplies production implementations of all three ports and injects them into:

- `AgentRunProductCommandFacade`;
- `ProductMailboxFacade`;
- the mounted Product Runtime command and Product mailbox routes.

No Session identifier, host transport field, executor/backend/delivery override, or second Runtime
read model belongs in these tables or adapters.
