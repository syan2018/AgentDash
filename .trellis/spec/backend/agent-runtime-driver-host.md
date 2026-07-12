# Integration Agent Runtime Driver Host

## 1. Scope / Trigger

本规范适用于受信 Integration 提供 Agent Runtime Driver、service instance 管理、activation/offer、sticky binding、driver lease、source coordinate、surface/hook apply gate与Host PostgreSQL persistence。新增first-party或企业Agent service、修改RuntimeOffer/profile求交、generation/lease/router或0061/0064 Host-owned schema时必须复核本规范。

## 2. Signatures

```rust
pub struct AgentRuntimeDriverContribution {
    pub definition: AgentServiceDefinition,
    pub factory: Arc<dyn AgentRuntimeDriverFactory>,
}

pub trait AgentRuntimeDriverFactory: Send + Sync {
    async fn create(
        &self,
        activation: ActivatedAgentService,
        credentials: Arc<dyn AgentRuntimeCredentialBroker>,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, AgentRuntimeFactoryError>;
}

impl AgentRuntimeHost {
    pub async fn activate(...) -> Result<RuntimeOffer, AgentRuntimeHostError>;
    pub async fn bind(...) -> Result<RuntimeBinding, AgentRuntimeHostError>;
    pub async fn dispatch(...) -> Result<(), AgentRuntimeHostError>;
    pub async fn recover_pending_bindings(&self) -> Result<usize, AgentRuntimeHostError>;
}
```

`AgentRuntimeHostRepository` 提供instance revision CAS、generation reserve、activation/offer、binding/source/lease与apply receipt ports。云端Managed Host使用durable实现；Local Integration Host使用进程incarnation内的ephemeral实现。Router输入必须携带binding/generation/lease，不能以裸executor ID发现owner。

## 3. Contracts

- Integration API只贡献immutable `AgentServiceDefinition + AgentRuntimeDriverFactory`。同一Integration可贡献多个definition，同一definition可创建多个instance；registry一次性collect后不可变，duplicate definition/factory/schema/protocol/credential定义fail fast。
- Service definition是编译期受信元数据；instance保存config、credential refs、placement、desired/observed state与revision。每个instance revision必须保留immutable history，activation始终引用精确历史快照。
- config JSON Schema、credential slot/ref/purpose与host permission必须在factory/driver side effect前验证。Factory只能得到Scoped Credential Broker，不能借机访问definition或instance未声明的slot、ref或purpose；secret不得Serialize/Debug/日志化。
- Activation生成单调generation与evidence-backed RuntimeOffer。effective profile严格等于service guarantee、placement transport guarantee与host policy的交集；self-report或配置文件存在不能提升能力。
- Runtime-owned `BoundAgentSurface`由Business Surface/admission编译；Host只保存`BoundAgentSurfaceReference`、apply evidence、Hook plan/artifact digest与per-point ack，不复制Capability Pack/ToolCatalog/Hook rules。
- RuntimeBinding固定exact offer digest、instance revision、generation、profile digest和surface ref。新binding只能使用仍available且current instance仍Active/healthy的offer；已durable Pending binding可以依靠immutable旧activation snapshot恢复。
- Driver bind intent先durable Pending，再执行幂等driver.bind，最后原子写Active binding与source coordinates。崩溃后`recover_pending_bindings`用同一identity恢复；失败显式收敛Failed/Lost，不产生无owner native session。
- DriverLease使用数据库时钟、owner/token/epoch/generation。相同owner+generation在未过期时幂等返回原lease；不同owner冲突；到期takeover产生新token/epoch并fence旧owner。
- Source coordinates按binding/generation维护canonical与driver ID双向唯一。Dispatch前校验lease，event sink对每个event再次校验binding、generation、source coordinate、owner/token和DB lease，防止dispatch期间takeover后的late event推进Runtime。
- `BindingLost`属于binding生命周期事件，不属于某个命令lease。Generation-fenced sink仍校验binding、generation、source coordinate与lease epoch，但即使命令lease已在上一轮结束后释放，也必须先把Lost提交到Managed Runtime，再调用`mark_binding_lost(binding_id, generation)`原子失效Host binding及残余lease。普通turn/item/interaction事件继续逐事件校验owner/token/有效期。
- Required surface/hook contribution只有在revision/digest/artifact与per-point applied ack匹配，且effective HookProfile满足actions/strength/failure policy/configuration boundary后才允许Turn dispatch。
- inventory中复用的offer与本次`activate()`新产生的offer必须经过同一个完整Surface admission函数；activation成功只证明service可用，不证明它满足当前AgentFrame。任何路径都不得把未求交offer延迟到Host `bind()`才发现workspace/hook不兼容。
- Integration profile与adapter apply acknowledgment必须由同一capability事实生成。`HostAdaptedExact` workspace要声明Host能够精确交付的最大capability；Hook failure policy必须与callback错误分支一致。Managed Runtime持久化effect不进入Driver action requirement。
- 0061的`agent_runtime_binding`/`agent_runtime_source_coordinate`是Managed Runtime引用的最小Host-owned anchor；0064保存instance history、activation/offer、binding detail、lease和完整coordinates。Runtime repository不写Host authority，Host不写Runtime journal/projection。
- Relay是placement transport而非service identity。Native/Codex/remote service通过相同contribution/Host seam接入，不在Application/router增加service类型分支。
- Local Host每次启动生成不可复用的`HostIncarnationId`。Offer advertisement、Runtime Wire placement request与relay provenance携带该identity；Local endpoint在解析Driver前同时校验host、transport、incarnation、instance与generation，使重启前的stream和command无法命中新进程中从1重新计数的generation。
- Local Host的instance、offer、binding、lease与coordinate属于单个process incarnation，使用production ephemeral repository从Integration definitions与profile重建；Managed Runtime继续持有binding intent与断连Lost裁决。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| duplicate definition/factory/schema/protocol | bootstrap fail-fast |
| config非法、credential缺失或purpose越权 | factory side effect前typed reject |
| service自报能力但无conformance evidence | offer不提升该guarantee |
| transport/host policy弱于service | effective profile按交集削弱 |
| config rev1已激活后更新rev2 | current推进rev2，rev1 history与旧generation仍可恢复 |
| stale/unhealthy/withdrawn offer创建新binding | reserve前typed reject |
| Pending bind后offer撤回或config更新 | 按原immutable activation snapshot幂等恢复 |
| binding anchor写入后detail constraint失败 | 全事务回滚，不泄漏0061 anchor |
| lease未过期时不同owner claim | conflict |
| lease过期takeover后旧token dispatch/event | stale generation/lease reject |
| turn结束且命令lease已释放后driver报告BindingLost | 接受Lost，Runtime与Host binding均收敛Lost |
| Lost envelope的binding/generation/source不匹配 | stale generation reject，不改变Runtime或Host |
| required Hook未ack或artifact digest不符 | Turn dispatch gate拒绝 |
| 新activation offer不满足当前workspace/hook surface | `ensure_offer`返回typed unavailable，不进入Host bind |
| Driver profile声明fail-open但callback错误仍向上失败 | conformance失败，不得发布该failure policy |
| source ID跨binding/generation复用 | composite unique/FK或typed conflict |

## 5. Good / Base / Bad Cases

**Good case:** 企业Integration只注册definition/factory；Host验证instance config/credential，依据conformance生成交集offer，Runtime用offer完成surface admission，Host持久Pending binding并幂等bind，获得apply ack与lease后按sticky binding路由。

**Base case:** Host在driver.bind后崩溃，重启扫描Pending binding，从原activation revision/generation恢复driver并用相同bind identity收敛Active，不受current instance新revision影响。

**Bad case:** Router通过`executor_id`或live-session probe选择connector并OR能力，或把Relay当service identity。这会丢失sticky ownership和generation provenance，必须由Host模型替代。

## 6. Tests Required

- Registry/Integration测试覆盖多definition、多instance、duplicate/factory/schema/protocol/credential定义与immutable collect。
- Instance/activation测试覆盖config/credential preflight、secret隐藏、revision CAS/history、deactivate/reactivate/unhealthy、evidence-backed profile intersection。
- Binding测试覆盖sticky/idempotent bind、stale offer、Pending recovery、orphan failure、surface/hook apply gate和configuration boundary。
- Provision测试分别覆盖既有offer与新activation offer，断言二者调用同一Surface admission；Native profile测试逐项满足实际platform Driver hook binding与VFS workspace requirements。
- Lease/source/router行为覆盖same-owner replay、DB-clock takeover、stale token、dispatch期间takeover、source双向唯一与old-generation event fencing。
- Binding生命周期测试覆盖“命令lease已释放后的BindingLost仍被接受”、Lost提交后Host binding为Lost且lease失效，以及错误binding/generation/source的Lost被fence。
- 真实embedded PostgreSQL覆盖0061/0064 ownership、instance并发CAS、history FK、binding完整复合FK、anchor rollback、offer锁与lease过期。
- API/Executor测试证明Integration不再贡献旧connector，Composite不再OR/broadcast/first-success；彻底删除legacy probe随WP08 cutover验证。
- Host/Integration/API/Executor/Infrastructure tests、contracts、migration guard、fmt、clippy与diff check必须通过。
- Local Host测试覆盖新incarnation重建、相同generation跨incarnation拒绝，以及无需数据库即可广告offer。

## 7. Wrong vs Correct

```rust
// Wrong: application按service类型硬编码driver并动态探测owner。
let connector = match connector_kind { Pi => build_pi(), Codex => build_codex() };
connector.prompt(executor_id, request).await?;

// Correct: Integration贡献factory，Host只按durable binding与lease路由。
let contribution = integration.agent_runtime_drivers();
host.dispatch(binding_id, generation, lease, command).await?;
```

```rust
// Wrong: 用已结束命令的lease决定长期binding是否允许报告断连。
validate_lease(owner, token, now)?;
sink.emit(binding_lost).await?;

// Correct: BindingLost使用binding代际围栏，并在Runtime提交后收敛Host状态。
validate_binding_generation_source(&binding_lost)?;
runtime_sink.emit(binding_lost).await?;
repository.mark_binding_lost(binding_id, generation).await?;
```

```rust
// Wrong: factory拿到能读取所有secret的全局broker。
factory.create(activation, global_credentials).await?;

// Correct: Host按definition+instance声明构造purpose-scoped broker。
factory.create(activation, scoped_credentials).await?;
```

```rust
// Wrong：动态activation后直接bind，绕过既有offer路径的surface求交
let offer = host.activate(request).await?;
host.bind(offer.id, surface).await?;

// Correct：activation与inventory offer共用同一admission
let offer = host.activate(request).await?;
ensure_offer_supports_surface(&offer, &surface)?;
host.bind(offer.id, surface).await?;
```
