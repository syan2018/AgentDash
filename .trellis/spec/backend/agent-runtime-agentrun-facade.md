# AgentRun Runtime Facade and Product Cutover

## 1. Scope / Trigger

本规范适用于 AgentRun 产品命令进入 Managed Agent Runtime、Runtime 状态读模型暴露给 API/UI、Project AgentRun 列表/详情产品投影，以及 Runtime service 的生产装配。新增 AgentRun 命令、Runtime 查询、列表字段、Integration service、数据库 binding 字段或前端命令按钮时必须复核本规范。

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

```rust
pub struct AgentRunProductQueryInput<'a> {
    pub run: &'a LifecycleRun,
    pub agent: &'a LifecycleAgent,
    pub has_runtime_binding: bool,
    pub runtime_projection: &'a dyn VfsSurfaceRuntimeProjection,
}

pub async fn AgentRunProductQuery::get(
    &self,
    input: AgentRunProductQueryInput<'_>,
) -> Result<AgentRunProductModel, ApplicationError>;
```

```rust
pub async fn ProjectAgentRunListQuery::list(
    &self,
    input: ProjectAgentRunListInput<'_>,
) -> Result<ProjectAgentRunListPage, ApplicationError>;
```

```text
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/context
GET  /agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson
GET  /projects/{project_id}/agent-runs?limit={limit}&cursor={cursor}
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/{compact|cancel|steer}
POST /agent-runs/{run_id}/agents/{agent_id}/runtime/interactions/{interaction_id}/respond
```

Migration `0065_agent_runtime_cutover.sql` removes the superseded runtime-session tables and product execution columns. `agent_run_agent_runtime_binding.runtime_thread_id` is unique but intentionally has no thread foreign key because product binding is persisted before `ThreadStart`; `runtime_binding_id` references the Host binding that already exists.

Migration `0066_agent_frame_hook_plan.sql` adds nullable `agent_frames.hook_plan jsonb`. Existing rows remain explicitly unmaterialized；所有生产Frame writer必须在新的revision中写入可校验的`AgentFrameHookPlan { revision, digest, requirements[] }`，Runtime surface adoption不得从permission policy、executor kind或Driver profile补造计划。

Migration `0077_agent_run_product_command_receipts.sql` restores product-command idempotency after the RuntimeSession cutover as a new product-owned table. It references canonical `runtime_thread_id + runtime_operation_id` and does not restore retired RuntimeSession/AgentRunTurn/ProtocolTurn columns.

Migration `0078_rebind_safe_runtime_outbox_coordinates.sql` persists each Runtime outbox entry's immutable `binding_id + binding_epoch + driver_generation`. Historical dispatch fences reference `agent_runtime_binding`, while `thread_id` independently retains thread ownership, so `ThreadRebind` can advance the thread without rewriting old outbox coordinates.

## 3. Contracts

- Application depends on the named `AgentRunRuntime` facade and owned Runtime contract. The facade maps product coordinates and commands; it does not own Thread/Turn/Item/Interaction state.
- AgentRun详情产品投影由具名`AgentRunProductQuery`组合Lifecycle read model、current AgentFrame/model config与VFS surface；API route只负责鉴权、runtime projection adapter和generated DTO映射，不直接编排repository。
- Project AgentRun列表由具名`ProjectAgentRunListQuery`组合LifecycleRun/LifecycleAgent、ProjectAgent identity、subject association、canonical `AgentLineage` forest与可选Managed Runtime summary。API route只负责Project Use鉴权、分页输入和generated DTO映射。
- 列表wire只承载当前consumer读取的title、Lifecycle status、activity time、subject、lineage children与`thread_status + active_turn_id`；不恢复旧`AgentRunWorkspaceShell.delivery_status`，不复制frame/run status，也不从列表状态生成Runtime command availability。
- `ManagedAgentRuntime` is the only writer of canonical operation, lifecycle event, snapshot, context head and compaction state. Product receipts store the accepted Runtime operation ID without encoding it as a protocol Turn ID.
- Production composition is built below the API boundary: Business Surface compiles product facts, Driver Host resolves an Integration `RuntimeOffer`, admission persists the bound surface and Host binding, and the facade persists the AgentRun-to-Runtime binding.
- Agent services enter through trusted Integration contributions. Native, Codex and enterprise remote services have the same definition/instance/offer/factory/binding lifecycle; Relay contributes placement transport only.
- `ThreadStart` carries immutable surface settings, tool-set revision and bound Hook plan. It is replayed with identical coordinates after a durable duplicate/retry.
- `AgentFrameHookPlan.requirements[].site`决定execution route。完整bound plan保留Managed Runtime、Tool Broker、Agent Core Callback与Driver Native entries；`DriverHookSurface`只投影Driver实际执行的site，Tool Broker approval不会成为Driver offer requirement。
- API/UI command availability comes only from the canonical Runtime view. A product-level status, connector kind or executor kind cannot enable a command.
- Runtime events use a durable cursor. Authoritative lifecycle events are not reconstructed from Backbone or transient broadcast state.
- Remote Driver events are ordered within a RuntimeWire stream. Response correlation and reverse HostPort calls may be concurrent, but canonical lifecycle notifications are emitted serially.
- A disconnected active binding converges exactly once to `BindingLost`; Thread, active Turn and accepted Operation become `Lost`. Re-registration creates a new generation, and late events from the old generation cannot revive canonical state.
- Restart recovery rebuilds non-serializable callable Tool/Hook handles against the exact durable surface coordinates. Durable context/workspace/presentation remain authoritative and are not replaced by startup-time discovery.
- An InProcess service with a lost binding reactivates its durable service instance into a new Host generation before `ThreadRebind`; Remote/LocalProcess recovery still requires a real advertised replacement offer.
- Runtime outbox rows retain the binding epoch/generation accepted with the command. Rebinding the thread never cascades or rewrites those historical fencing coordinates.
- Runtime Turn/Item/Interaction projections update stable entity rows in place. A normal Runtime UoW must not delete and recreate entity rows because ToolBroker calls, interactions and other durable side-effect records reference those identities across concurrent driver commits.
- Context compaction is a Managed Runtime operation: candidate preparation, remote activation, active-head convergence and recovery share one operation identity.

## 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| AgentRun has no durable Runtime binding | typed unavailable/not-found; no driver side effect |
| 列表项没有Runtime binding | `runtime=None`；Lifecycle产品事实仍可列出 |
| 列表项Runtime inspect失败 | 整个查询显式失败并携带run/agent坐标；不得静默伪装为idle |
| lineage包含环、自环或悬空parent | 全局visited forest忽略无效边，并确保每个LifecycleAgent恰好投影一次；不得吞行、无限展开或重复制造子树 |
| command absent from `command_availability` | typed rejected before mailbox/outbox dispatch |
| duplicate client command | original receipt returned; no second outbox delivery |
| Runtime accepts command but product-coordinate persistence fails | binding marked failed and canonical `BindingLost` emitted |
| Driver event is out of lifecycle order | critical protocol violation and `Lost` convergence |
| stale binding generation emits an event | event fenced; snapshot and terminal state unchanged |
| process restarts before callable Tool/Hook registry is rebuilt | recover executable handles, verify persisted Tool/Hook surface, then dispatch against the durable binding |
| lost InProcess binding has no newer offer | activate the durable owner at a new generation, fence the old offer, then Resume/Rebind |
| ThreadRebind encounters historical outbox rows | commit succeeds; old binding/epoch/generation values remain unchanged |
| concurrent driver facts advance a thread while ToolBroker owns an active Item | entity projection is upserted in place; the broker row survives and the Item reaches its durable terminal |
| active remote transport disconnects | exactly one `BindingLost`; active Thread/Turn/Operation become `Lost` |
| required surface/tool/hook revision is not applied | dispatch unavailable; no prompt-only degradation |
| current AgentFrame没有immutable HookPlan或digest复验失败 | frame/runtime materialization typed reject；不读取旧surface或permission policy补齐 |
| Tool Broker Hook requirement被投影到Driver admission | contract test失败；Driver surface必须只包含Driver execution sites |
| migration runs on an empty database | new schema created, old tables/columns absent |
| migration readiness observes old execution state | readiness fails with explicit diagnostic |

## 5. Good / Base / Bad Cases

**Good:** composer input is durably accepted by the AgentRun mailbox, provisioned through a generic Runtime offer, dispatched over RuntimeWire to a Local Host, and observed through canonical snapshot/events. Compaction and interaction resolution reuse the same facade and operation journal. Project列表由application query返回最小product DTO，活跃线程只有存在`active_turn_id`时才展示为执行中。

**Base:** retrying the same client command returns a duplicate receipt; reconnecting a healthy service creates a new placement generation without replaying an operation already terminalized as `Lost`.

**Bad:** enabling submit/cancel from a top-level product status, branching on `Pi`/`Codex` in API composition, injecting a terminal Backbone event into the canonical snapshot，或复用退役workspace shell为列表制造`delivery_status`，都会创建第二事实源。

## 6. Tests Required

- Facade tests assert product coordinate mapping, duplicate receipt replay, operation IDs and canonical command availability.
- Native and Codex production composition tests traverse facade/mailbox -> Host/outbox -> driver -> canonical events/snapshot.
- Enterprise remote E2E traverses Cloud generic proxy -> RuntimeWire -> Local PostgreSQL Host -> enterprise Native driver -> production Tool Broker and reverse Hook/HostPort calls; the tool call must survive concurrent Runtime projection commits and reach canonical Item/Turn terminal state.
- Enterprise remote E2E asserts compaction preparation/activation/recovery, disconnect, exactly-once `BindingLost`, reopen, late-event fencing and no duplicate outbox replay.
- Native production tests cover process-restart callable registry recovery, InProcess generation reactivation and a real `ThreadRebind` with historical outbox rows.
- Migration tests run `0065` against a fresh PostgreSQL root and assert removed tables/columns are absent; bootstrap tests must use isolated data roots.
- Migration/Frame repository tests断言`0066`使用`agent_frames.hook_plan jsonb`，新建、generic launch、workflow node与runtime surface revision writer都保存并复验相同digest。
- Contract generation, frontend typecheck/tests, workspace checks, Runtime crate tests and migration guard are required before completion.
- Product query tests覆盖current Frame execution profile到`model_config`的resolved/model-required投影；前端state测试覆盖workspace/runtime两路独立失败与refresh保留。
- Project列表测试覆盖run activity keyset cursor、canonical lineage递归/cycle/orphan下每个Agent恰好一次、无binding与Runtime summary；service/UI测试覆盖URL encoding及`thread_status + active_turn_id + lifecycle_status`展示映射。

## 7. Wrong vs Correct

```rust
// Wrong: product status manufactures execution authority.
let can_cancel = agent_run.status == AgentRunStatus::Running;

// Correct: the canonical Runtime snapshot owns command admission.
let can_cancel = runtime_view.command_availability.supports(RuntimeCommandKind::Interrupt);
```

```rust
// Wrong: API handler直接读取Lifecycle/Frame repository并拼装产品投影。
let frame = state.repos.agent_frame_repo.get_current(agent_id).await?;

// Correct: API鉴权后调用具名application query，再映射generated contract。
let product = state.services.agent_run_product_query.get(input).await?;
```

```rust
// Wrong: route直接拼Lifecycle、lineage和Runtime状态，或复用旧workspace list query。
let entries = build_agent_run_list_from_repositories(&state.repos).await?;

// Correct: route完成鉴权后调用具名application query。
let page = state.services.project_agent_run_list_query.list(input).await?;
```

```rust
// Wrong: every incoming DriverEvent is spawned independently.
tokio::spawn(async move { sink.emit(event).await });

// Correct: lifecycle events are drained in stream order; only responses and HostPort calls use reentrant paths.
driver_event_queue.send(event).await?;
```
