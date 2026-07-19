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
The typed authorization decision/audit evidence records the outer `snapshot_revision`, Agent
surface revision/digest, VFS revision/digest/provenance, Task revision/digest/provenance, pinned
Product binding digest and Host binding generation. One decision therefore cannot combine
resource families from different Product snapshots or callback generations.

Task executors map `AppliedTaskScope::Project | Task` and `AppliedTaskOperation` to their Runtime
execution grant. The authorizer resolves the concrete scope from read/write arguments, including
batch operations. The Product Task service applies the same scope again: a Task grant pins reads
to that Task and rejects create, snapshot, reorder or any operation naming a sibling; a Project
grant retains project-plan semantics. An absent scope or operation is a typed deny; Project scope
never implies Write. VFS and Task grants are committed in the same immutable Product snapshot/CAS
so neither authorization family can observe a mixed revision.

## Runtime activation fence

The Product runtime binding query consumed by `ProductRuntimeToolAuthorizer` must return one
committed activation record containing the Product binding, its canonical digest, the applied
resource snapshot revision and the Host binding generation. Those four facts are one atomic
read. Authorization always queries `applied_resource_surface(target, Some(pinned_revision))`;
querying the current/latest surface without the activation pin would allow an older callback
generation to observe newly expanded grants.

The committed resource snapshot must attest the exact Product binding digest, target and applied
Agent surface revision/digest reported by the Host-resolved callback. A missing pin, generation
mismatch, stale revision, changed binding digest or changed Agent surface evidence is a typed deny
before the executor is entered.

The W8 PostgreSQL implementation owns the unique migration, repository and production composition.
Its commit path must serialize writers for one `(run_id, agent_id)` authority key (for example,
with a transaction-scoped advisory lock) before reading the current pointer and performing the
immutable-row replay/CAS checks above. The first insert is subject to the same serialization; a
concurrent first writer cannot bypass the expected-null rule. Tests must exercise concurrent first
insert, same-row exact replay, same-revision full-row conflict, stale current-pointer CAS and an
old activation pinned to revision N while current has advanced to N+1.

## Production composition input

The Platform component exports the dependency-light `PlatformToolBroker` and
`RuntimePlatformToolHandler`. Application exposes a Product Task service contract; application-vfs
exposes a per-invocation VFS service input that accepts only a typed applied surface and canonical
owner coordinates. Neither Application crate normal-depends on Runtime or Agent Service API.
Infrastructure owns `ProductRuntimeToolAuthorizer`, the Runtime executor adapters and the
mechanical `final_runtime_tool_catalog` constructor.

W8 composition constructs one non-empty broker catalog only through
`final_runtime_tool_catalog(vfs_service, task_service)`. Its exact inventory is:

1. `mounts_list`
2. `fs_read`
3. `fs_glob`
4. `fs_grep`
5. `fs_apply_patch`
6. `shell_exec`
7. `task_read`
8. `task_write`

Each catalog definition obtains its parameter schema mechanically from the module that owns the
real serde parser. Application-VFS exports the six executor schemas from their concrete parameter
types; the Product Task service exports the read/write schemas from its concrete parameter and
operation types. The Infrastructure catalog only dispatches to those eight owner methods. This
keeps required fields, nested object shape, enum values, defaults, descriptions and
`additionalProperties` policy identical to execution-time parsing without introducing a second
handwritten schema or an Application dependency on Runtime concrete types.

The VFS service receives no construction-time AgentRun VFS. Every invocation maps the Host-resolved
callback context and authorized `RuntimeVfsExecutionGrant` into a fresh VFS plus exact access
policy, then statically dispatches one of the six Application-VFS-owned executors. Those executors
own their typed result/error/update contract and preserve the existing provider, overlay,
materialization, shell streaming/PTY and terminal-continuation behavior. The target
Runtime → Infrastructure → Application-VFS path therefore contains no `DynAgentTool`,
`AgentTool`, `ToolProtocolProjector`, `ToolUpdateCallback` or `agentdash-agent-types` value, while
each invocation remains isolated from every other AgentRun's mounts.

The service owns the bounded execution coordinators whose semantics intentionally span individual
executor objects. `fs_read` keeps one bounded LRU keyed by
`(run_id, agent_id, runtime_thread_id, mount_id, path, offset, limit)`, so repeated reads survive
fresh per-invocation executor construction while different Runtime owners remain isolated.
`fs_apply_patch` shares one mutation queue and keys locks by the actual
`(provider, backend_id, root_ref, normalized_path)` backing identity, so two mount aliases or two
concurrent callback invocations cannot mutate the same file concurrently.

The current-lane `AgentTool` surface is now a one-way adapter over the six direct executors. W8
removes its sole external consumer,
`crates/agentdash-application/src/runtime_tools/vfs_provider.rs`, and then deletes this complete
adapter set:

- `crates/agentdash-application/tests/runtime_tool_catalog.rs` is the test-only consumer that
  composes `VfsRuntimeToolProvider`; W8 must replace its catalog assertions with coverage of
  `final_runtime_tool_catalog` when the production composition switches, rather than retaining the
  current-lane provider for the test.
- `crates/agentdash-application-vfs/src/tools/factory.rs`
  (`VfsToolFactory` / `VfsToolFactoryInput` and its `DynAgentTool` catalog).
- `MountsListTool`, `FsReadTool`, `FsGlobTool`, `FsGrepTool`, `FsApplyPatchTool` and
  `ShellExecTool`, including their `AgentTool` impls, from the corresponding files under
  `crates/agentdash-application-vfs/src/tools/`.
- `legacy_result`, `legacy_error` and `legacy_update_sink` plus the legacy wrapper re-exports in
  `tools/mod.rs` and `tools/fs.rs`.
- Wrapper-only schema/projector fixtures and wrapper-shaped tests after equivalent direct-executor
  coverage is retained.
- The direct `agentdash-agent-types` and now-unused `agentdash-agent-protocol` Cargo dependencies
  after the wrapper imports are gone.

That deletion removes the Application-VFS `agentdash-agent-types` dependency and all remaining
Platform SPI `AgentTool` imports from the crate. The direct executor structs, parameter
deserialization, `VfsToolExecutionResult`, `VfsToolExecutionError`, `VfsToolUpdateSink`,
`AppliedVfsRuntimeToolService`, `VfsService`, overlays, materialization and terminal registries
remain as the canonical VFS execution path.

W8 injects the concrete atomic binding query, applied-surface query, Product Task service, VFS
service/materialization/terminal dependencies, then installs `RuntimePlatformToolHandler` as the
Complete Agent tool callback handler. The broker rejects an empty or duplicate catalog at
composition time. Its acceptance gate includes a source scan over the three target-path modules
(`runtime_tools.rs`, Infrastructure `runtime_tool_executors.rs` and the VFS execution contract)
and a final Cargo dependency scan after the adapter deletion.

The Complete Agent callback broker persists its reservation before entering either the tool or hook
handler and bounds that handler by the remaining absolute callback deadline. Crossing the deadline
records `InspectionRequired` because the platform side effect may already have occurred; restart or
duplicate delivery replays that durable nonterminal fact without invoking the handler again. Only
the existing explicit inspect/reconcile contract may settle it as a typed result or `Unknown`.

## Component verification

- Complete Agent callback tests: 11 passed, covering cooperative timeout, late-success quarantine,
  restart/replay, late side-effect completion and zero duplicate handler executions.
- Application Task schema test: passed against the concrete strict read/write serde parameter
  types, including nested operation items.
- Application-VFS library: 166 passed, including strict parser-derived `mounts_list` schema,
  owner-isolated read dedup and backing-identity patch serialization.
- Infrastructure final catalog: both inventory and exact eight-owner-schema tests passed.
- Owner crates passed `cargo clippy --lib --no-deps -D warnings` after acknowledging the two
  pre-existing VFS lints outside this component (`collapsible_if` and `too_many_arguments`).

This component requires no Agent Service API change, migration or PostgreSQL repository change.
The remaining `agentdash-application/tests/runtime_tool_catalog.rs` all-target compile failure
references interfaces already removed by the combined hard cut and belongs to the final
Product/deletion integration gate.
