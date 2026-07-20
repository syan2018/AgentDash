# Integration Complete Agent Host

## 1. Scope / Trigger

本规范适用于 Integration 提供 Complete Agent service、live attachment、selection、surface
admission、source route、generation fencing 与 reverse callback。新增 Agent provider、修改
Host state、callback route 或 optional integration bootstrap 时必须复核。

Host 是单进程路由器，不是 durable workflow owner。其状态可从 Integration definition、
Product association 与 concrete Agent authority 重建。

## 2. Signatures

```rust
pub struct CompleteAgentHost {
    live_catalog: SharedCompleteAgentLiveCatalog,
    state: RwLock<CompleteAgentHostLiveState>,
}

struct CompleteAgentHostLiveState {
    runtime_targets: BTreeMap<RuntimeThreadId, CompleteAgentRuntimeTarget>,
    bindings: BTreeMap<CompleteAgentBindingId, CompleteAgentBinding>,
    callback_routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    lost_runtime_threads: BTreeSet<RuntimeThreadId>,
}
```

```rust
impl CompleteAgentHost {
    pub async fn attach_verified_service(...);
    pub async fn provision_runtime_target(...);
    pub async fn restore_runtime_source_route(...);
    pub async fn runtime_binding_generation(...);
    pub async fn resolve_callback_route(...);
}
```

```rust
pub trait CompleteAgentService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError>;
    async fn create(&self, command: CreateAgentCommand) -> Result<AgentCommandReceipt, AgentServiceError>;
    async fn fork(&self, command: ForkAgentCommand) -> Result<ForkAgentReceipt, AgentServiceError>;
    async fn execute(&self, command: AgentCommandEnvelope)
        -> Result<AgentCommandReceipt, AgentServiceError>;
    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError>;
    async fn inspect(&self, identity: AgentEffectIdentity)
        -> Result<AgentEffectInspection, AgentServiceError>;
    async fn apply_surface(&self, command: ApplyBoundAgentSurface)
        -> Result<AppliedAgentSurfaceReceipt, AgentServiceError>;
}
```

## 3. Contracts

- Integration 贡献稳定 definition/factory/configuration；Host materialize 后验证 descriptor 与
  offer，再把 callable service attach 到当前进程的 live catalog。
- `CompleteAgentLiveAttachmentId`、placement、incarnation、availability、target、binding、
  generation、callback route 和 lost set 全部是 process-local。
- 同一当前 attachment + 相同 verified facts 注册幂等；同 identity 但事实不同是完整性冲突。
  不同进程 incarnation 产生新 attachment，不与旧 attachment 做数据库事实合并。
- Product execution profile 与 AgentFrame 是 desired intent；service descriptor 是 Agent
  guarantee；Host 在当前进程求交得到 bound surface，并由 concrete Agent
  `apply_surface/inspect` 证明 applied。
- Host generation 只 fence 当前进程 route。Host 重启后重新从 1 建立 generation 是合法的，
  因为旧 callback route/attachment 已经不可解析。
- stable effect identity 在 Product/Agent 协议中派生；Host 不保存 create/fork/command/surface
  effect ledger。回包未知时由 concrete Agent `inspect(effect_id)` 收敛。
- callback route、deadline 与 generation 在 Host 内存校验。真实 Tool/Hook handler 使用
  invocation idempotency key 保存或重放自己的副作用 receipt。
- optional Complete Agent 的 program、credential 或 materialization 不可用，只让对应
  selection 缺席并产生诊断；核心 AppState 与其他 Agent contribution 继续启动。
- definition 冲突、descriptor/verification 不一致、surface contract 破坏等平台完整性错误继续
  fail fast，因为这些错误说明已注册事实不可信，而不是某个可选 provider 暂时不可用。
- RuntimeWire/Relay 只承载 transport。断连 retire 当前 attachment/connection epoch；重新连接
  建立新 attachment，不回放旧进程 route。

## 4. Validation & Error Matrix

| 场景 | 必须结果 |
| --- | --- |
| optional program/credential/materialization 缺失 | typed unavailable diagnostic；应用继续启动 |
| duplicate definition 或 verified facts 冲突 | fail fast |
| target 引用非 live attachment | typed unavailable/rejected |
| desired surface 超出 verified offer | side effect 前 typed incompatible |
| 相同 live target + surface 重复 provision | 返回当前 target |
| 当前 target 未 Lost 却请求不同 target/surface | provisioning conflict |
| surface rebind expected generation 过期 | stale generation |
| callback route 未注册或 generation/source 不匹配 | typed reject；handler 零调用 |
| Host restart 后收到旧 callback | unknown route |
| Agent effect 回包未知 | 使用同一 effect identity inspect；不写 Host ledger |
| remote connection epoch 断开 | retire attachment；旧 frame/ack 永久 fence |

## 5. Good / Base / Bad Cases

- Good：Host 启动后 materialize Dash/Codex，按当前 profile 选中一个 verified service，应用
  surface 并在内存中建立 source/callback route。
- Base：Host 重启，Product association 仍指向同一 logical service/source；新 Host 重新 attach、
  apply surface、bind，Agent history 和 effect receipt 不变。
- Bad：保存 singleton Host revision graph，再把 attachment/generation 与 Agent receipt 比较。
  这会把进程身份错误提升成跨重启业务事实。

## 6. Tests Required

- live catalog 测试覆盖 attach idempotency、verified facts conflict、retire 与跨 incarnation。
- provision/rebind 测试覆盖 surface intersection、same-target replay、conflict、Lost 后新
  generation 和 stale generation。
- restart 测试构造全新 Host，证明无需数据库即可从 association + Agent service 重建。
- callback 测试覆盖 current route、unknown route、stale generation、source mismatch、deadline
  与 handler idempotent replay。
- bootstrap composition 测试覆盖 optional materialization 失败隔离和 integrity failure fail-fast。
- Remote/Wire 测试覆盖新 connection epoch、旧 frame 零 replay 与 callback route 映射。

## 7. Wrong vs Correct

```rust
// Wrong: Host startup 依赖恢复 singleton revision graph。
let snapshot = host_repository.load().await?;
host.recover(snapshot).await?;

// Correct: 当前进程从稳定两端重新建 route。
let selection = catalog.select(&binding.execution_profile).await?;
host.provision_runtime_target(request(selection)).await?;
host.restore_runtime_source_route(&binding.runtime_thread_id, binding.agent.source, effect, owner, ttl).await?;
```

```rust
// Wrong: 任一可选 Agent 启动失败终止核心服务。
register_optional_agent(contribution).await?;

// Correct: materialization 类失败成为 selection unavailable；完整性错误仍返回。
match register_optional_agent(contribution).await {
    Ok(_) => {}
    Err(error) if error.is_materialization_unavailable() => diagnose(error),
    Err(error) => return Err(error),
}
```
