# Design Option · Minimal Durable Agent Runtime Interface

> 本文是 DESIGN-IT-TWICE 中“最小化 interface、最大化单入口 leverage”的独立方案。
>
> 方案名：**Durable Operation Journal Runtime**（持久化操作日志式 Agent Runtime）。
>
> 目标不是复刻当前 service/port，而是让 application / AgentRun 只学习三个入口，把 session、turn、item、context、compaction、driver、协议和并发状态机全部隐藏在一个深 module 后面。

## 1. 强结论

建议 application 面向的 Agent Runtime interface 只有三个方法：

```rust
#[async_trait]
pub trait ManagedAgentRuntime: Send + Sync {
    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<OperationReceipt, ExecuteError>;

    async fn snapshot(
        &self,
        query: AgentSnapshotQuery,
    ) -> Result<AgentSessionSnapshot, SnapshotError>;

    async fn events(
        &self,
        subscription: AgentEventSubscription,
    ) -> Result<AgentEventStream, SubscribeError>;
}
```

interface 的核心语义：

1. `execute` 只表示“命令已被 durable 接收”，永远不把 driver 执行速度伪装成同步结果；
2. operation 的真实成功/失败只通过 ordered durable event 与 snapshot 表达；
3. `snapshot` 是带 revision、context fidelity 和 binding capability 的一致性快照，不是 application 自行拼接 read model；
4. `events` 使用 durable cursor，保证 backlog/live 无缝衔接；token delta 等 transient event 明确允许丢失，任何最终状态都有 durable completed event；
5. application 不直接获得 connector、repository、projection head、native session id、hook delegate、compaction request 或 protocol JSON。

这是一个刻意异步、操作日志式的设计。它放弃“一个方法调用直接返回 turn/compaction 最终结果”的便利，换取一个统一的 durable success boundary。

## 2. 为什么这个 seam 值得存在

### Module

`Managed Agent Runtime` 是一个业务 module，拥有一个受管理 Agent session 从创建到关闭的全部行为及不变量。

### Interface

application 只学习 `execute / snapshot / events`，以及强类型 command/event/snapshot。interface 包含明确的 ordering、幂等、错误与 fidelity 语义。

### Seam

seam 位于 application / AgentRun 与业务 Agent runtime 之间。Application 的工作到“表达用户意图并绑定产品身份”为止；从 session command decision 开始都在 seam 另一侧。

### Adapter

- Application adapter：把 AgentRun HTTP/use case 转为 `AgentCommandEnvelope`；
- Native Core driver adapter：把统一 driver command 转为 Agent Core 调用；
- Codex App Server driver adapter：把统一 driver command/event 映射为 Codex wire protocol；
- Remote Runtime driver adapter：把统一 driver contract 映射为企业远端 Agent wire protocol；
- Relay placement adapter：透明承载统一 wire protocol 与远端 placement，不提供 Agent service 语义；
- PostgreSQL store adapter、secret-vault adapter、in-memory test adapter。

### Depth / leverage / locality

- **Depth：** 三个入口隐藏 session actor、command availability、driver capability、context projection、compaction saga、outbox、approval、recovery 与 protocol mapping。
- **Leverage：** AgentRun、自动 resume、定时任务、API、后台运维和测试都用同一 execute/snapshot/events 契约。
- **Locality：** 新增一种 session state、compaction policy、driver guarantee 或 crash recovery 规则，只修改 runtime module；application 不再同步修改 snapshot、command service、API handler 和 executor mapper。

删除这个 module 后，其复杂度会重新散落到每个 caller，符合 deep module 的 deletion test。

## 3. 当前 interface 为何不适合作为基础

当前 `AgentConnector` interface 同时暴露 connector type、boolean capability、executor discovery、live session 查询、prompt、cancel、steer、approval、tool update、notification 等方法：`crates/agentdash-spi/src/connector/mod.rs:978-1087`。

其中：

- `ConnectorType` 硬编码为 `LocalExecutor / RemoteAcpBackend`：同文件 `:30-38`；
- capability 是一组松散 bool：`:40-50`；
- repository restore 是额外 boolean 特判：`:986-992`；
- 多个 capability 使用默认 false、默认 no-op 或运行时错误：`:1011-1085`。

这是一组浅方法，而不是严格 runtime contract：caller 必须组合多个 bool、live state 和默认行为才能推断真实保证。新的 seam 不继承 connector enum，也不允许默认 no-op 假装命令成功。

## 4. 强类型 identity 与 binding

### 4.1 公共 ID

所有身份使用不可互换的 newtype，不再用裸 `String`：

```rust
pub struct AgentSessionId(Uuid);
pub struct AgentTurnId(Uuid);
pub struct AgentItemId(Uuid);
pub struct AgentOperationId(Uuid);
pub struct AgentCommandId(Uuid);
pub struct AgentEventSeq(u64);
pub struct SessionRevision(u64);
pub struct ContextRevision(u64);
pub struct ItemRevision(u64);

pub struct IntegrationDefinitionId(Uuid);
pub struct IntegrationDefinitionRevision(u64);
pub struct IntegrationInstanceId(Uuid);
pub struct IntegrationInstanceRevision(u64);
pub struct RuntimeBindingId(Uuid);
pub struct RuntimeBindingGeneration(u64);
pub struct RuntimePlacementId(Uuid);
pub struct DriverKey(String);
pub struct DriverVersion(String);
pub struct RuntimeDescriptorDigest(String);
pub struct TransportDescriptorDigest(String);
pub struct ContextCheckpointId(Uuid);
pub struct ContextActivationId(Uuid);
```

### 4.2 两类 binding 必须分开

Application 自己拥有产品绑定：

```rust
pub struct AgentRunRuntimeBinding {
    pub run_id: LifecycleRunId,
    pub lifecycle_agent_id: LifecycleAgentId,
    pub session_id: AgentSessionId,
}
```

Managed Agent Runtime 内部拥有 driver binding：

```rust
struct DriverSessionBinding {
    binding_id: RuntimeBindingId,
    generation: RuntimeBindingGeneration,
    session_id: AgentSessionId,
    integration_instance_id: IntegrationInstanceId,
    integration_instance_revision: IntegrationInstanceRevision,
    driver_key: DriverKey,
    driver_version: DriverVersion,
    service_provenance: AgentServiceProvenance,
    placement_id: RuntimePlacementId,
    runtime_descriptor_digest: RuntimeDescriptorDigest,
    transport_descriptor_digest: Option<TransportDescriptorDigest>,
    effective_profile: CapabilityProfile,
    opaque_native_binding: EncryptedOpaqueBinding,
}
```

`opaque_native_binding` 可包含 Codex thread id、远端 route/session token 或 native runtime key，但绝不穿过 application seam。adapter 维护 native turn/item id 与 canonical ID 的 durable 映射。`placement_id` 表达 in-process、同机进程或 remote placement；它不改变 Agent service identity。

### 4.3 binding generation

每次显式 rebind 都增加 generation。所有 driver command/event 都携带 `binding_id + generation`：

- 旧连接迟到的事件不能污染新 binding；
- credential rotation 或 Integration instance 更新不会静默改变已绑定 session；
- driver upgrade 后必须显式 rebind/recover，并产生 durable binding event。

## 5. 三入口的完整类型

### 5.1 `execute`：所有 mutation 都是一种 Agent command

```rust
pub struct AgentCommandEnvelope {
    pub command_id: AgentCommandId,
    pub actor: AgentActor,
    pub causation: Option<AgentOperationId>,
    pub expectation: CommandExpectation,
    pub command: AgentCommand,
}

pub enum AgentCommand {
    CreateSession(CreateSessionCommand),
    StartTurn(StartTurnCommand),
    SteerTurn(SteerTurnCommand),
    InterruptTurn(InterruptTurnCommand),
    ResolveApproval(ResolveApprovalCommand),
    UpdateToolCatalog(UpdateToolCatalogCommand),
    CompactContext(CompactContextCommand),
    ForkSession(ForkSessionCommand),
    RebindRuntime(RebindRuntimeCommand),
    CloseSession(CloseSessionCommand),
}
```

每个 command struct 只携带自身合法字段，避免 `session_id/turn_id/item_id` 可选字段组合。

典型 expectation：

```rust
pub enum CommandExpectation {
    None,
    SessionRevision {
        session_id: AgentSessionId,
        expected: SessionRevision,
    },
    ActiveTurn {
        session_id: AgentSessionId,
        turn_id: AgentTurnId,
        expected_session_revision: SessionRevision,
    },
    PendingItem {
        session_id: AgentSessionId,
        turn_id: AgentTurnId,
        item_id: AgentItemId,
        expected_item_revision: ItemRevision,
    },
    ContextRevision {
        session_id: AgentSessionId,
        expected: ContextRevision,
    },
}
```

`execute` 的唯一成功返回：

```rust
pub struct OperationReceipt {
    pub operation_id: AgentOperationId,
    pub command_id: AgentCommandId,
    pub session_id: AgentSessionId,
    pub accepted_at_revision: SessionRevision,
    pub accepted_event_seq: AgentEventSeq,
    pub deduplicated: bool,
}
```

同一个 `command_id` 必须返回同一个 receipt。即使数据库 commit 后响应丢失，caller 也只需用原 command 重试，不需要查询“到底启动了几次”。

### 5.2 `snapshot`：单一事实源

```rust
pub struct AgentSnapshotQuery {
    pub session_id: AgentSessionId,
    pub at: SnapshotPoint,
    pub scope: SnapshotScope,
}

pub enum SnapshotPoint {
    Latest,
    AtEvent(AgentEventSeq),
    AtSessionRevision(SessionRevision),
}

pub enum SnapshotScope {
    Status,
    ContextManifest,
    ContextEntries,
    Full,
}
```

```rust
pub struct AgentSessionSnapshot {
    pub session_id: AgentSessionId,
    pub session_revision: SessionRevision,
    pub basis_event_seq: AgentEventSeq,
    pub status: AgentSessionStatus,
    pub binding: RuntimeBindingView,
    pub active_turn: Option<AgentTurnSnapshot>,
    pub pending_approvals: Vec<ApprovalSnapshot>,
    pub available_commands: Vec<CommandAvailability>,
    pub context: ContextSnapshot,
    pub tools: ToolCatalogSnapshot,
}
```

`available_commands` 由执行 command 的同一个 state machine 生成。UI availability 与 command fulfillment 不再各写一遍。

### 5.3 `events`：durable cursor + transient delta

```rust
pub struct AgentEventSubscription {
    pub session_id: AgentSessionId,
    pub after: Option<AgentEventSeq>,
    pub include_transient: bool,
}

pub type AgentEventStream =
    Pin<Box<dyn Stream<Item = Result<AgentRuntimeEnvelope, StreamFault>> + Send>>;
```

```rust
pub struct AgentRuntimeEnvelope {
    pub session_id: AgentSessionId,
    pub position: EventPosition,
    pub operation_id: Option<AgentOperationId>,
    pub turn_id: Option<AgentTurnId>,
    pub item_id: Option<AgentItemId>,
    pub binding_generation: RuntimeBindingGeneration,
    pub session_revision: SessionRevision,
    pub event: AgentRuntimeEvent,
}

pub enum EventPosition {
    Durable(AgentEventSeq),
    Transient {
        epoch: u64,
        transient_seq: u64,
        after_durable: AgentEventSeq,
    },
}
```

durable event 绝不跳号；订阅先建立 snapshot barrier，再发 backlog 和 live。transient delta 可以在断线时丢失，但对应 item 的 durable completed event 必须包含可重建的最终内容。

## 6. Command、event 与 error 模型

### 6.1 Command 不是 connector 方法镜像

主要 command 语义：

- `CreateSession`：引用 Integration instance、required capability、context binding 与 tool catalog；
- `StartTurn`：提交 typed user input，要求 session Ready 且无 active turn；
- `SteerTurn`：绑定 exact active turn，runtime 分配单调 input sequence；
- `InterruptTurn`：表达终止意图和所需 guarantee；
- `ResolveApproval`：绑定 pending item revision，decision 幂等；
- `UpdateToolCatalog`：替换目标 catalog revision，是否可热更新由 profile 决定；
- `CompactContext`：请求 managed durable compaction，而不是通知 connector“最好 compact 一下”；
- `ForkSession`：从明确 context revision 创建新 session；
- `RebindRuntime`：显式改变 Integration instance/driver binding；
- `CloseSession`：收敛 active operation 后关闭。

### 6.2 Event 是业务 runtime event，不是 wire passthrough

```rust
pub enum AgentRuntimeEvent {
    OperationAccepted(OperationAccepted),
    OperationSucceeded(OperationSucceeded),
    OperationFailed(OperationFailed),

    SessionCreated(SessionCreated),
    RuntimeBindingPrepared(RuntimeBindingPrepared),
    RuntimeBindingReady(RuntimeBindingReady),
    RuntimeBindingLost(RuntimeBindingLost),
    SessionClosed(SessionClosed),

    TurnStarted(TurnStarted),
    TurnSteerAccepted(TurnSteerAccepted),
    TurnInterruptRequested(TurnInterruptRequested),
    TurnCompleted(TurnCompleted),
    TurnFailed(TurnFailed),
    TurnInterrupted(TurnInterrupted),

    ItemStarted(ItemStarted),
    ItemUpdated(ItemUpdated),
    ItemCompleted(ItemCompleted),
    ApprovalRequested(ApprovalRequested),
    ApprovalResolved(ApprovalResolved),
    ToolInvocationRequested(ToolInvocationRequested),
    ToolInvocationCompleted(ToolInvocationCompleted),

    ContextSnapshotBuilt(ContextSnapshotBuilt),
    ContextCheckpointCommitted(ContextCheckpointCommitted),
    ContextActivationRequested(ContextActivationRequested),
    ContextCheckpointActivated(ContextCheckpointActivated),
    ContextActivationFailed(ContextActivationFailed),
    NativeContextCompactionObserved(NativeContextCompactionObserved),

    UsageUpdated(UsageUpdated),
    RuntimeDiagnostic(RuntimeDiagnostic),
}
```

Codex lifecycle item 可以作为 protocol presentation 映射，但 durable context commit 不能再藏进 `SessionMetaUpdate { key, value }`。

### 6.3 Error 分成“未接收”和“已接收后失败”

`execute` 只返回未接收错误：

```rust
pub enum ExecuteError {
    Rejected(CommandRejection),
    Unavailable {
        command_id: AgentCommandId,
        retry: RetryDirective,
        diagnostic_id: DiagnosticId,
    },
}

pub enum CommandRejectionCode {
    SessionNotFound,
    InvalidState,
    StaleSessionRevision,
    StaleContextRevision,
    TurnMismatch,
    ItemNotPending,
    CapabilityUnsatisfied,
    IntegrationInstanceUnavailable,
    BindingLost,
    ConfigInvalid,
    Conflict,
}
```

一旦返回 `OperationReceipt`，后续 driver、provider、tool、approval delivery、context activation 或 persistence failure 全部成为 durable `OperationFailed`，不再从原调用栈抛回。

`Unavailable` 只说明 caller 没拿到确定 receipt；必须用同一 command id 重试。module 保证不会由此重复执行。

driver 的原始 stderr、HTTP body、RPC error 不进入公共 error；公共 event 只含 typed fault code、retryability、phase 和 diagnostic id，完整诊断留在受控 observability store。

## 7. Context ownership 与 snapshot fidelity

### 7.1 Business Agent module 是 context 唯一所有者

Application 在 `CreateSession` 中传递 context binding，而不是已经拼平的 prompt：

```rust
pub struct ContextBinding {
    pub recipe: ContextRecipeRef,
    pub subject: ContextSubjectRef,
    pub source_policy: ContextSourcePolicy,
    pub initial_tool_catalog: ToolCatalogRevision,
}
```

- `ContextRecipeRef` 指向 Agent definition/template 的 context recipe revision；
- `ContextSubjectRef` 是 namespace + typed opaque id，可由 application 提供 task/story/project 等产品身份；
- 业务 Agent module 通过内部 `ContextSourceResolver` adapters 获取事实、组合 ContextFrame、应用 policy 并生成 canonical context snapshot；
- application 不再分别维护 task query builder 与 launch frame builder。

### 7.2 Canonical context 与 driver mirror 是两个明确平面

```rust
pub struct ContextSnapshot {
    pub revision: ContextRevision,
    pub basis_event_seq: AgentEventSeq,
    pub canonical_digest: ContextDigest,
    pub active_checkpoint: Option<ContextCheckpointId>,
    pub canonical_authority: ContextAuthority,
    pub driver_mirror: DriverContextMirror,
    pub manifest: ContextManifest,
    pub entries: Option<Vec<ContextEntry>>,
}

pub enum ContextAuthority {
    PlatformCanonical,
    DriverCanonical,
}

pub enum ContextFidelity {
    ExactRoundTrip {
        driver_revision: OpaqueDriverRevision,
        digest: ContextDigest,
    },
    EventProjected {
        basis_event_seq: AgentEventSeq,
    },
    Opaque,
}

pub enum ContextActivationGuarantee {
    IdempotentReplaceAndRecover,
    ReadOnlyExport,
    None,
}
```

“snapshot”不再默认声称等于 driver 真正看到的 context。每个 binding 都明确报告 authority、fidelity 与 activation guarantee。

### 7.3 Fidelity 对产品能力的直接约束

- `ExactRoundTrip + IdempotentReplaceAndRecover`：可以执行 managed durable compaction、exact fork、跨进程恢复；
- `EventProjected`：平台能展示/估算 history，但不能声称与外部 Agent 隐藏上下文完全相同；
- `Opaque`：只展示 session/turn/item 状态，不提供 context replacement；
- native external compact 只能产生 `NativeContextCompactionObserved`，不会推进 AgentDash canonical checkpoint head。

不存在“connector 不支持 exact restore 时悄悄退化成 markdown continuation”的 fallback。绑定前 requirements 不满足就被拒绝；若产品明确选择较低 fidelity，则 snapshot 和 availability 如实表达。

## 8. Compaction durable commit / activation

### 8.1 将“计算、持久化、激活”拆成显式 saga

分布式 driver 与 PostgreSQL 不可能共享一个物理事务。正确设计不是假装 atomic，而是用可恢复、幂等的状态机：

1. `CompactContext` 以 `expected ContextRevision` durable 接收；
2. runtime 获取该 session 的 context transition gate；
3. 从 committed canonical revision 计算 replacement，过程不修改 live driver；
4. PostgreSQL 事务写入不可变 checkpoint candidate、segments 和 `ContextCheckpointCommitted`，active head 保持不变；
5. transactional-context driver 收到带 `ContextActivationId` 的 `ActivateContext`；
6. driver 幂等应用 replacement，并返回 exact digest / driver revision；
7. PostgreSQL 事务以 base context revision 做 CAS，更新 active head、context revision，并同时写 `ContextCheckpointActivated + OperationSucceeded`；
8. 只有第 7 步之后，runtime 才能发布 compaction item completed 或允许等待中的 provider request 继续。

### 8.2 Crash recovery

- candidate commit 后、driver activation 前失败：candidate 标记 abandoned/failed，旧 head 继续 active；
- driver activation 失败：旧 head继续 active，operation failed；
- driver 已应用、数据库 activation 前 crash：recovery 用 `ContextActivationId + digest` 查询/重放 driver，确认后完成 CAS；
- driver 无法提供幂等 activation + recovery：profile 不得声明 managed compaction，command 在接收前被 `CapabilityUnsatisfied` 拒绝；
- stale base revision：activation CAS 失败，进入 reconciliation；context transition gate 与 revision rule 应使它只在实现 bug/外部非法改变时出现，不能覆盖新 head。

### 8.3 Auto compaction

auto compaction 不是另一套路径。`StartTurn` coordinator 在 provider 前评估 policy；若需要压缩，则创建 causation-linked child operation，完整执行同一 checkpoint saga。新 provider request 必须等待 `ContextCheckpointActivated`。

manual/auto 的差异只是 trigger/policy/operation actor，不是 persistence lifecycle。

## 9. Approval、steer、interrupt 与 tools

### 9.1 Approval

driver 的 native approval id 先映射为 canonical `AgentItemId`，runtime durable 写 `ApprovalRequested`。Application 用：

```rust
ResolveApprovalCommand {
    session_id,
    turn_id,
    item_id,
    decision_id,
    decision: ApprovalDecision::AllowOnce,
}
```

runtime 先以 item revision CAS 接收 decision，再通过 outbox 交付 driver。driver 必须按 `decision_id` 幂等。重复 decision 返回原 receipt；冲突 decision 被拒绝。`ApprovalResolved` 只在 driver ack 或 contract 明确规定“durable queued 即生效”时写入，保证由 capability profile 声明。

### 9.2 Steer

- 必须绑定 exact active turn；
- runtime 在 session transaction 内分配 `TurnInputSeq`；
- driver profile 声明 `SteerGuarantee::OrderedQueued` 或 `ImmediateBeforeNextProvider`；
- out-of-order/stale turn steer 在接收前拒绝；
- queued steer 即使进程重启也由 outbox 重投，不依赖进程内队列。

### 9.3 Interrupt

- `InterruptTurn` durable 写 `TurnInterruptRequested`；
- strict interactive driver 必须幂等接收并最终给出 `TurnInterrupted` 或已经 terminal 的确定结果；
- 仅支持 best-effort cancel 的 driver 可在 capability profile 中声明 weaker guarantee，但不能达到严格 interactive level；
- application 只看 canonical turn terminal，不直接调用 connector.cancel 后猜测状态。

### 9.4 Tools

- Tool catalog revision 属于 context snapshot；
- tool exposure、permission 和 capability pack 展开由业务 Agent context/tool policy 拥有；
- driver 只收到已经裁决的 `DriverToolContract`；
- native/core、Codex/MCP、remote Agent 的 tool call 都映射为同一 Item lifecycle；
- platform-hosted tool 通过 runtime 内部 `ToolHost` port 执行，结果用 idempotent driver command 回送；
- `UpdateToolCatalog` 在 turn boundary 生效；只有 profile 明确支持 hot replace 时才可在 active turn 内应用。

## 10. 可插拔 Integration 系统

### 10.1 继承项目 canonical taxonomy

本方案继承 `.trellis/tasks/06-04-plugin-extension-taxonomy/design.md` 已确认边界：

- Integration 是受信、编译期、宿主级扩展；
- 不做 dylib/WASM 动态加载；
- Extension / Capability Pack 才是数据驱动安装内容；
- 企业自研 Agent 的免重编译接入使用通用 remote-runtime Integration + versioned wire protocol。

因此“可插拔 Agent 服务”表示：

1. 核心不硬编码 connector/executor enum；
2. 受信 Integration 在编译期注册 driver factory；
3. 运行期可创建/更新 service definition、instance、config、credential references；
4. 企业可以自行实现远端协议端点，通过通用 remote-runtime factory 接入，无需把企业代码动态加载进 AgentDash 宿主。

### 10.2 Definition、instance、credential、factory 的分层

#### Trusted Integration / factory registration

```rust
pub trait AgentDriverFactory: Send + Sync {
    fn manifest(&self) -> DriverFactoryManifest;

    async fn validate_instance(
        &self,
        input: DriverInstanceValidationInput,
    ) -> Result<ValidatedInstanceDescriptor, DriverConfigError>;

    async fn create_driver(
        &self,
        input: DriverCreationInput,
        host: Arc<dyn DriverHost>,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFault>;
}
```

`AgentDashIntegration` 贡献 `Vec<Arc<dyn AgentDriverFactory>>`。composition root 按 `DriverKey` 建 registry；重复 key 直接使 bootstrap 失败。核心代码不写 `match Native/Pi/Codex/EnterpriseAgent`。Relay 不注册 Agent driver factory，因为它不是 Agent service。

factory 是 Integration seam 的 adapter provider，不是 application-facing Agent Runtime interface。

#### Integration definition

```rust
pub struct AgentServiceDefinitionRevision {
    pub definition_id: IntegrationDefinitionId,
    pub revision: IntegrationDefinitionRevision,
    pub driver_key: DriverKey,
    pub driver_version_requirement: VersionRequirement,
    pub config_schema: JsonSchemaDocument,
    pub config_schema_digest: SchemaDigest,
    pub credential_slots: Vec<CredentialSlotDefinition>,
    pub declared_max_profile: CapabilityProfile,
    pub presentation: IntegrationPresentation,
}
```

definition 是受信 factory manifest 的运行期 catalog 投影，不携带可执行代码。schema 由 factory 提供，数据库只能引用/pin revision，不能把未知 driver key 变成可执行实现。

#### Integration instance

```rust
pub struct AgentServiceInstance {
    pub instance_id: IntegrationInstanceId,
    pub revision: IntegrationInstanceRevision,
    pub definition_id: IntegrationDefinitionId,
    pub definition_revision: IntegrationDefinitionRevision,
    pub scope: IntegrationScope,
    pub non_secret_config: serde_json::Value,
    pub credential_bindings: BTreeMap<CredentialSlotKey, SecretReference>,
    pub enabled: bool,
}
```

instance 是部署/租户/组织可运行配置。修改配置产生新 revision；已绑定 session 不会自动漂移。

#### Credentials

- config JSON 禁止存 secret value；
- credential slot schema 描述 secret kind、required/optional、scope 与验证方式；
- instance 只保存 secret-vault reference；
- factory 创建 driver 时拿短期 `CredentialLease`，公共 snapshot/event 永不包含 secret；
- rotation 更新 credential binding revision，live session 是否 rebind 由显式 policy/command 决定。

#### Runtime descriptor

factory 对具体 instance 做 schema validation、semantic validation 和 runtime probe 后，local registry 发布 service descriptor。service descriptor 的 identity/provenance 不随 placement 改变：

```rust
pub struct AgentServiceDescriptor {
    pub service_id: AgentServiceId,
    pub provenance: AgentServiceProvenance,
    pub contract_version: RuntimeContractVersion,
    pub driver_key: DriverKey,
    pub driver_version: DriverVersion,
    pub instance_id: IntegrationInstanceId,
    pub instance_revision: IntegrationInstanceRevision,
    pub service_guarantee_level: GuaranteeLevel,
    pub service_capabilities: CapabilityProfile,
    pub limits: RuntimeLimits,
    pub config_fingerprint: ConfigFingerprint,
    pub service_descriptor_digest: RuntimeDescriptorDigest,
}
```

cloud 持久化同一份 service provenance/descriptor identity，并另外记录 remote placement 与 transport descriptor。最终 session binding 使用的是组合后的 bound runtime descriptor：

```rust
pub struct RuntimeDescriptor {
    pub contract_version: RuntimeContractVersion,
    pub service: AgentServiceDescriptorRef,
    pub placement: RuntimePlacementDescriptor,
    pub service_profile: CapabilityProfile,
    pub transport_profile: CapabilityProfile,
    pub host_policy_profile: CapabilityProfile,
    pub guarantee_level: GuaranteeLevel,
    pub capabilities: CapabilityProfile,
    pub descriptor_digest: RuntimeDescriptorDigest,
}
```

其中最终有效能力严格按下式求交，而不是把 transport 能力加到 service 上：

```text
bound profile = service guarantee ∩ transport guarantee ∩ host policy
```

最终 `GuaranteeLevel` 也从求交后的 profile 重新推导。session binding pin 住 service provenance、placement、transport descriptor 与最终 runtime descriptor digest。instance、placement 或 transport 更新后，新 session 使用新 descriptor；既有 session 只有显式 `RebindRuntime` 才切换。

### 10.3 Driver runtime seam

factory 创建的 driver 使用一个极小内部 interface：

```rust
#[async_trait]
pub trait AgentRuntimeDriver: Send + Sync {
    async fn apply(
        &self,
        command: DriverCommandEnvelope,
    ) -> Result<DriverAck, DriverFault>;
}
```

driver event 不从 `apply` 返回长寿命 stream，而是通过创建时注入的 `DriverHost` 回调：

```rust
#[async_trait]
pub trait DriverHost: Send + Sync {
    async fn publish(&self, event: DriverEventEnvelope)
        -> Result<DriverEventAck, DriverHostError>;
}
```

这样 native callback、Codex stdio/WebSocket 和 remote RPC 都能统一推送；`DriverEventAck` 告诉 adapter durable ingestion 到哪个 native/canonical cursor。driver command 必须携带 operation/activation/decision idempotency key。

## 11. Strict Guarantee Level 与 Capability Profile

### 11.1 二者不是二选一

- **Guarantee Level**：版本化、累积、严格的最低语义保证 bundle；用于快速判断一个 driver 的基础可靠性层级。
- **Capability Profile**：每一项具体能力及其保证强度；用于 command admission、UI availability 与精确 requirements。

level 不能由 factory 任意填数字。descriptor validator 根据 capability guarantee 计算最高满足层级；声明与计算不一致则 instance validation 失败。

### 11.2 V1 level

| Level | 名称 | 累积保证 |
| --- | --- | --- |
| L0 | Invocable | 能接收一个幂等请求，并给出确定 terminal result；不保证 session resume |
| L1 | ObservableTurn | L0 + 稳定 turn/item identity、ordered lifecycle、每个 started 最终 terminal |
| L2 | RecoverableSession | L1 + durable source-session binding、幂等 command delivery、断线恢复/重连与 cursor reconciliation |
| L3 | InteractiveSession | L2 + approval decision 幂等交付、ordered steer、acknowledged interrupt、tool item fidelity |
| L4 | TransactionalContext | L3 + exact context round-trip、digest、idempotent replace/activation/recovery，支持 managed durable compaction/fork |

level 是 cumulative baseline，不表示所有可选 feature。例如 L2 driver 可以额外支持 approval，但如果没有 ordered steer/ack interrupt，仍是 L2。

### 11.3 Typed capability examples

```rust
pub struct CapabilityProfile {
    pub turn: TurnCapability,
    pub resume: ResumeCapability,
    pub approval: Option<ApprovalCapability>,
    pub steer: Option<SteerCapability>,
    pub interrupt: InterruptCapability,
    pub tools: ToolCapability,
    pub context: ContextCapability,
    pub compaction: CompactionCapability,
    pub fork: Option<ForkCapability>,
    pub usage: UsageCapability,
    pub discovery: DiscoveryCapability,
}

pub enum CompactionCapability {
    None,
    NativeTelemetry,
    Managed {
        activation: ContextActivationGuarantee,
        preserves_tool_state: bool,
    },
}
```

每个 command 在业务 runtime 内有明确 `CapabilityRequirement`。Admission 同时检查 session state、level/profile 和 command expectation。

绑定请求也可以声明：

```rust
pub struct RuntimeRequirements {
    pub minimum_level: GuaranteeLevel,
    pub required_capabilities: Vec<CapabilityPredicate>,
}
```

没有 fallback：requirements 不满足则 Create/Rebind 在接收前拒绝。

### 11.4 Capability provenance

effective capability 可以来自明确组合的 adapter stack，例如 Agent service + platform-hosted tool bridge。profile 对每项记录 provenance：`Service / Transport / Platform / Composed`。组合首先应用 service/transport/host-policy 求交；任何 platform 贡献都必须是显式配置且具有自己的 guarantee，不能绕过 service 或 transport 的上限。这是绑定时确定的实现组合，不是运行时失败后的降级。

## 12. Native、Codex、Enterprise Agent drivers 与 Relay placement

### Native Core driver

- 受信 first-party Integration 注册 `native-core` factory；
- adapter 把 DriverCommand 映射为纯 Agent Core loop；
- platform canonical context 可 exact apply；
- Agent Core compaction 计算返回 replacement candidate，不先修改 live context；
- 可达到 L4；
- tools、approval、steer、interrupt 都通过 Core delegates 映射为 canonical DriverEvent。

### Codex App Server driver

- factory 注册 `codex-app-server` driver key，不需要核心 enum；
- instance config 描述 binary/endpoint、model/options 和 credential refs；
- adapter 负责 Codex request/notification 与 DriverCommand/DriverEvent 的 typed mapping；
- 原生 `thread/compacted` 只声明 `CompactionCapability::NativeTelemetry`；
- 若扩展后的协议提供 exact context export/import/digest/idempotent activation，可 probe 为 L4；否则通常为 L2/L3，不能执行 AgentDash managed durable compaction。

### Generic remote-runtime driver

- 这是受信、编译期 Integration，宿主内只包含通用 client 与 wire contract；
- 企业自研 Agent 在独立服务中实现 versioned remote-runtime protocol；
- runtime instance 配 endpoint、tenant/service identity、TLS/credential refs；
- handshake 返回 descriptor/capability claims，host contract suite/probe 校验后生成 effective RuntimeDescriptor；
- 远端服务可独立升级，不要求 AgentDash 为每个企业 Agent 增加 connector enum 或重新编译；只有 wire contract 大版本/宿主 Integration 本身升级才需要宿主发布。

### Relay placement transport

Relay 不是 Agent service、不是 driver、也不是 Integration。它只解决 local Agent service 被 cloud 发现和调用时的 placement transport：

1. local registry 发布 Pi/Codex/企业 Agent 的 `AgentServiceDescriptor`；
2. cloud 持久化完全相同的 service provenance 与 descriptor digest；
3. cloud 另外记录 `RuntimePlacementDescriptor::Remote` 与 relay transport descriptor；
4. Relay 透明转发统一 remote-runtime protocol，不重新解释 Agent command/event；
5. binding 时按 `service ∩ transport ∩ host policy` 计算 effective profile；
6. Relay 断线影响 placement health/transport guarantee，不把 service identity 改成“Relay Agent”。

因此 Relay 不出现在 driver factory registry，也没有 Relay 专属 Agent capability。若 transport 不支持某种流控、重放、payload 或双向交互保证，求交后的 bound profile 会降低，相关 command 在 admission 阶段不可用。

## 13. 依赖分类与内部 seams

### In-process

- session/turn/item aggregate；
- command admission / availability；
- context composition；
- compaction policy与 replacement 计算；
- canonical event reduction。

这些直接收进深 module，不为每个纯函数暴露 application port。

### Local-substitutable

- PostgreSQL event/operation/context checkpoint store；
- durable outbox；
- secret vault client 的本地实现；
- tool host / VFS 受控本地实现。

业务 module 拥有内部 store/secret/tool ports，production 使用 PostgreSQL/vault adapter，测试使用 in-memory 或本地可替代实现。它们不是 application interface。

### Remote but owned

- Relay placement transport 与企业 remote Agent service（前者是 transport，后者是 service/driver）；
- 内部 secret service；
- 远端 tool execution service。

在相应 seam 定义 port，production 使用 RPC adapter，测试使用 in-memory scripted adapter。

### True external

- Codex App Server / 外部 provider；
- 第三方 MCP 服务；
- 外部模型/API。

全部封装在 driver/tool adapter 内，以 mock/scripted adapter 跑 contract tests；业务 runtime 不解析其原始 wire JSON。

## 14. 调用顺序

### 14.1 Create / bind

1. Application 创建产品 `AgentRunRuntimeBinding` 意图；
2. 调 `execute(CreateSession { integration_instance_id, requirements, context_binding })`；
3. runtime 校验 instance revision、schema、credentials metadata 和 effective descriptor；
4. 一个 DB transaction 写 session、operation accepted、初始 revision、binding prepared、outbox command；
5. 返回 receipt；
6. driver factory 创建/恢复 driver，执行 OpenBinding；
7. native binding 以 encrypted opaque data 持久化；
8. 写 `RuntimeBindingReady + OperationSucceeded`；
9. Application 从 events/snapshot 观察 Ready，再保存/确认 run -> session product binding。

### 14.2 Start turn

1. Application 读 snapshot 获得 revision/availability；
2. `execute(StartTurn)` 携带 expected revision；
3. transaction 保证无 active turn，创建 canonical TurnId、operation、context-build intent；
4. runtime resolve/compose context；
5. 若 auto compact，先跑同一 durable activation saga；
6. 确认 driver mirror 已应用目标 context/tool revision；
7. outbox 发送 StartTurn；
8. driver events 经 binding generation 校验、native ID mapping、canonical reduction 后 durable append；
9. terminal event 与 session/turn/operation 状态在一个 transaction 收敛。

### 14.3 Approval

1. driver event -> durable ApprovalRequested item；
2. application events 收到 canonical item；
3. `execute(ResolveApproval)` 携带 item revision；
4. transaction CAS pending -> decision queued；
5. outbox 幂等交付 driver；
6. driver ack -> ApprovalResolved，turn 继续。

### 14.4 Steer / interrupt

均先 durable 接收并分配顺序，再出 outbox；不会先调进程内 connector、失败后才补 receipt。迟到 driver event 必须匹配 active turn 与 binding generation。

## 15. 并发与一致性不变量

1. 一个 session 同时最多一个 active turn；
2. `SessionRevision` 每次 durable state transition 单调递增；
3. `AgentEventSeq` 与 state revision 在同一 transaction 分配，durable event 不跳号、不回退；
4. `AgentCommandId` 全局或 scope 内唯一，重试只返回原 receipt；
5. command expectation 使用 CAS，availability 与 admission 使用同一 reducer；
6. driver side effect 只由 durable outbox 发出；outbox 可重复投递，driver command 必须幂等；
7. binding generation 不匹配的 event 被隔离并产生 diagnostic，不能进入 session reducer；
8. turn/item/native ID mapping 在接收首个 event 时原子创建，后续不可重绑；
9. approval decision、steer input、interrupt request 各有独立 idempotency key 和单调序列；
10. context transition gate 串行化 context refresh、compaction、fork/rebind；
11. active context head 只能以 expected base revision CAS 前进；candidate commit 不等于 activation；
12. provider request 只能消费已 activated 且 driver mirror guarantee 满足的 context revision；
13. runtime descriptor/instance revision 在 binding 生命周期内固定；
14. credential/config 更新不会隐式修改 live session；
15. durable subscription 使用 barrier，backlog/live 之间无缺口；transient delta 可丢但 terminal durable event 必须自洽；
16. terminal operation/turn/item 状态不可被后到事件改写。

跨进程并发不依赖进程内 mutex 作为事实源；使用 PostgreSQL transaction、revision CAS、lease/outbox。进程内 actor/lock 只优化吞吐。

## 16. Application 使用示例

### 16.1 创建并等待 session ready

```rust
let receipt = runtime
    .execute(AgentCommandEnvelope::create_session(
        command_id,
        CreateSessionCommand {
            integration_instance_id,
            requirements: RuntimeRequirements::interactive(),
            context_binding,
        },
    ))
    .await?;

let mut events = runtime
    .events(AgentEventSubscription::after(
        receipt.session_id,
        receipt.accepted_event_seq,
    ))
    .await?;

while let Some(event) = events.next().await {
    match event?.event {
        AgentRuntimeEvent::RuntimeBindingReady(_) => break,
        AgentRuntimeEvent::OperationFailed(failure)
            if failure.operation_id == receipt.operation_id => return Err(failure.into()),
        _ => {}
    }
}
```

### 16.2 启动 turn

```rust
let snapshot = runtime
    .snapshot(AgentSnapshotQuery::status(session_id))
    .await?;

ensure_available(&snapshot, AgentCommandKind::StartTurn)?;

let receipt = runtime
    .execute(AgentCommandEnvelope::start_turn(
        command_id,
        session_id,
        snapshot.session_revision,
        user_input,
    ))
    .await?;
```

### 16.3 durable compaction

```rust
let snapshot = runtime
    .snapshot(AgentSnapshotQuery::context_manifest(session_id))
    .await?;

let receipt = runtime
    .execute(AgentCommandEnvelope::compact_context(
        command_id,
        session_id,
        snapshot.context.revision,
        CompactionRequest::AtNextTurnBoundary,
    ))
    .await?;

// 调用方只等待 OperationSucceeded 或 OperationFailed；
// 不轮询 750ms，也不读取 compaction request repository。
```

## 17. Interface 后隐藏的 implementation

公开 caller 不需要知道：

- session aggregate 与 reducer；
- Product AgentRun 到 session 之外的 native ID mapping；
- command receipts、manual request、maintenance turn；
- ContextFrame source resolver/composer/delivery plan；
- transcript fold、MessageRef、checkpoint segments/head；
- compaction auto/manual policy、summarizer、failure fuse；
- driver registry/factory、config schema validator、credential lease；
- Codex/remote wire protocol；
- approval/steer/interrupt outbox；
- PostgreSQL transaction/CAS；
- crash recovery/reconciliation；
- durable/transient event fanout；
- protocol presentation mapping。

测试也应主要从同一个 execute/snapshot/events interface 观察这些行为，而不是绕过 seam 直接断言 repository row。

## 18. 优点、代价与适用边界

### 优点

- 三个入口，调用路径稳定；
- 所有 mutation 共享幂等、revision、receipt、event 和 failure model；
- compaction、approval、steer、interrupt 不再各自发明同步/异步语义；
- application 与 driver/协议/数据库彻底解耦；
- Integration instance/driver capability 完全数据驱动，核心无 connector enum；
- internal/external Agent 的差异由真实 guarantee/profile 表达，不伪装成能力相同；
- crash recovery 与 durable success boundary 成为 module 内部不变量；
- snapshot fidelity 防止平台 read model 冒充 driver exact context。

### 代价

- caller 必须接受 operation/event 异步模型；简单 command 也不能直接等待 connector 返回值；
- `AgentCommand` / `AgentRuntimeEvent` 是较大的类型代数，虽然方法少，interface 仍包含 Agent 能力本身的固有复杂度；
- 需要 outbox、reconciler、descriptor validation、driver contract suite，早期实现成本较高；
- exact managed compaction 对 driver 的要求非常严格，现有 Codex external adapter 可能暂时只能声明 native telemetry；
- snapshot/event schema 成为长期公共合同，需要版本纪律；
- Integration management 是另一个独立 admin module，不能为了“三个方法”把 definition/credential CRUD 硬塞进 AgentRun runtime command。

### 为什么不把 Integration admin 合进三个入口

Agent session execution 与 Integration definition/instance lifecycle 有不同 actor、权限、审计和变化频率。它们应是两个 module/seam。AgentRun runtime 只消费 `IntegrationInstanceId + RuntimeRequirements`；管理后台使用单独的深 `AgentIntegrationCatalog` interface。这样不会把 admin CRUD 污染每个 Agent caller。

## 19. Migration 影响

### Crate/module

建议最终形成：

- `agentdash-agent-runtime-contract`：三入口 trait、typed IDs、commands/events/snapshots/errors；
- `agentdash-managed-agent`：业务深 module implementation；
- `agentdash-agent-core`：纯 loop/compaction algorithm；
- `agentdash-agent-driver-contract`：factory/driver/host/capability/descriptor contract；
- `agentdash-executor`：Native/Codex/Remote driver adapters；
- `agentdash-infrastructure`：PostgreSQL、outbox、vault adapters；
- `agentdash-agent-protocol`：Codex App Server + AgentDash typed wire extension。

Application 只依赖 runtime contract；composition root 负责注入 implementation 与 adapters。

### 删除/替换

- 删除 `ConnectorType` 与 hardcoded connector branching；
- 删除 bool `ConnectorCapabilities`、`supports_repository_restore` 和默认 no-op methods；
- 删除 application-agentrun/runtime-session 两套 launch path types；
- 删除 manual compaction special runtime port、Todo adapter、750ms poll；
- 删除 executor 组装 string-key compaction JSON；
- 删除 SessionEventing 中 compaction parser/coordinator；
- 删除 application query/execution 两套 context builder；
- Agent Core 移除 AgentDash domain/protocol knowledge。

### Database migration

目标表可按最终命名评审调整，语义至少需要：

- `agent_service_definitions` / revisions；
- `agent_service_instances` / revisions；
- `agent_service_credential_bindings`（只存 secret refs）；
- `managed_agent_sessions`；
- `managed_agent_runtime_bindings`；
- `managed_agent_operations`；
- `managed_agent_events`；
- `managed_agent_native_id_bindings`；
- `managed_agent_context_checkpoints` / segments / heads；
- `managed_agent_driver_outbox`。

migration 直接创建新 canonical schema、一次性转换可证明等价的 runtime session/event/committed checkpoint 数据，然后删除旧 session compaction request/projection tables。项目未上线，不建立 compatibility view、双写或旧 API fallback。

不完整 manual request 不应伪装成 committed operation；migration 应按合法状态机显式转为 failed/abandoned terminal record或清理预研数据，最终数据库只允许新 invariant。

## 20. 验证策略

### Runtime interface behavior suite

所有 implementation 必须从 execute/snapshot/events 验证：

- command idempotency 与 response-loss retry；
- availability/admission 同源；
- revision CAS；
- ordered durable events；
- terminal state 不可改写；
- approval/steer/interrupt 并发；
- crash 后 outbox/reconciler 收敛；
- compaction candidate/activation 各 crash point；
- snapshot context fidelity 与 active checkpoint；
- binding generation 隔离迟到 event。

### Driver contract suite

每个 factory 根据 descriptor claims 自动运行 contract tests。声明 L3 就必须通过 approval、steer、interrupt、tool lifecycle；声明 L4 就必须通过 exact context digest、重复 activation、activation 后 crash recovery。不能靠人工相信 capability bool。

### Adapter tests

- Native Core AgentEvent -> DriverEvent；
- Codex wire -> DriverEvent 与 capability probe；
- remote-runtime protocol conformance；
- secret 不进入 config snapshot/event/log；
- config JSON Schema + semantic validation；
- old binding generation event quarantine。

### PostgreSQL tests

- operation accepted + event + state revision 原子；
- event seq/head 单调；
- context candidate commit 与 activation CAS；
- outbox 重投；
- concurrent StartTurn/Compact/Approval property tests。

## 21. 最终评价

这是三套候选中应当代表“最小 interface 极限”的方案：它没有为 launch、control、eventing、compaction、approval、context query 分别暴露 service，而是把所有 mutation 归一为 durable operation，把所有观察归一为 snapshot/event。

它的最大价值不是方法数量少，而是 interface 只承诺三件可长期稳定的事：

1. 意图是否已经 durable 接收；
2. 某个 revision 上 session 的一致状态是什么；
3. 从某个 cursor 以后发生了哪些 typed 事实。

Native Agent Core、Codex App Server 和企业 Remote Agent 是这个深 module 后的 service/driver adapters；Relay 单独作为 placement transport，透明承载同一协议。service 差异通过严格 GuaranteeLevel、typed CapabilityProfile、ContextFidelity 和 service descriptor 表达，最终 bound profile 再与 transport guarantee、host policy 求交。Integration 仍遵守受信编译期 taxonomy，运行期可管理 service instance/config/credential refs，企业自研 Agent 通过通用 remote-runtime wire contract 免重编译接入。

如果团队愿意接受 operation/event 异步心智模型及较高的一次性基础设施成本，这个方案能提供最高的 locality、最清楚的 durable success boundary，以及最小的 application 依赖面。
