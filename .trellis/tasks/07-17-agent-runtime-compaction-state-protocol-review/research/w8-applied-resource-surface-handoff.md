# W8 AgentRun Applied Resource Surface handoff

## Product authority

`AgentRunAppliedResourceSurfaceMaterializer::materialize` is the Product provision/materialize
entrypoint and must commit before Runtime activation. Its compiler input is the final
`AgentRunTarget`, the expected Product snapshot revision and the committed Product binding
digest. The compiler must receive these final facts from composition:

- LifecycleRun project id and the optional explicitly selected Workspace id.
- The applied Agent surface revision and digest that Host callbacks report.
- The fully compiled VFS mount inventory and explicit per-mount operations/path scopes.
- Explicit Product Task grants scoped to the project or one Task, with independent Read/Write
  operations. Target/project membership never grants Task access by itself.
- The canonical VFS digest and source/projection provenance.

The current repositories do not contain one transactional projection with all of those facts.
Composition must provide an `AgentRunAppliedResourceSurfaceCompilerPort`; startup-time assembly,
the removed AgentRun surface resolver, prompt data and Runtime/Host types are not authoritative
inputs.

## PostgreSQL schema contract

`agent_run_applied_resource_surface_snapshot`

- `run_id uuid not null`
- `agent_id uuid not null`
- `snapshot_revision bigint not null check (snapshot_revision > 0)`
- `project_id uuid not null`
- `workspace_id uuid null`
- `vfs_mounts jsonb not null`
- `default_mount_id text null`
- `vfs_grants jsonb not null`
- `agent_surface_revision bigint not null`
- `agent_surface_digest text not null check (agent_surface_digest <> '')`
- `vfs_digest text not null check (vfs_digest <> '')`
- `task_grants jsonb not null`
- `task_surface_revision bigint not null`
- `task_surface_digest text not null check (task_surface_digest <> '')`
- `task_source_kind text not null`
- `task_source_id text not null`
- `task_source_revision bigint not null`
- `task_projection_revision bigint not null`
- `task_captured_at_ms bigint not null`
- `product_binding_digest text not null check (product_binding_digest <> '')`
- `source_kind text not null`
- `source_id text not null`
- `source_revision bigint not null`
- `projection_revision bigint not null`
- `captured_at_ms bigint not null`
- primary key `(run_id, agent_id, snapshot_revision)`

All Rust `u64` values mapped to PostgreSQL `bigint` must be rejected at the Product boundary when
they exceed `i64::MAX`. Snapshot revisions are additionally positive. The remaining revision and
timestamp columns are non-negative, so SQL checks and Rust validation describe the same signed
storage range. The snapshot table does not use a uniqueness constraint over a subset of declared
digests: such a key cannot prove equality of VFS/Task payloads and provenance. Primary-key exact
row replay plus the current-pointer CAS is the idempotency authority.

`agent_run_applied_resource_surface_current`

- `run_id uuid not null`
- `agent_id uuid not null`
- `snapshot_revision bigint not null`
- primary key `(run_id, agent_id)`
- foreign key `(run_id, agent_id, snapshot_revision)` references the immutable snapshot table

Commit runs in one transaction. Insert the immutable snapshot, accepting only an exact replay of
all canonical evidence. Exact replay is determined by the primary-key row and byte-for-byte
equality of every scalar and typed JSONB column, not by a subset of caller-declared digests. A
same-revision row with any different payload, digest, revision or provenance is a conflict.
`AlreadyCurrent` is returned only when the current pointer references that exact row; an old
immutable row cannot replay successfully after the pointer has advanced.
Insert the first current pointer only when the expected revision is null; otherwise update it with
`where run_id = ? and agent_id = ? and snapshot_revision = expected_revision`. A zero-row CAS is
`Missing` or `Conflict`, never last-write-wins. A binding/Agent surface/VFS/Task digest change requires
`snapshot_revision + 1`; old grants are never copied implicitly. The query joins the current
pointer to the immutable snapshot in the same database snapshot and validates the echoed target
before returning the complete `AgentRunAppliedResourceSurfaceSnapshot`. It optionally fences an
expected snapshot revision and returns typed stale evidence; missing surface data is
`SurfaceNotApplied`, not a claim that Product binding is absent.

## Consumer mapping

Infrastructure authorizers compare the callback applied surface revision/digest byte-for-byte
with `agent_surface_revision`/`agent_surface_digest`, then match the requested mount, operation
and canonical relative path against `vfs_grants`. Paths are segmented on `/`; absolute paths,
backslashes, NUL, empty segments, `.` and `..` are rejected. A prefix matches itself or descendants
beginning at the next segment boundary, so `src` matches `src/lib.rs` but not `src2/lib.rs`.
`Read` never implies `List`, `Search`, `Write` or `Exec`; an absent grant never means the whole
mount. The workspace API may expose the same Product snapshot but must not independently
reconstruct VFS facts.
The typed authorization decision/audit evidence records the outer `snapshot_revision` together
with the Agent, VFS and Task revisions/digests, so one decision cannot accidentally combine
resource families from different Product snapshots.

Task executors map `AppliedTaskScope` and `AppliedTaskOperation` to their Runtime execution grant.
An absent scope or operation is a typed deny; Project scope never implies Write, and a Task scope
does not authorize sibling Tasks. VFS and Task grants are committed in the same immutable Product
snapshot/CAS so neither authorization family can observe a mixed revision.
