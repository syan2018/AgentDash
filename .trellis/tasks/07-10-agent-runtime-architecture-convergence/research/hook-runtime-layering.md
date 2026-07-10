# Hook Runtime 分层与 Agent 接入能力评估

## 1. 结论

Hook 不适合整体塞入某一个物理层。它是一个跨层子系统，但每类语义必须只有一个事实 owner：

```text
Workflow / Capability Pack / Project policy
        -> Business Agent Surface 编译 HookRequirement + HookPlanSnapshot
        -> Managed Agent Runtime 负责 outer/runtime hook 的调度、持久化与恢复
        -> Tool Broker 负责平台托管 tool call 的同步 policy hook
        -> Integration Driver Host 协商 HookProfile 并选择 driver route
        -> Native/Codex/Enterprise Adapter 承接必须进入 Agent inner loop 的 hook
        -> Infrastructure 提供 Rhai/command sandbox、repository 与 outbox
```

最重要的边界判断是：

1. **平台 Hook policy authority 不属于 Agent Core，也不属于 Driver。** Workflow、Capability Pack、Project 和 AgentFrame 共同形成声明，Business Agent Surface 将其编译为不可变、带 provenance 的 hook plan。
2. **跨所有 Agent 都成立的 lifecycle hook 由 Managed Runtime 执行。** Thread/Turn admission、terminal、mailbox、context checkpoint、compaction operation 等均已有平台事实边界，不需要 Agent 声明“支持 Hook”。
3. **平台工具的前后置 Hook 由 Tool Broker 执行。** 只要调用经过 Broker，即使外部 Agent 完全没有 native hook API，平台仍能精确执行 deny/rewrite/approval/audit。
4. **只有必须卡在 Agent 内部 provider/tool/stop 边界的 Hook 才需要 Driver/Agent 能力。** Native Agent Core 可用 callback facet；Codex 可以通过自身 lifecycle hook 机制接入；其他外部 Agent 只能按真实能力声明。
5. **callback + steer 是有用的弱化路由，但不是同步 policy hook。** 它适合 observed event、下一 boundary 的 context injection、follow-up 和 continuation，不足以冒充 pre-tool deny/rewrite、permission interception、pre-provider context rewrite 或 pre-compaction cancel。
6. **Hook 能力必须作为正交 `HookProfile` 协商，不属于 L1-L4 等级的必然继承项。** Runtime level 描述 Thread/Turn/Item 生命周期，HookProfile 描述可用触发点、动作、时序与语义强度。
7. **Codex adapter 不应把业务规则编译成散落脚本。** 更稳妥的方式是 materialize 一份受控、稳定的 bridge handler，由 Codex native hook 把 typed event 转发给平台 Hook Engine，再把 typed decision 转译回 Codex。业务规则、审计与 effect authority 留在平台。
8. **AgentFrame 是 resolved Hook surface 的版本锚点，Managed Runtime 是 HookRun 的运行态 owner。** 当前 `AgentFrameHookRuntime` 方向上已经选择了正确业务主键，但它把 pending action、notice、trace、token stats 和 compaction fuse 放在进程内对象中，不能成为目标 durable runtime。

首期不需要 ACP Agent adapter。若 ACP 仅作为平台会话状态传播协议，Hook 只需作为 canonical Runtime Event/Item 投影进入该消息流；ACP consumer 不因此获得任何同步 Hook capability。

## 2. 当前实现事实

### 2.1 已经正确的部分

现有规范和代码里有几项值得直接继承的判断：

- `.trellis/spec/backend/hooks/architecture.md` 明确“Hook 信息获取发生在 loop 外；控制决策发生在 loop 边界”。这个原则应保留。
- `HookControlTarget { run_id, agent_id, frame_id }` 已经把业务 owner 从 RuntimeSession 迁到 AgentRun/AgentFrame；`runtime_session_id` 只作为 adapter provenance。这和目标 AgentFrame surface 一致。
- `AppExecutionHookProvider` 从 workflow projection 构造 snapshot，active authority 来自 `effective_contract`，而不是 connector 自己查询 workflow。
- Rhai 公共执行内核在 infrastructure，Hook adapter 只负责 hook schema/helper/preset。这是正确的 policy/execution mechanism 分离。
- `UserPromptSubmit`、`BeforeTool`、`BeforeStop` 等不同触发点已经有不同的同步语义；`BeforeTool Ask` 在 Agent loop 边界同步等待审批的产品要求也是正确的。
- HookTrace 已经区分 durable/ephemeral/drop，说明项目并不要求每个无行为变化的 observer 都污染 durable history。
- ContextFrame、AgentRun Mailbox 已经分别承担模型可见 context 与 durable delivery，Hook 没有必要再创建一套并行消息系统。

### 2.2 当前职责混合

当前 `agentdash-spi::hooks::HookRuntimeAccess` 同时暴露：

- snapshot load/refresh/evaluate；
- trace ring 与 broadcast；
- pending action 队列；
- turn-start notice 队列；
- token stats；
- compaction failure fuse；
- capability set/delta；
- mutable snapshot replacement。

这不是单一 port，而是 policy store、live cache、event stream、context queue、compaction state 和 capability tracker 的组合。它使 connector/executor 看似只依赖一个 trait，实际却能操作多种业务事实。

`AgentFrameHookRuntime` 的以下状态目前都只在进程内：

- `trace` 最多保留 200 条；
- `pending_actions` 最多保留 64 条；
- `turn_start_notices` 最多保留 64 条并在 collect 时清空；
- token stats；
- consecutive compaction failure count；
- current capabilities。

这些字段一部分应成为 Managed Runtime durable facts，一部分应成为 AgentFrame/Context projection，一部分只应是可丢 live cache。把它们继续放在统一 HookRuntime object 中，会重现当前 SessionRuntime 的多事实源问题。

### 2.3 错误语义不一致

当前错误处理根据调用路径不同而变化：

- Agent Core delegate 路径把 Hook error 映射为 `AgentRuntimeError`，可能直接终止当前 loop boundary；
- Hub 的 `emit_session_hook_trigger` 在 evaluation/refresh/persist 失败时只记录 warning，并返回默认空 effects，相当于隐式 fail-open；
- `ExecutionHookProvider::advance_workflow_step` 和 `append_execution_log` 有默认成功实现；
- `HookRuntimeAccess` 的 compaction/capability/subscribe 方法也有默认 no-op；
- `HookEffect { kind, payload: Value }` 没有由类型表达执行契约、幂等键和 failure policy。

目标态不能由调用位置偶然决定 fail-open/fail-closed。每条 HookRequirement 必须声明 failure policy，运行时按 requirement 执行。

### 2.4 Hook 与 compaction 目前过度绑定

现有 `HookRuntimeDelegate` 同时计算 token pressure、默认 reserve/keep-last、compaction fuse，并把 HookResolution 转成 CompactionParams。目标态中：

- compaction policy、candidate、activation 和 failure recovery 由 Managed Agent Runtime 拥有；
- Hook 可以在 `BeforeContextCompact` 提供 typed policy decision，例如 deny、参数约束或额外 summary instruction；
- Hook 不拥有 token accounting、candidate 写入、active head 或 retry fuse；
- `AfterContextCompact` 只观察已经提交的 canonical operation/result，不能修改既成 checkpoint。

### 2.5 Hook 与 Capability Pack / AgentFrame 的当前关系

项目 taxonomy 已把 Capability Pack 定义为 Agent 级引用清单。目标架构设计也要求将 Pack 展开为 Skill/Tool/MCP/Workflow/Permission/Hook/Context contributions。

因此 Hook contribution 的正确生命周期应是：

```text
Capability Pack / Workflow / Project / Integration builtin contribution
  -> HookDefinitionRef + HookRequirement
  -> Business Agent Surface 按 source precedence / scope / trust 编译
  -> 写入 AgentFrame revision 的 HookSurfaceRef + digest
  -> RuntimeBinding 根据 Driver HookProfile 生成 BoundHookPlan
  -> Managed Runtime/Tool Broker/Driver Adapter 分站执行
```

Capability Pack 不运行脚本，也不直接修改 `.codex`。它只声明 hook definition refs、required/optional、作用域、触发点和所需语义；脚本解析、materialization 与 execution 都由宿主拥有。

## 3. 目标 Hook 概念模型

建议把当前笼统的 “Hook runtime” 拆成五个对象。

### 3.1 HookDefinition

HookDefinition 是用户/工作流/能力包可声明的静态规则：

```rust
struct HookDefinition {
    id: HookDefinitionId,
    source: HookSourceRef,
    trigger: HookPoint,
    matcher: HookMatcher,
    handler: HookHandlerRef,
    required_actions: BTreeSet<HookAction>,
    failure_policy: HookFailurePolicy,
    trust: HookTrustPolicy,
}
```

它不携带 live runtime handle、RuntimeSession ID、pending queue 或 connector object。

### 3.2 HookRequirement

HookRequirement 表达某条规则对执行语义的最低要求：

```rust
struct HookRequirement {
    point: HookPoint,
    phase: HookPhase,
    actions: BTreeSet<HookAction>,
    timing: HookTimingRequirement,
    semantic_strength: HookSemanticStrength,
    failure_policy: HookFailurePolicy,
    required: bool,
}
```

建议的核心类型：

```text
HookPhase:
  Before | After | Observed

HookAction:
  Observe
  AddContext
  Block
  RewriteInput
  RewriteResult
  RequestApproval
  ContinueTurn
  RefreshSurface
  EmitEffect

HookSemanticStrength:
  ExactSynchronous       # 原动作发生前拿到decision并遵守
  ExactDurableBoundary   # 平台operation boundary上事务化执行
  BoundaryAdapted        # 在下一可控boundary生效
  ObservedOnly           # 只能报告既成事实

HookFailurePolicy:
  FailClosed
  FailOpenWithDiagnostic
  RetryDurableEffect
  ObserveOnly
```

例如安全型 `BeforeTool` deny 必须要求 `ExactSynchronous + FailClosed`；遥测型 `AfterTurn` 可以是 `ObservedOnly + FailOpenWithDiagnostic`。

### 3.3 HookPlanSnapshot

Business Agent Surface 将所有来源合并为不可变计划：

```rust
struct HookPlanSnapshot {
    agent_frame_id: AgentFrameId,
    frame_revision: u64,
    policy_digest: Digest,
    entries: Vec<ResolvedHookEntry>,
    source_refs: Vec<HookSourceRef>,
}
```

`AgentFrame` 只需要持有该 snapshot/ref/digest 和 model-visible Hook surface摘要。运行中不重新从 workflow/template/Capability Pack 做隐式查询。Workflow step 或 Pack 变化时先产生新的 AgentFrame revision，再通过统一 surface adoption 边界进入 runtime。

### 3.4 HookProfile / BoundHookProfile

Agent service offer 通过 `HookProfile` 声明 Driver 能提供的 inner hook surface：

```rust
struct HookProfile {
    points: BTreeMap<HookPoint, HookPointCapability>,
    configuration_boundary: HookConfigurationBoundary,
    transport: HookTransport,
}

struct HookPointCapability {
    actions: BTreeSet<HookAction>,
    semantic_strength: HookSemanticStrength,
    scope: BTreeSet<HookScope>,
    acknowledgment: HookAcknowledgment,
}

enum HookConfigurationBoundary {
    StaticService,
    Binding,
    ThreadStart,
    TurnStart,
    HotReplace,
}
```

这里的声明者是受信 Integration/Driver factory，而不是模型在 prompt 中自报能力。Driver Host 在 service instance 探测和 binding 时得到实际 `BoundHookProfile`；conformance harness 用行为验证 descriptor，UI/command admission 只消费 bound profile。

`HookProfile` 只描述必须由 Driver/Agent 执行的部分。平台 outer hook 和 Broker hook 由 host profile 提供，不要求 Agent 重复声明。

### 3.5 BoundHookPlan 与 HookRun

Binding resolver 将每个 resolved entry 路由到真实 execution site：

```rust
struct BoundHookRoute {
    definition_id: HookDefinitionId,
    site: HookExecutionSite,
    delivered_strength: HookSemanticStrength,
    driver_mapping: Option<DriverHookMapping>,
}

enum HookExecutionSite {
    ManagedRuntime,
    ToolBroker,
    AgentCoreCallback,
    DriverNative,
    ObservedEventReaction,
}
```

required requirement 无法满足时，AgentFrame/Capability Pack activation 应返回 typed incompatibility；不能把它悄悄降成 prompt 或 next-turn steer。

每次真实执行生成 canonical `HookRun`：

```text
accepted/running -> completed | blocked | failed | stopped | cancelled
```

行为改变、security/completion gate、context injection、真实 effect 和实质 diagnostic 进入 durable journal。纯 observer/no-op 可以 ephemeral/drop。HookRun 通过 `hook_run_id` 关联 Turn、Item、ToolCall、ContextFrame、Interaction 和 operation，而不是依赖进程内 trace sequence。

## 4. 五类 Hook 的 ownership

### 4.1 Outer lifecycle hook：Managed Runtime

适合由 Managed Runtime 拥有的触发点：

- Before/After Thread start、resume、fork、close；
- Before Turn admission / After Turn terminal；
- Before/After mailbox envelope adoption；
- Before/After runtime binding activation；
- canonical Item/Interaction/terminal 生成后的 reaction；
- AgentRun-level completion gate。

这些触发点位于平台 operation/state machine 边界，平台可以在 driver dispatch 前做 durable decision，不依赖 Agent inner loop。

允许的效果：

- block operation；
- 生成 ContextFrame/AdditionalContext contribution；
- 创建 durable Interaction；
- 创建 mailbox envelope；
- 写 outbox effect；
- 刷新/推进业务 surface。

这里不允许直接改 Agent Core memory、写 vendor config 或调用工具实现。

### 4.2 Managed Runtime hook：context/compaction operation

适合由 Managed Runtime 拥有：

- Before/After context checkpoint activation；
- Before/After managed compact；
- restore/fork context validation；
- context budget policy observation。

`BeforeContextCompact` 可以约束参数或阻断 operation，但不能替代 runtime 的 candidate/activation saga。`AfterContextCompact` 只能在 active head已经 CAS 提交后运行。

如果外部 Agent只支持 native opaque compaction，平台只能产生 `ObservedOnly` native compact event；不能把平台 `BeforeContextCompact` 声明为 exact。如果要 exact，就必须扩展该企业 Agent/Driver 的 prepare-activate 合同。

### 4.3 Tool Broker hook：平台托管工具边界

适合由 Tool Broker 拥有：

- BeforeTool：allow/deny/rewrite/request approval；
- AfterTool：typed result observation、bounded rewrite/redaction、effect；
- tool timeout/cancel/result terminal；
- Permission policy 与 VFS/workspace enforcement。

这一路径不要求 Agent native Hook。只要工具调用通过 direct callback 或 MCP façade进入 Platform Tool Broker，平台就拥有执行前的同步截断点。

能力声明必须区分：

- `brokered_tool_hooks = ExactSynchronous`；
- `driver_native_tool_hooks = Driver profile`；
- 只从 Item stream看到 native tool call，则为 `ObservedOnly`。

不能因为 Agent 能调用平台 MCP tools，就声称平台能拦截它所有 native shell/file tools。

### 4.4 Agent Core inner-loop hook：Core callback / Driver native

只有以下触发点通常必须进入 Agent 实现内部：

- BeforeProviderRequest；
- Driver-native Before/AfterTool；
- provider retry/overflow 的内部边界；
- Agent 自己的 BeforeStop/continuation boundary；
- Agent-native compaction 的 pre/post boundary；
- subagent spawn/stop 的内部边界。

Clean Agent Core 不应认识 Workflow、Capability Pack、Rhai 或 HookDefinition。它只提供小而通用的 policy/observer callback，例如 provider request、tool invocation、turn stop callback。Native Adapter 把这些 callbacks接到平台 Hook Coordinator。

企业 Agent Core 可调整时，推荐直接实现统一 callback contract；这比在 prompt 上模拟 hook 更可靠，也不需要为了“协议纯粹”把边界做成无法演进的标准。

### 4.5 Observed event hook：Managed Runtime event reaction

Canonical Runtime journal 已经能提供：

- Turn/Item started/completed；
- Interaction created/resolved；
- ToolCall result；
- context compacted；
- thread/driver status；
- HookRun itself。

对这些事实做异步 reaction 不需要 Agent 声明 Hook 能力。它是平台 event reaction/automation：可以发通知、写审计、创建下一条 mailbox message或触发可重试 effect，但不能修改已经完成的动作。

建议在类型和 UI 中明确称为 `ObservedEventReaction`，避免用户以为它拥有同步拦截语义。

## 5. callback + steer 能做到什么

### 5.1 可以保真实现

当平台有可靠 callback/event，并且 steer 在明确 boundary 被 Agent确认时，可以实现：

- AfterTurn/AfterItem 观察；
- 下一轮追加 ContextFrame/feedback；
- BeforeStop callback 后的 `ContinueTurn`，前提是 Agent 明确支持 stop gate；
- 异步 workflow/effect 完成后的 durable mailbox follow-up；
- diagnostics/telemetry；
- 已发生动作的补偿提示。

这些应标记 `BoundaryAdapted` 或 `ObservedOnly`。

### 5.2 不能保真实现

只有 callback + 普通 steer 时，不能声明：

- 工具执行前 deny/rewrite/approval；
- provider request 发出前修改 context/system/developer input；
- 修改已经返回给模型的 tool result；
- 在当前生成步骤中保证 policy 立即生效；
- 在 native compaction 提交前取消或修改 candidate；
- security policy fail-closed；
- exactly-once hook effect。

原因是 steer 是另一条输入/continuation 通道，不是当前动作的同步 decision return path。即使最终行为“看起来相似”，竞态、失败恢复和审计保证也不同。

### 5.3 推荐分类

```text
callback only                         -> ObservedOnly
callback + next-turn message          -> BoundaryAdapted(AddContext)
callback + acknowledged stop-continue -> BoundaryAdapted(ContinueTurn)
pre-action callback + typed decision  -> ExactSynchronous
platform operation transaction        -> ExactDurableBoundary
```

## 6. Codex adapter 的 Hook 接入

### 6.1 Codex 已有的 native surface

当前 `references/codex` 中 Codex lifecycle hook vocabulary 包含：

- `PreToolUse`
- `PermissionRequest`
- `PostToolUse`
- `PreCompact`
- `PostCompact`
- `SessionStart`
- `UserPromptSubmit`
- `SubagentStart`
- `SubagentStop`
- `Stop`

handler type 包含 command/prompt/agent，execution mode包含 sync/async，scope包含 Thread/Turn。Hook run有 running/completed/failed/blocked/stopped状态，输出 entry区分 warning/stop/feedback/context/error。App Server Protocol 暴露 `hooks/list`、`hook/started` 与 `hook/completed`，可以作为配置观测和执行回执的一部分。

Codex hook config支持 system/user/project/managed/session/plugin等来源，project `.codex` 来源还涉及 trust/hash review。因此 AgentDash 不能把“能写 `.codex/hooks.json`”简单等同于“已安全安装平台 hook”。

### 6.2 推荐 bridge 方案

Codex Adapter 在 binding/thread-start boundary materialize：

1. 一份稳定、版本化的 AgentDash hook bridge command；
2. 本次 binding需要的 Codex hook event/matcher配置；
3. 受控 callback endpoint/pipe 与短期 credential；
4. `binding_id + generation + hook_plan_digest`；
5. trust/source配置和清理生命周期。

Codex 调用 bridge 时：

```text
Codex native hook JSON
  -> AgentDash Codex Hook Bridge
  -> normalize为HookInvocation
  -> 平台Hook Engine/Coordinator评估
  -> typed HookDecision
  -> bridge转译为Codex native hook output
  -> hook/started + hook/completed关联canonical HookRun
```

业务 Rhai/policy不要生成成大量项目脚本。这样 policy refresh、审计、failure policy和effect仍由平台掌握；Codex 本地文件只是 adapter materialization。

### 6.3 配置位置与更新边界

优先使用 Adapter拥有的隔离 Codex home、session/managed config layer或专用 plugin hook source，不直接改写用户仓库的项目 `.codex/hooks.json`。只有产品明确选择把配置作为项目资产时，才通过受审计的项目变更流程写入。

当前 App Server只有 list/notification，没有一个足以证明 arbitrary hot hook replace已经被运行中 Thread采纳的通用 API。因此首期 Codex HookProfile应声明：

```text
configuration_boundary = ThreadStart 或 Binding
```

Hook plan变化时采用 rebind/new Thread，除非实际实现并通过 hot-reload acknowledgment测试。

### 6.4 Codex 能力不能只看事件名

每个 event要逐动作映射。例如：

- 能收到 `PreToolUse` 不等于所有 tool input都支持 rewrite；
- 能收到 `PostToolUse` 不等于能回滚已经执行的工具；
- 有 `hook/completed` notification 不等于 Hook decision参与了同步执行；
- 有 `Stop` hook不等于任意时刻 steer；
- 有 `PreCompact` 不等于平台拥有 exact context candidate。

因此 Codex Adapter必须声明逐 point/action的 HookPointCapability，而不是一个 `supports_hooks=true`。

## 7. AgentFrame、ContextFrame、Mailbox 与 HookRun

### 7.1 AgentFrame

AgentFrame revision固定：

- resolved HookPlanSnapshot/ref；
- required/optional HookRequirement；
- source/trust provenance；
- policy digest；
- model-visible hook surface摘要。

RuntimeBinding固定：

- BoundHookProfile；
- BoundHookPlan；
- driver materialization revision；
- binding generation。

新的 Hook contribution 或 capability grant需要先创建新的 AgentFrame revision，再做 atomic runtime surface adoption。不能只更新 in-memory HookRuntime snapshot。

### 7.2 ContextFrame

Hook产生的模型可见内容应生成有 provenance 的 ContextFrame：

```text
hook_run_id
hook_definition_id
source_ref
delivery boundary
model channel
rendered text / structured sections
```

它进入统一 context builder/checkpoint，不再依赖 `HookTurnStartNotice` 的进程内 collect-and-clear队列。对 active turn的 steer通过 canonical command/mailbox delivery；对 next turn的 context进入 durable pending context contribution。

### 7.3 Mailbox

Hook可以请求创建 mailbox effect，但不拥有 scheduler、dedup、barrier或resume。`BeforeStop` follow-up、AfterTurn steer、auto-resume都应由 durable effect/outbox创建 mailbox envelope，source identity包含 hook_run_id/effect_id。

### 7.4 HookRun persistence

建议不把 HookTrace另建成第二套 runtime event store。HookRun可以作为 Runtime Item/Operation child或专门 projection，但 authority仍是同一个 runtime journal：

- started/completed/blocked/failed是 canonical event；
- actionful run durable；
- no-op observer ephemeral/drop；
- effect通过 outbox和 idempotency key执行；
- driver notification只有通过 source-id mapping、binding generation和状态顺序校验后才进入 journal。

## 8. Error、阻断与副作用语义

### 8.1 默认策略

| Hook 类型 | 推荐 failure policy |
| --- | --- |
| security/permission/tool admission | FailClosed |
| completion/stop gate | FailClosed |
| context enrichment | FailOpenWithDiagnostic，除非Pack声明required |
| telemetry/observed reaction | ObserveOnly |
| post-operation domain effect | RetryDurableEffect |
| driver bridge transport failure | 按required requirement决定FailClosed或当前operation Lost/Failed |

不能保留“所有错误 warn 后返回空 resolution”或“缺少 implementation默认成功”的通用语义。

### 8.2 typed effects

当前 `HookEffect { kind, payload: Value }` 可以保留 namespaced extensibility，但进入执行前必须解析为已注册 typed effect descriptor：

```text
effect_type
schema/version
idempotency_key
target authority
retry policy
payload digest
```

effect executor只通过 durable outbox运行。同步 gate decision和异步 domain effect必须分开，不能由调用点自行决定“是否/如何执行 effects”。

### 8.3 阻断与改写

- `Block` 只在动作尚未执行的 owner boundary有效；
- `RewriteInput` 必须由实际执行者 acknowledgment并返回 applied digest；
- `RewriteResult` 只能作用于尚未提交给 Agent/model的 broker result；
- post hook无法回滚已提交动作，只能补偿或发 continuation；
- required hook timeout不是 no-op，必须产生 typed failure/interaction/terminal。

## 9. 首期实施建议

建议把 Hook 纳入现有工作流，而不是创建独立大任务：

### Runtime Contract workstream

- 定义 HookPoint/Action/Requirement/Profile/Run/Event；
- conformance harness验证 exact pre-action decision、timeout、terminal、generation fencing；
- Hook 继续是正交 profile，不进入 L1-L4累积继承。

### Managed Runtime workstream

- 实现 outer/runtime Hook Coordinator；
- HookRun进入 canonical journal；
- context injection、mailbox effect、Interaction和outbox durable化；
- compaction hook只围绕 managed operation，不拥有 compaction engine。

### Business Agent Surface workstream

- Capability Pack/Workflow/Project contribution编译成 HookPlanSnapshot；
- AgentFrame revision持有 Hook surface/digest/requirements；
- required requirement不满足时阻止 activation。

### Integration Driver Host workstream

- service offer声明 HookProfile；
- binding协商并生成 BoundHookPlan；
- materialization有 generation、digest和配置更新 boundary。

### Native Adapter workstream

- Clean Core只保留通用 inner boundary callbacks；
- Native Adapter连接 Hook Coordinator；
- 删除 Core 对 workflow/Rhai/Hook snapshot的认识。

### Codex Adapter workstream

- 实现 Codex native hook bridge/materializer；
- 按 point/action声明能力；
- 关联 `hook/started`、`hook/completed`与canonical HookRun；
- 首期按 binding/thread-start配置，不伪造 hot replace。

### Tool Broker workstream

- 平台 tool pre/post/permission hook成为 broker精确边界；
- native tool和brokered tool profile分开；
- MCP只负责call transport，不成为 Hook policy authority。

### ACP state stream

- 首期不建立 ACP Agent adapter或 ACP HookProfile；
- 如需要对外传播平台会话状态，将 HookRunStarted/Completed、ContextInjected、ToolPolicyDecision等canonical event投影到 ACP-shaped stream；
- consumer只获得观察能力，不获得同步阻断/改写权。

## 10. 验收场景

至少需要以下行为测试：

1. 外部 Agent没有 native Hook能力，但调用 brokered platform tool时，BeforeTool deny/approval仍然精确生效。
2. 外部 Agent只提供 callback + steer时，descriptor显示BoundaryAdapted/ObservedOnly，required synchronous Hook会使 Pack/Frame activation失败。
3. Native Core在 pre-provider/pre-tool/stop callback上返回typed decision，Core不查询workflow/repository。
4. Codex binding materialize bridge后，PreToolUse rewrite/block的applied decision可与canonical HookRun、tool call和hook notification关联。
5. Codex bridge断开时，required security hook fail-closed；telemetry hook fail-open并写diagnostic。
6. Hook plan revision变化不会只改live cache；先持久化AgentFrame revision，再在binding boundary采用新plan。
7. Hook产生的ContextFrame和mailbox envelope在进程重启后仍可恢复且不会重复投递。
8. actionful HookRun进入durable journal；silent observer不推进durable cursor。
9. managed compaction hook失败不会让active context head与driver context分叉。
10. ACP-shaped outbound stream能观察HookRun事实，但不能通过该流声明或获得同步Hook控制能力。

## 11. 最终边界摘要

| Concern | Owner | 是否要求 Agent Hook 能力 |
| --- | --- | --- |
| Hook definition / source merge | Business Agent Surface | 否 |
| Hook requirement / AgentFrame surface | Business Agent Surface | 否 |
| Thread/Turn outer lifecycle | Managed Runtime | 否 |
| context/checkpoint/managed compaction hook | Managed Runtime | 否；native opaque compact除外 |
| brokered tool pre/post/approval | Tool Broker | 否 |
| native provider/tool/stop boundary | Agent Core callback或Driver native | 是 |
| Codex `.codex`/managed hook materialization | Codex Adapter | 是，由BoundHookProfile描述 |
| observed runtime event reaction | Managed Runtime event reaction | 否 |
| Rhai/command sandbox | Infrastructure | 否 |
| HookRun journal/outbox repository | Infrastructure adapter，authority在Managed Runtime | 否 |

整体上，最合适的 Agent-specific Hook 接入层是 **Integration Driver Adapter**，但平台 Hook 的业务 authority 仍在 **Business Agent Surface + Managed Runtime**。这样既能完整利用 Codex/可调整企业 Core 的 native hooks，也能让没有 Hook API 的外部 Agent继续获得 outer lifecycle、managed context和brokered tool policy能力，而不会把弱化的 callback/steer冒充成强一致 Hook。
