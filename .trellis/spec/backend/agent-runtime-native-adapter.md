# Dash Complete Agent 与 Clean Agent Core

## 1. Scope / Trigger

本规范适用于 first-party Dash Complete Agent、`DashAgentRepositoryState`、Agent Core、
provider bridge、execution callbacks、native history/context/fork/compaction 与 live event。
修改 Dash source document、Core callback、terminal evidence 或 Complete Agent adapter 时必须复核。

Dash 是 concrete Agent owner：它保存自己的 source/history/effect，并实现平台中立
`CompleteAgentService`。Runtime 与 Product 不复制这些事实。

## 2. Signatures

```rust
pub struct DashAgentRepositoryState {
    pub store: DashAgentStore,
    // source-owned execution/context/surface facts live in native history
}

pub trait DashAgentRepository {
    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError>;
    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError>;
}
```

```rust
pub struct DashAgentCompleteService { /* store + bridge + live channels */ }

impl CompleteAgentService for DashAgentCompleteService {
    async fn create(...);
    async fn resume(...);
    async fn fork(...);
    async fn execute(...);
    async fn read(...);
    async fn changes(...);
    async fn live_events(...);
    async fn inspect(...);
    async fn apply_surface(...);
}
```

```rust
pub trait DashExecutionCallbacks {
    async fn on_event(&self, event: DashExecutionEvent) -> Result<(), DashCoreError>;
}

pub trait DashHistoryCallbacks {
    async fn on_committed(&self, commit: DashHistoryCommit) -> Result<(), DashServiceError>;
}
```

```rust
pub trait DashConversationNamer {
    async fn generate(
        &self,
        request: DashConversationNamingRequest,
    ) -> Result<String, DashServiceError>;
}

pub enum HistoryPayload {
    ThreadNameChanged { thread_name: String },
    // other native history facts
}

pub enum ContextFrameSection {
    ToolSchemaDelta {
        added_tools: Vec<RuntimeToolSchemaEntry>,
        removed_tools: Vec<String>,
        changed_tools: Vec<RuntimeToolSchemaEntry>,
    },
    // other platform presentation sections
}
```

```rust
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/default_system_prompt.md");

fn dash_surface_from_bound(
    surface: &BoundAgentSurface,
) -> Result<DashSurface, AgentServiceError>;

fn materialization_digest(
    instructions: &[DashSurfaceInstruction],
    tools: &[DashToolDefinition],
) -> Result<String, serde_json::Error>;

pub fn dash_complete_agent_build_digest() -> AgentPayloadDigest;
```

## 3. Contracts

- 一个 Dash source 使用一个 canonical repository document 保存 history、context、branch、
  command/effect state 与 compaction facts。document 内部 CAS 可以使用 owner revision；数据库
  不再拆出 branch/history/command/effect/change 关系镜像。
- `DashSurface` 以 `SurfaceApplied/SurfaceRevoked` native history entry 表达，当前 surface 从
  history fold 得出。repository root 不保存第二个 `surface` 字段。
- `DashSurface` 保存按应用顺序排列的materialized instructions
  `[{ key, channel, text }]`、callable tools与最终accepted ContextFrames。ContextFrame保存typed
  sections、delivery metadata和唯一的`rendered_text`；provider materializer与
  `ContextFrameChanged`都读取该值，因此模型输入、history重放和前端审计不会产生第二套文本渲染。
- concrete Agent内建工作基底属于该Complete Agent实现，不属于Product或AgentFrame。Dash以稳定
  intrinsic instruction key把编译期提示词放在Product instructions之前，再把合并后的完整
  `DashSurface`提交到native history；provider prompt与Identity ContextFrame都从该entry投影。
- `AppliedAgentSurface.digest`证明Product binding，`DashSurface.digest`证明concrete Agent实际接纳的
  instructions/tools内容，两者不共享数值。intrinsic prompt内容进入Dash verified profile evidence，
  build digest标识发布构建；实现基底变化通过正常rebind追加新的`SurfaceApplied`事实。
- Dash registration claim 与 production verification template 必须调用同一个
  `dash_complete_agent_build_digest()`。build digest 证明当前服务构建，profile digest 由独立
  verification 字段证明；共享构造保证进程重启恢复 route 时不会因两端格式漂移而拒绝内建 Agent。
- intrinsic instruction的namespace由concrete Agent保留。Product contribution与保留key冲突时，
  Native Adapter在history mutation前返回typed invalid argument，保证accepted surface中每个key
  只有一个owner。
- callable tools在`RuntimeToolDefinition`中同时保存名称、description、input schema、owner
  projector与typed provenance；Product surface无损携带capability/source/tool path/context usage，
  Native adapter按字段接纳而不从runtime name或route反推。provider `tools[]`携带完整机器契约；
  accepted tool保留同一provenance。
  ToolSchema ContextFrame从同一列表生成完整、确定性的nested-schema可读文本，同时在section中保存
  added/removed/changed真增量。机器调用契约与可读上下文是同一事实的两个投影。
- Dash provider port在每个逻辑provider round固定一份request snapshot：当前accepted
  ContextFrames生成system/context文本，当前tools生成structured tool contract，并同时固定该round
  的owner projector。工具结果提交与surface mutation完成后，下一round重新物化；同一round重试
  复用已固定snapshot，原因是一次已发出的provider请求不能在中途改变语义。
- Product的skill、memory、MCP、workspace与context requirement必须先物化为Agent实际接纳的
  instruction/tool surface，再由Native Adapter按channel映射为Identity、Environment、
  SystemGuidelines、AssignmentContext、CapabilityStateDelta、MemoryContext或UserContext。
  Adapter不读取Product表反补ContextFrame，也不从展示文本反猜Agent输入。
- source metadata 与 repository描述同一个 concrete Agent source，必须在同一 Dash source
  document/atomic commit 中更新。
- Create 前还没有 source coordinate 的 effect 可以保留独立 `effect_id` lookup receipt。
  receipt 属于 Dash Complete Agent，用于 `inspect`，不进入 Runtime/Host/Product。
- `create/fork/execute/apply_surface` 使用稳定 effect identity。相同 identity + 相同 request
  返回原 receipt；不同 request 返回 typed conflict。
- `read` 从 Dash document 投影 authoritative history/context/lifecycle。`changes` 只发布 Dash
  自己真正保存的 change evidence；平台不能替 Dash 发明 durable cursor。
- Provider/Core 失败以真实 `code + message + retryable` 写入 Dash terminal history并通过
  `read/changes` 原样投影。通用错误文案不能替代 owner evidence。
- source service 打开时必须把真实 `DashExecutionCallbacks` 与 `DashHistoryCallbacks` 绑定到
  source-scoped Complete Agent live sink。未绑定 callback 是 composition error；当前没有
  subscriber 不是错误。
- 成功 CAS 后，`DashHistoryCallbacks` 把本次提交的 exact history suffix 经 canonical projector
  发布为 durable live record；外层原子事务只能在事务提交成功后发布。live 通知失败不改变已提交
  history 的真值，消费者通过重新 `read` 恢复。
- `DashExecutionCallbacks` 只发布 provider/Core 尚未提交的 ephemeral delta。它不补造
  `TurnStarted`、user input 或 terminal lifecycle。
- `AgentLiveEvent.sequence` 只在当前 service process + source 内单调。broadcast lag 返回
  retryable unavailable；消费者重新 `read`，不从 Runtime DB replay。
- `InitialContextInstalled`、`SurfaceApplied` 与 `SurfaceRevoked` 必须从 Agent 实际保存的 native
  history 投影 `Platform(ContextFrameChanged)`；Product intent 或 repository metadata 不能直接
  冒充 Agent 已接纳 context。
- 会话标题是 Dash Agent 从已接纳 user input 与已完成 Agent output 生成的原生展示事实。
  首个具备上述内容的 terminal 回合后，Dash 通过 `DashConversationNamer` 生成非空标题，并以
  `ThreadNameChanged` 提交到同一 source history；`read`、`changes` 与 durable live event
  从该 entry 分别投影 `AgentThreadNameSnapshot`、`AgentChangePayload::ThreadNameChanged` 与
  canonical `ThreadNameUpdated`。Product 消费 Agent snapshot 中的首次有效标题来初始化自己的
  AgentRun 标题；这两个 owner 各自持久化一次的原因是 Dash thread 与 AgentRun 可以独立存在，
  且 AgentRun 标题允许用户独立修改。Product 不持续同步，也不从消息文本推导标题。
- 标题生成不是回合 terminal 的组成部分：Succeeded、Failed、Lost、Interrupted 都保留各自原始
  terminal；只要该回合已有非空 user input 与 Agent output，就在 terminal 提交后尝试命名。
  Accepted/interaction 尚未终态时不命名；无 Agent output 的失败回合也没有可命名证据。命名失败时，
  未命名 source 可在后续满足条件的 terminal 回合再次尝试。history 一旦已有标题，自动命名不再重复调用。
- Core 只拥有 provider-neutral inference/stream/tool loop，不依赖 Product workflow、
  Lifecycle、PostgreSQL repository、Codex DTO 或 Runtime persistence。
- Core 不维护隐式 provider round 上限。一次Agent turn只由provider `Stop`、typed provider/tool/
  callback failure、interaction、context overflow或显式cancel结束；工具调用后继续请求provider是同一
  turn的正常执行，不具有可由内部计数推导的业务终态。未来若产品需要deadline或成本预算，应作为
  显式执行策略在owner边界触发cancel/typed terminal，而不是重新引入Core匿名轮次常量。
- Tool/Hook 通过 Host callback route调用真实 handler。Dash 在 callback identity 上重试；
  handler owner负责副作用幂等。
- callback result以typed text/image/reasoning content parts与structured details穿过Core、Dash event和
  folded state。provider transcript只选择其支持的content，Native Adapter按owner projector生成
  AgentDash ThreadItem；两者不解析序列化callback envelope。
- owner projector固定展示family，不决定工具执行准入。presentation所需的path/command等参数缺失时，
  callback仍到达实际工具owner，由owner的typed validation result形成同一ToolResult。
- compaction/fork/context state 属于 Dash source。Product只保存 fork lineage与 source
  association，不复制 checkpoint/history digest作为执行 authority。
- provider protocol terminal决定一次 model response是否完成。transport EOF不能冒充 terminal；
  解码/断流保留 provider分类。

## 4. Validation & Error Matrix

| 场景 | 必须结果 |
| --- | --- |
| source document CAS conflict | reload owner document并按 typed command重试/冲突 |
| 同surface digest重放但instruction列表不同 | typed idempotency conflict；迁移必须从已提交callback surface恢复精确列表 |
| 连续surface只增加一个tool | 只发布该tool的`added_tools`，不重放完整当前schema |
| tool同名但description/schema变化 | 发布到`changed_tools`，不同时出现在`added_tools` |
| tool从surface消失 | tool name进入`removed_tools` |
| surface revision变化但instruction/tool语义未变 | 不发布伪ContextFrame delta |
| Product contribution使用Dash intrinsic保留key | side effect与history mutation前typed invalid argument |
| Product binding digest与Dash materialization digest比较 | 分别解释上游binding与实际接纳内容，不要求相等 |
| 内建提示内容升级 | verified profile/build evidence变化；rebind提交新的实际surface，既有history保持原事实 |
| registration claim 与 verification template 构建摘要 | 两端使用同一 helper，摘要完全一致 |
| effect identity相同且request相同 | 返回原 receipt |
| effect identity相同但request不同 | typed idempotency conflict |
| unsupported input family | Core side effect 前 typed unsupported |
| 空输入 | side effect 前 rejected |
| provider失败 | terminal history保留真实 code/message/retryable |
| 正常任务需要超过8次provider响应 | 继续执行，直到收到真实terminal或显式cancel |
| source callback未绑定 | composition/configuration error |
| 没有 live subscriber | execution/history commit 正常完成 |
| live subscriber lagged | retryable unavailable；重新 read |
| history commit成功但live通知失败 | commit保持成功；subscriber重新read authoritative history |
| 命名输入缺少非空user input或Agent output | 不生成标题，不提交history entry |
| 回合仍为Accepted或等待interaction | 不调用namer，等待terminal |
| Failed/Lost/Interrupted terminal且已有Agent output | 尝试首次命名；原terminal保持不变 |
| namer返回空标题 | 拒绝标题提交；原回合terminal保持不变 |
| namer/provider失败 | 不提交标题；原回合terminal保持不变，后续满足条件的terminal回合可重试 |
| history已有标题 | 不调用namer，不重复提交自动标题 |
| fork cutoff/context digest不匹配 | typed reject；source document不变 |
| transport在provider terminal前EOF | retryable `stream_disconnected` |
| provider terminal后transport继续开放 | 逻辑 response立即完成且只完成一次 |

## 5. Good / Base / Bad Cases

- Good：Dash command 原子更新 source document，成功后把同一 committed suffix 发布为 durable
  live record；Core callback只在其间发布partial delta，重连从同一document read得到完整终态。
- Good：首个具备非空 user input 与 Agent output 的 terminal 回合提交后，Dash 根据该 source 的原生对话生成标题并追加
  `ThreadNameChanged`；snapshot与live notification来自该 entry，Product据此仅初始化一次
  AgentRun标题，之后两个owner可以独立修改各自标题。
- Good：Product提交skills/MCP/memory instructions与tool surface，Native Adapter在接纳边界生成
  typed ContextFrames并与tool definitions一起写入`SurfaceApplied`；provider与canonical projector
  消费同一已接纳frame。
- Good：Dash Adapter先加入自身intrinsic instruction，再物化Product surface；Agent收到的基础行为
  规则和平台展示的Identity frame来自同一source-owned history entry。
- Base：live subscriber掉线，Core继续执行并提交 history；新 subscriber先 read再订阅。
- Base：同一surface幂等重放不产生新的history entry，因此不产生重复ContextFrame。
- Base：首回合在多轮工具执行后以Failed结束，但已保存Agent output；Dash仍生成会话标题，回合保持Failed。
- Base：标题生成暂时失败，原回合terminal不变且source保持未命名；下一满足条件的terminal回合可以再次生成。
- Bad：Dash 同时写 repository JSONB 与 history/effect镜像，再逐次校验相等。镜像没有独立
  owner，只会制造 drift。
- Bad：Product 从首条用户消息截断标题。该值没有 Agent 接纳与生成的证据，不能作为 AgentRun
  的首次标题。
- Bad：生产 composition 注入 Noop execution callback。输入可能执行成功，但用户永远看不到
  live delta。
- Bad：在Dash、canonical projector或provider bridge各自格式化工具说明。三处都能单独工作，但
  surface热更新后无法证明模型所见文本、结构化schema与平台审计属于同一accepted revision。

## 6. Tests Required

- Dash repository测试覆盖首次 create、CAS、restart read、fork、compaction与 owner document
  原子性。
- Complete Agent tests覆盖 create/resume/fork/execute/read/changes/inspect/apply surface及
  stable effect replay/conflict。
- failure tests断言 provider/Core code、message、retryable 在 terminal history与 Agent snapshot
  一致。
- Core loop测试至少执行12轮工具调用后再返回`Stop`，断言不会由内部轮次计数终止；provider真实
  失败的回归测试断言此前完成的每条ToolCall/ToolResult仍在native history中。
- live composition test在执行前订阅，断言顺序为 durable user input、durable `TurnStarted`、
  ephemeral text/reasoning/tool delta 与 durable terminal；执行后从 read断言同 turn 已终态。
- surface/context tests断言 native history保存实际instruction、tools与accepted ContextFrames，
  provider prompt逐字包含frame `rendered_text`，snapshot与live逐项发布同一frame，repository root
  不存在平行surface字段。
- intrinsic surface测试断言内建`.md`进入provider request与Identity ContextFrame，Product applied
  receipt不认领该contribution，且Product binding digest不同于Dash materialization digest。
- surface delta tests连续应用三版surface，覆盖tool新增、修改、删除，断言read重放只返回真实
  变化、`rendered_text`包含完整nested字段说明且section保留原始schema；context channel矩阵覆盖
  system、identity、workspace、workflow、skills、MCP、memory与user context的typed frame映射。
- active-turn测试在第一轮tool callback期间替换surface，断言旧call沿已接纳route完成、下一round
  读取新ContextFrame/tools/projector，且native user/tool transcript没有重复。
- naming test断言Succeeded与“已有Agent output的Failed”回合都把accepted user input与最终
  Agent output交给namer，只提交一次非空`ThreadNameChanged`，且`read/changes/live`均投影同一标题；
  Accepted、interaction与无output失败不命名，命名失败不改变原回合terminal。
- verification test断言 native registration claim 与 production template 的 build digest 都来自
  `dash_complete_agent_build_digest()`，profile digest 仍由独立字段校验。
- lag/no-subscriber tests区分临时没有观察者与 callback未装配。
- source scan/migration test断言 Dash关系镜像表和 production Noop callback缺席。
- Core dependency test断言不依赖 Application/Domain workflow/vendor DTO/repository。

## 7. Wrong vs Correct

```rust
// Wrong: 同一个 source 写两份执行事实。
save_dash_document(&state).await?;
replace_history_rows(&state.store.history).await?;

// Correct: Dash owner document是唯一原子事实。
repository.compare_and_swap(expected, replacement).await?;
```

```rust
// Wrong: Product根据输入推导标题，或在每次read时持续覆盖AgentRun标题。
let title = infer_title(prompt);
lifecycle_agent.workspace_title = snapshot.thread_name;

// Correct: Dash保存原生标题；Product以该证据仅初始化一次自己的AgentRun标题。
store.commit(HistoryPayload::ThreadNameChanged { thread_name })?;
lifecycle_agents.initialize_title_from_agent(&target, &thread_name).await?;
```

```rust
// Wrong: 只有Succeeded回合才有资格形成会话标题。
if matches!(receipt.state, DashReceiptState::Terminal(DashTerminalOutcome::Succeeded)) {
    try_assign_thread_name().await?;
}

// Correct: terminal只决定何时尝试；是否可命名由已提交的user input + Agent output证据决定。
if matches!(receipt.state, DashReceiptState::Terminal(_)) {
    try_assign_thread_name().await?;
}
```

```rust
// Wrong: live delta缺失时从平台 durable projection恢复。
let replay = runtime_projection.load_changes(source).await?;

// Correct: partial stream失效后读取 concrete Agent authority。
let snapshot = dash_complete_agent.read(AgentReadQuery {
    source,
    at_revision: None,
}).await?;
```

```rust
// Wrong: 把当前完整snapshot伪装成每次都新增的tool delta。
let added_tools = surface.tools.iter().map(runtime_tool_schema_entry).collect();

// Correct: Native Adapter比较同一source history中的相邻surface事实。
let previous = history.state_at(entry.sequence - 1)?.surface;
let (added_tools, removed_tools, changed_tools) = diff_tools(previous, &surface);
```

```rust
// Wrong: Product compiler复制concrete Agent的默认行为，或provider bridge在最后隐藏拼接。
requirements.push(dash_specific_default_prompt());
provider.system_prompt.push_str(HIDDEN_DEFAULT);

// Correct: concrete Agent先形成自己的intrinsic contribution和accepted ContextFrames。
let mut instructions = vec![dash_intrinsic_instruction()];
instructions.extend(materialize_product_instructions(bound_surface)?);
let digest = materialization_digest(&instructions, &tools)?;
let context_frames = materialize_accepted_context(&instructions, &tools, previous_surface)?;
history.commit(HistoryPayload::SurfaceApplied {
    surface: DashSurface { digest, instructions, tools, context_frames, .. },
})?;
```

```rust
// Wrong: 用内部响应次数伪造Agent turn终态。
for round in 1..=8 { run_provider_round(round).await?; }
return Err(ProviderRoundLimit);

// Correct: Core只响应真实协议终态；外部策略通过显式cancel进入typed terminal。
loop {
    match run_provider_round().await? {
        ProviderTerminal::Stop => break,
        ProviderTerminal::ToolCalls => continue,
    }
}
```
