# Integration Agent Runtime Driver Host

## 1. Scope / Trigger

本规范适用于受信 Integration 提供 Agent Runtime Driver、Complete Agent live attachment、sticky binding、driver lease、source coordinate、surface/hook apply gate 与 Host PostgreSQL persistence。新增 first-party 或企业 Agent service、修改 RuntimeOffer/profile 求交、attachment/generation/lease/router 或 Host-owned schema 时必须复核本规范。

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

fn is_optional_complete_agent_materialization_failure(
    error: &CompleteAgentCompositionError,
) -> bool;
```

`CompleteAgentLiveCatalog` 管理当前 Host incarnation 内的 descriptor、verification、effective offer、placement、remote mapping 与 callable handle；它以 opaque `CompleteAgentLiveAttachmentId` 解析服务，不写 PostgreSQL。`CompleteAgentHostRepository` 只保存 exact target snapshot、binding/source/generation、lease、effect 与恢复证据。Router 输入必须携带 binding/generation/attachment/incarnation/lease，不能以裸 executor 或逻辑 instance key 发现 owner。

## 3. Contracts

- Integration API只贡献immutable `AgentServiceDefinition + AgentRuntimeDriverFactory`。同一Integration可贡献多个definition，同一definition可创建多个instance；registry一次性collect后不可变，duplicate definition/factory/schema/protocol/credential定义fail fast。
- API Host启动时，optional Complete Agent的factory materialization或service describe失败只使该
  contribution保持未注册、未进入recovery selection，并写入Warn诊断；Host继续注册其它
  contribution。原因是provider运行环境或凭据不可用不改变平台核心装配的正确性。声明冲突、
  descriptor mismatch、verification、Host persistence等完整性失败继续fail fast。
- Service definition 是编译期受信元数据；Product profile 保存 config、credential scope/ref 与不可变 digest。逻辑 service instance key 只用于当前 incarnation 的 materialization 缓存和恢复兼容性匹配，不是 dispatch endpoint。
- config JSON Schema、credential slot/ref/purpose与host permission必须在factory/driver side effect前验证。Factory只能得到Scoped Credential Broker，不能借机访问definition或instance未声明的slot、ref或purpose；secret不得Serialize/Debug/日志化。
- Materialization 通过 Host verification 后 attach 到 live catalog。attachment identity 覆盖逻辑 service key、Host incarnation、placement identity 和 remote connection epoch；同一 identity + 同一 verified facts 幂等，不同 facts typed conflict，retired attachment 永久不可解析。effective profile 严格等于 service guarantee、placement transport guarantee 与 host policy 的交集；self-report 或配置存在不能提升能力。
- Runtime-owned `BoundAgentSurface`由Business Surface/admission编译；Host只保存`BoundAgentSurfaceReference`、apply evidence、Hook plan/artifact digest与per-point ack，不复制Capability Pack/ToolCatalog/Hook rules。
- RuntimeBinding 固定 `CompleteAgentBindingTarget` exact snapshot：logical instance、live attachment、definition、verified build/profile、offer profile、placement/incarnation 与 remote mapping。新 binding 只能使用 live catalog 当前可解析的 exact attachment。
- Remote attachment 只冻结 transport 侧 service identity、remote binding generation 与 connection epoch；per-thread Host binding generation 由 Host binding/recovery 单独拥有。Remote proxy 在出站边界把任意正的 Host generation 改写为 remote generation，并在 `apply_surface` 时建立 `callback route -> Host generation` 映射；反向 callback 先校验 remote target，再按 exact route 恢复 Host generation。这样一个 placement 可以服务多个 Runtime thread，也不会把注册期 generation 误当成线程生命周期 generation。
- Binding intent 先 durable Pending，再执行幂等 service bind/apply，最后原子写 Active binding 与 source coordinates。进程重启后旧 attachment 不可解析，旧 binding 保持 fenced；恢复只能选择当前兼容 attachment，创建 `previous generation + 1` 的 target/binding，并以原 effect identity inspect/reconcile 未决副作用。
- Complete Agent Create、Resume、Fork 与 surface apply 都先持久化稳定 effect intent。Agent 已返回 applied outcome 后，binding/target provision、surface receipt 或 lifecycle outcome 的 Host commit 失败只进入 `InspectionRequired/Unknown`，原因是外部副作用已经可能成立，不能据平台持久化失败宣称业务失败。重启后 Host 以同一 effect inspect 并幂等完成 provision/settlement；Fork 已知 child source/history 时，Runtime 先持久化 `ChildKnown` evidence，再单调升级为 `Provisioned`，不得更换 effect identity 或丢失已知 child。
- DriverLease使用数据库时钟、owner/token/epoch/generation。相同owner+generation在未过期时幂等返回原lease；不同owner冲突；到期takeover产生新token/epoch并fence旧owner。
- Source coordinates按binding/generation维护canonical与driver ID双向唯一。Dispatch前校验lease，event sink对每个event再次校验binding、generation、source coordinate、owner/token和DB lease，防止dispatch期间takeover后的late event推进Runtime。
- `DriverError::Terminalized`当前只表达Managed Runtime event sink已提交critical terminal后的pump flow-control，不授予Driver声明canonical终态的权力。Host/outbox consumer必须回读durable Operation/Thread/binding后决定ack或release；只有canonical已terminal/obsolete才可ack。
- `BindingLost`属于binding生命周期事件，不属于某个命令lease。Generation-fenced sink仍校验binding、generation、source coordinate与lease epoch，但即使命令lease已在上一轮结束后释放，也必须先把Lost提交到Managed Runtime，再调用`mark_binding_lost(binding_id, generation)`原子失效Host binding及残余lease。普通turn/item/interaction事件继续逐事件校验owner/token/有效期。
- Required surface/hook contribution只有在revision/digest/artifact与per-point applied ack匹配，且effective HookProfile满足actions/strength/failure policy/configuration boundary后才允许Turn dispatch。
- Live catalog 中复用的 selection 与本次 materialization 新产生的 selection 必须经过同一个完整 Surface admission 函数；attach 成功只证明 service 可用，不证明它满足当前 AgentFrame。任何路径都不得把未求交 offer 延迟到 Host bind 才发现 workspace/hook 不兼容。
- Integration profile与adapter apply acknowledgment必须由同一capability事实生成。`HostAdaptedExact` workspace要声明Host能够精确交付的最大capability；Hook failure policy必须与callback错误分支一致。Managed Runtime持久化effect不进入Driver action requirement。
- `agent_runtime_binding`/`agent_runtime_source_coordinate` 是 Managed Runtime 引用的最小 Host-owned anchor。`0089` 后 Host 表保存 exact target snapshot、binding detail、lease、effect 与完整 coordinates；live inventory 不进入 schema。Runtime repository 不写 Host authority，Host 不写 Runtime journal/projection。
- Relay是placement transport而非service identity。Native/Codex/remote service通过相同contribution/Host seam接入，不在Application/router增加service类型分支。
- Local Host每次启动生成不可复用的`HostIncarnationId`。Offer advertisement、Runtime Wire placement request与relay provenance携带该identity；Local endpoint在解析Driver前同时校验host、transport、incarnation、instance与generation，使重启前的stream和command无法命中新进程中从1重新计数的generation。
- Runtime Wire placement 断连后成为 retired connection epoch：catalog retire 旧 attachment，所有 send/frame/ack/open 永久 fence。重新连接必须以新的 transport epoch 产生新 attachment，再通过显式 recovery 提升 binding generation；不能重放旧 epoch 未确认 work。
- Local Host 的 live catalog 属于单个 process incarnation，由 Integration contribution 与 Product profile 重建；binding、effect、source、lease 与 recovery evidence 属于 durable Host repository。Managed Runtime 继续持有 canonical operation/journal/projection 与断连 Lost 裁决。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| duplicate definition/factory/schema/protocol | bootstrap fail-fast |
| optional Complete Agent factory/service materialization失败 | 记录不可用诊断，跳过该instance；Product恢复时仍必须从持久化的精确execution profile重新materialize，Host继续启动 |
| optional Complete Agent descriptor/verification/Host注册失败 | bootstrap fail-fast，不把完整性错误伪装成普通不可用 |
| config非法、credential缺失或purpose越权 | factory side effect前typed reject |
| service自报能力但无conformance evidence | offer不提升该guarantee |
| transport/host policy弱于service | effective profile按交集削弱 |
| 相同逻辑 service 跨 Host incarnation attach | 产生不同 live attachment；旧 attachment 不可被新 handle 替代 |
| stale/unhealthy/withdrawn attachment 创建新 binding | durable intent 前 typed reject |
| attachment 断连或进程退出 | 旧 binding 收敛 Lost；选择兼容新 attachment 后以更高 generation 恢复 |
| remote command 使用正的 Host generation | proxy 按 attachment 的 remote generation 出站；callback 通过 exact route 恢复原 Host generation |
| remote callback 的 target generation 或 route 未匹配 | typed stale reject；不调用 Host callback |
| binding anchor 写入后 detail constraint 失败 | 全事务回滚，不泄漏 Host anchor |
| lease未过期时不同owner claim | conflict |
| lease过期takeover后旧token dispatch/event | stale generation/lease reject |
| turn结束且命令lease已释放后driver报告BindingLost | 接受Lost，Runtime与Host binding均收敛Lost |
| Driver返回`Terminalized`但canonical Operation仍active | outbox release/no-ack；不得把adapter错误当作canonical终态 |
| Create/Resume applied 后 Host binding、surface 或 lifecycle settlement commit失败 | 保持同一effect `InspectionRequired`；重启inspect/reconcile，不终结为Failed |
| Fork applied且child已知后target/binding/final settlement commit失败 | 保持Running与`ChildKnown` evidence；同一effect恢复后单调升级`Provisioned` |
| Lost envelope的binding/generation/source不匹配 | stale generation reject，不改变Runtime或Host |
| required Hook未ack或artifact digest不符 | Turn dispatch gate拒绝 |
| 新 materialization offer 不满足当前 workspace/hook surface | Surface admission 返回 typed unavailable，不进入 Host bind |
| Driver profile声明fail-open但callback错误仍向上失败 | conformance失败，不得发布该failure policy |
| source ID跨binding/generation复用 | composite unique/FK或typed conflict |
| backend断连后以相同generation/incarnation显式冷重绑 | typed reject并保持retired；replacement offer使用新generation/incarnation后才fresh open，旧placement仍Lost且零旧frame replay |

## 5. Good / Base / Bad Cases

**Good case:** 企业Integration只注册definition/factory；Host验证instance config/credential，依据conformance生成交集offer，Runtime用offer完成surface admission，Host持久Pending binding并幂等bind，获得apply ack与lease后按sticky binding路由。

**Base case:** Host在driver.bind后崩溃，Product从持久化binding读取完整execution profile，materialize当前进程的新attachment；Host校验definition/build/profile/offer兼容后以更高generation恢复，不受同名service或启动顺序影响。

**Bad case:** Router通过`executor_id`或live-session probe选择connector并OR能力，或把Relay当service identity。这会丢失sticky ownership和generation provenance，必须由Host模型替代。

## 6. Tests Required

- Registry/Integration测试覆盖多definition、多instance、duplicate/factory/schema/protocol/credential定义与immutable collect。
- API composition测试覆盖factory/service materialization失败被隔离、后续contribution仍可注册，以及descriptor/verification/Host错误继续终止bootstrap。
- Live catalog 测试覆盖 config/credential preflight、secret 隐藏、同 incarnation 幂等、verified facts conflict、跨 incarnation 新 attachment、retire 不可逆与 evidence-backed profile intersection。
- Binding 测试覆盖 exact attachment/incarnation target、sticky/idempotent bind、stale attachment、higher-generation recovery、orphan failure、surface/hook apply gate 和 configuration boundary。
- lifecycle failpoint测试覆盖Create binding provision、surface receipt settlement、Create/Resume outcome settlement与Fork child target provision；逐项断言Agent applied后不产生确定Failed，重建Host后通过同一effect inspect收敛，Fork保留child source/history并升级provisioning evidence。
- Provision 测试分别覆盖既有 live selection 与新 materialization selection，断言二者调用同一 Surface admission；Native profile 测试逐项满足实际 platform Driver hook binding 与 VFS workspace requirements。
- Lease/source/router行为覆盖same-owner replay、DB-clock takeover、stale token、dispatch期间takeover、source双向唯一与old-generation event fencing。
- Binding生命周期测试覆盖“命令lease已释放后的BindingLost仍被接受”、Lost提交后Host binding为Lost且lease失效，以及错误binding/generation/source的Lost被fence。
- outbox composition测试覆盖Driver伪造`Terminalized`时active work仍可重领，以及canonical terminal后二次读取才ack。
- 真实 embedded PostgreSQL 覆盖 `0089` hard cut、inventory 表缺席、exact target snapshot、binding/source/effect 复合坐标、anchor rollback 与 lease 过期。
- API/Executor测试证明Integration不再贡献旧connector，Composite不再OR/broadcast/first-success；彻底删除legacy probe随WP08 cutover验证。
- Host/Integration/API/Executor/Infrastructure tests、contracts、migration guard、fmt、clippy与diff check必须通过。
- Local Host测试覆盖新incarnation重建、相同generation跨incarnation拒绝，以及无需数据库即可广告offer。
- Runtime Wire registry测试覆盖register不自动reopen、same-provenance resolver拒绝、replacement generation/incarnation建立fresh connection epoch、旧placement永久fence与abandoned frame零replay。
- Remote proxy测试覆盖同一attachment上的多个正Host generation、出站remote generation改写、`apply_surface` route映射、callback反向generation恢复，以及未知route/remote generation漂移的typed fence。

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

// Correct：动态activation与已有live selection共用同一admission
let offer = host.activate(request).await?;
ensure_offer_supports_surface(&offer, &surface)?;
host.bind(offer.id, surface).await?;
```

```rust
// Wrong: 任一可选provider的运行时不可用都会终止整个AppState构建。
let descriptor = complete_agent.register_contribution(contribution).await?;

// Correct: 只隔离materialization/describe失败；完整性与Host错误仍向上返回。
match complete_agent.register_contribution(contribution).await {
    Ok(_) => {},
    Err(error) if is_optional_complete_agent_materialization_failure(&error) => {
        diagnose_unavailable(instance_id, error);
    }
    Err(error) => return Err(error.into()),
}
```
