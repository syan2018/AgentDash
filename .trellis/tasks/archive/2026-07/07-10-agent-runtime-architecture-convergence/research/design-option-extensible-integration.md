# 设计选项 B：面向企业 Integration 的可扩展 Agent Runtime

> 定位：这是 `DESIGN-IT-TWICE` 中“最大扩展性”分支。它刻意不同于仅为当前 Pi/Codex 抽取一组最小方法的方案：本方案把 Agent 服务本身建模成 Integration 能力，在稳定 Runtime Protocol 上允许企业自研 Agent 接入，同时维持 application / AgentRun 的窄 seam。
>
> 本文是研究设计，不修改生产代码。项目尚未上线，迁移按最终正确模型硬切，不保留旧 connector、事件或数据库字段的兼容双轨。

## 一、结论先行

推荐建立一个深的 **Agent Runtime Integration Host module**，对 application / AgentRun 只暴露四个动作：发现可用 runtime、绑定 session、提交 typed command、读取绑定状态。Integration definition、instance config、credential resolution、driver factory、协议协商、session ownership、能力保证、事件持久化、恢复和隔离全部隐藏在该 seam 后面。

该方案的关键不是“让任意 Rust 插件动态实现一个大 trait”。项目已有 canonical taxonomy 已确认：

- **Integration** 是受信、编译期、宿主级扩展；由核心团队或部署接入部门维护，明确不做动态 dylib / WASM 加载。
- **Extension** 是 Project 级数据驱动工作台扩展。
- **Capability Pack** 是 Agent 级数据驱动能力包。
- **Shared Library** 是 Extension / Capability Pack 的分发与归属层。

因此企业 Agent 的扩展路径分为两种：

1. 需要宿主深集成的实现，在私有 Integration crate 中编译注册一个 driver factory；它能够跟随 upstream，而不需要修改 core。
2. 希望免重编译接入的企业自研 Agent，不向 cloud 宿主动态加载代码；它在部署端注册真实的 Agent service/driver并实现 AgentDash Runtime Wire Protocol，cloud 通过内置的通用 `remote-runtime` Integration连接该service，同时保留原始service provenance。Relay只决定remote placement及传输路径，不成为service identity。

`AgentConnector` 和 `RuntimeDriver` 必须分层：connector 只负责字节/帧/进程/网络连接，没有 Agent 业务保证；driver 才负责 session/turn/item、能力协商、ID 绑定、事件与失败归一化。只有通过 Runtime conformance 的 driver 才能被 AgentRun 选择。

这会形成三类稳定扩展面：

```text
受信宿主扩展          编译期 Integration -> DriverFactory
远端企业 Agent        企业 Service/Driver -> Runtime Wire Protocol -> remote placement
Agent 内容与工具能力   Capability Pack / Extension -> 数据与声明
```

它们不能再被一个含糊的“plugin/connector”概念覆盖。

## 二、DESIGN-IT-TWICE 问题框定

### 2.1 任何方案都必须满足的约束

1. application / AgentRun 不知道 Pi、Codex、relay、企业 Agent、进程管理或 JSON-RPC。
2. 内部业务 Agent 和外部 Agent 使用同一套 session / turn / item lifecycle，不只是输出相同事件外形。
3. start、resume、fork、read、compact、interrupt、steer、approval、tool revision 都必须有独立 typed 语义，不能继续塞进 `prompt()`。
4. capability 必须按所选 executor 和已绑定 session 描述，不能对 composite child 做 OR。
5. “支持”必须包含可验证 guarantee；声明一个 bool 不足以让 application 安全调用。
6. context owner、snapshot fidelity、compaction commit 边界必须显式，不能把外部 opaque compaction 当成平台 projection 成功。
7. authoritative lifecycle 不能依赖允许 lag/drop 的 broadcast。
8. 远端企业 Agent 的免重编译扩展必须经通用 wire protocol；宿主不加载不可信 native code。
9. Integration definition 与运行中的 instance 必须分离；definition 决定合法 schema，instance 只承载配置、credential reference、placement 和 policy。
10. 旧协议和旧数据库字段不构成兼容约束；versioning 用于新生态的明确协商，不用于静默 fallback。

### 2.2 依赖类别

按 `codebase-design` 的依赖分类，本设计涉及：

| 依赖 | 分类 | 设计方式 |
| --- | --- | --- |
| capability intersection、ID/state machine、manifest/schema validation、command routing | In-process | 合并进 Integration Host implementation，通过外部 interface 直接测试，不额外暴露 port |
| definition/instance/binding/event journal 存储、credential store | Local-substitutable | 使用 PostgreSQL production adapter 与事务型测试 adapter；这些是 Host 内部 seam |
| cloud ↔ local relay、企业自研 Agent 服务 | Remote but owned | Host 拥有 Runtime Wire port，relay/WebSocket/gRPC 是 adapter；测试用 loopback/in-memory adapter |
| Codex app-server、第三方模型/Agent vendor | True external | Codex driver 持有 vendor adapter；用 fake JSON-RPC peer 做 conformance，不让 vendor type进入 Host interface |
| 受信 Integration crate | In-process / compile-time | 由 composition root 静态注册 factory；不是运行时 plugin loader |

这个分类决定测试策略：状态机、协商和策略在 Host seam 上测；远端 transport 用替换 adapter；Codex 用 mock peer；不为每个内部 helper 暴露浅 interface。

### 2.3 为什么选择更大的 Integration seam

最小接口方案可以很快把 `prompt/cancel` 改成若干 typed 方法，但企业扩展会迫使 application 继续了解：driver 类型、配置 schema、凭据、版本、远端位置、能力差异、session ownership 和 native ID。每新增一个企业 Agent，知识就会扩散到 API、application、executor、local 和前端。

本方案把这些知识集中到一个深 module：

- **Depth**：application 学会四个动作，就能使用内置 Pi、Codex、relay 和未来企业 Agent。
- **Leverage**：协商、路由、幂等、恢复、事件可靠性、凭据和策略只实现一次，被所有 driver 复用。
- **Locality**：新增协议版本、能力 profile 或 driver 时，变化集中在 Integration Host、driver adapter 和 conformance kit。
- **Deletion test**：若删除 Host，definition、instance、factory、协商、binding、capability、事件、重连与策略复杂度会重新散落到每个 caller，说明该 module 确实在隐藏复杂度，而不是 pass-through。

## 三、canonical taxonomy 与本方案术语

### 3.1 六个容易混淆的对象

| 名称 | 含义 | 生命周期 | 是否加载宿主代码 |
| --- | --- | --- | --- |
| `AgentRuntimeIntegrationDefinition` | 受信 Integration 在编译期贡献的 runtime 类型、factory key、schema 与宿主权限 | 随二进制发布 | 代码已编译进宿主，不动态加载 |
| `AgentRuntimeIntegrationInstance` | definition 的运行期配置实例，例如某个企业 endpoint、某个 Codex 安装或某个 native runtime 配置 | 管理员创建、启停、升级配置 | 否，只选择已注册 factory |
| `AgentServiceDefinition` | Pi、Codex或企业Agent的真实逻辑service identity、作者、版本和能力来源 | 由local Integration registry发布，跨placement保持不变 | 否，是service provenance |
| `AgentServiceInstance` | 某个service definition的可运行部署实例；可以位于cloud host或local host | activation/placement生命周期 | 否，由已注册driver承载 |
| `RuntimeExecutorOffer` | service instance协商后发现的可选 Agent executor/variant | 可刷新、有 descriptor revision | 否，是运行期发现结果 |
| `RuntimeSessionBinding` | AgentDash session 到某一service instance、executor、driver generation、placement与native coordinates的持久绑定 | session 生命周期 | 否，是事实记录 |

企业可能创建多个 instance：

```text
remote-runtime Integration definition（内置、编译期桥接能力）
  └─ connection instance: enterprise-local-host@prod
       └─ local Integration registry publishes
            ├─ service: corp.finance-research-agent@2.1.0
            └─ service: corp.legal-review-agent@1.4.0
```

这些service不是remote-runtime Integration自己的executor，也不是Relay executor。cloud持久化local registry发布的原始 `ServiceProvenance`，只额外记录 `RuntimePlacement::Remote`、transport identity和remote host。这样同一企业service在local/cloud发现结果中保持同一语义身份。

```rust
struct AgentServiceProvenance {
    service_definition_id: AgentServiceDefinitionId,
    publisher_integration: IntegrationDefinitionId,
    publisher_instance: IntegrationInstanceId,
    service_version: ServiceVersion,
    service_build_digest: ServiceBuildDigest,
}

enum RuntimePlacement {
    InProcess,
    LocalProcess { host: RuntimeHostId },
    Remote { host: RuntimeHostId, transport: PlacementTransportId },
}
```

`AgentRuntimeIntegrationInstance` 是宿主Integration的配置/激活对象；`AgentServiceInstance` 是AgentRun最终选择的service对象。一个Integration instance可以发布零到多个service instances，两者不能复用同一ID。

### 3.2 AgentConnector 与 RuntimeDriver 的明确划分

**AgentConnector** 是低层 transport adapter，interface 只允许出现连接与帧：

```rust
trait AgentConnector {
    async fn connect(&self, endpoint: ConnectorEndpoint) -> Result<DuplexFrameChannel, ConnectError>;
}
```

它可以是 child-process stdio、WebSocket、relay、HTTP/2 或 in-memory channel。它不知道 AgentRun、session projection、compaction checkpoint、approval policy；它也不可以被 application 直接发现或选择。

**RuntimeDriver** 是 Agent Runtime seam 的 adapter，负责把一个 native runtime 提升到 AgentDash-owned protocol：

```rust
trait RuntimeDriver {
    async fn negotiate(&self, hello: RuntimeHostHello) -> Result<DriverHello, DriverError>;
    async fn execute(&self, frame: RuntimeCommandFrame, sink: DriverEventSink)
        -> Result<DriverAcceptanceFrame, DriverError>;
    async fn inspect(&self, query: DriverStateQuery) -> Result<DriverState, DriverError>;
}
```

`RuntimeDriver` implementation 可以内部使用一个或多个 connector，但 connector 的形态不会泄漏到上层。driver interface 也不直接暴露每个业务动作的方法；稳定 typed command union 承担扩展，避免每加入一个 operation 就修改所有 factories。

## 四、目标 module 与 seam

### 4.1 总体依赖图

```text
HTTP / AgentRun use cases / Workflow
              |
              v
      AgentRuntimeGateway interface
              |
  +-----------+-----------------------------------+
  | Agent Runtime Integration Host implementation |
  | discovery / policy / negotiation / binding    |
  | command routing / journal / recovery / audit  |
  +-----------+------------------+----------------+
              |                  |
       compiled factory      internal persistence ports
              |
      RuntimeDriver adapter
       /       |         \
 Native      Codex      RemoteRuntime
 Driver      Driver        Driver
   |           |             |
Business   app-server     Relay/WebSocket/gRPC
Agent Core connector      connector
```

### 4.2 Application / AgentRun 外部 interface

application 只依赖 `agentdash-agent-runtime-contract` 中的 types 和 gateway port：

```rust
#[async_trait]
pub trait AgentRuntimeGateway: Send + Sync {
    async fn discover(
        &self,
        query: RuntimeDiscoveryQuery,
    ) -> Result<Vec<RuntimeOffer>, RuntimeGatewayError>;

    async fn bind(
        &self,
        request: BindRuntimeSession,
    ) -> Result<BindingAcceptance, RuntimeGatewayError>;

    async fn submit(
        &self,
        command: RuntimeCommandEnvelope,
    ) -> Result<CommandAcceptance, RuntimeGatewayError>;

    async fn inspect(
        &self,
        query: RuntimeInspectionQuery,
    ) -> Result<RuntimeInspection, RuntimeGatewayError>;
}
```

interface 的完整语义包括：

- `discover` 返回 policy 过滤、协商完成、带证据的 effective offer，不返回 integration 自报的原始 capability。
- `bind` 承担 new/resume/fork 三种互斥 binding intent；返回 acceptance，不把“native process 已启动”误当成 durable session 已绑定。
- `submit` 只接收绑定后的 typed command；accepted command 最终必须在 authoritative journal 中出现一个 command terminal。
- `inspect` 读取 Host 的 durable binding/runtime view，不直接窥探 driver 内存。
- authoritative runtime event 由 Host 注入的 `RuntimeEventJournalPort` 先持久化再发布；application projection 消费 journal，不从 gateway 订阅一个可丢 live broadcast。

四个动作之所以比一个万能 `execute(Value)` 更深，是因为它们分别守住 discovery、ownership establishment、mutation 和 observation 的不同不变量；再缩成一个方法只会把 ordering/error mode 藏进 untyped payload。

### 4.3 Integration 管理 interface

Integration 的安装是编译行为；运行期管理只操作 instance：

```rust
#[async_trait]
pub trait AgentRuntimeIntegrationAdmin {
    async fn definitions(&self) -> Result<Vec<IntegrationDefinitionView>, IntegrationAdminError>;
    async fn put_instance(&self, change: PutIntegrationInstance) -> Result<IntegrationInstanceView, IntegrationAdminError>;
    async fn activate(&self, request: ActivateIntegrationInstance) -> Result<ActivationReport, IntegrationAdminError>;
    async fn deactivate(&self, request: DeactivateIntegrationInstance) -> Result<DeactivationReport, IntegrationAdminError>;
}
```

该 interface 与 AgentRun seam 分开。普通 AgentRun use case 看不到 config JSON、credential slot 或 factory key；管理 API 也不能借此直接发送 turn command。

### 4.4 Host implementation 隐藏的内部 seams

以下 interface 只存在于 Host implementation 内部，不进入 application contract：

- `IntegrationDefinitionRegistry`：bootstrap 时收集编译注册项。
- `IntegrationInstanceRepository`：instance revision/CAS、desired/observed state。
- `CredentialBroker`：按 slot 和调用目的解析 secret，记录审计。
- `RuntimeDriverFactoryRegistry`：按 factory key 找已编译 factory。
- `RuntimeSessionBindingRepository`：binding、lease、generation、native coordinates。
- `RuntimeEventJournalPort`：authoritative append、dedupe、per-session sequence。
- `ContextCheckpointPort`：snapshot/head/compaction 原子提交。
- `DriverSupervisor`：进程、连接、health、restart fencing。
- `RuntimePolicyEvaluator`：tenant/workspace/agent/profile/credential/tool policy。

这些 internal seams 有 production/test adapter 的真实变化理由，但不会因测试需要而污染外部 interface。

## 五、Integration definition、instance 与 manifest

### 5.1 编译期 Definition manifest

每个受信 Integration 在 bootstrap 贡献 immutable definition：

```yaml
apiVersion: agentdash.io/runtime-integration/v1
kind: AgentRuntimeIntegrationDefinition
metadata:
  id: agentdash.remote-runtime
  version: 1.0.0
spec:
  factoryKey: remote-runtime
  runtimeContract:
    supported: ["2.0"]
  configSchema: schemas/remote-runtime-config.schema.json
  credentialSlots:
    - key: endpoint-auth
      kind: bearer-or-mtls
      required: true
      purpose: runtime-transport
  hostPermissions:
    network: [configured-endpoint]
    processSpawn: false
    workspaceRead: mediated
  declaredExecutorDiscovery: dynamic
```

manifest 是身份、schema 和宿主权限声明，不是任意代码加载说明。`factoryKey` 必须在已编译 registry 中存在；不存在即 bootstrap/configuration error，绝不从 manifest 路径加载 DLL、脚本或 WASM。

内置 definitions 至少包括：

- `agentdash.native-business-agent`
- `agentdash.codex-app-server`
- `agentdash.remote-runtime`

企业需要自定义 host 深集成时，可以在私有编译期 Integration crate 中增加 definition + factory，不修改 AgentDashboard core。

### 5.2 运行期 Instance

```rust
struct AgentRuntimeIntegrationInstance {
    id: IntegrationInstanceId,
    definition: IntegrationDefinitionRef, // id + exact version + build digest
    tenant_scope: TenantScope,
    display_name: String,
    config: JsonObject,                    // 通过 definition schema 校验
    credentials: BTreeMap<CredentialSlot, CredentialRef>,
    placement: RuntimePlacement,
    policy_binding: RuntimePolicyBinding,
    desired_state: InstanceDesiredState,
    observed_state: InstanceObservedState,
    revision: InstanceRevision,
}
```

规则：

- `config` 不允许内嵌 secret；敏感字段只能是 definition 声明过的 `CredentialRef`。
- 更新使用 `expected_revision` CAS；activation 总是针对一个 immutable instance revision。
- `definition` 精确固定到二进制内 definition build digest，不能因显示版本相同而静默换 factory 行为。
- instance 的 endpoint、workspace scope、credential、placement 和 policy 可以变化，但已绑定 session 不自动漂移到新 revision。
- 若不允许多 generation 并存，管理员必须先 drain session 再 activation；不能给 live session 暗中切换 driver。

### 5.3 Schema 与凭据

definition 提供三类 schema：

1. `config_schema`：非 secret、可持久化、可审计的连接与行为配置。
2. `credential_slots`：secret 类型、用途、required/optional、允许的 delivery mechanism。
3. `instance_policy_schema`：该 runtime 可接受的文件、网络、tool、model、approval policy 上限。

UI hint 与 validation schema 分离，避免把展示字段当成执行合同。Credential broker 返回短生命周期、目的限定的 credential lease；remote connector 可由宿主注入 header/mTLS，driver 不一定获得 secret 原文。审计事件只记录 credential ref/version 和用途，不记录值。

### 5.4 Driver factory 与“加载”

factory 是编译期 Integration interface：

```rust
trait AgentRuntimeDriverFactory: Send + Sync {
    fn definition_id(&self) -> IntegrationDefinitionId;

    async fn create(
        &self,
        activation: ActivatedInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn RuntimeDriver>, DriverLoadError>;
}
```

bootstrap 流程：

```text
compiled Integration list
  -> register immutable definitions
  -> register factory by definition_id/factory_key
  -> reject duplicate identity or schema digest
```

instance activation 流程：

```text
load instance revision
  -> validate config/schema/credential bindings/policy
  -> locate compiled factory
  -> factory.create(...)
  -> driver.negotiate(...)
  -> verify conformance evidence
  -> persist effective descriptor + activation generation
  -> mark instance Active
```

这里的“load”是选择一个已编译 factory 并连接/启动它管理的 runtime，不是加载第三方宿主代码。通用 remote factory 可以连接任何实现 Wire Protocol 的企业 Agent；Codex factory 可以启动固定 app-server child process；native factory 可以创建 in-process Business Agent Runtime。

## 六、协议版本与协商

### 6.1 五类版本必须分离

| 版本 | 作用 |
| --- | --- |
| Integration definition version | 受信 factory interface/manifest 的发布版本 |
| Manifest schema version | definition/instance 配置文档结构 |
| Agent Runtime Contract version | command/response/event/error 的平台语义 |
| Driver adapter version | 某个 native/Codex/remote adapter implementation revision |
| Vendor protocol version | Codex app-server 或企业 Agent 自有 native protocol |

不能再让 Rust Codex DTO 版本、npm app-server 版本和 AgentDash contract 版本隐式绑在一起。

### 6.2 Activation negotiation

Host 发送：

```rust
struct RuntimeHostHello {
    contract_versions: Vec<RuntimeContractVersion>,
    required_core_semantics: CoreSemanticRevision,
    host_features: HostFeatureSet,
    context_formats: Vec<ContextSnapshotFormat>,
    max_frame_bytes: u64,
    instance_revision: InstanceRevision,
    policy_digest: PolicyDigest,
    nonce: NegotiationNonce,
}
```

driver 返回：

```rust
struct DriverHello {
    selected_contract: RuntimeContractVersion,
    driver_identity: DriverIdentity,
    native_protocol: Option<NativeProtocolDescriptor>,
    executors: Vec<AdvertisedExecutor>,
    extension_namespaces: Vec<ExtensionNamespaceDescriptor>,
    limits: RuntimeLimits,
    nonce_proof: NegotiationNonceProof,
}
```

Host 不直接信任 advertised capability。先计算service自身保证，再在binding时与placement transport和host policy取交集：

```text
ServiceGuarantee = DeclaredByServiceDefinition
                 ∩ NegotiatedByServiceDriver
                 ∩ CertifiedByConformance

BoundProfile = ServiceGuarantee
             ∩ PlacementTransportGuarantee
             ∩ HostPolicy
```

Integration definition限制service driver可以发布的上限，instance policy限制host允许的上限。任一 required profile 不满足，activation 或 session binding 明确失败；禁止静默降级到弱语义。major version 无交集同样失败。minor feature 只在双方声明并通过 conformance 时启用。

### 6.3 Session 级 binding negotiation

activation descriptor 说明“instance 通常能做什么”，binding negotiation 还要结合具体 executor、模型、workspace、credential、context mode 与 placement，产出 immutable `EffectiveRuntimeDescriptor`。descriptor 有稳定 digest；session binding 固定该 digest。

若 runtime 的模型或 native session 导致能力变化，driver 必须在 bind 前返回差异。session 已 Active 后只能通过 typed `DescriptorChanged` 提议重协商；Host fence 当前 binding 并由 application 做明确迁移决策，不能继续用旧假设执行命令。

## 七、支持层级、Profile 与 Guarantee

### 7.1 线性 conformance level

level 表达 AgentConnector / RuntimeDriver 到 managed runtime 的最低能力阶梯：

| Level | 名称 | 最低保证 | AgentRun 可否直接选择 |
| --- | --- | --- | --- |
| L0 | `ConnectorOnly` | 只建立 transport/frame channel，没有 AgentDash lifecycle | 否 |
| L1 | `TurnInvocation` | typed session bind、turn start、exactly-one terminal、typed error；context 由平台每 turn提供 | 是，限简单执行 |
| L2 | `InteractiveTurn` | L1 + item lifecycle、approval round-trip、steer/interrupt correlation、tool revision ack | 是 |
| L3 | `StatefulConversation` | L2 + native resume/fork/read、durable native ID binding、restart后明确恢复语义 | 是 |
| L4 | `ManagedContext` | L3 + snapshot contract、context fidelity、two-phase compaction/checkpoint、平台可验证恢复 | 是，满足完整业务 Agent |

高 level 包含低 level 的 invariant，但不意味着支持所有正交特性；例如 image input、parallel turns、specific tool transport 用 profile 表达。

L0 正是 `AgentConnector` 所在层；当前把 ConnectorCapabilities 放在 L0 facade 上的做法无法证明 L1-L4 语义。

### 7.2 正交 capability profile

建议提供 versioned profile bundle：

- `agentdash.chat.v1`：single active turn、text/structured content、terminal guarantee。
- `agentdash.interactive-tools.v1`：item、approval、steer、interrupt、tool revision。
- `agentdash.conversation.v1`：resume/fork/read 与 durable native binding。
- `agentdash.managed-context.v1`：normalized snapshot、projection-preserving compaction。
- `agentdash.enterprise-remote.v1`：end-to-end correlation、reconnect、backpressure、audit source、transport fencing。
- `agentdash.multimodal.v1`：image/file/structured input 的声明与限额。

profile 是 conformance suite 的命名集合，不是 driver 自定义字符串。企业自有 extension profile 必须使用 namespaced id，例如 `corp.example.research-index.v1`，且不能改变 core lifecycle invariant。

### 7.3 Capability 不再是 bool

```rust
struct Capability<TGuarantee, TConstraint> {
    availability: Availability,
    semantics: CapabilitySemantics,
    guarantee: TGuarantee,
    constraints: TConstraint,
    evidence: CapabilityEvidence,
}

enum CapabilitySemantics {
    PlatformManaged,
    DriverNative,
    HostAdapted,
}

enum Availability {
    Supported,
    Conditional { condition_code: String },
    Unsupported { reason_code: String },
}
```

`HostAdapted` 只有在 adapter 通过同一 conformance、能够保持目标 guarantee 时才成立；不能把 process kill 包装成 protocol interrupt，也不能把 opaque native compaction包装成 projection commit。

### 7.4 EffectiveRuntimeDescriptor 示例

```rust
struct EffectiveRuntimeDescriptor {
    descriptor_id: RuntimeDescriptorId,
    descriptor_digest: DescriptorDigest,
    integration_instance: IntegrationInstanceId,
    service_instance: AgentServiceInstanceId,
    service_provenance: AgentServiceProvenance,
    placement: RuntimePlacement,
    executor: RuntimeExecutorId,
    activation_generation: DriverGeneration,
    contract_version: RuntimeContractVersion,
    conformance_level: RuntimeConformanceLevel,
    profiles: BTreeSet<CapabilityProfileId>,

    conversation: ConversationCapabilities,
    turn: TurnCapabilities,
    items: ItemCapabilities,
    context: ContextCapabilities,
    compaction: CompactionCapabilities,
    approval: ApprovalCapabilities,
    steering: SteeringCapabilities,
    interrupt: InterruptCapabilities,
    tools: ToolCapabilities,
    extensions: Vec<ExtensionOperationDescriptor>,
    limits: RuntimeLimits,
    certification: ConformanceEvidence,
}
```

descriptor 是 discovery、binding 和审计的核心事实；前端展示与 application gate 都消费同一生成 contract，不再维护手写缺字段的 capability type。

## 八、typed ID 与持久 Session Binding

### 8.1 ID 命名空间

```rust
struct RuntimeSessionId(Uuid);       // AgentDash canonical session
struct RuntimeTurnId(Uuid);          // AgentDash canonical turn
struct RuntimeItemId(Uuid);          // AgentDash canonical item
struct RuntimeCommandId(Uuid);
struct RuntimeApprovalId(Uuid);
struct RuntimeSnapshotId(Uuid);
struct RuntimeCompactionId(Uuid);
struct RuntimeSessionBindingId(Uuid);

struct DriverSessionId(OpaqueId);     // 只属于 adapter/native source
struct DriverTurnId(OpaqueId);
struct DriverItemId(OpaqueId);
```

平台 ID 与 driver ID 绝不复用 String 字段。application command 只接受 platform IDs；driver coordinates 由 Host 根据 binding 翻译，只在 diagnostics/source coordinates 中出现。

### 8.2 Binding 数据模型

```rust
struct RuntimeSessionBinding {
    id: RuntimeSessionBindingId,
    session_id: RuntimeSessionId,
    instance_id: IntegrationInstanceId,
    instance_revision: InstanceRevision,
    service_instance_id: AgentServiceInstanceId,
    service_provenance: AgentServiceProvenance,
    placement: RuntimePlacementBinding,
    executor_id: RuntimeExecutorId,
    descriptor_digest: DescriptorDigest,
    driver_generation: DriverGeneration,
    driver_session_id: Option<DriverSessionId>,
    state: BindingState,
    lease_epoch: BindingLeaseEpoch,
    runtime_revision: RuntimeRevision,
    context_head: Option<ContextHeadRevision>,
    tools_revision: Option<ToolsRevision>,
}
```

`BindingState` 至少包含 `Pending / Active / Suspending / Lost / Closed / Failed`。session→instance/executor 的 binding 是 durable fact，不允许 Composite 在 cancel 时广播猜 owner。

### 8.3 Bind intent

```rust
enum BindRuntimeSessionIntent {
    Open { initial_context: ContextBootstrap },
    Resume { snapshot: RuntimeSnapshotRef, expected_fidelity: SnapshotFidelity },
    Fork { parent: RuntimeSessionId, at: ForkPoint },
}
```

Open、Resume、Fork 互斥。`follow_up_session_id: Option<String>` 不能继续同时暗示 resume/fork。

## 九、Command / Response / Event / Error contract

### 9.1 Command envelope

```rust
struct RuntimeCommandEnvelope {
    command_id: RuntimeCommandId,
    idempotency_key: IdempotencyKey,
    binding_id: RuntimeSessionBindingId,
    expected_binding_epoch: BindingLeaseEpoch,
    expected_runtime_revision: Option<RuntimeRevision>,
    deadline: Option<Timestamp>,
    actor: RuntimeActor,
    command: RuntimeCommand,
}

enum RuntimeCommand {
    StartTurn(StartTurn),
    SteerTurn(SteerTurn),
    InterruptTurn(InterruptTurn),
    ReadConversation(ReadConversation),
    CompactContext(CompactContext),
    ResolveApproval(ResolveApproval),
    ApplyToolsRevision(ApplyToolsRevision),
    UpdateRuntimeSurface(UpdateRuntimeSurface),
    CloseSession(CloseSession),
    InvokeExtension(InvokeExtensionOperation),
}
```

`InvokeExtensionOperation` 是受控 escape hatch：operation id 必须 namespaced，request/response/event schema 在 negotiated descriptor 中声明并验证。core application 不可根据 extension payload 推断 session/turn/compaction 事实；需要影响核心状态机的能力必须先升级 Runtime Contract。

### 9.2 Acceptance 与完成分离

```rust
enum CommandAcceptance {
    Accepted {
        command_id: RuntimeCommandId,
        accepted_revision: RuntimeRevision,
    },
    Rejected {
        command_id: RuntimeCommandId,
        error: RuntimeError,
    },
}
```

accepted 只表示 Host/driver 已接管命令，不表示操作完成。每个 `Accepted` 最终必须在 journal 出现恰好一个：

```rust
CommandCompleted { command_id, result }
CommandFailed    { command_id, error }
```

同步速度不能改变 application receipt 类型；删除现有 compact command 通过 750ms轮询决定 `Completed` 或 `Launched` 的竞态语义。

### 9.3 Authoritative event

```rust
struct RuntimeEventEnvelope {
    event_id: RuntimeEventId,
    session_id: RuntimeSessionId,
    binding_id: RuntimeSessionBindingId,
    sequence: RuntimeEventSequence,
    runtime_revision: RuntimeRevision,
    command_id: Option<RuntimeCommandId>,
    coordinates: RuntimeCoordinates,
    source: RuntimeEventSource,
    event: RuntimeEvent,
}

enum RuntimeEvent {
    SessionBound(SessionBound),
    SessionStateChanged(SessionStateChanged),
    TurnAccepted(TurnAccepted),
    TurnStarted(TurnStarted),
    TurnTerminal(TurnTerminal),
    ItemStarted(ItemStarted),
    ItemUpdated(ItemUpdated),
    ItemTerminal(ItemTerminal),
    ApprovalRequested(ApprovalRequested),
    ApprovalResolved(ApprovalResolved),
    ToolsRevisionApplied(ToolsRevisionApplied),
    ContextSnapshotPublished(ContextSnapshotPublished),
    CompactionStarted(CompactionStarted),
    CompactionPrepared(CompactionPrepared),
    CompactionCommitted(CompactionCommitted),
    CompactionNoop(CompactionNoop),
    CompactionFailed(CompactionFailed),
    CommandCompleted(CommandCompleted),
    CommandFailed(CommandFailed),
    RuntimeLost(RuntimeLost),
    ProtocolViolation(ProtocolViolation),
    Extension(ExtensionRuntimeEvent),
}
```

authoritative event 使用 journal append + dedupe + per-session monotonic sequence。token delta、debug log、resource telemetry 可以走独立 best-effort observer channel；不能把 terminal、approval、compaction 或 binding 放在可丢 broadcast 中。

### 9.4 Typed error

```rust
struct RuntimeError {
    code: RuntimeErrorCode,
    scope: RuntimeErrorScope,
    retryability: Retryability,
    terminality: ErrorTerminality,
    safe_message: String,
    source: RuntimeErrorSource,
    source_code: Option<String>,
    redacted_details: Option<JsonValue>,
    coordinates: Option<RuntimeCoordinates>,
}

enum RuntimeErrorCode {
    InvalidInput,
    UnsupportedCapability,
    PolicyDenied,
    AuthenticationRequired,
    NotFound,
    StaleRevision,
    BindingFenced,
    Busy,
    DeadlineExceeded,
    Cancelled,
    DriverUnavailable,
    TransportFailure,
    ProtocolViolation,
    SnapshotFidelityInsufficient,
    CompactionCommitConflict,
    PersistenceFailure,
    NativeFailure,
    Internal,
}
```

driver adapter 必须保留 native code/data 并在 seam 内归一化；禁止通过错误 message 字符串猜 retryability。认证和 policy 错误不会伪装成 connector unavailable。

## 十、Context ownership、Snapshot fidelity 与 Compaction

### 10.1 Context ownership

```rust
enum ContextOwnership {
    PlatformOwned,
    SharedCheckpointed,
    DriverOwned,
}
```

- `PlatformOwned`：平台持有 canonical model-visible context，每 turn可完整 materialize；native Business Agent 的持久事实应采用此模式。
- `SharedCheckpointed`：driver 有 live state，但必须按 negotiated format export/import snapshot，并接受 platform checkpoint fencing。
- `DriverOwned`：平台只能绑定 opaque native session；不能承诺平台 resume/fork/managed compaction。

ownership 不是优劣评级，而是决定哪些 command 合法。application 可以要求 profile；Host 负责匹配。

### 10.2 Snapshot fidelity

```rust
enum SnapshotFidelity {
    ExactNormalized,     // AgentDash normalized content/roles/refs/tools可无损往返
    ReplayEquivalent,    // 重放后模型可见语义等价，native隐藏状态不保证
    ProviderVisibleOnly, // 只保证下一次 provider request可见内容
    OpaqueNative,        // 只能交还同一driver/native session
    Unsupported,
}
```

descriptor 还要声明 snapshot 包含：system/developer/user/additional-context channels、attachments、tool call/result correlation、message refs、tool revision、model/provider metadata 以及大小上限。单纯返回 summary 不能宣称 `ReplayEquivalent`。

### 10.3 Platform-preserving compaction 的两阶段提交

L4 managed compaction 必须执行：

```text
CompactContext(command, expected_context_head)
  -> driver/runtime evaluates candidate
  -> CompactionPrepared(
         compaction_id,
         based_on_snapshot,
         replacement_snapshot,
         boundary,
         provenance,
         activation_token)
  -> Host validates fidelity/boundary/revision
  -> ContextCheckpointPort atomic CAS:
         record + replacement segments + new head + request terminal
  -> Host sends ActivateCompaction(checkpoint_id, activation_token)
  -> driver swaps live context
  -> CompactionCommitted(checkpoint_id, new_head)
  -> CommandCompleted
```

并发或失败规则：

- `Prepared` 之前不能修改 canonical durable head。
- durable commit 成功之前，driver 不能把 replacement 激活为下一次 provider request 的 live context。
- CAS 冲突产生 `CompactionCommitConflict`；candidate 作废，不做隐式重试。
- durable commit成功、activation失败时 binding 进入 `Suspending/Lost`，只能从已提交 checkpoint明确恢复；不回滚 durable fact，也不假装当前 live state正确。
- 同一 context head 只能有一个 committed successor。
- manual request 的完成由同一个 command/event lifecycle投影得出，不再有 delegate/eventing 多 writer。

### 10.4 Native opaque compaction

Codex 等 driver 如果只报告 native `thread/compacted`，descriptor 应声明：

```text
context.owner = DriverOwned
compaction.semantics = DriverNative
compaction.snapshot_fidelity = OpaqueNative
compaction.platform_projection_commit = Unsupported
```

该事件只能映射为 executor telemetry 或 namespaced native operation，不能产生 `CompactionCommitted`，也不能让 common manual compact command成功。只有 Codex protocol extension 返回 snapshot、boundary、provenance 并通过 L4 conformance 后，才可升级为 managed-context profile。

## 十一、Approval、Steer、Interrupt 与 Tools

### 11.1 Approval

`ApprovalRequested` 必须含 `RuntimeApprovalId`、turn/item、请求类型、风险摘要、允许响应集合、deadline、policy context digest。`ResolveApproval` 使用 expected unresolved revision；重复同一响应幂等，不同响应返回 conflict。

adapter 不能自动接受 native request 后又宣称 round-trip approval。若某个 runtime 按预配置 policy自动裁决，descriptor 必须声明 `ApprovalMode::DriverPolicyOnly`，且不满足 interactive approval profile。

### 11.2 Steering

`SteerTurn` 必须携带 canonical `RuntimeTurnId`、expected turn revision 与 structured input。Host 根据 binding 映射到 driver turn id。只有 active turn 可接受 steer；terminal 后返回 typed `NotFound/Conflict`，不能路由到“第一个 live connector”。

descriptor 明确：

- steer 是 queue at boundary、immediate provider injection 还是 native best-effort；
- 是否支持 attachment/tool/context frame；
- accepted steer 是否有 applied event。

common `agentdash.interactive-tools.v1` 要求 `SteerApplied` 或 terminal 明确吸收/拒绝，不接受 silent success。

### 11.3 Interrupt 与 Cancel

application 的业务 cancel 可以编排多个对象，但 runtime operation 是 `InterruptTurn`：

```text
InterruptTurn accepted != TurnTerminal
```

driver 必须最终产生 exactly-one `TurnTerminal { Interrupted | Completed | Failed | Lost }`。process kill 只能作为 supervisor 的故障处置；若没有 protocol-level terminal guarantee，capability只能声明 `ProcessAbort` 弱语义，不能满足 L2。

### 11.4 Tools 与 runtime surface

tools 使用 immutable `ToolsRevision`：

```rust
ApplyToolsRevision {
    expected_current: Option<ToolsRevision>,
    next: ToolSurfaceSnapshot,
    effective_at: ToolRevisionBoundary,
}
```

driver 返回 acceptance 后必须发 `ToolsRevisionApplied`，其中包含真实 applied revision。若 driver 不支持 hot update，descriptor 可要求 `NextSessionOnly`；Host 在 active session直接拒绝，而不是记录成功但不生效。

context frames、permission/admission、MCP/VFS refs 和 tool schema 统一属于 `RuntimeSurfaceSnapshot`，有 revision/digest；relay 必须原样承载，不再把它们缩成 prompt/env/MCP 子集。

## 十二、调用顺序与并发不变量

### 12.1 Instance activation

```text
Admin put_instance(expected_revision)
  -> schema + credential slot + policy validation
  -> persist Desired=Active revision
Admin activate(instance_revision)
  -> compiled factory create driver
  -> protocol negotiate
  -> conformance evidence verification
  -> descriptor/policy/transport intersection
  -> persist activation generation + offers
  -> Observed=Active
```

失败不会落半个 Active instance。activation report 保留 typed phase/error；secret 不进入 report。

### 12.2 Session bind

```text
AgentRun select RuntimeOffer(descriptor_digest)
  -> Host revalidate instance revision/policy/offer
  -> reserve Pending binding + lease epoch
  -> driver Open | Resume | Fork with idempotency key
  -> receive native coordinates
  -> atomic persist Active binding + SessionBound event
  -> publish journal cursor
```

如果 driver 成功而 binding commit失败，Host 使用相同 idempotency key查询/关闭 orphan native session，并把 binding记为 Failed；不能返回一个无 durable owner 的 session。

### 12.3 Turn

```text
StartTurn(expected binding/runtime/tools/context revisions)
  -> Host policy + capability + revision check
  -> journal TurnAccepted / command accepted
  -> route exactly once to bound driver generation
  -> driver TurnStarted / Item... / TurnTerminal
  -> Host validate state machine + append sequence
  -> CommandCompleted or CommandFailed
```

### 12.4 全局不变量

1. 一个 `RuntimeCommandId` 只有一个 acceptance 和一个 terminal；同 idempotency key重试返回同结果。
2. 一个 accepted turn 最终恰好一个 `TurnTerminal`；EOF before terminal 转为 `Lost`，不是 `Completed`。
3. 默认 profile 同一 session 同时最多一个 active turn；支持并行的 runtime 必须声明 branch isolation 和最大并发，并使用不同 turn IDs。
4. 事件 per session 严格单调；driver generation、binding epoch 或 descriptor digest不匹配的迟到事件被 quarantine并记录 protocol violation。
5. session 同一时刻只有一个 active binding lease owner；rebind 前必须 fence旧 generation。
6. definition/instance upgrade 不改变 live binding；明确 suspend/drain/rebind 后才使用新 generation。
7. approval 一次 resolve；tool revision 单调；steer只针对 active canonical turn；interrupt ack不等于terminal。
8. compaction candidate基于明确 snapshot/head，commit使用CAS，commit前不激活live replacement。
9. authoritative event先 durable append，再对 projection/UI fanout；observer channel丢失不影响事实。
10. extension event不能伪造 core RuntimeEvent discriminant或推进核心 projection。

## 十三、Native、Codex、Relay 与企业 Agent adapters

### 13.1 Native Business Agent Driver

该 adapter 是 L4 reference implementation：

- Integration definition：`agentdash.native-business-agent`。
- factory 编译期创建 Business Agent Runtime；后者组合干净 Agent Core、context materializer、compaction policy、tool/admission 和 checkpoint port。
- context 为 `PlatformOwned`，snapshot 为 `ExactNormalized`。
- Core只返回 provider/tool loop outcome，不知道 AgentRun repository、Lifecycle URI或 Codex DTO。
- compaction使用两阶段 prepared/commit/activate；Core live cache是可丢弃cache，不是恢复事实源。

它承担 conformance suite 的强语义基准，而不是让其他 driver模仿内部 implementation。

### 13.2 Codex App Server Driver

该 adapter 隐藏 vendor JSON-RPC 和 DTO：

- 正确映射 thread start/resume/fork/read、turn start/steer/interrupt、approval response。
- structured user input、system/developer/additional context按 descriptor channel传递，不拍平成单个 user text。
- binding持久化 AgentDash IDs ↔ Codex thread/turn/item IDs。
- vendor error code/data映射到 typed RuntimeError并保留 redacted source details。
- 未扩展 snapshot/compaction protocol前，最多达到 L3 + `OpaqueNative` compaction；不能宣称 L4。
- 若项目扩展 Codex App Server Protocol返回 normalized snapshot/replacement provenance，可通过新的 conformance revision升级L4，不需要改 application seam。

### 13.3 Remote service proxy 与 Relay placement transport

Relay 是 placement transport的 `AgentConnector` adapter，不是Agent service Integration、不是executor，也不是第二套 application runtime。内置 `remote-runtime` Integration只提供cloud侧的 `RemoteServiceProxyDriver`；它代理local registry已发布的真实service/driver，并在Relay上承载同一versioned command/response/event frame：

```text
local Integration registry
  -> publishes AgentServiceDescriptor(service provenance + service guarantee)
  -> local Pi / Codex / enterprise service driver

cloud Integration Host
  -> persists same AgentServiceProvenance + RuntimePlacement::Remote
  -> RemoteServiceProxyDriver
  -> RelayConnector / direct WebSocket/gRPC placement connector
  -> same remote service driver
```

必须端到端保留：service provenance、command id、binding/session/turn/item coordinates、expected revisions、descriptor digest、runtime surface、context snapshot、approval/tools/compaction和terminal。local 不重新运行 cloud 的 launch planner，不创建第二套无映射 application session；cloud也不能把远端service重新命名为 `relay` executor。

最终bound profile严格等于 `service guarantee ∩ placement transport guarantee ∩ host policy`。例如service自身满足L4，但Relay不能提供有序重放/ack时，该remote placement不得激活 `enterprise-remote.v1` 或依赖可靠two-phase commit的profile；这不会反向降低同一service在local placement上的保证。

### 13.4 企业自研 Agent

推荐路径：

1. 企业 Agent 实现 Runtime Wire Protocol server，并由部署端local Integration registry发布自己的 `AgentServiceDefinition`、provenance和service guarantee。
2. 管理员创建 `agentdash.remote-runtime` connection instance，配置remote host/endpoint、credential refs、workspace/tenant policy；Relay只作为可选placement connector。
3. activation完成protocol negotiation，cloud镜像同一service provenance并发现executor offers，而不是生成remote-runtime/relay名下的伪service。
4. conformance kit 在企业 Agent CI和部署验收中运行；Host只启用已认证且本次协商仍满足的profile。
5. AgentRun按包含service provenance与remote placement的effective descriptor选择和绑定。

只有需要宿主 Auth、特殊 mount、专用进程监管或非标准企业设施时，部署方才新增受信编译期 Integration crate。即使如此，它仍应输出同一 RuntimeDriver contract。

## 十四、Versioning、Conformance 与 Isolation

### 14.1 Versioning 规则

- Runtime Contract major 变化不兼容；没有共同 major则 activation失败。
- minor 只追加可协商 operation/event/field，core lifecycle字段缺失仍是 protocol violation。
- extension namespace独立 version，例如 `corp.example.research-index/2`。
- persisted binding记录 contract、descriptor digest、driver generation；恢复时必须使用相同语义版本或执行明确 migration。
- config schema升级需要正式 instance data migration；项目不双写旧新字段。
- database migration至少新增 integration definitions/instances、activation generations、executor offers、runtime bindings、command receipts、event journal sequence、conformance evidence；旧 connector/session字段迁移后删除。

version negotiation 是显式合同选择，不是“能跑就跑”的fallback。未知 critical command/event直接失败；只有 descriptor声明的 non-critical telemetry extension可忽略。

### 14.2 Conformance suite

认证分三层：

1. **Static validation**：manifest、config/credential schema、operation schema、版本范围。
2. **Driver contract tests**：按 L1-L4和profile运行状态机测试。
3. **Transport loopback**：remote/relay真实frame序列、断线、重放、backpressure、late event和fencing。

最低用例：

- Open/Resume/Fork 语义互斥；
- exactly-one turn/item/command terminal；
- structured input和所有context channel；
- approval request/resolve correlation；
- steer expected turn检查；
- interrupt ack后terminal；
- tools revision ack；
- snapshot export/import fidelity；
- compaction prepare/commit/activation失败；
- stale revision、duplicate command、late generation event；
- EOF/transport loss => Lost；
- unknown critical frame => ProtocolViolation；
- relay重连与journal dedupe；
- credential redaction和policy denial。

effective descriptor中的 `ConformanceEvidence` 包含 suite revision、driver build、运行时间和通过的 profiles。driver self-report不能替代证据。

### 14.3 Isolation 模型

本方案不引入动态宿主代码沙箱，因为 Integration 本来就是受信编译期能力。隔离按执行位置处理：

| 执行形态 | 隔离 |
| --- | --- |
| Native Business Agent | 受信同进程；通过 workspace/tool/credential policy限制业务能力 |
| Codex app-server | 受监管 child process；限制工作目录、环境、network、resource、kill/reap和output size |
| Remote enterprise Agent | 网络/部署隔离；mTLS/短期token、tenant audience、rate/frame limits、audit和server identity pinning |
| Relay | transport-level auth、sequence/ack、frame size、replay protection、connection fencing |

CredentialBroker、ToolBroker、Workspace/VFS broker 是资源中介 seam；remote runtime只获得 descriptor允许的引用/短期授权。driver/connector不能自行读取数据库secret、宿主全局env或任意workspace路径。

driver失控时 supervisor可以断开/kill并记录 `RuntimeLost`；它不能伪造业务 terminal。资源隔离失败归类 `IsolationViolation/PolicyDenied`，而不是 generic runtime string。

## 十五、Application / AgentRun 的最终职责

AgentRun application 只保留：

- 产品授权、command receipt/idempotency入口；
- 根据 Agent定义、workspace policy和用户选择提出 runtime profile需求；
- 调用 `discover/bind/submit/inspect`；
- 将 Host journal投影为 AgentRun/UI read model；
- 根据 typed terminal更新产品 aggregate。

它不再负责：

- 按 connector id判断context consumption；
- 拼装 Codex/relay prompt DTO；
- 用维护 prompt模拟 compaction；
- 轮询 compaction request判断同步/异步；
- 注入 runtime delegate；
- 解析 `SessionMetaUpdate(key, Value)`；
- 广播 cancel/approve猜测session owner；
- 根据 stream EOF合成成功；
- 维护 native/executor IDs。

Business Agent Runtime 负责内部 Agent的 managed session/context/compaction语义；Integration Host负责所有 driver共享的发现、协商、binding、可靠 command/event 与策略；两者不是同一个 module。

## 十六、失败语义与运维行为

### 16.1 失败阶段

| 阶段 | 典型失败 | 对外结果 |
| --- | --- | --- |
| Definition bootstrap | duplicate id、schema/factory不一致 | 宿主启动配置失败，不能跳过 |
| Instance validation | config、credential slot、policy错误 | instance保持Inactive，typed validation report |
| Activation | transport、auth、version、conformance失败 | Observed=Failed，不发布offer |
| Bind | descriptor过期、policy变化、driver open失败、binding commit失败 | binding Failed；清理/标记orphan |
| Command rejection | capability、stale revision、busy、policy | `Rejected`，不产生accepted lifecycle |
| Accepted command failure | native error、timeout、lost、persistence | authoritative `CommandFailed`；必要时TurnTerminal/RuntimeLost |
| Event violation | bad order、duplicate terminal、wrong generation | quarantine + ProtocolViolation；核心projection不消费 |
| Compaction commit | CAS/persistence/activation失败 | 明确 conflict/failed/lost；不推进错误live state |

### 16.2 恢复不是 fallback

descriptor 声明 `RecoveryGuarantee`：Unsupported、ReconnectOnly、ResumeOpaqueNative、ResumeFromNormalizedSnapshot。Host只执行声明且conformance验证过的恢复流程；否则 binding进入Lost并要求显式新bind。它不会因某个命令失败而偷偷从Codex切到Pi，也不会把resume改成fork。

## 十七、迁移影响与硬切顺序

### 阶段 1：Runtime Contract 与 schema

- 新建项目自有 typed IDs、commands/responses/events/errors、descriptor、levels/profiles。
- 建立Rust→TS/runtime validator与wire schema生成。
- 建立L1-L4 conformance harness。
- 冻结exactly-one terminal、binding、compaction事务不变量。

完成条件：不接生产driver也能用fake driver从Gateway interface跑完整状态机。

### 阶段 2：Integration definition/instance 与 Host

- 在现有编译期 `AgentDashIntegration` seam 增加 runtime definition/factory贡献。
- 建立instance、activation generation、offer、binding、command receipt与event journal表和migration。
- 实现definition registry、credential broker、policy/descriptor intersection。

完成条件：无Pi/Codex类型的Host contract tests通过；不存在动态代码加载路径。

### 阶段 3：Native L4 reference vertical slice

- 抽出Business Agent Runtime，净化Agent Core。
- 将context construction、manual/auto compaction、checkpoint orchestration、tool/admission收进Business Runtime。
- 通过Native Driver接入Host，eventing只消费typed journal。
- 修正durable commit前不激活live compaction。

完成条件：native driver通过L4/managed-context conformance。

### 阶段 4：AgentRun 切到 Gateway

- launch、restore、manual compaction、cancel/steer/approval/tools全部改用bind/submit。
- 删除application-agentrun到concrete session runtime的bridge/port与750ms轮询。
- 删除connector id context profile判断、Composite OR capability与广播ownership。

完成条件：application生产依赖只看runtime contract/gateway和自身repository。

### 阶段 5：Codex Driver 重写

- vendor DTO限制在adapter。
- 实现native start/resume/fork/read/interrupt/approval与ID binding。
- 保留structured input/context channel。
- 先以真实L3/opaque compaction发布；若扩展协议满足snapshot contract再升L4。

完成条件：Codex driver通过其声明level/profile的全部conformance，不再过度宣称能力。

### 阶段 6：Remote Runtime / Relay 硬切

- 内置 `remote-runtime` Integration和versioned wire protocol；local registry发布Pi/Codex/企业Agent的真实service descriptor。
- cloud持久化相同service provenance与remote placement；relay只做connector/transport，不成为service/executor，也不在local重跑application session pipeline。
- 企业fake server与真实cloud→relay→local loopback通过enterprise-remote conformance。

完成条件：一个仓库外企业Agent只实现wire protocol并创建instance即可被AgentRun发现/绑定/执行。

### 阶段 7：旧模型删除与数据库收口

- 删除旧 `AgentConnector` mega trait；名称仅保留给低层transport interface，或采用更明确 `RuntimeTransportConnector`。
- 删除 `ConnectorCapabilities`、Composite routing、`RelayPromptRequest`、special compaction turn mode、string meta核心事实、Codex DTO re-export。
- 完成binding/journal/projection新表migration后删除旧字段与读写路径，不保留双格式。

## 十八、方案优劣与和最小接口方案的差异

### 18.1 优点

1. **企业扩展 leverage 最大。** 新远端Agent只实现协议与conformance，不改application/executor路由；深集成只新增编译期Integration crate。
2. **seam 稳定。** application的四个动作不随driver、transport、vendor operation增加。
3. **能力诚实。** level/profile/guarantee/evidence避免“同名能力、不同语义”。
4. **状态 locality 高。** session owner、native IDs、generation、descriptor、revision和恢复集中在Host。
5. **安全模型清楚。** 受信Integration、远端服务、Extension/Capability Pack不混为动态插件。
6. **可测试性强。** interface就是测试面；同一conformance suite可运行于native、Codex和remote/relay。
7. **协议演进不污染domain。** vendor/wire types留在adapter，核心事件与错误由项目拥有。

### 18.2 代价

1. 初始设计和migration规模明显大于最小接口；需要先做contract、journal、binding和conformance。
2. descriptor/profile体系需要严格治理，否则会变成新的自由字符串目录。
3. Host是关键深module，implementation复杂；必须防止它继续吸收Business Agent专属context/compaction policy。
4. 两阶段compaction与可靠event journal增加driver实现门槛；但这正是L4强保证的真实成本，弱driver可以诚实停在L1-L3。
5. 编译期Integration不支持任意第三方宿主代码热安装；这是canonical trust选择。免重编译扩展通过remote-runtime协议解决，而不是另造动态loader。
6. extension operation的schema escape hatch需要治理，避免企业用namespaced JSON绕过核心状态机。

### 18.3 与“最小 Runtime interface”明显不同之处

| 维度 | 最小方案倾向 | 本 Integration 方案 |
| --- | --- | --- |
| seam | 针对当前Pi/Codex抽方法 | Application-facing Host + compile-time Integration + wire protocol |
| 扩展 | 新driver改composition/executor | 编译期definition/factory，或由remote-runtime connection代理保留真实provenance的企业service |
| capability | typed enum/少量flags | level + profile + per-capability guarantee + evidence + transport intersection |
| state | application持有较多session知识 | durable binding/generation/native coordinates由Host拥有 |
| protocol | in-process trait优先 | in-process与remote共用语义合同，wire是一级设计对象 |
| 配置/凭据 | composition root手工注入 | definition schema + runtime instance + credential broker |
| 测试 | adapter单测 | 跨driver level/profile conformance + transport loopback |
| 生态 | 为现有实现收敛 | 为企业自研Agent和长期协议生态建立平台 |

本方案不是把所有逻辑做成可插拔 micro-module。相反，它用一个深Host interface隐藏大量实现，并只在已经存在真实变化的地方保留seam：受信factory、远端transport、持久化、credential和driver adapter。这符合“两个adapter才是真seam”的原则，也避免产生一串只转发调用的浅ports。

## 十九、建议决策

若团队已经确认“Agent服务直接成为可插拔Integration系统，并支持企业内部自研Agent”是长期产品方向，推荐选本方案作为目标架构，而不是把它当成最小接口方案之后的可选增强。definition/instance、binding、协议协商和conformance一旦晚做，旧connector语义会再次渗入application和数据库，届时迁移成本更高。

同时建议守住三个限制，防止“最大扩展性”演变为无约束平台：

1. Integration 继续是受信编译期扩展，不引入动态dylib/WASM宿主插件。
2. core lifecycle command/event保持closed typed contract；开放扩展只走namespaced、schema验证且不能推进核心projection的operation/event。
3. driver只能宣称conformance验证过的level/profile/guarantee；capability discovery不是自报广告。

在这些限制下，这个 seam 既能承载内部Business Agent、Codex App Server和relay，也能让企业Agent免改核心地接入，同时让application保持最小必要知识。
