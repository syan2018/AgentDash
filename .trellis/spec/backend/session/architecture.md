# Agent Runtime Conversation Architecture

## 1. Scope / Trigger

本规范定义 AgentRun 产品坐标如何映射到 Managed Agent Runtime conversation。新增消息、steer、interrupt、interaction、context read/compact、fork/resume 或 runtime trace 功能时复核。产品 Lifecycle/AgentFrame 仍拥有业务归属与期望 surface；Runtime 独占执行会话事实。

## 2. Signatures

```rust
#[async_trait]
pub trait AgentRunRuntime: Send + Sync {
    async fn inspect(&self, target: AgentRunRuntimeTarget) -> Result<AgentRunRuntimeView, Error>;
    async fn send_message(&self, command: SendAgentRunMessage) -> Result<RuntimeCommandReceipt, Error>;
    async fn compact_context(&self, command: GuardedAgentRunCommand) -> Result<RuntimeCommandReceipt, Error>;
    async fn steer_turn(&self, command: SteerAgentRunTurn) -> Result<RuntimeCommandReceipt, Error>;
    async fn interrupt(&self, command: GuardedAgentRunCommand) -> Result<RuntimeCommandReceipt, Error>;
    async fn resolve_interaction(&self, command: ResolveAgentRunInteraction) -> Result<RuntimeCommandReceipt, Error>;
}
```

```text
AgentRun product command
  -> durable AgentRun mailbox/client command id
  -> AgentRunRuntime facade
  -> Runtime binding/provisioning
  -> canonical Runtime operation + outbox
  -> Integration Driver Host
  -> Driver event
  -> canonical snapshot/event cursor
```

## 3. Contracts

- `AgentRunRuntime` facade 只做 product coordinate、authorization/admission input 与 canonical Runtime command 的映射，不保存 Thread/Turn/Item/Interaction 状态。
- `agent_run_runtime_binding` 是 `run_id + agent_id` 到 Runtime thread/Host binding 的唯一产品锚点。Host binding 与 Managed Runtime binding 由 Host activation 原子创建；产品锚点不复制 driver/source coordinate authority。
- mailbox 保存 canonical accepted Runtime operation ID。client command 重试返回同一 receipt，不产生第二 outbox side effect。
- Managed Runtime journal、snapshot、context head、HookRun/effect、tool call 与 durable cursor 是执行会话唯一事实源。
- AgentFrame 与 Business Surface 提供产品期望；`RuntimeOffer` 提供 service 实际保证；admission 持久化 `BoundAgentSurface`。required contribution 未应用时 dispatch 不可用。
- command availability 来自 canonical Runtime snapshot/profile。Lifecycle status、AgentFrame status、Backbone 或 transcript 只用于产品展示，不能制造执行权限。
- compaction 使用 candidate preparation、driver activation、active-head CAS 与 recovery saga；opaque context 不得进入平台 active head。
- disconnect 对 active binding exactly-once 收敛为 `BindingLost`，并 terminalize active Thread/Turn/Operation 为 `Lost`；旧 generation 晚到事件被 fence。

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| AgentRun 无 durable Runtime binding | provision through Integration offer or return typed unavailable |
| duplicate client command | replay original operation receipt; no duplicate dispatch |
| expected revision/active turn 不匹配 | typed stale rejection before side effect |
| command 不在 availability 中 | typed unsupported/unavailable before outbox |
| required surface revision 未应用 | dispatch unavailable |
| Driver event 生命周期非法 | quarantine + critical Lost convergence |
| stale generation event | fence without cursor advance |
| context activation crash | recovery resumes same compaction operation |

## 5. Good / Base / Bad Cases

- Good：用户消息进入 mailbox，facade provision/复用 binding，Managed Runtime 原子接受 operation/outbox，Driver 完成后 UI 从 snapshot/events 观察同一事实。
- Base：重复提交同一 `client_command_id` 返回原 operation receipt；worker claim 到期后由新 lease token 接管。
- Bad：Application 自己维护 active turn，或从 product status 判断可 interrupt，再直接调用具体 Codex/Native client。

## 6. Tests Required

- Facade 与 mailbox tests：coordinate mapping、idempotency、stale guard、availability、operation receipt。
- PostgreSQL tests：binding、operation/event sequence、outbox/worker lease、context/hook/tool exactly-once。
- Native/Codex production composition tests 与 enterprise remote RuntimeWire E2E。
- API/frontend tests：runtime snapshot/events/context endpoints 与 snapshot-only command availability。
- Migration test：旧 session/delivery tables、columns 与 production readers 全部不存在。

## 7. Wrong vs Correct

```rust
// Wrong
if lifecycle_agent.status.is_running() { connector.cancel(session_id).await?; }

// Correct
let view = agent_run_runtime.inspect(target.clone()).await?;
view.require_available(RuntimeCommandKind::Interrupt)?;
agent_run_runtime.interrupt(command.guarded_by(&view)).await?;
```

## 8. Immutable Session Presentation Contract

### 8.1 Scope / Trigger

本节适用于 connector、Tool Broker、application producer、Runtime journal、AgentRun history/NDJSON 与 `features/session` 之间的会话展示链路。之所以在 producer 边界固定完整 payload，是因为 source ID、event timestamp、显式 `null`、事件顺序与具体 item family 都是会话可观察行为；这些信息一旦被压缩成 Runtime 摘要，读取侧无法可靠恢复。

### 8.2 Signatures

```rust
pub struct ImmutablePresentationEvent {
    pub durability: PresentationDurability,
    pub event: agentdash_agent_protocol::BackboneEvent,
}

pub enum RuntimeJournalFact {
    Presentation(ImmutablePresentationEvent),
    Internal(RuntimeEvent),
}

pub struct RuntimeCarrierMetadata {
    pub thread_id: RuntimeThreadId,
    pub recorded_at_ms: u64,
    pub sequence: Option<EventSequence>,
    pub transient: Option<RuntimeTransientCoordinate>,
    pub coordinate: RuntimePresentationCoordinate,
    // operation/binding/revision/idempotency metadata omitted here
}
```

### 8.3 Contracts

- `ImmutablePresentationEvent.event` 是受保护正文。Codex 标准 family 使用 `0.144.1` 生成的 AgentDash-owned 同构类型，AgentDash extension 使用同一 typed protocol 中的显式扩展 variant；两者都在 producer boundary 一次构造完成。
- Runtime carrier 只拥有 canonical routing、source correlation、sequence、revision、binding、operation 与 durability。持久化、replay、fork projection、GET、NDJSON 和 frontend adapter 可以替换外层会话坐标，但必须逐字段保留正文。
- durable 与 ephemeral presentation 共享同一正文合同；durability 由 producer 显式声明，不从 cursor 或 item kind 推断。`Internal(RuntimeEvent)` 只服务执行状态机，不进入 session presentation stream。
- source thread/turn/item/request ID 与 Runtime canonical ID 同时存在于不同层。carrier correlation 不替换正文中的 source identity，`PresentationThreadId` 则来自产品 delivery session 并贯穿 binding、outbox 与 driver dispatch。
- Journal GET、initial stream、live、reconnect、refresh 与 fork inherited projection消费同一 `Presentation` facts。`features/session` 继续使用同一 reducer/renderer；仅 envelope unwrap seam 理解 carrier。
- 标准 `ThreadNameUpdated` 是 durable presentation，同时由 Runtime reducer投影到
  `RuntimeSnapshot.thread_name`。session stream 保留原事件用于 live UI invalidation，
  snapshot 则提供可重启恢复的当前名称；两条读取路径共享同一 journal fact，因而不会形成
  独立标题事实源。

### 8.4 Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| producer payload不能反序列化为owned protocol | typed protocol error；不生成文本、空对象或诊断消息替代正文 |
| append idempotency identity携带不同events | `IdempotencyConflict`，不覆盖既有记录 |
| durable sequence重复、跳号或乱序 | 整批拒绝，正文和projection都不产生部分写入 |
| ephemeral event晚于同item terminal | 丢弃stale transient；不得复活terminal presentation |
| journal读取到`Internal` fact | 从presentation查询中排除；API不得把它转换成会话事件 |
| GET/stream/fork需要目标会话坐标 | 只改allowlisted carrier/envelope字段，受保护正文保持deep equality |
| optional字段为显式`null` | 按owned protocol保留`null`；不得改成omitted、默认字符串或空数组 |
| live `ThreadNameUpdated` | session正文保真，同时触发读取侧projection invalidation |
| initial hydration包含历史名称事件 | reducer恢复presentation，但不重复执行页面refetch副作用 |

### 8.5 Good / Base / Bad Cases

- Good：Codex/Native/tool/application producer提交完整typed event，Runtime原子保存，Journal重新包装后旧 `features/session` reducer得到与producer相同的event body。
- Good：名称事件的`threadId + threadName(string|null)`在journal、GET与live stream中deep equal；
  Runtime snapshot独立提供当前值，但二者来自同一commit。
- Base：同一idempotency key携带完全相同的有序events时返回原receipt；reconnect从durable/transient cursor继续，不复制或改写正文。
- Bad：producer只保存`role + text + tool_name`摘要，再由API按名称猜回ThreadItem；这会丢失协议variant、ID、timestamp、nullable语义与事件顺序。
- Bad：前端用live payload直接patch一个标题缓存、同时list/workspace继续查询后端；这会产生两个优先级实现。

### 8.6 Tests Required

- contract tests逐字段覆盖serialize/deserialize/reducer/snapshot roundtrip，并包含source IDs、timestamp、显式`null`与数组顺序。
- memory/PostgreSQL tests覆盖ordered batch、idempotency conflict、durable/transient race、terminal fencing与commit→read→replay deep equality。
- pinned Main parity tests对同一输入分别执行Main/current production path，只允许声明过的eventstream envelope差异；正文comparator不配置字段ignore list。
- Journal parity覆盖GET、initial/live、reconnect/refresh、fork inherited、heartbeat、lagged与closed；frontend parity覆盖原reducer/renderer和无phantom tool card。
- Thread name parity覆盖set/replace/clear的protected body deep equality，并断言live事件触发refetch、
  hydration历史事件不触发refetch、refetch结果应用后端统一display-title resolver。

### 8.7 Wrong vs Correct

```rust
// Wrong: read side reconstructs a protocol event from a lossy Runtime summary.
let event = project_summary_as_thread_item(record.summary)?;

// Correct: producer-owned body is persisted once; read side only wraps it.
let RuntimeJournalFact::Presentation(presentation) = record.fact() else { return None };
let envelope = wrap_for_target(record.carrier(), presentation.event.clone());
```

```ts
// Wrong: session reducer直接维护产品标题优先级。
workspace.title = event.payload.threadName ?? "新会话";

// Correct: live事件只invalidate；产品查询返回统一组合后的display title。
if (isLive && event.type === "thread_name_updated") {
  invalidateAgentRunWorkspaceAndList();
}
```
