# Business Agent Surface and Platform Tool Broker

## 1. Scope / Trigger

本规范适用于 Business Agent Surface 的 capability contribution 编译与 profile binding，以及平台 callable tool 通过 Direct Callback 或 session-scoped MCP façade 执行的统一 Broker 状态机。修改 Capability Pack、HookPlan 编译、ToolCatalog、Workspace/Skill 适配、tool policy顺序、approval、credential、VFS或tool-call persistence时必须复核本规范。

## 2. Signatures

```rust
impl AgentSurfaceCompiler {
    pub fn compile(
        revision: u64,
        packs: impl IntoIterator<Item = CapabilityPack>,
    ) -> Result<AgentSurfaceSnapshot, SurfaceCompileError>;
}

impl AgentSurfaceSnapshot {
    pub fn bind_profile(
        &self,
        profile: &RuntimeProfile,
    ) -> Result<BoundAgentSurface, SurfaceBindError>;
}

impl HookPlanSnapshot {
    pub fn bind_runtime_plan(
        &self,
        thread_id: RuntimeThreadId,
    ) -> Result<RuntimeHookPlanBinding, SurfaceBindError>;
}

impl PlatformToolBroker {
    pub async fn invoke(
        &self,
        invocation: ToolBrokerInvocation,
    ) -> Result<ToolBrokerOutcome, ToolBrokerError>;
}
```

`ToolBrokerRepository` 持有 broker call projection与recovery；`ToolBrokerRuntimeJournal` 持有 canonical Runtime Item/Interaction accept与terminal convergence；policy、credential、executor和Hook通过独立窄port注入。

## 3. Contracts

- Business Surface 以 protocol-neutral contribution 为输入，稳定展开 Instruction、Context、Tool、MCP、Skill、Workflow、Permission 与 Hook；按 priority和稳定key确定性排序，同key不同定义必须typed conflict。
- API composition通过`NativeAgentRunSurfaceCompiler`等显式production source取得AgentRun/AgentFrame/workspace/tool/Hook业务事实；provisioner只接受编译完成且带真实revision/digest的`MaterializedDriverSurface`，不构造默认或空surface。immutable surface必须先持久化，再进入Host bind，产品binding最后落库；确定性Thread/Binding ID保证中途崩溃后可重放。
- Direct ProjectAgent launch 必须在 `AgentRunProductDelivery` 触发首次 Runtime provision 前，通过 application-owned owner composer 持久化完整 current AgentFrame revision；execution profile、capability、context、MCP 与 canonical VFS default mount 位于同一 revision。Repository bootstrap 早于 VFS bootstrap 时使用一次性 late-bound construction port，并在 AppState 对外可见前完成绑定，不把 VFS composition 下沉到 Runtime compiler。
- `AgentSurfaceSnapshot` 是业务期望的immutable事实，`BoundAgentSurface`是与实际RuntimeProfile求交后的业务admission结果。Driver Host只能持久化revision/digest/hook refs等轻量reference，不得复制或重新编译contribution。
- Required contribution不满足即typed incompatible；只有显式optional贡献可以省略。`PromptOnly`不满足callable Tool、exact Workspace/Skill或required Hook语义。
- Tool需要真实callable route：Direct Callback、session-scoped MCP façade或Driver Native。runtime tool name、tool path、MCP server identity、configuration boundary和schema/provenance必须无冲突。
- 每个进入最终Tool Catalog的`ToolContribution`必须由owner声明protocol projector与family；Surface compile缺projector即typed reject。Command、FileChange、FS、MCP、VFS、RuntimeAction、Workspace Module、Companion、Task、Wait、LifecycleComplete与explicit Dynamic使用各自typed family，禁止按tool name猜测或以Dynamic作为缺省。
- Tool item lifecycle的唯一projection owner是`ToolContribution + ManagedRuntimeToolJournal`。Driver/Native callback只传canonical Runtime item/turn、独立source coordinate、binding/generation、tool identity与arguments；不得提前构造另一份ItemStarted/terminal payload。
- Broker首次accept使用owner projector原子提交authoritative ItemStarted；CAS conflict reload，相同turn与initial payload重放幂等，不同payload返回`IdempotencyConflict`。Tool update通过同一journal进入Runtime transient publisher，由store分配generation内单调sequence与唯一event ID。
- Shell start按owner arguments区分Platform与MountExec；read/write/status/resize/terminate使用独立TerminalControl variant。ApplyPatch使用真实parser逐entry保存path/kind/diff/move path，多文件patch不得把整包diff复制到每个change。
- 每个Hook Definition只能绑定一个execution route；required hook必须满足actions、minimum strength、failure policy与acknowledgment。编译结果直接形成WP02 `RuntimeHookPlanBinding`，不重算revision/digest。
- Broker调用保留Thread、Turn、canonical ToolCall Item、Binding、generation、ToolSet revision、tool/capability/path/channel坐标。外部driver不接收trait object、application delegate、本地VFS对象或credential material。
- 执行顺序固定为：bound catalog与binding/generation/tool-set校验 -> canonical Item durable accept -> broker call/idempotency accept -> BeforeTool同步Hook（含rewrite/block/approval）-> 再校验binding/capability -> permission -> VFS -> credential materialization -> durable Running -> executor side effect -> AfterTool同步Hook -> broker terminal -> canonical Item terminal convergence。
- BeforeTool rewrite后的effective arguments必须在Running和任何executor副作用之前持久化；AwaitingApproval/Running恢复不得漂移arguments、channel或tool provenance。
- approval先由Runtime journal创建canonical durable Interaction，再将其ref写入Broker call。Direct Callback与MCP façade调用同一状态机。
- executor收到canonical `RuntimeItemId`作为side-effect idempotency identity。duplicate/recovery不会重复执行已完成副作用；Broker terminal已写而Runtime terminal失败时，重放只收敛canonical Item terminal。
- cancellation与timeout产生typed durable terminal；AfterTool同步观察成功、executor failure、timeout和cancel结果，完成result rewrite/effect后才允许terminal commit。
- Credential material只在local executor boundary解引用，不实现Serialize或Debug；MCP schema与日志只暴露credential ref/provenance，不暴露secret。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| contribution稳定key相同但内容不同 | typed compile conflict，不覆盖 |
| required Tool/Skill/Hook/Workspace仅有PromptOnly或弱route | `IncompatibleContribution` |
| ProjectAgent launch 无 canonical workspace default mount | frame construction typed reject；不创建 Host binding，不使用进程 cwd 或任意 backend |
| frame construction port 在 VFS bootstrap 后仍未绑定 | AppState composition fail-fast；请求不可进入半装配状态 |
| 一个Hook definition分配多个route | `ConflictingHookRoute` |
| stale binding generation/tool-set revision | side effect与broker call前typed reject |
| 相同Item ID但arguments/channel/provenance不同 | `IdempotencyConflict` |
| 最终Catalog contribution缺protocol projector | Surface assembly typed reject，不进入Runtime |
| Driver与Broker对同一tool item提交不同started payload | 拒绝双projection；Driver只调用Broker owner seam |
| 同一turn连续tool updates | store分配不同sequence/event ID，cursor replay不丢update |
| BeforeTool block | 不解引用credential、不调用executor，durable terminal |
| approval required | 先创建canonical Interaction，再进入AwaitingApproval |
| permission/VFS deny | credential/executor前durable failure |
| Running进程崩溃 | `recoverable()`返回完整effective arguments并以Item ID重放 |
| 并发terminal结果不同 | 真实PG行锁只接受一个terminal，另一方typed conflict |
| Runtime Item terminal暂时失败 | Broker terminal保留，duplicate只重试journal convergence |

## 5. Good / Base / Bad Cases

**Good case:** MCP tool call按catalog坐标进入同一个Broker，BeforeTool改写参数后先持久化，policy/VFS/credential校验完成才以canonical Item ID调用executor，AfterTool改写结果后原子记录Broker terminal并收敛Runtime Item。

**Base case:** required approval创建durable Interaction并暂停；恢复时用同一Item和effective arguments继续，不重新接受或执行副作用。

**Bad case:** 把Capability Pack拍成prompt、向driver传`DynAgentTool`或在permission/VFS之前解引用secret。这些行为会伪报能力或绕过执行点policy，必须由类型和顺序测试阻止。

## 6. Tests Required

- Surface测试覆盖确定性编译、各contribution必填字段、所有identity冲突、required/optional/PromptOnly矩阵、Hook唯一route与profile strength。
- embedded PostgreSQL Lifecycle launch 测试断言：product delivery 前 current AgentFrame 已包含 canonical workspace mount/backend/root/capability/context 与本次 Run execution profile；无 default workspace 的 Project 在 frame construction 边界失败。
- Broker behavior覆盖Direct/MCP同状态机、rewrite/block/approval、permission/VFS/credential顺序、cancel/timeout/executor failure/result rewrite。
- Projector matrix枚举最终production catalog，覆盖每个family的started/update/completed/failed/approval/identity；至少Shell与ApplyPatch必须经过真实owner→Registry→Broker→Runtime链。
- Shell测试覆盖Platform/MountExec、TerminalControl五类操作及command/cwd/output/exit/status；ApplyPatch覆盖add/update/move/delete多文件逐entry diff。
- Native callback测试使用刻意不同的source/runtime item IDs，证明Broker只消费canonical identity且重复accept幂等。
- 覆盖duplicate identity、AwaitingApproval与Running crash recovery、effective arguments不可漂移、canonical Item terminal convergence。
- 真实embedded PostgreSQL覆盖0063 migration、Thread/Turn/Item/Interaction/Binding generation复合FK、accept幂等、typed transition、并发terminal与FK失败全事务回滚。
- 验证CredentialMaterial不可序列化/调试输出，MCP tool list不含secret。
- Runtime/Infrastructure全套、contracts check、migration guard、fmt、clippy与diff check必须通过。

## 7. Wrong vs Correct

```rust
// Wrong: application把本地工具对象直接交给外部driver。
driver.update_tools(Vec<Arc<dyn DynAgentTool>>).await?;

// Correct: surface发布schema/provenance，调用经canonical Broker route。
let bound = snapshot.bind_profile(&offer_profile)?;
broker.invoke(invocation_from(&bound, call)).await?;
```

```rust
// Wrong: 先执行工具，再保存rewrite后的参数和Running状态。
let result = executor.execute(args).await?;
repository.mark_running(call, args).await?;

// Correct: effective arguments和Running先durable，executor使用canonical Item幂等键。
repository.transition(call, ToolBrokerTransition::Running { effective_arguments }).await?;
executor.execute(runtime_item_id, effective_arguments).await?;
```
