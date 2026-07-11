# PR #93 Agent Runtime 目标架构与当前分支语义适配

## 结论

PR #93（`codex/agent-runtime-architecture-convergence`，head `efdfa5dc`）适合作为
Agent 会话执行内核的目标基线，但不能把它对旧 `application-runtime-gateway`、旧
Session action 和旧 Canvas surface 的处置原样覆盖到当前分支。

推荐的最终模型是：

```text
Product callers / AgentRun facade / Workflow / Canvas / Extension / Channel adapter
                |                         |
                | conversation command    | exact platform capability
                v                         v
        Managed Agent Runtime      Canonical Operation Gateway
        Thread/Turn/Item           OperationRef/admission/placement
        RuntimeOperation           OperationScript nested calls
        AgentRuntimeInteraction            ^
                |                          |
                +---- Platform Tool Broker-+
                |
                v
        Integration Driver Host
                |
                v
        RuntimeWire -> concrete Agent driver
```

即：**保留 PR 的 Managed Runtime / AgentRun facade / Business Agent Surface / Tool
Broker / Driver Host / RuntimeWire 主干；保留当前分支的 canonical Operation、
OperationScript、shared Interaction、Extension 与 Channel 各自事实源；只在 Business
Surface 和 Tool Broker 处建立单向适配。** 不建立兼容 facade、双 gateway 或
RuntimeSession fallback。

## 证据基线

- PR：[#93](https://github.com/syan2018/AgentDash/pull/93)，base
  `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`，head
  `efdfa5dc585530b1c8285e9b2a399ba92830c45e`。
- 当前分支：`codex/workspace-duplex-interaction-planning`，head
  `7070f6b0c28963c4cd04c67312ebd0571189e4b4`。
- merge-base：`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。两条分支都从该点开始重构，
  因而本文把“同名类型/模块的新语义”视为语义冲突，不按文本冲突处理。
- 下文 `PR:` 行号均指 `efdfa5dc`；`CURRENT:` 行号均指 `7070f6b0`。

## 必须保留的 PR 不变量

### 1. Managed Runtime 是 Agent 会话执行事实的唯一写者

PR 已明确分开四个 owner：AgentRun facade 只拥有产品映射，Managed Runtime 拥有
Thread/Turn/Item/Runtime Interaction/Runtime Operation/context/terminal，Driver Host
拥有 service/binding/placement，Adapter 终止具体协议。

证据：

- `PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/design.md:7-10`
- `PR:crates/agentdash-agent-runtime-contract/src/gateway.rs:15-29`
- `PR:crates/agentdash-agent-runtime-contract/src/command.rs:63-132`

任何整合都不能让 canonical Operation Gateway、Extension transport、Channel 或 shared
Interaction repository 写入 Runtime Thread/Turn terminal；反向也不能让 Managed Runtime
拥有这些产品领域对象。

### 2. Runtime mutation 必须 durable accept before side effect

Runtime command 的 acceptance、projection、authoritative event 与 outbox 必须共享一次
原子 commit；driver side effect 发生在 commit 之后。Runtime 的 `OperationReceipt` 表达的是
会话 mutation 的 durable acceptance，不是某个 provider capability 的返回值。

证据：

- `PR:.trellis/spec/backend/agent-runtime-kernel.md:42-58`
- `PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/design.md:308-325`
- `PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/design.md:845-850`
- `PR:crates/agentdash-agent-runtime-contract/src/command.rs:154-167`

### 3. AgentRun facade 必须保持薄且具名

`AgentRunRuntime` 只解析 `run_id + agent_id`、授权、client command id、guard 与 Runtime
binding，然后调用通用 Runtime Gateway；它不选 driver、不保存 active turn，也不解析
Extension/Channel/Interaction provider。

证据：

- `PR:crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:20-68`
- `PR:crates/agentdash-application-agentrun/src/agent_run/runtime_facade.rs:95-140`
- `PR:.trellis/spec/backend/agent-runtime-agentrun-facade.md:29-46`

### 4. 期望 surface、实际 offer、admission 与 applied ack 必须分离

以下四个对象不能重新合成 connector capabilities 或一个自报式 definition：

| 对象 | 唯一含义 |
| --- | --- |
| `AgentSurfaceSnapshot` | 平台根据 AgentFrame/product facts 编译的期望能力 |
| `RuntimeOffer` | service × transport × host policy 后的实际保证 |
| `BoundAgentSurface` | Runtime admission 求交后的唯一 delivery route |
| `AppliedAgentSurface` | Adapter 对 revision/digest/per-point 的真实应用回执 |

证据：

- `PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/target-crate-shape.md:109-120`
- `PR:crates/agentdash-agent-runtime/src/surface.rs:199-238`
- `PR:crates/agentdash-agent-runtime-host/src/model.rs:70-82`
- `PR:crates/agentdash-agent-runtime-host/src/model.rs:98-128`
- `PR:crates/agentdash-agent-runtime-host/src/model.rs:143-177`

当前分支的 Operation descriptor 应成为 Business Surface 的一种 product contribution；
不能被复制成 driver 自报 tool capability，也不能绕过 applied ack 直接开放给 Agent。

### 5. Tool Broker 是 Agent tool lifecycle owner，不是 provider authority

Tool Broker 保留 Runtime Thread/Turn/Item、binding generation、tool-set revision、approval、
timeout/cancel 与 side-effect idempotency；具体 capability 的 exact identity、actor visibility、
schema/readiness、业务授权和 placement 仍由 canonical Operation Gateway 负责。

PR 的 surface 已区分 `DirectToolCallback`、`McpToolFacade` 与 `DriverNativeTool`
（`PR:crates/agentdash-agent-runtime/src/surface.rs:213-222`），因此 canonical Operation 最自然的
接入方式是 Host-owned callback executor，而不是新建第二套 tool/provider registry。

### 6. Driver Host 只拥有 service/binding/lease/source coordinate

Driver Host 不解析 OperationRef、Interaction attachment、Extension installation 或 Channel
membership；它只持久化 bound surface reference、apply evidence、sticky binding、generation、
lease 与 source coordinate。

证据：

- `PR:crates/agentdash-agent-runtime-host/src/model.rs:98-128`
- `PR:crates/agentdash-agent-runtime-host/src/model.rs:143-177`
- `PR:.trellis/spec/backend/agent-runtime-driver-host.md:26-42`

### 7. RuntimeWire 只承载 Runtime/Driver placement protocol

RuntimeWire 可以承载 Driver command/event 以及反向 `ToolInvoke`/`HookExecute` HostPort，但不能
成为 generic Operation、Extension protocol 或 Channel message transport。其现有 frame 明确是
Runtime command、Driver request 与 HostPort union。

证据：

- `PR:crates/agentdash-agent-runtime-wire/src/lib.rs:31-60`
- `PR:crates/agentdash-agent-runtime-wire/src/lib.rs:76-107`
- `PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/target-crate-shape.md:246`

## 与当前分支的关键语义冲突

### 冲突 A：两个 `Operation` 不是同一个领域概念

PR 的 `RuntimeOperation` 是 Agent Runtime mutation 的 acceptance/idempotency/recovery 单元；
当前分支的 canonical `Operation` 是平台能力的 exact provider-qualified invocation。

当前分支证据：

- `CURRENT:crates/agentdash-domain/src/operation.rs:5-16`：`OperationRef` 固定 namespace、
  provider key、operation key 与 contract version。
- `CURRENT:crates/agentdash-domain/src/operation.rs:34-90`：principal、scope 与 origin 是平台调用
  坐标，覆盖 AgentRun、Workflow、Extension、Canvas、Interaction 与 script nested call。
- `CURRENT:crates/agentdash-application-runtime-gateway/src/runtime_gateway/operation_types.rs:288-330`：
  trusted host command 与最终 invocation envelope 分离，browser 无法反序列化 authority。
- `CURRENT:crates/agentdash-application-runtime-gateway/src/runtime_gateway/operation_core.rs:20-77`：
  surface/admission、placement、dispatch、result 与 audit 是 canonical execution core 的边界。
- `CURRENT:crates/agentdash-application-runtime-gateway/src/runtime_gateway/operation_core.rs:201-260`：
  dispatch 前重新解析 authority/descriptor，并把 placement 与 attachment 写入可信 envelope。

结论：两者必须保留，但名称与 API 要显式区分。建议 PR 类型使用
`AgentRuntimeOperationId` / `RuntimeOperationReceipt`；平台能力继续使用 `OperationRef` /
`OperationExecutionResult`。**不能把 OperationRef 塞进 `RuntimeCommand` union，也不能用 Runtime
Operation journal 取代 provider invocation/audit。**

### 冲突 B：两个 `Interaction` 的 authority 完全不同

PR 的 Runtime Interaction 是一个 Turn 内 driver 发起、等待 approval/user input/MCP elicitation
响应的 durable request；当前分支 `InteractionInstance` 是 Human/Agent 共享操作的长期产品状态。

当前分支证据：

- `CURRENT:.trellis/spec/backend/interaction/architecture.md:4-7`：Instance 是 canonical state、revision、
  command 与 event 的唯一事实源。
- `CURRENT:crates/agentdash-domain/src/interaction/instance.rs:100-115`：Attachment subject 是
  AgentRun/UserWorkshop/WorkflowRun，role 独立存在。
- `CURRENT:crates/agentdash-domain/src/interaction/instance.rs:117-165`：attachment capability 由 role
  派生，且 attachment 有独立 identity/lifetime。
- `CURRENT:crates/agentdash-api/src/bootstrap/runtime_gateway.rs:356-376`：Agent command 每次重新检查
  active attachment 与 `can_submit_commands`，并把 attachment id 纳入 authority revision。

结论：建议把 PR 侧文档/API 统一称为 `AgentRuntimeInteraction`，shared 产品侧继续称
`InteractionInstance`。Attachment 永远锚定 AgentRun/control-plane identity，不改绑到
RuntimeThread/driver binding；Runtime restart 或 rebind 不改变 shared Interaction 生命周期。

### 冲突 C：PR 对 `application-runtime-gateway` 的处置已过时

PR 规划时该 crate 仍主要是 Session-bound Extension action gateway，因此计划将其重命名为
`agentdash-application-extension-gateway`
（`PR:.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/target-crate-shape.md:73`）。
当前分支已经把它重建为 MCP、Extension、Interaction、Workflow、setup 与 host capability 共用的
canonical Operation core；并删除 Session-bound action gateway。

结论：不能采用 PR 的 extension-only 重命名和旧文件形状。最终应重命名为更准确的
`agentdash-application-operation-gateway`（或保留现名但规范中只称 Operation Gateway）；PR 中仅为
切断 RuntimeSession 而产生的改动要重做在 canonical Operation 实现上。

### 冲突 D：Business Surface 与 Operation catalog 会形成重复 tool authority

PR 的 `ToolCatalogRevision` 当前可以由各种 product facts 直接编译；当前分支已经让 Extension、
Interaction、MCP 等能力先投影为 exact `OperationDescriptor`。若合并时继续为这些来源各写一套
Tool contribution builder，会出现 descriptor/schema/visibility/readiness 两套事实。

结论：Business Surface 新增一个 `OperationSurfaceSource` adapter：从当前 principal/scope/
attachment 下的 canonical Operation surface 生成稳定 tool contribution，至少固定
`OperationRef + descriptor digest + input/output schema + effect/replay policy + authority revision`。
Driver 只看到 runtime tool name/schema；OperationRef 与 authority mapping 留在 Host Tool Broker。

### 冲突 E：OperationScript 不能变成 Runtime command 或 durable conversation object

当前 OperationScript 是单次 Rhai program，allowed operations 与 authority 都由 server 重建；每个
nested call 都重新进入 canonical gateway：

- `CURRENT:crates/agentdash-application-ports/src/operation_script.rs:17-40`
- `CURRENT:crates/agentdash-application-ports/src/operation_script.rs:87-117`
- `CURRENT:crates/agentdash-application-ports/src/operation_script.rs:259-289`
- `CURRENT:crates/agentdash-application-runtime-gateway/src/runtime_gateway/operation_script_adapter.rs:16-59`

结论：脚本引擎和 preflight token 留在 Operation/application/infrastructure 边界。若 Agent surface
允许执行脚本，它只是一个由 Tool Broker 调用的 host capability；Runtime journal记录父 ToolCall
Item 与结果/失败，nested Operation 使用同一 trace 的 child invocation。不要为每个 nested call
生成 Turn、RuntimeCommand 或 AgentRuntimeOperation；需要跨步骤恢复的组合仍进入 Workflow。

### 冲突 F：Extension 与 Channel 的 Integration contribution 必须正交合并

PR 给 `AgentDashIntegration` 新增 `agent_runtime_drivers()`
（`PR:crates/agentdash-integration-api/src/integration.rs:50-64`）；当前分支新增独立
`channel_binding_providers()`，并由 Channel 自己维护 provider registry
（`CURRENT:crates/agentdash-integration-api/src/integration.rs:119-125`、
`CURRENT:crates/agentdash-application/src/channel/provider.rs:15-63`）。

结论：最终 Integration trait 同时保留 Agent runtime driver contribution、Operation provider
contribution（若需要 trusted host registration）和 Channel binding provider contribution；三者分别
进入 Driver Host、Operation Gateway 与 ChannelService registry。不能因为同一个 Extension package
同时贡献多种能力，就把这些 registry 或生命周期合并。

### 冲突 G：RuntimeSession 删除后，current trace/attachment 适配必须重做

当前分支已把 RuntimeSession 降为 connector delivery/trace evidence，并从 canonical Operation 与
Interaction identity 中移除；PR 则完整删除 RuntimeSession 与 execution anchor，改用 Runtime
Thread/Operation/Event。目标应采用 PR 的最终删除，不保留 trace fallback；但需要把当前分支所有
`optional_attachment_ref`、Operation trace 和 AgentRun surface resolver 从旧 session anchor 改为
`run_id + agent_id + AgentFrame revision + RuntimeThread/Turn/Item refs` 的显式组合。

## 推荐目标边界

### Managed Runtime / AgentRun facade

- `send/steer/interrupt/compact/runtime-interaction respond` 只进入 AgentRun facade ->
  `AgentRuntimeGateway`；这些不是 canonical platform Operations。
- AgentRun facade 负责把 product identity 映射为 Runtime binding；不把 RuntimeThread 暴露成
  Interaction attachment 或 Channel owner。
- Runtime event/journal 可以记录外部 Operation invocation ref、trace id、result ref 作为 Item evidence，
  但不复制 provider result/state authority。

### Business Agent Surface

- AgentFrame、Capability Pack、VFS、Skill、Hook 等继续按 PR 编译。
- MCP、Extension、shared Interaction、Channel-facing agent action 统一从 canonical Operation surface
  投影为 tool contributions，避免每个业务模块直接给 driver 拼 schema。
- active Interaction attachment 决定相关 exact Interaction Operations 是否进入 surface；attachment
  变化生成新 immutable surface revision。Hot apply 只有 Driver 返回 matching applied ack 后才开放；
  不支持 hot update 时明确要求新 Thread/rebind。

### Platform Tool Broker

- 新增 host-side `OperationToolExecutor`，用 Runtime Item/binding/tool-set 坐标查回 exact
  `OperationRef`，构造 server-owned principal/scope/origin/trace/attachment，再调用
  `OperationGateway::invoke`。
- Broker 保留 Before/AfterTool、approval、permission/VFS/credential、timeout/cancel、Runtime Item terminal；
  Operation Gateway 保留 descriptor、actor visibility、current authority/readiness、placement、provider
  dispatch 与 output schema。两层都必须执行自己的 admission，不能互相替代。
- Runtime Item ID 可作为 Operation idempotency key/provenance 的组成部分，但两边 transaction 不伪装成
  一个跨数据库原子事务；未知外部 side effect 按现有 replay policy 与 Runtime Lost/Failed 规则收敛。

### Shared Interaction / Attachment

- `InteractionInstance`、state revision、command/event/effect outbox 继续由 Interaction subsystem 独占。
- Attachment 只连接 AgentRun/control-plane subject 与 shared instance，并决定 read/submit/render 能力。
- Agent 操作 shared Interaction 走 Interaction `OperationProvider` -> canonical Operation core；
  `RuntimeCommand::InteractionRespond` 只响应 driver-originated AgentRuntimeInteraction。
- 建议把当前 `InteractionRuntimeBinding` 更名为 `InteractionResourceBinding`，避免与 Driver Host
  `RuntimeBinding` 混淆；这是目标态清理，不需要兼容别名。

### Extension

- Project Extension installation/artifact 继续是 Extension Operations 的发现与 provenance 事实源；当前
  `ExtensionOperationProvider` 的 action/protocol/backend 三类 dispatch 保留。
- Agent 是否看见 Extension Operation 由 current Operation surface + Business Surface admission 决定；
  Driver Host 不读取 installation，也不直接激活 TS Extension Host。
- Extension 本机执行继续使用其 typed placement/relay handler；不得把 Extension invocation envelope
  包进 RuntimeWire generic frame。

### Channel

- ChannelService 独占 owner、membership、policy、binding、message 与 delivery recovery；每次 publish/
  ingress 重新 admission。
- Agent-facing Channel action 若需要 tool 形态，应提供 canonical Operation provider，再由 Business
  Surface/Tool Broker 接入；Runtime 不复制 Channel registry。
- 真实 Channel 消息触发 Agent 时，先进入 AgentRun Mailbox/facade，再产生 Runtime Turn；普通 hook
  auto-resume 仍是 mailbox control effect，不制造 Channel identity。

### Driver Host / RuntimeWire

- Driver Host 只接收 bound surface reference 与 materialized tool schema，不接收
  `OperationInvocationEnvelope`、Interaction attachment aggregate、Extension installation 或 Channel document。
- 远端 driver 的 tool request 经 RuntimeWire `HostPort::ToolInvoke` 返回 Host Tool Broker；之后是否调用
  local/cloud Extension/MCP/Interaction Operation 由 Operation placement resolver 决定。
- RuntimeWire 不添加 generic `OperationInvoke`、`ExtensionProtocolInvoke` 或 `ChannelPublish` frame。

## 对整合设计的直接约束

1. 以 PR head 的 Agent Runtime crates、migration `0065` 和 production composition 为 Agent 会话基线。
2. 在其上重新移植当前分支 canonical Operation/OperationScript/Interaction/Extension/Channel；不要保留
   PR 分支中的旧 `extension_actions/session_actions` gateway 形状。
3. 先完成 Operation Gateway crate/name 与两组 Operation/Interaction 类型命名收束，再接 Business
   Surface adapter 和 Tool Broker executor，避免 merge 后形成临时双 authority。
4. `AgentDashIntegration` 合并三类正交 contribution；启动期分别做 duplicate key 检查。
5. migration 最终只保留 PR Managed Runtime schema、当前 Interaction/Channel schema 与 canonical
   Operation 所需表；旧 RuntimeSession/Canvas/action gateway 表直接删除，不做 dual reader/write。
6. 核心验收应包含：Agent attachment -> surface revision -> driver applied ack -> tool call -> canonical
   Operation -> Interaction state commit；Extension Operation remote placement；Channel message -> mailbox ->
   Runtime Turn；以及 RuntimeWire 断连后 Operation/Interaction/Channel 事实不被误 terminalize。
