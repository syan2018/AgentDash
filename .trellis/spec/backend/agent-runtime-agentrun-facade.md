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

```rust
pub struct AgentRunPresentationDraft {
    pub content: Vec<UserInputBlock>,
    pub source: UserInputSource,
    pub launch_source: LaunchPresentationSource,
    pub submission_kind: UserInputSubmissionKind,
}

pub trait AgentRunMessageSubmissionStore {
    async fn accept_message(
        &self,
        command: AcceptAgentRunMessageSubmission,
    ) -> Result<AcceptAgentRunMessageSubmissionResult, DomainError>;

    async fn complete_submission(
        &self,
        completion: CompleteAgentRunMessageSubmission,
    ) -> Result<AgentRunCommandReceipt, DomainError>;
}

pub async fn RuntimeAgentRunMailbox::promote(
    &self,
    target: &AgentRunRuntimeTarget,
    message_id: Uuid,
) -> Result<AgentRunMailboxMessage, RuntimeMailboxError>;

pub trait AgentRunMailboxRepository {
    async fn promote_message(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        id: Uuid,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError>;
}
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

Migration `0079_reconcile_agent_run_mailbox_delivery.sql` removes the retired AgentRun/protocol turn reference columns and executor/backend second facts from mailbox rows, adds the typed lease-reconciliation fact and stable delivery digest, and links each product receipt to at most one exact mailbox message.

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
- Runtime events use a durable cursor。Session GET、live、replay与fork cutoff都使用`inherited prefix length + raw Runtime EventSequence`；过滤internal-only facts后留下的cursor空洞合法，不能重新enumerate为稠密序列。Authoritative lifecycle events are not reconstructed from Backbone or transient broadcast state.
- Remote Driver events are ordered within a RuntimeWire stream. Response correlation and reverse HostPort calls may be concurrent, but canonical lifecycle notifications are emitted serially.
- A disconnected active binding converges exactly once to `BindingLost`; Thread, active Turn and accepted Operation become `Lost`. Re-registration creates a new generation, and late events from the old generation cannot revive canonical state.
- Restart recovery rebuilds non-serializable callable Tool/Hook handles against the exact durable surface coordinates，并把active context checkpoint覆盖到materialized driver surface、descriptor、tool/hook/workspace与provider transcript。Durable context/workspace/presentation remain authoritative and are not replaced by startup-time discovery.
- command-owned presentation record持久化owning operation ID；provider transcript按operation排除当前TurnStart输入，使同一user prompt在initial dispatch与rebind后都只出现一次。
- An InProcess service with a lost binding reactivates its durable service instance into a new Host generation before `ThreadRebind`; Remote/LocalProcess recovery still requires a real advertised replacement offer.
- Runtime outbox rows retain the binding epoch/generation accepted with the command. Rebinding the thread never cascades or rewrites those historical fencing coordinates.
- Runtime Turn/Item/Interaction projections update stable entity rows in place. A normal Runtime UoW must not delete and recreate entity rows because ToolBroker calls, interactions and other durable side-effect records reference those identities across concurrent driver commits.
- Context compaction is a Managed Runtime operation: candidate preparation, remote activation, active-head convergence and recovery share one operation identity.
- Mailbox persistence owns delivery intent, barrier, priority, claim lease and an immutable typed input draft. The draft records authored content/source semantics but contains no admission timestamp, canonical `PresentationTurnId`, `PresentationItemId`, active Runtime turn or expected Runtime revision.
- Runtime facade admission is the presentation identity owner. A new launch generates its `t<millis>` / `:user-input:0` identity and start time at actual admission; steer admission binds the current active presentation turn and creates the historical mailbox item shape with a fresh dynamic suffix. Accepted duplicate/recovery reads the durable Runtime Operation instead of regenerating presentation.
- Promote is a typed mailbox policy transition through the application owner. The repository performs owner, origin, delivery and claim-state validation in one conditional update; API routes do not assemble mailbox persistence fields.
- `AgentRunMessageSubmissionService` owns product request digest, exact response replay and current-message result association. Its PostgreSQL UoW atomically creates the pending product receipt, identity-free mailbox row and receipt-to-message link; a queue scheduler may advance another row but cannot complete the current product receipt with that row's Operation.
- Product request/delivery digest只覆盖稳定业务语义，不包含mailbox UUID、Runtime operation/presentation动态identity、claim/snapshot guard或`AuthIdentity`展示字段。相同client command在用户展示资料刷新、Runtime状态变化和网络重试后仍命中同一receipt/message；真正改变输入、target、actor、execution profile、backend或delivery intent时返回typed conflict。
- `ProjectAgentRunStartService`拥有ProjectAgent解析、Lifecycle graph launch、产品结果projection与失败清理，但只能向Message Submission提交产品级initial-message intent。Mailbox origin/source/delivery/barrier、草稿编译、message identity和receipt-message attach判断属于Submission owner；attach失败必须返回typed `Unattached | Attached | Unknown`，只有确定`Unattached`且graph尚无可见execution event时才能删除整份draft run/agent/frame并固化失败receipt。
- ProjectAgent initial input只有一份canonical `Vec<UserInputBlock>`；Message Submission owner从该输入同时生成presentation draft与Runtime `UserInput`，并从唯一product projector生成frozen Started/Steered/Failed结果，避免双数据源分叉。
- Delivery coordination owns fresh availability inspection and mailbox claim. It calls one Runtime `accept_message` ingress; the ingress first reconciles a stable existing Operation and otherwise chooses start/steer from one canonical snapshot.
- Expired consuming leases become typed reconciliation work. Reconciliation bypasses ordinary availability only to ask Runtime admission whether the stable Operation already exists; if not, the draft returns to ordinary policy planning. Repository recovery never guesses whether a Runtime side effect happened.
- User visible recall payload uses protocol `UserInputBlock` shape. Hidden Runtime input/delivery command remains separate and is atomically removed with user payload after successful acceptance when retention is disabled.

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
| visible journal夹有internal-only sequence gap | live cursor、GET/replay与fork cutoff仍使用相同raw sequence，不丢紧随gap的tool terminal/ContextFrame |
| recovery时active head新于启动surface | callable registry与driver bind统一使用active checkpoint版本，不混用旧tool/hook/context |
| lost InProcess binding has no newer offer | activate the durable owner at a new generation, fence the old offer, then Resume/Rebind |
| ThreadRebind encounters historical outbox rows | commit succeeds; old binding/epoch/generation values remain unchanged |
| concurrent driver facts advance a thread while ToolBroker owns an active Item | entity projection is upserted in place; the broker row survives and the Item reaches its durable terminal |
| active remote transport disconnects | exactly one `BindingLost`; active Thread/Turn/Operation become `Lost` |
| required surface/tool/hook revision is not applied | dispatch unavailable; no prompt-only degradation |
| current AgentFrame没有immutable HookPlan或digest复验失败 | frame/runtime materialization typed reject；不读取旧surface或permission policy补齐 |
| Tool Broker Hook requirement被投影到Driver admission | contract test失败；Driver surface必须只包含Driver execution sites |
| migration runs on an empty database | new schema created, old tables/columns absent |
| migration readiness observes old execution state | readiness fails with explicit diagnostic |
| stored mailbox payload contains canonical turn/item identity | contract failure; persistence accepts only the typed draft and product target |
| Promote target has wrong run/agent owner | typed not-found; no row changes |
| Promote target is non-user, terminal, consuming or delivery-result-unknown | typed conflict; claim/policy remains unchanged |
| active turn terminates between mailbox inspect and command admission | claim returns to queued planning state; next attempt launches from the same draft |
| accepted client command is replayed with different draft/input/actor | typed client-command conflict; original operation remains authoritative |
| duplicate product client ID has a different canonical request digest | typed request-digest conflict; no second receipt/message/operation |
| scheduler dispatches another mailbox row while handling submit | current submit result remains queued; the other row's receipt is not attached |
| Runtime accepts but mailbox settlement crashes | lease recovery claims reconciliation work; facade returns the durable Operation receipt without a second presentation or side effect |
| queued user message is recalled | return the original `UserInputBlock[]`; RuntimeInput remains private |
| queued message waits before launch | visible start time and `t<millis>` come from admission, not enqueue time |

## 5. Good / Base / Bad Cases

**Good:** composer input is durably accepted by the AgentRun mailbox as an identity-free draft, provisioned through a generic Runtime offer, admitted by the facade with canonical presentation identity, dispatched over RuntimeWire to a Local Host, and observed through canonical snapshot/events. Compaction and interaction resolution reuse the same facade and operation journal. Project列表由application query返回最小product DTO，活跃线程只有存在`active_turn_id`时才展示为执行中。

**Base:** retrying the same client command returns its first persisted product result; a promoted user draft steers when the turn is still active and starts the next turn when terminal convergence wins before Runtime acceptance; reconnecting a healthy service creates a new placement generation without replaying an operation already terminalized as `Lost`.

**Bad:** enabling submit/cancel from a top-level product status, branching on `Pi`/`Codex` in API composition, injecting a terminal Backbone event into the canonical snapshot，由API/Mailbox预生成active presentation坐标，或复用退役workspace shell为列表制造`delivery_status`，都会创建第二事实源。

## 6. Tests Required

- Facade tests assert product coordinate mapping, duplicate receipt replay, operation IDs and canonical command availability.
- Native and Codex production composition tests traverse facade/mailbox -> Host/outbox -> driver -> canonical events/snapshot.
- Enterprise remote E2E traverses Cloud generic proxy -> RuntimeWire -> Local PostgreSQL Host -> enterprise Native driver -> production Tool Broker and reverse Hook/HostPort calls; the tool call must survive concurrent Runtime projection commits and reach canonical Item/Turn terminal state.
- Enterprise remote E2E asserts compaction preparation/activation/recovery, disconnect, exactly-once `BindingLost`, reopen, late-event fencing and no duplicate outbox replay.
- Native production tests cover process-restart callable registry recovery, InProcess generation reactivation and a real `ThreadRebind` with historical outbox rows.
- Journal tests覆盖含internal gap的live→replay、nested fork cutoff与继承prefix重编号；provider request测试断言当前user prompt exact-once。
- Migration tests run `0065` against a fresh PostgreSQL root and assert removed tables/columns are absent; bootstrap tests must use isolated data roots.
- Migration/Frame repository tests断言`0066`使用`agent_frames.hook_plan jsonb`，新建、generic launch、workflow node与runtime surface revision writer都保存并复验相同digest。
- Contract generation, frontend typecheck/tests, workspace checks, Runtime crate tests and migration guard are required before completion.
- Product query tests覆盖current Frame execution profile到`model_config`的resolved/model-required投影；前端state测试覆盖workspace/runtime两路独立失败与refresh保留。
- Project列表测试覆盖run activity keyset cursor、canonical lineage递归/cycle/orphan下每个Agent恰好一次、无binding与Runtime summary；service/UI测试覆盖URL encoding及`thread_status + active_turn_id + lifecycle_status`展示映射。
- Mailbox tests serialize the stored command and assert that no presentation turn/item identity is present; active Promote must steer, terminal-before-claim must launch the next turn, and stale admission must requeue the unchanged draft then replan successfully.
- PostgreSQL tests assert Promote's conditional update cannot take a consuming claim, non-user row, terminal row, unknown delivery result or another AgentRun's message.
- Facade parity fixtures compare launch/steer event payload families, content, source, submission kind and historical ID shape with `main-reference`, normalizing only dynamic identities.
- Submission UoW tests cover zero partial state on receipt/mailbox failure, same-digest concurrent acceptance, different-digest conflict, queued exact replay and conditional completion that cannot overwrite an accepted result.
- Recovery tests cover accepted-before-mailbox-settlement for started and steered Operations, including terminal/Lost thread state, and assert one Runtime Operation/presentation sequence.
- Recall/retention tests assert queued text/image payload round-trips in protocol shape and successful user acceptance clears visible and hidden drafts atomically while retained system/workflow payload remains.

## 7. Wrong vs Correct

```rust
// Wrong: product status manufactures execution authority.
let can_cancel = agent_run.status == AgentRunStatus::Running;

// Correct: the canonical Runtime snapshot owns command admission.
let can_cancel = runtime_view.command_availability.supports(RuntimeCommandKind::Interrupt);
```

```rust
// Wrong: queue policy freezes an identity that may be stale before delivery.
stored.presentation_turn_id = inspected.active_presentation_turn_id;

// Correct: the queue retains an identity- and time-free draft; facade admission binds identity.
let draft = stored.presentation;
runtime.accept_message(AcceptAgentRunMessage { presentation: draft, .. }).await?;
```

```rust
// Wrong: API handler直接读取Lifecycle/Frame repository并拼装产品投影。
let frame = state.repos.agent_frame_repo.get_latest(agent_id).await?;

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
