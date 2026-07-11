# AgentRun Runtime Facade and Product Cutover

## 1. Scope / Trigger

本规范适用于 AgentRun 产品命令进入 Managed Agent Runtime、Runtime 状态读模型暴露给 API/UI，以及 Runtime service 的生产装配。新增 AgentRun 命令、Runtime 查询、Integration service、数据库 binding 字段或前端命令按钮时必须复核本规范。

## 2. Signatures

```rust
#[async_trait]
pub trait AgentRunRuntime: Send + Sync {
    async fn inspect(&self, target: AgentRunRuntimeTarget) -> Result<AgentRunRuntimeView, Error>;
    async fn send_message(&self, command: SendAgentRunMessage) -> Result<OperationReceipt, Error>;
    async fn compact_context(&self, command: GuardedAgentRunCommand) -> Result<OperationReceipt, Error>;
    async fn steer_active_turn(&self, command: SteerAgentRunTurn) -> Result<OperationReceipt, Error>;
    async fn interrupt_active_turn(&self, command: GuardedAgentRunCommand) -> Result<OperationReceipt, Error>;
    async fn resolve_interaction(&self, command: ResolveAgentRunInteraction) -> Result<OperationReceipt, Error>;
    async fn read_context(&self, target: AgentRunRuntimeTarget) -> Result<RuntimeContextView, Error>;
    async fn read_events(&self, query: ReadAgentRunEvents) -> Result<Box<dyn RuntimeEventStream>, Error>;
}
```

```text
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/{compact|cancel|steer}
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/interactions/{interaction_id}/{approve|reject}
```

Migration `0065_agent_runtime_cutover.sql` removes the superseded runtime-session tables and product execution columns. `agent_run_agent_runtime_binding.runtime_thread_id` is unique but intentionally has no thread foreign key because product binding is persisted before `ThreadStart`; `runtime_binding_id` references the Host binding that already exists.

## 3. Contracts

- Application depends on the named `AgentRunRuntime` facade and owned Runtime contract. The facade maps product coordinates and commands; it does not own Thread/Turn/Item/Interaction state.
- `ManagedAgentRuntime` is the only writer of canonical operation, lifecycle event, snapshot, context head and compaction state. Product receipts store the accepted Runtime operation ID without encoding it as a protocol Turn ID.
- Production composition is built below the API boundary: Business Surface compiles product facts, Driver Host resolves an Integration `RuntimeOffer`, admission persists the bound surface and Host binding, and the facade persists the AgentRun-to-Runtime binding.
- Agent services enter through trusted Integration contributions. Native, Codex and enterprise remote services have the same definition/instance/offer/factory/binding lifecycle; Relay contributes placement transport only.
- `ThreadStart` carries immutable surface settings, tool-set revision and bound Hook plan. It is replayed with identical coordinates after a durable duplicate/retry.
- API/UI command availability comes only from the canonical Runtime view. A product-level status, connector kind or executor kind cannot enable a command.
- Runtime events use a durable cursor. Authoritative lifecycle events are not reconstructed from Backbone or transient broadcast state.
- Remote Driver events are ordered within a RuntimeWire stream. Response correlation and reverse HostPort calls may be concurrent, but canonical lifecycle notifications are emitted serially.
- A disconnected active binding converges exactly once to `BindingLost`; Thread, active Turn and accepted Operation become `Lost`. Re-registration creates a new generation, and late events from the old generation cannot revive canonical state.
- Context compaction is a Managed Runtime operation: candidate preparation, remote activation, active-head convergence and recovery share one operation identity.

## 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| AgentRun has no durable Runtime binding | typed unavailable/not-found; no driver side effect |
| command absent from `command_availability` | typed rejected before mailbox/outbox dispatch |
| duplicate client command | original receipt returned; no second outbox delivery |
| Runtime accepts command but product-coordinate persistence fails | binding marked failed and canonical `BindingLost` emitted |
| Driver event is out of lifecycle order | critical protocol violation and `Lost` convergence |
| stale binding generation emits an event | event fenced; snapshot and terminal state unchanged |
| active remote transport disconnects | exactly one `BindingLost`; active Thread/Turn/Operation become `Lost` |
| required surface/tool/hook revision is not applied | dispatch unavailable; no prompt-only degradation |
| migration runs on an empty database | new schema created, old tables/columns absent |
| migration readiness observes old execution state | readiness fails with explicit diagnostic |

## 5. Good / Base / Bad Cases

**Good:** composer input is durably accepted by the AgentRun mailbox, provisioned through a generic Runtime offer, dispatched over RuntimeWire to a Local Host, and observed through canonical snapshot/events. Compaction and interaction resolution reuse the same facade and operation journal.

**Base:** retrying the same client command returns a duplicate receipt; reconnecting a healthy service creates a new placement generation without replaying an operation already terminalized as `Lost`.

**Bad:** enabling submit/cancel from a top-level product status, branching on `Pi`/`Codex` in API composition, or injecting a terminal Backbone event into the canonical snapshot creates a second authority and is invalid.

## 6. Tests Required

- Facade tests assert product coordinate mapping, duplicate receipt replay, operation IDs and canonical command availability.
- Native and Codex production composition tests traverse facade/mailbox -> Host/outbox -> driver -> canonical events/snapshot.
- Enterprise remote E2E traverses Cloud generic proxy -> RuntimeWire -> Local PostgreSQL Host -> enterprise Native driver -> production Tool Broker and reverse Hook/HostPort calls.
- Enterprise remote E2E asserts compaction preparation/activation/recovery, disconnect, exactly-once `BindingLost`, reopen, late-event fencing and no duplicate outbox replay.
- Migration tests run `0065` against a fresh PostgreSQL root and assert removed tables/columns are absent; bootstrap tests must use isolated data roots.
- Contract generation, frontend typecheck/tests, workspace checks, Runtime crate tests and migration guard are required before completion.

## 7. Wrong vs Correct

```rust
// Wrong: product status manufactures execution authority.
let can_cancel = agent_run.status == AgentRunStatus::Running;

// Correct: the canonical Runtime snapshot owns command admission.
let can_cancel = runtime_view.command_availability.supports(RuntimeCommandKind::Interrupt);
```

```rust
// Wrong: every incoming DriverEvent is spawned independently.
tokio::spawn(async move { sink.emit(event).await });

// Correct: lifecycle events are drained in stream order; only responses and HostPort calls use reentrant paths.
driver_event_queue.send(event).await?;
```
