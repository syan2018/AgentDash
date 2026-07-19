# Final protocol、types 与 codegen hard-cut inventory

## Purpose

本文固定 S5 后半段的 owner、consumer 与激活顺序。目标是在不丢失实际产品路径的前提下，
删除 `agentdash-agent-protocol`、`agentdash-agent-protocol-codegen` 和
`agentdash-agent-types`，并收窄 `agentdash-platform-spi`。本文描述的是
`d13e923c` hard-cut staging revision 上仍需迁移的真实消费者，不是新的兼容层。

## Current executable facts

### Crate consumers

`agentdash-agent-types` 仍有五个 normal direct dependents：

- `agentdash-agent-protocol`
- `agentdash-application`
- `agentdash-application-lifecycle`
- `agentdash-application-vfs`
- `agentdash-platform-spi`

`agentdash-agent-protocol` 仍有十二个 normal direct dependents：

- `agentdash-agent-protocol-codegen`
- `agentdash-application-ports`
- `agentdash-application`
- `agentdash-application-workflow`
- `agentdash-application-agentrun`
- `agentdash-application-lifecycle`
- `agentdash-api`
- `agentdash-workspace-module`
- `agentdash-contracts`
- `agentdash-application-vfs`
- `agentdash-relay`
- `agentdash-platform-spi`

根 `contracts:check` 仍直接执行 `agentdash-agent-protocol-codegen`。因此 codegen、protocol
和 types 必须在 consumer migration 与生成入口切换完成后一起退出 workspace。

### Direct consumer migration matrix

| Protocol consumer | Real current use | Final target |
| --- | --- | --- |
| protocol-codegen | Codex generated roots | Codex integration private generator |
| application-ports | UserInputBlock、ControlPlane projection enums | Runtime input content + Product projection contract |
| application | input helpers、Backbone/Platform event producers | Runtime command + independent Product feeds |
| application-workflow | Cargo dependency only | delete dependency |
| application-agentrun | title projection reason | Product-owned reason enum |
| application-lifecycle | journey Backbone fold、control projection | Runtime snapshot/change + Product feeds |
| API | control-plane notification | Product generated contract |
| workspace-module | workspace presentation PlatformEvent | Workspace Product projection |
| contracts | Session/Codex/Backbone TS roots | Runtime/Product generators |
| application-vfs | Cargo dependency only | delete dependency |
| Relay | Prompt/Steer/UserInput/session event | Runtime Wire placement lane |
| Platform SPI | Backbone persistence、HookTrace、ContextFrame | final owner contracts or delete |

| Types consumer | Final target |
| --- | --- |
| protocol | delete transcript projection；move input conversion to owning boundary |
| application | move wait/product tools to Platform Tool executors |
| application-lifecycle | move advance-node tool to Platform Tool executor |
| application-vfs | VFS-owned typed direct execution + Infrastructure Runtime adapter |
| Platform SPI | delete Agent type exports、runtime surface、delegates and old tool injection |

Application cannot replace `agentdash-agent-types` with a dependency on `agentdash-agent`; that
would only rename the type bucket and violate Product → Dash dependency direction.

### Existing final owners

`agentdash-agent-runtime-contract` 已拥有 Managed Runtime 的 platform-neutral
Turn、Item、Interaction、content、snapshot、change 与 availability 基础 vocabulary。

`agentdash-agent-service-api` 已拥有 Complete Agent 的 source-authoritative
Turn、Item、Interaction、snapshot、change 与 input vocabulary。

`agentdash-agent` 已拥有从旧 types bucket 迁入的 Dash Agent/Core types。旧
`agentdash-agent-types` 与其存在重复定义，不能通过 serde transcode 或 re-export 继续
维持第二套 identity。

### Missing presentation coverage

当前 final contracts 的 ToolCall/ToolResult body 仍较窄，而旧 presentation path 真实承载：

- user input、agent output、reasoning、plan；
- command execution、shell execution、terminal control；
- file change、filesystem read/grep/glob；
- MCP、dynamic tool、collaboration/sub-agent activity；
- web search、image view/generation、sleep、review mode；
- approval、user input interaction、context compaction；
- output chunks、status、exit code、patch、structured content 与 terminal evidence。

这些字段被 Session UI card registry、tool bodies、plan/interaction/compaction view 与
Product generated DTO 实际消费。删除旧 protocol 前，final Runtime/Service contract 必须
提供完整、平台中立、可持久化的 canonical presentation body；只保留 name/arguments/result
会造成编译可通过但功能静默退化。

当前 Codex projector 只对 user/agent/compaction/dynamic tool 提供有限 typed 映射，其余
ThreadItem 主要被压成只含 vendor `type/status` 的 Extension；前端再把 tool/result/
extension 映射成 generic dynamic tool/JSON 文本。当前 compaction view 还会把
failed/interrupted/lost 等所有非 running 状态投成 completed。P1 必须先关闭这条有损链。

### Canonical body、update 与 terminal evidence

Complete Agent source item 与 Managed Runtime normalized item 均需要覆盖：

- UserMessage、HookPrompt、AgentMessage、Reasoning、Plan；
- CommandExecution、FileChange、FileRead、FileSearch；
- McpToolCall、DynamicToolCall、CollaborationToolCall、SubagentActivity；
- WebSearch、ImageView、ImageGeneration、Sleep、ReviewMode；
- TerminalControl、ContextCompaction、GenericToolActivity、Error。

common content blocks 需要 typed text、image/detail、local resource、resource link、skill
reference、mention 与 versioned structured content。工具 arguments/result 使用 JSON 是
工具合同本身的语义；`namespace=codex + raw vendor ThreadItem` 不属于 canonical extension。

source item 至少固定：

```text
id + status + body
started_at? + updated_at?
terminal evidence?
body_digest + presentation_digest
```

`presentation_digest` 覆盖 body、status、terminal 和时间证据。Change 仍可携带最新完整
snapshot，但必须附 typed transition：

```text
Started
Updated:
  TextAppended | ReasoningAppended | ContentAppended
  CommandOutputAppended | PatchChanged | PlanChanged
  ToolProgress | CollaborationChanged | BodyReplaced
Terminal:
  Completed | Failed | Interrupted | Lost
  completed_at? + duration? + process_exit? + error?
```

terminal status 必须有 terminal evidence；accepted/running 不得有 terminal evidence；
process exit、tool logical success 与 Agent item outcome 是三类不同事实；snapshot fold 与
change fold 必须等价。

Interaction 不能只保留 `kind + prompt + resolved bool`。Approval、UserInput、
McpElicitation、DynamicTool 需要各自 typed request/detail/resolution，status 至少覆盖
Pending、Resolved、Cancelled、Expired、Lost。

## Final ownership

| Vocabulary / behavior | Final owner | Reason |
| --- | --- | --- |
| Complete Agent source snapshot/change item body | `agentdash-agent-service-api` | Host 下方所有完整 Agent 的公共 read boundary |
| Managed Runtime normalized presentation snapshot/change | `agentdash-agent-runtime-contract` | Application/UI 唯一稳定读取合同 |
| Codex App Server DTO、schema、fixtures 与 generator | `agentdash-integration-codex` | vendor 类型只能由对应 integration 看见 |
| Codex DTO → Complete Agent canonical projector | `agentdash-integration-codex` | vendor fidelity 与 source authority 在 adapter 内收口 |
| Dash history/Core output → Complete Agent canonical projector | `agentdash-integration-native-agent` | Native adapter 是 Dash anti-corruption boundary |
| Complete Agent item → Managed Runtime projection/change | `agentdash-agent-runtime` | Runtime 拥有 normalized durable projection |
| Product mailbox、workspace、terminal、canvas、project notification DTO | `agentdash-contracts` / Product owner | 它们是独立 Product channels，不是 Agent history |
| Browser presentation types | generated Runtime/Product contracts | 前端只消费 AgentDash-owned canonical DTO |
| Runtime/Complete Agent remote framing | `agentdash-agent-runtime-wire` | Relay 与 Remote 的共享 transport boundary |
| Tool definition、authorization、execution receipt | Runtime Tool Broker contracts | 平台 Tool policy/effect 的唯一 owner |
| VFS/Task/Workspace concrete execution | Product service + Infrastructure adapter | Application 不反向依赖 Runtime concrete/service API |
| Non-Agent auth/function/MCP/mount/routine/extension/VFS ports | `agentdash-platform-spi` | 仍有独立平台 adapter 边界 |

`PlatformEvent` 中的 Product notification、Hook trace、workspace presentation 与 terminal
状态按上表进入各自 Product contract；它们不进入 Complete Agent history，也不继续由
universal Backbone envelope 聚合。

## Activation order

### P1 — Freeze complete canonical presentation

1. 扩展 Service API source item body，覆盖真实消费者需要的完整 canonical families。
2. 扩展 Runtime Contract normalized item body和 durable change delta。
3. 为 status、output、patch、interaction、compaction 与 extension body 固定 typed
   discriminant、identity、digest、authority/fidelity。
4. generator/schema/TypeScript freshness tests证明所有 root 可机械生成。

P1 不修改 production caller，不删除旧 protocol。

### P2 — Move source projectors

1. Codex vendor generator/generated Rust/fixtures/lock 移入 `agentdash-integration-codex`。
2. Codex projector只在 integration 内把 vendor ThreadItem/notification 映射到 Service API。
3. Native projector把 Dash history/Core/tool evidence映射到同一 Service API。
4. Codex、Native、Remote conformance证明完整 body、started/updated/terminal 顺序与
   compaction failure/lost。

Codex private generator 最小物理形态：

```text
crates/agentdash-integration-codex/
  src/bin/generate_codex_protocol.rs
  src/vendor_generated/
  protocol-fixtures/{schemas,roots,projection}/
  codex-protocol-codegen.lock.json
```

generated module最多 `pub(crate)`；projector使用typed private DTO；lock记录vendor
revision/schema/generator digest；不输出browser vendor TypeScript。

### P3 — Activate Runtime/Product consumers

1. Runtime repository持久化完整 canonical body与 source evidence，change/outbox携带同一
   committed revision。
2. Product/API/contracts/frontend一次切换到 Runtime/Product generated contracts。
3. Session UI 目录名可以保留为界面模块名，但其中状态只能来自 history-maintained
   canonical conversation；platform mailbox/workspace/terminal/control state继续走各自
   Product feed。
4. thread name、cursor gap、reconnect、tool card、file/shell/MCP、interaction与compaction
   tracer bullets通过。

P3 的完整 data flow、ownership、PostgreSQL validation、legacy deletion prerequisites 与
tracer gates 见
[`product-canonical-presentation-cutover.md`](product-canonical-presentation-cutover.md)。

### P4 — Migrate Tool、Platform SPI 与 Relay consumers

1. Runtime Tool Broker拥有最终 tool definition/result/effect vocabulary。
2. VFS、Task、Workspace concrete services不依赖 Runtime concrete或 Service API；
   Infrastructure adapter实现 Runtime executor。
3. `agentdash-platform-spi` 删除 AgentTool、Agent runtime surface、session persistence、
   old Hook/prompt/protocol re-export，只保留仍有真实非 Agent adapter理由的 ports。
4. Relay删除 Prompt/Cancel/Steer和 session-event protocol，只消费 Runtime Wire
   command/receipt/change/callback framing。

Relay 在删除旧 Agent lane 前必须先实现 production `RuntimeWirePlacement`。最终 Agent
lane至少具有：

```text
RuntimeWireOpen / RuntimeWireOpenAck
RuntimeWireFrame { stream_id, placement provenance, envelope }
RuntimeWireAck
RuntimeWireClosed / PlacementLost
ServiceOfferAdvertisement
```

Relay只增加 multiplex stream identity、placement provenance、health/backpressure；
Runtime Wire frame ID/ack仍是流内唯一执行顺序。MCP、Terminal、Extension、Workspace等
非 Agent transport lane可继续保留。

完整 production lifecycle、Host verification authority、placement fields、backpressure、
activation order 与 tracer bullets 见
[`relay-runtime-wire-placement-activation.md`](relay-runtime-wire-placement-activation.md)。

### P5 — Delete legacy crates atomically

1. `rg` 与 `cargo metadata` 证明 protocol/types/codegen normal consumers为零。
2. 根 scripts 改为 Integration Codex private codegen + Runtime/Service/Wire/Product
   canonical generators。
3. 删除三个 legacy crate目录、workspace members/dependencies与旧 generated
   `backbone-protocol.ts` / vendor frontend tree。
4. 删除 0084 中 retired store/table与 zero-consumer persistence。
5. 生成唯一 `Cargo.lock`，运行 contracts、Rust、frontend、Relay、migration与 tracer
   bullets。

## Stable gates

- Service API、Runtime Contract和Wire不依赖 vendor/Product implementation。
- Runtime/Application/frontend没有 Codex DTO或 `BackboneEvent`。
- Application不依赖 Service API、Host、Dash Agent、Codex或 Runtime concrete。
- canonical item body roundtrip保留完整 payload，而不是 summary或 opaque vendor JSON。
- vendor generator只在 Codex integration package内产生 Rust/vendor fixtures；vendor TS
  不进入 browser contract。
- Relay只承载 Runtime Wire，不拥有 Agent identity、history、surface或Product policy。
- `agentdash-platform-spi` 的保留模块均能以非 Agent infrastructure adapter边界说明存在
  理由。
- 删除后 direct input/output、Fork、Companion、Compaction、Tool/Hook、reconnect、
  Workspace/Terminal/Product feeds均有真实路径。

S5 production composition、Platform SPI 收窄、legacy consumer 分类、migration 补全与最终
删除顺序见
[`s5-production-composition-and-deletion.md`](s5-production-composition-and-deletion.md)。
