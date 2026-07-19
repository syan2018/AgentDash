# Agent Runtime 最终架构收敛与状态协议重构

## Goal

把本任务升级为当前分支最终的 Agent Runtime 架构收敛任务，继承
`07-10-agent-runtime-architecture-convergence` 尚未完成的目标，并修正当前 07-17
方案把平台状态、完整 Agent 内部状态和 execution coordination 合并进单一
`AgentSession` aggregate 的错误边界。

最终架构必须同时满足：

1. AgentDash 只有一套面向所有 Agent 实现的 `Managed Agent Runtime` 外层；
2. Dash Agent、Codex、pi-coding-agent/企业 Agent 都以“完整 Agent 实现”身份接入；
3. Dash Agent 内部继续拆分为拥有完整运行生命周期的 Agent 层和 pi-like
   无隐藏持久状态的 `AgentCore`；
4. Runtime 根据平台期望能力与 Agent 实际能力做逐项 admission、binding 和
   semantic fidelity 判断；
5. 平台状态、Agent 内部状态、只读映射、有限命令和 Host coordination 各有唯一
   owner；
6. Fork、compaction、reconnect、Tool、Hook、effect recovery 和协议投影在 Native、
   Codex、Remote 等实现之间保持统一产品语义，同时诚实表达实现能力差异；
7. 完成 07-10 尚未落地的 crate 物理收敛，删除错误命名、重复 crate、旧 SPI 和
   journal-centric 事实链；
8. 完整继承
   `07-12-canonical-runtime-session-presentation-convergence` 已固定的会话展示契约：
   AgentDash-owned、Codex App Server Protocol-shaped 的 canonical presentation、
   AgentDash typed extensions、可复现 Rust/TypeScript 生成链与现有
   `features/session` reducer/renderer 产品行为不得因状态权威重构而改变。

## Task-scoped contract precedence

本任务修正 07-12 的持久化与恢复 owner：`RuntimeJournalFact` 不再作为 universal state
envelope，Runtime、Host、Dash Agent、External Agent 与 Product 分别使用自己的 durable
authority，Application/UI reconnect 改为 Runtime snapshot revision + committed change
tail。

该修正不废除 07-12 的可观察 presentation contract。对于会话消息协议、事件 family、
payload、source identity、顺序、`null`/omitted 形状、AgentDash extension、前端
feed/reducer/renderer/tool registry 与用户可见副作用，
`07-12-canonical-runtime-session-presentation-convergence` 继续作为本任务的
authoritative baseline。物理 crate、文件名和 carrier 可以重组，但 canonical protocol
生态与产品行为必须无损迁移并通过同一 parity gate。

## First-principles invariants

### FP1 — 统一外层

AgentRun、UI、Workflow 和平台业务不能理解具体 Agent vendor。所有 Agent 都必须先经过
统一 Runtime，再由 Host 绑定到某个完整 Agent service。

### FP2 — 完整 Agent 是可替换边界

完整 Agent 至少拥有自己的 history、fork、context/compaction 与运行生命周期。Codex
和 pi-coding-agent 是这种边界的完整实现；Dash Agent 也是其中一个实现，不是统一
Runtime 本身。

### FP3 — AgentCore 只是 loop machine

`AgentCore` 只处理一次输入到输出的 provider/tool loop、streaming、cancel 和纯计算
primitive。它不拥有 durable history、fork、compaction、平台 binding、AgentRun 或
recovery 事实。

### FP4 — 状态权威按事实类别切分

同一事实只能有一个写 owner：

- 平台 Runtime 写 platform command/operation、admission、normalized
  snapshot/change、surface、AgentRun mapping 和平台 recovery 事实；
- Host 写 service instance、offer、binding、placement、generation、lease 和 effect
  delivery；
- 完整 Agent 写自己的 history、AgentSession/Thread、fork、context、compaction、
  internal tool loop 和 native lifecycle；
- Application 写 AgentRun、Lifecycle/Frame、Companion 和产品业务状态；
- protocol/UI/journal 只消费 committed change，不反向驱动业务状态。

### FP5 — 有限命令，不共享内部状态机

平台通过有限 typed command 改变完整 Agent，通过 typed snapshot/change/observation
读取其可公开事实。平台不能复制外部 Agent 内部表并宣称自己是其恢复事实源。

### FP6 — 能力是逐项可证明的合同

工具注入、Hook、Fork、compaction、steer、interrupt、typed interaction、context
read/apply、change tail 等能力必须由 `RuntimeOffer`、`AgentSurfaceSnapshot`、
`BoundAgentSurface` 和 `AppliedAgentSurface` 表达。布尔
`supports_tools/supports_hooks`、默认成功和隐式降级都不成立。

### FP7 — Fork 是一等产品能力

Fork 已被 AgentRun 分叉、Companion 上下文继承和后续分支会话依赖，必须保留为 typed
command、capability、receipt、lineage 和 recovery saga，不能退化成 prompt 拼接或
presentation journal 截断。

### FP8 — 未上线项目直接到达正确终态

不保留旧 Runtime journal、connector、schema、API 或前端 reducer 的兼容读取、双写、
fallback 和过渡 facade。数据库通过 forward migration 到达唯一最终模型。

### FP9 — Session 只表示 history-maintained state

一个对象只有在其完整业务状态可由有序 history 唯一维护和重建时，才可以使用
`Session` 命名。输入通过形成新的 history contribution 改变 Session；fork 是 history
tree 分叉；compaction 是带 provenance 的 history 变换；resume 是从 history 恢复。

需要 operation、mailbox、surface、binding、credential、placement、lease、effect、
recovery ledger 或平台业务表才能确定的状态不属于 Session。为查询或性能建立的
projection/index 可以存在，但不得成为独立写入的第二事实源。

### FP10 — 状态权威重构不改写 presentation language

Complete Agent source language、Managed Runtime normalized language 与 App Server
presentation language 可以分层存在，但前两者必须通过穷尽、无损、可机械验证的 projector
收敛到同一 AgentDash-owned canonical presentation。Runtime vocabulary 不能直接取代
浏览器会话协议，vendor DTO 也不能泄漏到 Runtime/Application/frontend。

canonical presentation 必须完整保留 07-12 固定的 Codex App Server standard families、
AgentDash typed extensions、事件数量与顺序、source IDs、timestamps、显式 `null`、typed
interaction、tool lifecycle 与 terminal evidence。前端继续使用既有 `features/session`
feed/reducer/renderer；只允许更换 snapshot/change hydration 与 carrier unwrap seam。

## Confirmed current facts

1. 当前分支已经形成
   `Application / AgentRun → Managed Agent Runtime → Integration Driver Host → Adapter`
   的统一外层基础，并实现 RuntimeOffer、Surface、binding、placement、lease、recovery
   等主要骨架。
2. 当前 Integration seam 仍以
   `AgentRuntimeDriverContribution + AgentRuntimeDriverFactory + AgentRuntimeDriver`
   为中心，Codex 和 Native 仍被抽象成低层 driver，缺少“完整 Agent service”的
   command/read/change/fork 合同。
3. 当前 `RuntimeJournalFact` 同时承载 presentation 和 internal coordination；
   Runtime、AgentRun feed/fork、context 和 adapter 又依赖该 journal，仍存在循环
   ownership。
4. 当前 AgentRun fork 聚焦测试 5/5 通过，Native checkpoint fork 测试 1/1 通过；
   因此 Fork 没有被整体改坏。
5. 当前 Codex 产品 fork 会调用原生 `thread/fork`；Native 产品 fork 只创建新 source
   binding，没有接上已经存在的 checkpoint/history import 路径。
6. 当前 fork 的 product receipt、child graph、Agent service fork、Host binding、
   surface publication 和 AgentRun mapping 跨多个 durability boundary，只覆盖同步
   compensation，尚未形成 crash-safe saga。
7. 当前 Companion dispatch 主要是
   `fresh Agent + prompt/capability inheritance`，并不等价于 AgentRun/Agent history
   fork；其需要完整 fork 的模式尚未与 Agent service contract 对齐。
8. `references/codex` 证明 Codex 自己拥有 ThreadStore/history、fork、compaction、
   resume 和 recovery 真相，但没有提供 exact context apply 或稳定 durable
   `changes(after cursor)` 的公共合同。
9. `references/pi-mono` 的低层 agent loop 是适合作为 AgentCore 的干净参考；其
   AgentHarness/pi-coding-agent 层才负责 history、tree、fork、compaction 和完整
   lifecycle。
10. 当前 `agentdash-agent` 仍依赖 `agentdash-agent-types` 并承担 loop 之外的类型与
    状态职责；`agentdash-agent-types`、`agentdash-agent-protocol`、
    `agentdash-executor`、`agentdash-spi`、`agentdash-application-hooks` 等 07-10
    目标删除/重命名项仍在 workspace。
11. 07-17 中 typed Turn、compaction saga、stable effect identity、generation
    fencing、snapshot+change、outbox、queue promotion 和 journal 删除方向具有价值，
    但只能按正确 owner 分别落入 Runtime、Host 或 Dash Agent，不能形成一个跨所有
    Agent 实现的万能 `AgentSession`。
12. `agentdash-application-runtime-session` crate 已经从 workspace 物理删除，但
    `agentdash-application-ports`、API、contracts、SPI、Relay 与 gateway 中仍有
    `RuntimeSession*` ports/DTO/field/event 命名和旧 delivery/live capability 语义；
    这些平台状态不满足 FP9，仍属于 07-10 未完成的语义清理。

以上事实的详细代码、migration 与测试证据位于：

- `research/current-runtime-fork-state-boundaries.md`
- `research/codex-session-history-runtime-boundaries.md`
- `research/pi-mono-agentcore-crate-convergence.md`
- `research/current-compaction-state-and-codex-reference.md`
- `research/agent-boundary-and-journal-ownership.md`

## Terminology

| 术语 | 含义 |
| --- | --- |
| `AgentRun` | Application/Product 层可授权、可编排的 Agent 运行对象 |
| `Managed Agent Runtime` | 覆盖所有 Agent 实现的统一平台外层 |
| `Runtime State` | Runtime 自己拥有的 operation、admission、projection、mapping 与平台一致性事实 |
| `Complete Agent Service` | Runtime/Host 接入的完整 Agent 替换边界 |
| `Dash Agent` | AgentDash 自有完整 Agent 实现 |
| `AgentSession` | Dash Agent 内部完全由有序 history 维护和重建的状态；输入、fork、compaction、resume 都表现为 history 语义 |
| `AgentCore` | Dash Agent 下方无隐藏持久状态的 provider/tool loop |
| `Agent Surface` | 平台希望交付给 Agent 的 instructions/tools/hooks/workspace/context requirements |
| `RuntimeOffer` | 某 Agent service instance 实际可兑现的能力与 fidelity |
| `RuntimeBinding` | Runtime thread/AgentRun 到 service instance、generation、surface 的稳定绑定 |

`Session` 不作为跨层万能架构词汇。超出 history-maintained state 的对象不得使用
`Session` 命名。该词只在满足 FP9 的 Dash Agent 内部对象、vendor 原生术语、协议字段
或待删除旧符号的准确引用中出现。

## Requirements

### R1 — 最终分层与依赖方向

目标调用链必须是：

```text
API / UI / Workflow
  -> Application / AgentRun facade
  -> Managed Agent Runtime
  -> Agent Runtime Host
  -> Complete Agent Service seam
      -> Dash Agent -> AgentCore
      -> Codex
      -> pi-coding-agent / Enterprise Agent
      -> Remote proxy -> remote Complete Agent
```

- Application 不依赖具体 Agent、Host、vendor DTO 或 Agent 内部 repository。
- Runtime 不依赖 Dash Agent、Codex、Relay 或具体 transport。
- Complete Agent service contract 不依赖 Product Domain。
- AgentCore 不依赖 Runtime Contract、AgentRun、Codex、Relay、Infrastructure 或
  Dash Agent persistence。
- 只有 composition root 同时看到 Runtime、Host、Integration、Agent 实现和
  Infrastructure adapter。

### R2 — Complete Agent Service contract

建立 AgentDash-owned 的完整 Agent 接入合同，至少覆盖：

- `describe`：报告 capability profile、fidelity、configuration boundary；
- create/start、resume、fork、close；fresh create 可携带 immutable typed
  `InitialAgentContextPackage`，其 applied digest 必须进入 receipt/inspect；
- submit input、steer、interrupt、request compaction、resolve interaction；
- read authoritative snapshot；
- 可选 ordered change subscription；
- inspect command/effect 结果以支持 unknown-outcome recovery。
- apply/update/revoke `BoundAgentSurface` 并返回 `AppliedAgentSurface` evidence；
- Agent-native Tool/Hook 向 Runtime Host callbacks 发起 typed reverse call。

合同使用有限 typed vocabulary、stable command/effect identity、accepted/terminal
receipt 和 opaque source coordinate。它不暴露 Runtime repository、Agent 内部表或
vendor DTO。Reverse Tool/Hook call 必须携带 binding generation、Turn/Item/effect
identity、deadline、idempotency 和 semantic requirement；remote 实现通过同一 wire
提供 request/decision/result/ack/replay。

### R3 — Capability、Surface 与 admission

继续以 07-10 的四对象模型作为唯一能力拼装链：

1. Runtime 编译 `AgentSurfaceSnapshot`；
2. Host 从 Agent service descriptor、instance、credential、health、placement 和
   transport guarantee 归一 `RuntimeOffer`；
3. Runtime 逐项求交得到 `BoundAgentSurface`；
4. Agent adapter/materializer 返回 `AppliedAgentSurface`。

能力必须按 command、state read、delivery route 和 semantic strength 分项描述。
required 能力不足时 typed reject 且不产生 side effect；optional 项只在 manifest
明确允许时省略。PromptOnly、Observed、Approximation 不能满足 Exact requirement。
Initial context 另行声明可接受的 contribution kinds、`TypedNative` /
`CanonicalRendered` delivery fidelity 与 applied-digest evidence；只有产品 requirement
允许的 fidelity 才能通过 admission。

Fork 作为 Runtime 的正式 capability 与 command 保留。当前代码已经以
`LifecycleCapability::ThreadFork` 和 command availability 按 binding/profile 做准入，
因此 Agent service 可以注册为不支持 Fork 的受限实现；“完整支持档位”以及任何
branch/Companion history inheritance 的 Agent Surface 必须要求 exact Fork。工具注入、
Hook 等同样按 Agent 实现支持程度分层，并固定唯一 causal route。

### R4 — 权威状态与持久化边界

设计必须给每一类字段标注：

- authoritative write owner；
- durable repository；
- 允许的读取者；
- command-writable / read-only / derived；
- revision/fidelity/provenance；
- crash recovery 依据。

平台 normalized Thread/Turn/Item/Interaction 是 AgentDash 产品合同的可持久 read
projection；对于外部完整 Agent，它不是对方 history/context 的恢复事实源，也不能
反向写回。Dash Agent 的有序 history 才是其 `AgentSession` 状态、
fork/context/compaction 的权威；AgentSession projection 必须能由 history 重建，不能
通过旁路状态写入改变。

Host binding/effect/placement/recovery 继续保持独立 aggregate，通过 stable identity
与 Runtime operation 关联，不合并进 Dash Agent `AgentSession`。

### R5 — Runtime command、operation、mailbox 与 change

Managed Runtime 统一负责：

- AgentRun 到 Runtime/Agent source coordinate 的 stable mapping；
- command admission、idempotency、expected revision 和 availability；
- platform operation lifecycle 与 pending command/mailbox；
- Agent command effect 的 durable dispatch、inspect 和 reconciliation；
- normalized snapshot、durable platform change tail 和 outbox；
- surface/binding revision 与 platform consistency。

Product mailbox、Runtime pending command 和 Agent 内部 queue 必须使用不同类型与表，
不能共享 phase 或依靠命名猜 owner。

UI/Application reconnect 使用 Runtime snapshot revision + platform change tail；
cursor gap 时重读 snapshot。Agent service 的 source change tail 可按 capability 分级，
但 Runtime 对 Application 的 durable change contract 不得因此降级。

### R6 — Fork 与 Companion

Fork 必须重写为 crash-safe platform saga + complete Agent native fork：

1. Application/AgentRun transaction 建立唯一 durable fork saga，预分配稳定 child
   product IDs，保存 immutable cutoff 和 product receipt；
2. Runtime 根据 BoundAgentSurface admission，并 durable accept operation/effect
   intent；Host 只在 intent 持久化后调用 Agent；
3. Agent 以 stable effect identity 执行幂等 native fork；unknown outcome 只通过
   inspect/reconcile，不更换 identity；
4. Host 提交 child source coordinate/binding，Runtime 提交 provisioning child
   mapping/projection；
5. Application 提交 child product graph/lineage/Runtime binding，再显式激活 Runtime
   child；
6. saga 最后 terminalize product receipt；任意阶段 restart 从 durable phase 继续。

Agent 已创建 child 但平台无法完成映射时，saga 进入 `Lost` 并保留已知 child
coordinate、effect identity 和预分配 product IDs，禁止第二次 fork 或同步删除式
“补偿”。

平台不得通过 presentation journal 拼接重建 Agent history。Native/Dash Agent 产品 fork
必须接入真实 history/checkpoint fork；Codex 使用原生 `thread/fork`。

Companion 必须显式区分：

- `CompanionSliceMode::Full`：通过 Complete Agent exact Fork 继承 parent Agent
  history lineage；
- `Compact / WorkflowOnly / ConstraintsOnly`：创建 fresh Agent，并只交付对应 typed
  context package。

通用 Complete Agent 合同使用平台中立的 `InitialAgentContextPackage`，不得包含
Companion、AgentRun 或 vendor DTO。Package 具有 stable package ID、schema version、
mode、typed contributions、每项 authority/revision/digest provenance 和整体 digest；
contribution vocabulary 至少区分 compact summary、workflow context 与 constraint set。
Workspace/VFS、Tool、Hook、credential 和 capability grant 仍由 Agent Surface 交付，
不得塞入 context package。

Fresh Agent 的 `CreateAgentCommand` 原子携带 package；create receipt/inspect 必须证明
相同 package digest 以何种 delivery fidelity 被应用，Runtime 在该 evidence 到达前不
激活 child。Companion 的派发任务在 create 成功后作为首个普通 `SubmitInput` 提交；
`SubmitInput` 不得替代初始 context 安装，也不得把 package 退化为不可追踪 prompt。

`adoption_mode` 只控制 child 结果如何回到 parent，不参与 history 创建方式。需要历史
继承的 `Full` 不得用 prompt 模拟 fork；裁剪模式不得先复制完整 history 再隐藏。Fork
child 在 ancestor binding 替换、journal retention 或 ancestor 删除后仍必须具备明确、
可验证的恢复语义。

### R7 — Compaction ownership 与 tracer bullet

Compaction 是完整 Agent 生命周期能力，不是所有 Adapter 共用的平台 context kernel：

- Dash Agent 内部拥有 typed ContextRevision、manual/automatic compaction、active
  maintenance Turn、queue 与 A/B/C continuation；
- Codex 使用自己的 native compaction/history replacement，Runtime 只发送支持的命令并
  映射可证明的 activity/item/result；
- 其它 Agent 通过 capability 声明 Agent-owned compaction、exact platform-supplied
  context control、read-only observation 或 unsupported。

Dash Agent compaction 必须保持 07-17 已建立的强约束：

- manual compaction active 时新输入 durable deferred，不 steer 进 compaction；
- active normal Turn 期间 manual compact 可排队，但 queued 状态不创建伪 Turn/Item；
- automatic overflow 的 Agent Turn A、Compaction Turn B、Continuation Turn C identity
  独立；
- B terminal commit 不隐式创建 C，C 由独立 durable continuation request promotion；
- clean failure exactly-once terminalize dependent continuation，Lost 阻塞 promotion；
- context head、compaction/item/turn terminal 和 agent-local operation 在 Dash Agent
  transaction 中收敛；
- worker claim/retry/release 只是 delivery，不是 Agent 业务 phase。

Runtime 对外只投影统一的 typed activity、operation 和 availability，不要求外部 Agent
复制 Dash Agent 的内部 ContextRevision schema。

### R8 — Tool 与 Hook ownership

- Runtime 编译平台 Tool/Hook requirements；
- Host/adapter materialize Bound surface 并记录 applied evidence；
- Runtime Tool Broker 拥有平台暴露工具的 policy、permission、effect 与回调路由；
- 完整 Agent 拥有自己内部 tool loop 和 native hooks；
- Dash Agent 可以把 bound tools/hooks 注入 AgentCore callback；
- Codex/企业 Agent 按真实 native/broker/host 能力声明；
- 每个 Tool/Hook contribution 只有一个执行 owner，不允许 Runtime 与 Agent 双触发。

Hook capability 按 HookPoint、timing、blocking/mutation/effect semantic strength 描述，不使用
`supports_hooks=true`。Complete Agent service apply surface 时获得 typed
`AgentHostCallbacks` route；Agent-native tool call、blocking/mutating Hook 必须经该
reverse channel 返回 correlated result/decision，不能越过 service seam 调具体 adapter
API。

### R9 — Journal、协议与前端

删除 `RuntimeJournalFact` 作为 universal state envelope：

- Runtime platform facts 写 normalized tables + platform change/outbox；
- Dash Agent 内部事实写 Dash Agent repository/change；
- 外部 Agent 事实通过 snapshot/change/observation 映射；
- App Server notification、AgentRun feed、audit、search、analytics 都是 change 下游；
- command admission、Agent recovery、fork、context、terminal 不从 presentation
  journal replay 推导。

Runtime committed projection/change 必须携带足以无损生成 canonical App Server
presentation 的完整 typed body 与 source evidence；不得先压成
`ManagedRuntimeItemBody` 摘要，再在 API/frontend 按 kind、tool name 或 generic JSON
猜回协议事件。若 Service API 与 Runtime Contract 保留独立 vocabulary，必须对所有
standard/extension families 提供穷尽 projector、roundtrip/parity fixtures 与 generated
closure gate。

浏览器会话协议继续采用 AgentDash-owned、Codex App Server Protocol-shaped canonical
contract：

- Codex standard DTO 从 pinned upstream schema 机械生成，并与 AgentDash typed
  extensions 组合；
- physical codegen crate 可以迁移或合并，但 Rust/TypeScript owned roots、schema lock、
  freshness check 和 vendor↔owned parity 必须保留；
- App Server notification、Turn/Item/Input/Interaction 的 identity、payload、顺序与
  nullable/omitted 语义必须与 07-12 baseline deep equal；
- `features/session` 原 feed、reducer、renderer、tool registry、context/interaction/
  compaction cards 与 side-effect behavior 保持产品合同；只把数据源从 presentation
  journal 换成 Runtime snapshot/change projection；
- Workspace、Terminal、Canvas、mailbox、Lifecycle/control-plane 等平台业务继续使用
  独立 Product feed，不塞入 Agent conversation protocol，也不由 Session UI 反推平台
  状态。

Codex-shaped `ContextCompaction item/started → item/completed`、失败 terminal、Turn/Item
identity 和顺序是上述完整 App Server parity 的一部分，而不是唯一需要保留的 family。
前端不得固定把 compaction 解释成 completed，也不得复制另一套运行状态机。

### R10 — Crate 最终收敛

本任务必须完成而不是再次推迟 07-10 的物理清理：

- 将当前低层 `agentdash-agent` 清理/迁名为 `agentdash-agent-core`；
- 新建或重建 `agentdash-agent` 作为 Dash Agent 中层，以有序 history 维护
  `AgentSession`，并拥有 fork、compaction 与 history-derived lifecycle；
- 删除 `agentdash-agent-types`，按 owner 拆入 Core、Dash Agent、Runtime Contract 或
  Agent service contract；
- 保留并收窄 `agentdash-agent-runtime-contract` 为 Application ↔ Runtime 公共合同；
- 保留 `agentdash-agent-runtime` 为统一平台 Runtime；
- 保留 `agentdash-agent-runtime-host` 为 service/binding/placement/effect host；
- 新建 dependency-light 的 `agentdash-agent-service-api` 作为 Complete Agent Service
  contract；保留 broader `agentdash-integration-api`，其 Agent 模块只依赖/re-export
  service API；
- 拆分并删除当前混合职责的 `agentdash-agent-protocol` crate：vendor DTO 归
  Codex integration，Runtime wire 与 Product projection 分归各 owner；其中
  AgentDash-owned canonical App Server presentation vocabulary、typed extensions、
  Rust/TypeScript 生成 roots 与 parity gates 必须先整体迁入 dependency-light 的最终
  protocol/contract owner，不能随旧 Backbone journal/platform envelope 一起删除；
- 删除/替换 `agentdash-executor` 的旧 connector/execution 路径；
- 清理 `agentdash-spi` 的 Agent 内容，剩余平台 SPI 迁名 `agentdash-platform-spi`；
- 删除 `agentdash-application-hooks` 和其它已被 Runtime Surface/Tool Broker/Agent
  seam 吸收的 pass-through crate；
- `agentdash-application-runtime-session` 已物理删除，继续清除
  `agentdash-application-ports`、API、contracts、SPI、Relay/gateway 中不满足 FP9 的
  `RuntimeSession*` delivery/live/capability/DTO/event 残留，迁为 Runtime Thread、
  Runtime Binding、AgentRun 或明确 vendor source terminology；
- 将 `agentdash-application-runtime-gateway` 迁名为不与 Agent Runtime 混淆的 extension
  gateway；
- `agentdash-relay` 只承载 Runtime/Agent service wire placement，不拥有 Agent 状态；
- 保留 `agentdash-agent-runtime-wire` 作为 Runtime gateway、Remote Complete Agent 与
  Relay 的共享跨进程 framing/codegen 边界；Runtime/Agent 业务 DTO 仍分别归各自
  contract module；
- 保留 `agentdash-agent-runtime-test-support` 作为 Native/Codex/Remote 共用
  conformance harness，删除其中无消费者的旧 facade。

每个保留 crate 必须有不可由相邻 crate 吸收的依赖边界、生成协议、替换点或
infrastructure adapter 理由。

### R11 — Migration、hard cut 与验证

- 提供一次 forward migration，把 Product、Runtime platform state、Host
  coordination、Dash Agent internal state、external Agent projection 分区；
- 删除旧 journal/session/connector/hook/executor 相关表与字段，不保留 dual path；
- PostgreSQL constraint 表达 active slot、idempotency、effect identity、binding
  generation、projection revision 和 Dash Agent context head CAS；
- 对 Runtime、Complete Agent service、Dash Agent/Core、Native、Codex、Remote、
  Tool/Hook、Fork、Compaction、Relay 和 App protocol 建立分层 conformance tests；
- Dash Agent command/effect settlement、history append/head CAS、derived change 与下一
  continuation intent 在同一 `DashAgentCommit` transaction 中提交；不建立可能丢失或
  重复 history contribution 的跨事务 append handoff；
- 保留当前 6 个通过的 fork 回归，并增加 Native 产品 fork、Companion、crash point、
  ancestor retention、cutoff fidelity 和 unknown outcome 测试；
- 通过依赖负向检查证明 Core、Application、Runtime、Host、Adapter 之间没有反向依赖或
  vendor/product 类型泄漏；
- W1–W9 只作为需求、依赖与验收分解；实施派发使用粗粒度纵向 bundle，可交接的稳定
  边界使用 S0–S6 checkpoint，三者不得机械地一一对应；
- S0–S4 允许在隔离 test composition 中建立 target lane，但不得提前改变 production
  composition、canonical generated contract、正式 repository/schema 或默认 caller；
- S5 把 caller、contract/crate identity、composition、repository/schema、projection
  与 legacy deletion 作为一个 cutover unit 激活。S5 完成后必须只有一条 production
  path、一个事实 owner 和一套 canonical schema/contract；
- 每个 stable checkpoint 都通过真实 tracer bullet 证明 direct run、fork、Companion、
  compaction、tool/hook、reconnect/recovery 与协议投影仍有完整路径，不能只以编译通过或
  negative search 代替。

## Constraints

- C1. 本任务是当前分支最终架构收敛任务，07-10 未完成目标全部纳入，不再创建一个只修
  compaction 的局部后续任务。
- C2. 项目尚未上线，不提供旧 API/schema/state/interface 的兼容层、fallback、双写或
  backfill；数据库结构必须通过 forward migration 正确切换。
- C3. `Session` 只表示完整状态可由有序 history 唯一维护和重建的对象。统一 Runtime
  不得用 `Session` 合并 platform operation、mailbox、surface、binding、credential、
  placement、lease、effect、recovery 或其它平台业务状态；这些事实也不得进入 Dash
  Agent `AgentSession`。
- C4. AgentDash 自有完整 Agent 的正式简称为 `Dash Agent`。
- C5. Fork 是必须保留并补全的产品能力；不得因抽象收敛删除、弱化或改成 prompt
  approximation。
- C6. 能力不足使用 typed unsupported/incompatible 与 admission gate 表达，不实施
  compatibility/fallback。
- C7. 本规划可修改 Trellis task artifacts；在用户审阅并批准前不得运行
  `task.py start` 或修改实现代码。
- C8. 工作区并行修改属于其他会话，实施和检查必须按文件 ownership 避免覆盖。
- C9. 多智能体实施只使用当前主会话内嵌 subagent 工具，不建立 Trellis channel。派发按
  Platform Runtime、Dash/Native、External Agents、Product/Protocol、Hard Cut 与 Final
  Conformance 粗粒度 bundle 组织；除共享热点外，bundle owner 有权围绕纵向结果调整内部
  模块和文件粒度。

## Out of Scope

- 在本规划阶段修改 Rust/TypeScript 实现、执行 destructive migration 或启动完整开发环境。
- 为尚无调用方的 ACP 创建 execution driver；未来只能作为 Runtime snapshot/change 的
  read-side projection。
- 在本任务中新增一个真实 pi-coding-agent adapter；pi-mono 作为 Core/Agent ownership
  参考，但目标 seam 必须允许后续无平台侵入接入。
- 扩展通用 analytics/search/audit 产品功能；本任务只固定其 change 下游位置。
- 为旧预研数据、旧 journal event、旧 connector 或已经被 canonical contract 取代的
  前端 DTO 保留兼容读取、双写、fallback。
- 以 Runtime-aware renderer、简化 `ManagedRuntimeItem` UI 或 generic JSON card 重写
  07-12 已固定的 canonical Session 产品行为。

## Acceptance Criteria

- [ ] AC1. PRD/design 明确保留 07-10 的统一 Runtime 外层，并把 Dash Agent、Codex、
  pi-coding-agent/企业 Agent 画为 Complete Agent seam 下的同级实现。
- [ ] AC2. 目标依赖图包含 `Managed Runtime → Host → Complete Agent Service` 与
  `Dash Agent → AgentCore`，且每条禁止反向依赖都有自动或 `rg/cargo metadata`
  检查。
- [ ] AC3. Complete Agent contract 定义 typed command、snapshot、可分级 change、
  capability/fidelity、receipt、inspect、surface apply/revoke、AgentHostCallbacks、
  source coordinate 与 `InitialAgentContextPackage` create/apply evidence，不暴露
  Product/Companion、vendor DTO 或 Agent 内部 repository。
- [ ] AC4. `AgentSurfaceSnapshot × RuntimeOffer → BoundAgentSurface →
  AppliedAgentSurface` 对 Fork、Tool、Hook、Context、Compaction、Interaction 等逐项
  给出 required/optional、route 与 semantic strength。
- [ ] AC5. ownership matrix 对 Application、Runtime、Host、Complete Agent、Dash
  Agent/Core、protocol/UI 的每类状态标明唯一写 owner、durability、读写权限和 recovery
  依据。
- [ ] AC6. Runtime normalized conversation 是平台 durable projection；外部 Agent
  history/context recovery 不依赖该 projection；Dash Agent `AgentSession` 的全部
  状态可从有序 history 重建，且不存在 operation/binding/effect 等旁路写入。
- [ ] AC7. Fork 设计覆盖 Application-owned durable saga、预分配 child IDs、product
  cutoff、Runtime durable intent、native fork、Host binding、Runtime provisioning、
  product graph/lineage commit、explicit activation、unknown-outcome inspect 和每个
  crash boundary reconciliation，不从 presentation journal 重建 history。
- [ ] AC8. 当前 fork 6/6 回归继续通过，并新增 Native 产品 fork 真实继承、Companion
  fork/fresh 区分、initial package digest/fidelity/首个 input 顺序、多级 fork、
  entry/turn cutoff、ancestor retention 和所有 crash boundary 测试。
- [ ] AC9. Compaction tracer bullet 分别给出 Dash Agent 内部 exact 状态机、Codex
  native 映射和其它 Agent capability gate；平台不要求外部 Agent 复制
  ContextRevision。
- [ ] AC10. Dash Agent automatic overflow 测试证明 A/B/C identity 独立、B terminal
  不隐式创建 C、clean failure/Lost 与 mailbox promotion 符合 R7。
- [ ] AC11. Tool/Hook 测试证明每项 contribution 只有一个 causal owner，required
  apply 未 ack 不可 dispatch，PromptOnly/Observed 不满足 Exact，Agent-native
  blocking/mutating call 只经 typed AgentHostCallbacks/wire 往返。
- [ ] AC12. 删除 `RuntimeJournalFact` 后，Runtime snapshot/change、AgentRun feed、
  reconnect、fork、Dash Agent context/compaction、Codex read/recovery 均不依赖
  presentation journal replay；snapshot/change 同时保存并投影完整 canonical
  presentation，不从窄 Runtime summary 反向重建。
- [ ] AC13. 07-12 的 main/current presentation parity gate 继续通过：App Server
  standard families、AgentDash extensions、完整 payload、identity、顺序、
  nullable/omitted、typed tools/interactions/context/compaction 与
  `features/session` reducer/renderer/side effects 均保持 deep equality；另外覆盖
  cursor gap snapshot reload 和 derived availability。
- [ ] AC14. `design.md` 给出目标 schema、forward migration、effect reconciliation、
  binding generation、projection revision 与 Dash Agent internal repository 的事务
  边界。
- [ ] AC15. `implement.md` 将工作拆成依赖明确、可独立验证的 Runtime contract、
  Agent service seam、Dash Agent/Core、Runtime/Host persistence、Native/Codex/Remote、
  Fork/Compaction、Application/UI、crate deletion 工作包。
- [ ] AC16. `implement.jsonl` 与 `check.jsonl` 只引用最终设计仍适用的 spec/research；
  不再把 07-10 旧 research 的实施前问题当作当前事实。
- [ ] AC17. 最终 workspace 不再包含 `agentdash-agent-types`、旧
  `agentdash-agent-protocol`、Agent connector 版 `agentdash-executor`、
  Agent 内容版 `agentdash-spi`、`agentdash-application-hooks` 或其它已被吸收的
  pass-through crate；`agentdash-application-runtime-session` 保持物理缺席，且
  Application/API/contracts/SPI/Relay 不再以 `RuntimeSession*` 命名平台非 history
  状态。旧 protocol crate 删除前，canonical App Server owned vocabulary、typed
  extensions、codegen/freshness/parity 与 frontend generated roots 已由最终 owner
  完整接管，删除后不存在协议能力或 Session UI 行为缺口。
- [ ] AC18. 每个保留 crate 都能以依赖方向、替换边界、生成协议或 infrastructure
  adapter 说明独立存在理由；crate graph 无环且 Core 可完全独立测试。
- [ ] AC19. migration 从实施前一版本前向执行后，只有最终 schema 被生产代码读取，
  且 PostgreSQL/in-memory behavior、并发约束、DashAgentCommit 原子性和 recovery
  tests 通过。
- [ ] AC20. 规划完成 PRD convergence pass、独立 sub-agent review 和用户审阅；在明确
  批准前任务保持 `planning`。
- [ ] AC21. `transition-architecture.md` 中 Current → Target、S0–S6、activation-ready
  change set、S5 cutover unit、粗粒度内嵌 subagent 派发和 finding 返工路由已被
  `design.md`、`implement.md` 与 manifests 引用；每个 checkpoint 都有单一路径、唯一
  owner、tracer bullet 与独立 check 证据。
