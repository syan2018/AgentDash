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

enum ToolExecutionClaim {
    Acquired(ToolBrokerCall),
    InProgress(ToolBrokerCall),
    Terminal(ToolBrokerCall),
}

trait ToolBrokerRepository {
    async fn claim_execution(
        &self,
        item_id: &RuntimeItemId,
        effective_arguments: Value,
    ) -> Result<ToolExecutionClaim, ToolBrokerStoreError>;
}

struct ToolCallCoordinates {
    thread_id: RuntimeThreadId,
    turn_id: RuntimeTurnId,
    item_id: RuntimeItemId,
    presentation_item_id: PresentationItemId,
    source_thread_id: DriverThreadId,
    source_turn_id: DriverTurnId,
    source_item_id: DriverItemId,
    binding_id: RuntimeBindingId,
    binding_generation: RuntimeDriverGeneration,
    tool_set_revision: ToolSetRevision,
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
- Complete Agent Product surface从同一canonical AgentFrame闭包编译：final VFS先派生skills、guidelines
  与memory inventory；MCP server事实同时产生动态callable tools和模型可见server instruction；
  workspace/context requirement按Complete Agent声明的immutable delivery route投递。编译器不得绕过
  AgentFrame在launch时重新读取另一份Product capability表。
- Product execution profile 是 Product intent 的完整引用；Complete Agent offer profile 是服务侧
  guarantee。Surface rebind 必须携带原 Product profile 重新编译 desired surface，再由 Host 与当前
  offer 求交。两个 digest 证明的事实集合不同，不具有可比较的相等语义。
- AgentFrame `surface` document 是一条 revision 内 capability/context/VFS/MCP/hook 的 canonical
  形态，拆分字段只是 repository read projection。Canvas create/attach/copy 先生成新的 owner-local
  AgentFrame revision并修改该 document，再刷新拆分投影和 Complete Agent surface；因此授权、能力
  发现与后续 materialization 都能从同一历史 revision 恢复。
- tool可见性与调用授权必须来自同一applied Product resource surface。`task_write`可见时必须存在
  对应project/task Write grant。`mounts_list`自身只需要List准入，但返回的mount catalog必须描述
  该mount在applied surface中的完整Read/Write/List/Search/Exec集合与path scopes，不能把本次调用
  的List grant误当成mount全部能力。
- Product binding 只证明 `runtime_thread -> AgentRun target` 的当前关联；一次 callback 携带的
  applied surface revision 证明该回合实际接纳的 immutable authorization snapshot。授权器按该
  revision 读取 owner-local AgentFrame history并重新编译 grant，新 binding 不会撤销已开始回合的
  旧 grant，也不会把旧 grant 授给 rebind 后发起的新调用。
- VFS mount 的通用事实是 provider、root、capabilities 与 path scopes。`backend_id`只属于需要转发到
  concrete backend 的 provider；Canvas、inline、skill 等逻辑 provider 可以由自己的 root identity
  完整路由，因此其空 backend 不影响 surface evidence 的完整性。
- 每个进入最终Tool Catalog的`ToolContribution`必须由owner声明protocol projector与family；Surface compile缺projector即typed reject。Command、FileChange、FS与MCP使用各自typed family，普通VFS、RuntimeAction、Workspace Module、Companion、Task、Wait与LifecycleComplete显式声明Dynamic，禁止按tool name猜测或以Dynamic作为缺省。Dynamic的展示身份就是callable tool name，不附加owner namespace；来源归属已经由accepted definition与projector证据表达，不进入Card标题。
- 每个binding的effective presentation route必须把owner projector与`VendorStream|ToolBroker` emitter作为一个原子事实发布和替换。owner显式声明Dynamic时才允许Dynamic；route缺失是typed protocol violation，不能把原本的FS、Command、FileChange或MCP静默换型。
- Tool item lifecycle必须exactly-one presentation producer：`VendorStream`由connector mapper发布、Broker只维护internal canonical state；`ToolBroker`由`ToolContribution + ManagedRuntimeToolJournal`发布、connector mapper抑制同一tool的vendor lifecycle。Native、Codex与Remote均消费binding effective route，不能由全局默认或工具名推断owner。
- Driver callback同时传递canonical Runtime item、独立session-visible `PresentationItemId`、原样source thread/turn/item三元组及binding/generation/tool identity/arguments。canonical ID只用于Runtime state与side-effect idempotency；presentation ID只进入Backbone payload；source坐标只进入carrier correlation，三者不得互相替代。
- readable tool-result ref与presentation item必须由同一个session-scoped identity allocator生成，并按effective `ToolProtocolProjection`判断Tool/Command family。调用首事件必须同时固定该call的projector、emitter与readable family；后续update/result不能按工具名或更新后的surface重新猜测，否则会中途换producer或把大结果ref回填到另一张card。
- Broker首次accept使用owner projector原子提交authoritative ItemStarted；CAS conflict reload，相同turn与initial payload重放幂等，不同payload返回`IdempotencyConflict`。Tool update通过同一journal进入Runtime transient publisher，由store分配generation内单调sequence与唯一event ID。
- `tool_set_revision`只在首次accept前作为catalog admission fence。Item durable accept后，progress、approval与terminal convergence按binding、generation及persisted Item/Turn identity继续；调用自身触发的Surface/ToolSet hot-replace不得使该调用变成stale。hot-replace后的新调用仍必须携带current tool-set revision。
- Shell start按owner arguments区分Platform与MountExec；read/write/status/resize/terminate使用独立TerminalControl variant。ApplyPatch使用真实parser逐entry保存path/kind/diff/move path，多文件patch不得把整包diff复制到每个change。
- Platform shell terminal registration必须直接持有当前`PlatformToolExecutionContext`的`run_id`、`agent_id`与canonical `runtime_thread_id`，并记录`terminal_id -> backend/mount/cwd`路由。production VFS tool composition必须显式注入同一个AgentRun terminal registry；registry缺失时不得暴露可执行`shell_exec`或返回无法续接的`running` handle。local runtime `ShellSessionManager`仍是process与retained output buffer的唯一事实源，application registry只负责control routing与有界产品投影。
- Shell start返回`running`后，read/write/status/resize/terminate必须用同一typed owner解析原terminal；不同run、agent或runtime thread的continuation必须typed reject。start结果与后续增量chunks都更新同一terminal projection；增量snapshot必须按stream追加并保持`next_seq`单调，空增量不得清除既有preview。
- 每个Hook Definition只能绑定一个execution route；required hook必须满足actions、minimum strength、failure policy与acknowledgment。编译结果直接形成WP02 `RuntimeHookPlanBinding`，不重算revision/digest。
- Broker调用保留Thread、Turn、canonical ToolCall Item、presentation Item、source thread/turn/item、Binding、generation、ToolSet revision、tool/capability/path/channel坐标。外部driver不接收trait object、application delegate、本地VFS对象或credential material。
- 执行顺序固定为：bound catalog与binding/generation/tool-set校验 -> canonical Item durable accept -> broker call/idempotency accept -> BeforeTool同步Hook（含rewrite/block/approval）-> 再校验binding/capability -> permission -> VFS -> credential materialization -> durable Running -> executor side effect -> AfterTool同步Hook -> broker terminal -> canonical Item terminal convergence。
- BeforeTool rewrite后的effective arguments必须在Running和任何executor副作用之前持久化；AwaitingApproval/Running恢复不得漂移arguments、channel或tool provenance。
- approval先由Runtime journal创建canonical durable Interaction，再将其ref写入Broker call。Direct Callback与MCP façade调用同一状态机。
- executor收到canonical `RuntimeItemId`作为side-effect idempotency identity。Repository以原子`Accepted|AwaitingApproval -> Running` claim决定唯一executor owner；并发观察到`Running`的调用只等待/读取同一terminal或返回typed in-progress，不进入executor。进程重启后遗留`Running`表示副作用可能已发生，不能自动重放；Broker terminal已写而Runtime terminal失败时，重放只收敛canonical Item terminal。
- cancellation与timeout产生typed durable terminal；AfterTool同步观察成功、executor failure、timeout和cancel结果，完成result rewrite/effect后才允许terminal commit。
- executor failure、timeout、cancel与policy denial的`ToolBrokerResult.output`必须同时包含机器可读`error`与typed `content_items: [{ "type": "inputText", "text": diagnostic }]`；Native callback和presentation projector只消费typed content，不从顶层error补猜测展示。
- Workspace Module的create/attach/copy属于surface mutation；present属于presentation intent。present只提交control-plane presentation，不请求资源visibility、不写AgentFrame、不推进tool set。
- Application presentation producer提交canonical Runtime turn/item，由Managed Runtime补全runtime-to-presentation turn映射；只有真实外部producer stream才填写source correlation坐标。
- Normalized context equality只包含可呈现业务语义：assignment比较fragments，tool schema provenance使用稳定owner layer；AgentFrame revision/UUID保留在Business Surface provenance，不得放大成assignment或全工具delta。
- Dynamic工具typed content提供可读分行摘要，结构化details用于机器消费；两者不得以单行协议JSON互相替代。
- Native presentation接收Broker envelope时优先恢复typed `content_items`；只有不存在typed/text content的未知payload才允许使用JSON诊断兜底。
- Native `AfterTool`的结构化`result`只更新details；仅显式typed `content`可改写用户正文，Hook不得从details调用`to_string()`覆盖executor content。
- Complete Agent callback将Broker/VFS envelope解码为typed content parts与details后写入Agent native
  history；Read、Shell、FileChange等AgentDash专属Card从typed正文投影，实时与重载不重新解释原始
  executor JSON。
- protocol projector固定展示family并随ToolCall保存；实际工具owner独占参数校验和执行，因此Card
  formatter即使缺少path、pattern或command等可选展示字段，也会保持family并等待owner结果。
- Credential material只在local executor boundary解引用，不实现Serialize或Debug；MCP schema与日志只暴露credential ref/provenance，不暴露secret。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| contribution稳定key相同但内容不同 | typed compile conflict，不覆盖 |
| required Tool/Skill/Hook/Workspace仅有PromptOnly或弱route | `IncompatibleContribution` |
| ProjectAgent launch 无 canonical workspace default mount | frame construction typed reject；不创建 Host binding，不使用进程 cwd 或任意 backend |
| lifecycle VFS声明SkillAsset keys但final frame discovery未发现对应skill | frame construction typed reject；不持久化缺能力的AgentFrame |
| Product surface暴露`task_write`但没有Write grant | surface construction/authorization test失败；不能把运行期`missing_task_grant`留给Agent |
| `mounts_list`调用自身只有List准入 | 调用可执行，结果仍展示applied surface授予该mount的完整operation/path scope |
| Canvas等逻辑mount没有backend id | 依据provider/root/capabilities形成有效surface，不伪造backend |
| Surface rebind 时旧回合继续调用工具 | 按callback固定的AgentFrame revision授权；当前Product binding只校验target关联 |
| MCP server存在但动态catalog为空 | server instruction准确展示已配置server；不伪造callable tool |
| frame construction port 在 VFS bootstrap 后仍未绑定 | AppState composition fail-fast；请求不可进入半装配状态 |
| 一个Hook definition分配多个route | `ConflictingHookRoute` |
| stale binding generation/tool-set revision | side effect与broker call前typed reject |
| 已accept调用执行中推进tool-set revision | 同一binding generation下继续progress/approval/terminal；不得返回`StaleCoordinates` |
| hot-replace后以旧tool-set revision发起新调用 | admission阶段typed reject，不创建canonical Item |
| 相同Item ID但arguments/channel/provenance不同 | `IdempotencyConflict` |
| 最终Catalog contribution缺protocol projector | Surface assembly typed reject，不进入Runtime |
| Driver与Broker对同一tool item提交不同started payload | 拒绝双projection；Driver只调用Broker owner seam |
| canonical/source/presentation item ID彼此不同 | internal state只索引canonical；start/update/completed payload始终使用同一presentation ID；carrier原样保留source三元组 |
| owner-declared Command使用非`shell_exec`名称且结果被截断 | start与readable result ref均为同一`turn_NNN:cmd_NNN`；不按名称降成tool family |
| effective route缺projector或emitter | typed protocol violation；不发布Dynamic替代item |
| 同一turn连续tool updates | store分配不同sequence/event ID，cursor replay不丢update |
| BeforeTool block | 不解引用credential、不调用executor，durable terminal |
| approval required | 先创建canonical Interaction，再进入AwaitingApproval |
| permission/VFS deny | credential/executor前durable failure |
| executor/timeout/cancel/permission failure投影 | terminal output同时具有`error`与非空typed `content_items`；工具卡不得显示`null` |
| Running进程崩溃 | 保留完整effective arguments并返回typed in-progress/待人工恢复；`recoverable()`不得把Running当作executor重放许可 |
| 同一Item并发执行 | 原子claim只产生一个`Acquired`；其余调用观察`InProgress`并共享首次terminal，executor只调用一次 |
| 并发terminal结果不同 | 真实PG行锁只接受一个terminal，另一方typed conflict |
| Runtime Item terminal暂时失败 | Broker terminal保留，duplicate只重试journal convergence |
| production VFS composition漏注入terminal registry | composition/工具构造测试失败；不得产生孤儿`running` handle |
| 同一owner以terminal_id read已完成的短命令 | 路由到local retained buffer，返回最终state/exit/output；application projection保持历史tail |
| 另一run/agent/runtime thread续接terminal_id | typed owner mismatch；不向local runtime发送control operation |

## 5. Good / Base / Bad Cases

**Good case:** MCP tool call按catalog坐标进入同一个Broker，BeforeTool改写参数后先持久化，policy/VFS/credential校验完成才以canonical Item ID调用executor。executor可触发Surface hot-replace；同一accepted Item仍按原binding generation完成AfterTool、Broker terminal和Runtime Item convergence。

**Good case:** Project launch把lifecycle SkillAsset投影进final VFS，frame construction从该VFS发现
skill/guideline并写回同一AgentFrame document；surface compiler从该frame生成skill/MCP instructions
与native tools，授权器从同一applied resource surface生成Task/VFS grant。

**Base case:** required approval创建durable Interaction并暂停；获批后由唯一claim owner使用同一Item和effective arguments继续。若进程在Running后消失，后续调用保留该不确定边界并返回typed in-progress，不猜测副作用是否发生。

**Bad case:** 把Capability Pack拍成prompt、向driver传`DynAgentTool`、在permission/VFS之前解引用secret、把持久化Running直接当成executor重放许可，或让可选terminal registry静默产出孤儿`running` handle。这些行为会伪报能力、绕过执行点policy、重复不可逆副作用或切断terminal control，必须由类型、composition与顺序测试阻止。

## 6. Tests Required

- Surface测试覆盖确定性编译、各contribution必填字段、所有identity冲突、required/optional/PromptOnly矩阵、Hook唯一route与profile strength。
- frame discovery回归测试断言final VFS中的SKILL.md与AGENTS.md分别进入同一持久AgentFrame的
  capability/context source；Product surface测试断言skills、MCP server与动态MCP tool均进入实际
  Agent surface。
- runtime authorization测试断言Project AgentRun允许run-scoped`task_write`，read-only fixture仍拒绝
  write；`mounts_list`结果包含applied surface完整能力；final Product broker tracer覆盖Workspace Module
  list/describe/operate/invoke/present的read/write/presentation边界。
- embedded PostgreSQL Lifecycle launch 测试断言：product delivery 前 current AgentFrame 已包含 canonical workspace mount/backend/root/capability/context 与本次 Run execution profile；无 default workspace 的 Project 在 frame construction 边界失败。
- Broker behavior覆盖Direct/MCP同状态机、rewrite/block/approval、permission/VFS/credential顺序、cancel/timeout/executor failure/result rewrite。
- Broker lifecycle必须覆盖调用在accept后触发Surface/ToolSet hot-replace，断言progress与terminal成功、terminal唯一；另以旧revision发起新调用必须在accept前失败。
- 失败结果测试必须断言executor failure、timeout、cancel和policy denial均具有非空typed diagnostic content。
- Projector matrix枚举最终production catalog，覆盖每个family的started/update/completed/failed/approval/identity；至少Shell与ApplyPatch必须经过真实owner→Registry→Broker→Runtime链。
- Shell测试覆盖Platform/MountExec、TerminalControl五类操作及command/cwd/output/exit/status；ApplyPatch覆盖add/update/move/delete多文件逐entry diff。
- Shell lifecycle测试必须覆盖production composition注入typed owner registry、running start→write/read→completed retained output、跨owner拒绝，以及`after_seq`增量snapshot追加后不清空既有terminal projection。该测试必须在删除composition注入时失败，而不只直接构造adapter。
- `workspace_module_present`是对当前观察者的只读展示请求：permission/effect固定为
  `ProductRead/ReadOnly`，成功结果在typed details中携带完整`workspace_module_presentation`。
  concrete Agent提交该ToolResult后，canonical projector从同一结果派生平台展示事件；工具执行轨迹、
  Workspace Module资源与展示命令分别由Agent history、Workspace projection与live consumer承担。
- Native callback测试使用刻意不同的source/runtime/presentation item IDs，证明Broker internal state只消费canonical identity，Backbone start/update/completed只消费presentation identity，carrier保留source三元组且重复accept幂等。
- Native/Codex/Remote emitter矩阵覆盖VendorStream与ToolBroker route，断言每个logical tool只出现一个started和一个completed；ToolBroker route不得由connector重复发布，VendorStream route不得由Broker重复发布。
- readable identity测试至少覆盖owner-declared Command alias、大结果截断、连续多工具、跨turn与重绑水位，start/completed/readable ref必须复用同一ID。
- 覆盖duplicate identity、AwaitingApproval、并发execution claim、Running crash不自动重放、effective arguments不可漂移、canonical Item terminal convergence；并发同Item测试必须断言`executor.calls == 1`且调用方共享同一terminal。
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

```rust
// Wrong: 看到持久化Running就再次进入不可幂等executor。
if call.status == ToolBrokerCallStatus::Running {
    return executor.execute(call).await;
}

// Correct: 只有原子claim owner执行；观察者等待首次terminal或得到typed in-progress。
match repository.claim_execution(&item_id, effective_arguments).await? {
    ToolExecutionClaim::Acquired(call) => executor.execute(call).await,
    ToolExecutionClaim::InProgress(call) => await_terminal_or_in_progress(call).await,
    ToolExecutionClaim::Terminal(call) => replay_terminal(call).await,
}
```

```rust
// Wrong: optional composition dependency让start成功但后续terminal_id无法解析。
let provider = VfsRuntimeToolProvider::new(vfs, materialization);

// Correct: production composition显式提供typed terminal routing owner。
let provider = VfsRuntimeToolProvider::new(vfs, materialization, terminal_registry);
```

```rust
// Wrong: 用mounts_list这一次调用的List权限描述整个mount。
catalog.capabilities = invocation_grant.operations;

// Correct: List只做当前调用准入，catalog从同一applied mount的全部grants合并得到。
authorize_operation(&applied_mount, RuntimeVfsOperation::List)?;
catalog.capabilities = union_applied_mount_operations(&applied_mount);
```

```rust
// Wrong: 用调用开始时的catalog revision约束已接受调用的整个生命周期。
let thread = load_thread(&call.thread_id).await?;
ensure!(thread.tool_set_revision == call.tool_set_revision, StaleCoordinates);
record_tool_terminal(call).await?;

// Correct: revision只做新调用准入；已接受调用按稳定owner与persisted Item收敛。
let thread = load_bound_thread(&call.invocation).await?;
let item = thread.items.get(&call.item_id).ok_or(StaleCoordinates)?;
ensure_same_turn_and_active(item, call)?;
record_tool_terminal(call).await?;
```

```rust
// 展示请求由已提交工具结果携带，不建立独立effect/ack仓储。
AgentToolResult::Completed {
    output: json!({
        "content": [{ "type": "text", "text": "presentation requested" }],
        "is_error": false,
        "details": { "workspace_module_presentation": presentation },
    }),
}
```
