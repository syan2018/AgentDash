# External Agent × AgentDash Feature / Tool Integration Matrix

> 目标：用当前代码与协议能力建立 Native Pi、Codex App Server、ACP Agent、企业 Remote Agent 的集成矩阵，明确真正的 seam、保证强度、UI availability 和演进路径。
>
> 本文中的 `Unsupported` 表示**当前 adapter / 当前协议合同尚不能安全表达**，不是永久产品限制。企业 Agent Core、driver 与 AgentDash 可以协同调整；优先目标是收敛 module ownership、依赖方向和状态所有权，而不是把本轮类型冻结成永久外部标准。
>
> **范围更新（2026-07-10）：** ACP Agent列保留为协议能力研究，不代表首期实现承诺。正式实施已删除ACP Runtime Driver工作包；未来若有真实外部viewer需求，仅评估授权脱敏后的read-side ACP presentation projection，见`acp-event-projection.md`。
>
> Hook能力的正式模型已从本文较粗的`LifecycleProfile`细化为逐trigger/action/timing/strength的`HookProfile`，见`hook-runtime-layering.md`与`codex-hook-projection.md`；本文矩阵中的hook行继续作为feature事实，不作为最终Hook contract。

## 1. 结论先行

### 1.1 外部 Agent 仍然可以获得平台托管工具，但必须有 callable channel

外部 Agent 不能直接接收 AgentDash `DynAgentTool`，不等于它永远不能使用平台工具。安全适配有三种：

1. **Direct host tool callback**：driver 接收 tool schemas，Agent 发 structured tool call，host 执行并回传 structured result；
2. **MCP tool broker**：AgentDash 把 platform tools 暴露为带 session-scoped credential 的 MCP server，外部 Agent 原生连接；
3. **Driver-native tools**：工具由外部 Agent 自己拥有，AgentDash 只观察 item lifecycle/approval，不能把它计作平台托管工具。

具体可行性：

- Native Pi：直接持有 `DynAgentTool`，完整支持；
- Codex App Server：协议已有 `ThreadStartParams.dynamic_tools` 与 `item/tool/call`，可以做 direct host callback，但当前 bridge 尚未接入；
- ACP：标准 `session/new` / `session/load` 接受 MCP servers，因此 MCP tool broker 是最自然的 HostAdapted 路径；ACP 标准没有 client 直接下发任意 tool schema 的等价入口；
- 企业 Remote Agent：通过协同定义的 driver contract 可支持 direct callback、MCP 或两者，可达到 Native/L4，但必须由实际 profile 声明。

**提示注入不是第四种工具适配。** 把工具名称、JSON schema、Skill 说明写进 prompt，只会让模型“知道有个工具”，不会建立调用、审批、结果、取消、身份和审计通道。AgentRun/UI 必须将这种情况视为 `Unsupported`，不能显示工具可用。

### 1.2 当前只有 Native 路径具备完整 AgentDash L4 语义

当前只有 Native Pi 能把以下能力作为一套可验证状态机闭环：

- AgentFrame revision 的 capability/context/VFS/MCP 同步；
- platform `DynAgentTool` 的直接注入与 active-session replace-set；
- Runtime delegates 在 before-provider / before-tool / after-tool / before-stop / compaction / turn-boundary 内运行；
- exact projected context restore + MessageRef；
- platform-managed compaction replacement checkpoint；
- mailbox 在同一 Agent loop 内的 stop/continue decision；
- VFS access policy 与 tool execution 同一进程内执行。

这不是外部 Agent 的永久上限。Codex driver 可以补 dynamic tool callback、typed instruction、approval 和 exact context extension；企业 Remote Agent 可以与 AgentDash 共同调整 Agent Core/driver contract，达到 L4。ACP baseline 更偏互操作协议，若需要 L4，可通过版本化 ext methods 或由企业 remote contract 承载。

### 1.3 Capability 必须拆成 profile，不能用一个 level 或 boolean 代替

`L0-L4` 只能表达粗粒度、累积的最低保证；feature admission 必须看 typed profile。一个 Agent 可能：

- 能 MCP，但不能 steer；
- 能 native fork，但不能导出 exact context；
- 能 dynamic tools，但只能 thread-start 固定工具集；
- 能 cancel，但不能 acknowledged interrupt；
- 能展示 usage，却不知道 platform canonical context。

因此 UI/AgentRun 必须消费最终 bound profile：

```text
effective profile = service guarantee ∩ placement transport guarantee ∩ host policy
```

Relay 只影响 placement transport，不是矩阵中的 Agent service 类型。

## 2. 评级图例

| 标记 | 含义 | 能否驱动 UI availability |
| --- | --- | --- |
| **Native** | driver / 协议直接表达该语义，并有可验证的 ordering/error/identity 保证 | 可以 |
| **HostAdapted-Exact** | host 通过 tool broker、MCP、materialization、outer coordinator 等实现，语义与目标合同等价 | 可以，显示实现 provenance |
| **HostAdapted-Boundary** | 只能在 session/turn boundary 实现，不能进入外部 Agent 内部 provider/tool loop | 仅开放相应弱语义 command |
| **Observed** | 可观察 telemetry/read model，但不能控制或恢复真实 driver state | 只读 UI |
| **PromptOnly** | 只能作为普通 prompt 内容；没有优先级、调用或状态保证 | **不算 capability** |
| **Unsupported (current)** | 当前 adapter/合同没有安全原语 | 禁用，并显示缺失的 profile predicate |
| **Evolvable** | 通过改 driver/Agent Core/版本化扩展可提升 | 仅设计提示，不自动开放 |

## 3. 当前 AgentDash feature surface 事实

### 3.1 AgentFrame 是持久 surface revision

`AgentFrameSurfaceDocument` 同一 revision 包含 capability state、context slice、VFS、MCP、execution profile、Canvas/workspace visibility：`crates/agentdash-domain/src/workflow/agent_frame.rs:6-37`。每次 surface 变化产生新 revision：同文件 `:40-75`。

`CapabilityState` 又包含 tool/MCP、companion、channel、VFS、Skill、memory、workspace module 等维度，并明确 AgentFrame revision 是持久化权威源：`crates/agentdash-spi/src/connector/mod.rs:400-539`。

**含义：** 外部 Agent 的 profile 不能只回答“能不能 prompt”。它还要说明 AgentFrame 的每个维度如何被应用、何时生效、是否可热更新，以及 driver mirror fidelity。

### 3.2 ExecutionContext 当前把过多 Native 对象直接交给 connector

`ExecutionSessionFrame` 包含 cwd、env、AgentConfig、MCP、VFS、VFS access policy、backend placement 和 identity：`crates/agentdash-spi/src/connector/mod.rs:61-91`。

`ExecutionTurnFrame` 包含 capability state、runtime delegates、restored messages、ContextFrames、delivery plan 和 `Vec<DynAgentTool>`：同文件 `:224-254`。

这对 in-process Native adapter 很方便，但不是跨 Agent service 的稳定 seam：外部 driver 无法序列化 trait object、hook runtime 或本地 VFS 对象。

### 3.3 launch 会先装配 tools，再构造多类 ContextFrame

tool assembly 从 runtime tool provider 与 MCP discovery 合并 `DynAgentTool` / schemas / MCP readiness：`crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:15-101`。

turn preparation 再构造 identity、user、environment、guidelines、memory、assignment、capability delta、pending action 等 frames，并生成 delivery plan：`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:141-173,175-233,333-440`。

### 3.4 Native Pi 实际消费完整 surface

- structured input 通过 `PromptPayload::to_content_parts`，image 保持 `ContentPart::Image`：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:663-686`；
- ContextFrame system/developer channel 被组装为 system prompt：同文件 `:709-716,1166-1195`；
- assembled tools 直接 `agent.set_tools`：`:806-836`；
- restored messages 用 `replace_messages_with_refs`：`:826-836`；
- runtime delegates 直接设置到 Agent Core：`:845-850`；
- tools 可 active-session replace-set：`:1088-1124`；
- approval、steer、cancel 都直达 Agent Core：`:1039-1085,1142-1162`。

### 3.5 当前 Codex bridge 丢失了协议本来具备的能力

- bridge 把 structured input 调 `to_fallback_text`，再把所有 ContextFrame 拼成一段 prompt：`crates/agentdash-executor/src/connectors/codex_bridge.rs:176-187`；
- 最终 `TurnStart` 只发送一个 `UserInput::Text`：同文件 `:930-940`；
- 但 Codex App Server 的 `UserInput` 原生支持 Text/Image/LocalImage/Skill/Mention：Cargo git checkout `codex-rs/app-server-protocol/src/protocol/v2/turn.rs:238-309`；
- `ThreadStartParams` 也原生支持 base/developer instructions、workspace roots 和 dynamic tools：同 checkout `protocol/v2/thread.rs:95-156`；
- current bridge 没有设置这些字段，仅传 model/cwd/approval/sandbox/config：`codex_bridge.rs:242-282`。

因此“Codex 不支持多模态/context channel/platform tools”不是协议事实，而是当前 adapter 缺口。

### 3.6 当前 Codex approval/user-input 语义不安全

Codex protocol 会向 client 发 command/file approval、requestUserInput 和 dynamic tool call。当前 bridge 却：

- command/file request 自动返回 `acceptForSession`；
- requestUserInput 自动返回空 answers；
- unknown server request 返回 null；
- connector 的 approve/reject methods 又直接报未接入。

证据：`crates/agentdash-executor/src/connectors/codex_bridge.rs:553-565,1012-1030`。

虽然它把 `supports_permission_policy=true` 暴露给上层：同文件 `:578-587`，但这不能等同于“AgentDash approval round-trip 已支持”。

### 3.7 当前 relay payload 只透传 input/MCP/最小 workspace/config

`RelayPromptRequest` 只有 input、mount root/workspace identity、working dir、env、executor config 和 MCP servers：`crates/agentdash-application-ports/src/backend_transport.rs:117-153`。

当前 `RelayAgentConnector` 没有下发 ContextFrames、delivery plan、assembled tool schemas、capability state、VFS access policy、runtime delegates 或 restored context：`crates/agentdash-application/src/relay_connector.rs:88-155`。

这说明当前 relay connector 把 placement transport 和 Agent service adapter 混在一起。目标设计中 Relay 透明传统一 driver protocol；实际 capability 由 service profile 与 transport profile 求交。

## 4. 总览矩阵

说明：单元格描述“当前可达到语义 → 可协同演进方向”。Enterprise Remote 列不假设所有实现都具备能力，而是表示统一 driver contract 可声明/验证的上限。

### 4.1 Input / context delivery

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| Structured text input | **Native** | 协议 Native；当前 bridge 先 flatten，再发 Text | **Native** baseline ContentBlock::Text | Profiled Native |
| Image / local image | **Native** exact content part | 协议 Native；当前 **Unsupported**，被 fallback text 丢失 | Native when `promptCapabilities.image`; local path 需同 placement/materialize | Profiled Native，可要求 blob/resource transport |
| Audio | provider/profile dependent | 当前 Codex UserInput 无 audio | Native when ACP `audio=true` | Profiled Native |
| Resource / file reference | Native context/VFS/tool | Mention/Skill/path Native，但任意 embedded resource 无直接 UserInput | ResourceLink baseline；embedded Resource 要 capability | Profiled Native |
| System instruction | Native system tier | 协议 `base_instructions` 可 Native；当前 **PromptOnly** | 标准无 system channel；当前 **Unsupported** | 可定义 Native typed instruction |
| Developer instruction | Pi 当前与 system 合并，Native-high-priority 但非独立 tier | 协议 `developer_instructions` Native；当前 **PromptOnly** | 标准无 developer channel | 可定义 Native typed channel |
| Additional context | ContextFrame + plan Native | 普通上下文可 HostAdapted 为 typed UserInput；当前无差别 flatten | Embedded Resource 可 HostAdapted-Exact；文本只能普通 prompt priority | Profiled Native ContextEnvelope |
| ContextFrame ordering/channel/cache | **Native**（AgentDash contract） | 当前 Unsupported；可把 base/dev/user context 分通道映射，Codex 不认识 AgentDash完整 frame metadata | Unsupported baseline；可通过 ext method/remote contract协同增加 | 可达到 Native |

### 4.2 Tools / MCP / VFS / Skills

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| Platform-hosted tool schema | **Native** `DynAgentTool` | 协议 dynamic tools 可 **HostAdapted-Exact**；当前未接入 | 无任意 schema入口；MCP broker 可 **HostAdapted-Exact** | Direct callback 或 MCP，Profiled Native |
| Platform-hosted tool call | **Native** same process | `item/tool/call` callback 可 Exact；当前 unknown request 返回 null | MCP call 由 Agent 发往 broker，Exact | Profiled Native |
| Driver-native tools | **Native** if Core/provider owns | Codex native command/file/MCP/tools | ACP Agent 自有 tools + ToolCall updates | Profiled Native |
| Dynamic tool update | **Native** active replace-set | dynamic tools 只在 thread start；当前 Unsupported；可 rebind/new thread 或扩展 update primitive | MCP list-changed 是否被 Agent采用无 baseline guarantee；通常 turn/session boundary | 可协同支持 HotReplace / TurnBoundary |
| MCP server injection | Host discovers并包装为 `DynAgentTool`，Native | Codex 有 MCP config/runtime，但当前 bridge 未投 ExecutionContext MCP；可用 config 或 broker dynamic tools | **Native**：new/load session带 MCP；stdio baseline，HTTP/SSE按 caps | Profiled Native |
| MCP readiness / tool policy | AgentFrame + host discovery Native | HostAdapted；必须在 tool callback/broker server侧 enforcement | HostAdapted-Exact 在 broker侧；Agent自连 MCP 的 readiness需事件扩展 | Profiled Native |
| Physical cwd/workspace | Native | Codex `cwd/runtimeWorkspaceRoots` Native；当前只传 cwd | ACP `cwd` Native | Profiled Native/placement |
| Logical VFS/mounts | **Native** via VFS tools + access policy | HostAdapted only via broker/materialization；Codex native shell/file可绕过逻辑 VFS时不能声称 exact | client fs methods 可 HostAdapted；远端 Agent自有 fs需隔离 | 可传 mount descriptor或强制 broker，达到 Exact |
| VFS path policy | **Native**同执行点 enforcement | 只有所有访问经 broker或受控 sandbox时 Exact；prompt声明无效 | Agent调用 client fs时可 enforcement；Agent本机访问不可控 | Profiled guarantee |
| Skill inventory/content | Native ContextFrame/Skill dimension | Codex native Skill/path/skills registry；AgentDash Skill需 materialize + provenance mapping | 可作为 Resource/embedded context；无 native Skill lifecycle | 可定义 Skill artifact/profile |
| Capability Pack | Host 先展开为 Skill/MCP/workflow/permission，**HostAdapted-Exact** | 不需要 pack wire type；每个贡献必须分别可映射 | 同左；通常 Skill context + MCP，inner hook可能缺失 | Profiled；可完整展开 |

### 4.3 Interaction / control

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| Permission policy | AgentDash tool policy delegate Native | Codex approval/sandbox policy Native；当前仅粗映射 | Agent自有 policy；client permission request Native | Profiled Native |
| Approval round-trip | **Native** | 协议 Native；当前自动接受/正式恢复链路缺失，故 current Unsupported | **Native** `session/request_permission` | Profiled Native |
| Agent asks user input | 可由 platform tool/delegate实现 | 协议 `item/tool/requestUserInput` Native；当前自动空答 | 标准无通用 elicitation；需 ext method或结束 turn后新 prompt | Profiled Native |
| Steer active turn | **Native** | `turn/steer` Native，current bridge 已接入 expected turn | ACP baseline无 steer；current Unsupported，ext可演进 | Profiled Native |
| Interrupt/cancel | **Native** acknowledged idle | Codex `turn/interrupt` Native；current bridge主要 cancel token/kill process，只有 best-effort | ACP `session/cancel` baseline，PromptResponse应返回 Cancelled | Profiled Native |
| Tool progress/items | **Native** canonical mapping | Native Codex items/progress | Native ACP ToolCall/Update | Profiled Native |
| Plan/reasoning stream | **Native** | Native Codex plan/reasoning | ACP Plan/AgentThought | Profiled Native |

### 4.4 Hooks / mailbox / turn boundary

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| SessionStart/UserPromptSubmit outer hook | Native | **HostAdapted-Exact** before driver command | **HostAdapted-Exact** | Host or driver Native |
| AfterTurn/SessionTerminal outer hook | Native | **HostAdapted-Exact** from canonical terminal | **HostAdapted-Exact** after PromptResponse/update | Host or driver Native |
| BeforeTool policy hook | **Native** | Exact only for brokered/dynamic tool or approval callback；driver native unobserved tools取决协议 | Exact for MCP broker/client permission；Agent内部工具可能只 observed | Profiled Native |
| AfterTool result transform | **Native** | Exact for brokered tools；native tool通常只能 Observed after completion | Exact for MCP broker；Agent内部 tool observed | Profiled Native |
| BeforeProviderRequest | **Native** runtime delegate | current Unsupported；需要 driver/Core extension | ACP baseline Unsupported | 可协同加 driver hook point |
| BeforeStop / same-loop continue | **Native** | Host只能 terminal后新 turn；非同一 loop语义 | Host只能新 prompt；非同一 loop语义 | 可定义 Native stop-decision callback |
| Before/AfterCompact hook | **Native** managed compaction | native compact只有 Observed；managed需 L4 extension | Unsupported baseline | 可达到 Native L4 |
| Mailbox -> active steer | **Native** | Native if steer ready | Unsupported baseline | Profiled Native |
| Mailbox at turn boundary | **Native** same-loop/next-turn | **HostAdapted-Boundary**：terminal后启动新 turn；不能声称 same-loop | HostAdapted-Boundary 新 prompt | 可协同支持 Native turn-boundary callback |
| Hook auto-resume | Native | HostAdapted as new turn/fork | HostAdapted as new prompt | Profiled |

### 4.5 Session / context / compaction

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| Platform event-projected context read | **Native Exact canonical** | **Observed/EventProjected**，不等于 Codex隐藏模型上下文 | **Observed/EventProjected** | 至少 EventProjected |
| Exact driver context export | Native | 当前 Unsupported；thread/read/history不等于 exact model context | load replay history不等于 exact system/tool/model context | Profiled，可协同达到 L4 |
| Platform context replace/import | **Native** `replace_messages_with_refs` | 当前 Unsupported；需扩展 import/digest/activation | ACP baseline Unsupported | Profiled L4 |
| Native resume/restore | Native projected restore | Codex thread continuation/fork Native；current bridge用 follow-up ThreadFork | ACP load_session，unstable resume | Profiled Native |
| Exact platform restore | **Native L4** | Unsupported current | Unsupported baseline | 可达到 L4 |
| Native fork | AgentDash projection branch | Codex ThreadFork Native | ACP unstable session/fork | Profiled Native |
| Exact fork from ContextRevision | **Native L4** | Unsupported current，native fork只保证Codex自己的history | Unsupported baseline | 可达到 L4 |
| Native compaction | Native algorithm | Codex ContextCompaction item/notification | ACP无标准 compact | Profiled |
| Native compact telemetry | Native typed event | **Observed**，notification只有 threadId/turnId | Unsupported baseline | Profiled |
| Platform managed durable compaction | **Native L4** | Unsupported current；需 exact export/import/digest/idempotent activation | Unsupported baseline | 可协同达到 L4 |

### 4.6 Usage / model / config

| Feature | Native Pi | Codex App Server | ACP Agent | Enterprise Remote |
| --- | --- | --- | --- | --- |
| Token usage | Native provider/Core stats | Native `thread/tokenUsage/updated`，current已映射 | unstable usage / PromptResponse usage | Profiled Native |
| Context window | Native provider metadata + estimate | Codex usage可观察，exact model limit由 model metadata/config | 非 baseline；extension/config metadata | Profiled |
| Model selection | Native AgentConfig/provider registry | Thread/Turn model Native；current部分映射 | unstable session/model；standard config option可表达 selector | Profiled Native |
| Reasoning/thinking | Native provider config | effort/summary/config Native；current只映射 reasoning effort config | 可用 session config options/modes | Profiled Native |
| Permission/sandbox config | Native host policy | Native approval/sandbox/permissions | Agent modes/config + client permission | Profiled Native |
| Discovery/options | Native provider registry | Codex protocol丰富；current discovery patch把 models/agents设空 | session response modes/config options；agent capabilities | Profiled descriptor |
| Source session title/status | Native platform | Native Codex notifications，current已映射 | ACP SessionInfoUpdate可表达部分 | Profiled |

## 5. 重点问题：外部 Agent 无法手动赋予工具集时怎么办

### 5.1 Codex：优先 dynamic tool callback，而不是 prompt

Codex App Server protocol 已提供：

- `ThreadStartParams.dynamic_tools: Vec<DynamicToolSpec>`：checkout `protocol/v2/thread.rs:39-84,154-156`；
- server request `item/tool/call` 与 `DynamicToolCallResponse`：checkout `protocol/v2/item.rs:1359-1393`、`protocol/common.rs:1350-1352`。

建议 adapter：

1. Business Agent Tool Catalog 产出 protocol-neutral schemas；
2. Codex driver 转为 DynamicToolSpec；
3. tool call server request 映射到 canonical `AgentItemId`；
4. Managed Agent Runtime 做 capability/permission/admission；
5. Tool Broker 执行 `DynAgentTool`；
6. result 转为 DynamicToolCallResponse；
7. approval、timeout、cancel、VFS identity 与 session/turn/item 全链路关联。

限制：当前 dynamic tools 是 thread-start surface，未发现 active-thread replace primitive。工具变化必须声明 `InitialOnly` / `RebindRequired`，或协同扩展 update。不能调用当前默认 `update_session_tools` 后假装 Codex已更新；`AgentConnector` 默认实现会直接 `Ok(())`：`crates/agentdash-spi/src/connector/mod.rs:1060-1069`。

### 5.2 ACP：优先 MCP tool broker

ACP 标准：

- `NewSessionRequest` 与 `LoadSessionRequest` 都携带 MCP servers：cargo registry `agent-client-protocol-schema-0.11.2/src/agent.rs:699-746,842-895`；
- stdio MCP 是 baseline，HTTP/SSE 由 `McpCapabilities` 声明：同文件 `:2281-2304,3431-3469`；
- prompt 本身只携带 ContentBlocks，没有任意 client-provided tool schema 字段：同文件 `:2548-2604`。

因此 AgentDash 应提供 session-scoped MCP Tool Broker：

- 只暴露 effective AgentFrame tools；
- 使用短期 session credential，不把用户 secret放入 prompt/config；
- 每次 call 重做 tool policy、permission、VFS access 与 binding generation 校验；
- namespaced tool identity 与 canonical AgentItemId 关联；
- broker断线/工具变化反映 readiness 与 profile；
- Agent不支持可达 MCP transport时，相关 platform tools就是 Unsupported。

### 5.3 企业 Remote：driver contract 可以协同调整

企业 Remote 不必被 Codex/ACP baseline 限死。可在统一 remote-runtime contract 中直接定义：

- `ToolCatalogApplied { revision, digest }`；
- `ToolCallRequested` / `ToolCallResult`；
- `ToolCatalogReplace`；
- `ApprovalRequested`；
- `ContextApplied`；
- binding generation / operation idempotency。

只要 Agent Core 与 driver 共同实现并通过行为测试，就可声明 Direct/HotReplace/L4。该合同首先是内部 module seam，可随项目重构演进，不必在本轮追求永久行业标准。

### 5.4 何时 broker 仍然不够

MCP/tool broker只能提供 callable tools，不能自动解决：

- 外部 Agent 自己的 native shell/file tools绕过 AgentDash VFS policy；
- external hidden context与 platform ContextRevision不一致；
- before-provider/before-stop等内部 loop hook；
- active-turn steer/turn-boundary continuation；
- exact managed compaction activation。

这些必须由 driver guarantee、sandbox/placement、Agent Core改造或更高 level contract解决。

## 6. 哪些 feature 当前只对 Native L4 成立

| Native-only current feature | 为什么外部 adapter 不能仅靠包装获得 | 可演进路径 |
| --- | --- | --- |
| Exact AgentFrame -> live runtime mirror | 需要每个 surface revision apply ack/digest | 企业 driver加 SurfaceApplied；Codex按 instruction/tool/workspace分 primitive映射 |
| Active-turn arbitrary tool replace-set | 需要 Agent loop重建 provider tool schema并确认生效 | direct driver update primitive；否则明确 RebindRequired |
| Runtime delegates inside provider/tool/stop loop | 外部协议只暴露 turn/item事件，时机已经过去 | 协同在 Agent Core/driver加 hook callback points |
| Exact projected message restore with MessageRef | native可直接替换 Core messages；external resume通常只恢复自己的thread | context import/export/digest contract |
| Platform-managed durable compaction | 需要 candidate、idempotent apply、recover、activation digest | L4 transactional context extension |
| Exact fork from platform ContextRevision | native可从 canonical projection建新 state | export/import或fork-with-basis-revision extension |
| Same-loop mailbox stop/continue | host收到terminal后已经退出外部 loop | driver turn-boundary decision callback |
| VFS enforcement over all file/shell access | 外部 Agent可能有不经broker的native tools | sandbox/placement限制native access，或所有工具经host broker |
| BeforeProviderRequest / BeforeStop hooks | baseline协议无这些内部时点 | 协同扩展 driver/Core hook protocol |

“只对 Native L4”是现状结论，不是长期产品路线。优先选择真正需要这些不变量的 feature，再决定升级对应 driver，不需要为所有外部 Agent一次性补齐全部 L4。

## 7. 建议 Capability Profile 拆分

### 7.1 InputProfile

```text
text
image: none | url | inline_blob | local_path
audio: none | inline_blob | resource
resource_link
embedded_resource
skill_reference
mention_reference
```

### 7.2 InstructionProfile

```text
system: none | initial_only | turn_update
developer: none | initial_only | turn_update
additional_context: prompt_content | typed_resource | context_envelope
delivery_plan_fidelity: none | ordered | channel_exact | cache_exact
```

`prompt_content` 不能满足 system/developer requirement。

### 7.3 ToolProfile

```text
schema_ingress: none | session_initial | turn_boundary | hot_replace
invocation: driver_native | host_callback | mcp_broker
result_content: text | structured | multimodal
approval: none | policy_only | round_trip
policy_enforcement: host_broker | driver_verified | unverified
progress: none | observed | ordered
```

### 7.4 WorkspaceProfile

```text
cwd
physical_roots
logical_vfs
mount_descriptors
client_fs_proxy
terminal_proxy
path_policy_enforcement
materialization
```

### 7.5 InteractionProfile

```text
steer: none | active_turn_ordered
interrupt: best_effort | acknowledged_terminal
approval_round_trip
user_input_elicitation
tool_progress
plan/reasoning stream
```

### 7.6 LifecycleProfile

```text
outer_session_hooks
outer_turn_hooks
broker_tool_hooks
inner_provider_hooks
inner_tool_hooks
before_stop_decision
turn_boundary_decision
mailbox_active_steer
mailbox_next_turn
```

### 7.7 ContextProfile

```text
authority: platform | driver
read_fidelity: opaque | event_projected | exact_round_trip
resume: none | native | platform_import
fork: none | native | exact_revision
compaction: none | native_telemetry | managed_transactional
activation: none | replace | idempotent_replace_recover
```

### 7.8 TelemetryConfigProfile

```text
usage
context_window
model_discovery/set
reasoning config
mode/config options
source title/status
```

GuaranteeLevel 从这些 profile 的必备组合推导，但 command admission 直接检查 predicate。例如 manual compact 检查 `managed_transactional + idempotent_replace_recover`，不能只检查 `level >= 4` 字样。

## 8. AgentRun / UI availability 规则

### 8.1 availability 必须来自 bound profile + session state

建议 snapshot 返回：

```text
CommandAvailability {
  command_kind,
  enabled,
  disabled_code,
  required_predicates,
  missing_predicates,
  implementation_provenance,
  semantic_strength
}
```

典型规则：

- `context.compact`：仅 managed transactional；native telemetry不开放按钮；
- `turn.steer`：仅 active-turn ordered steer；ACP baseline隐藏/禁用；
- `turn.interrupt`：best-effort可显示不同文案，acknowledged才承诺 terminal convergence；
- `tool.update`：只在 HotReplace开放；InitialOnly显示“新会话生效”，而非假成功；
- `session.fork`：区分 DriverNativeFork 与 ExactContextRevisionFork；
- `approval.respond`：必须有真实 pending item + round-trip，不因 `permission_policy=true`开放；
- `skill.invoke`：Skill artifact必须已 materialize/driver确认或能作为真正 resource；文本提示不算；
- Capability Pack：required contribution有一项不满足，整个 pack标 incompatible；optional contribution必须在manifest明确可选，不能静默部分生效。

### 8.2 UI 应展示能力 provenance，不硬编码 executor

UI 可显示：

- Tool: `Host callback` / `MCP broker` / `Driver native`；
- Context: `Exact` / `Projected` / `Opaque`；
- Compaction: `Managed durable` / `Driver native telemetry`；
- Workspace: `Logical VFS enforced` / `Physical workspace only`；
- Tool update: `Hot replace` / `Rebind required`。

UI 不应出现 `if executor == codex` 之类分支，只消费 profile/availability。

## 9. Protocol-specific assessment

### 9.1 Codex App Server

协议上限高于当前 adapter：

- structured multimodal UserInput；
- base/developer instructions；
- cwd/runtime roots/sandbox/approval；
- dynamic tools callback；
- steer/interrupt；
- approvals/request user input；
- native skills/MCP/items/usage/fork/compact telemetry。

当前优先改造顺序：

1. 停止 `to_fallback_text + compose_prompt_text`；
2. UserInput逐变体原样映射；
3. ContextFrame按 system/developer/additional context分别映射；
4. dynamic tools + tool callback接 Platform Tool Broker；
5. server requests进入 canonical approval/user-input state machine，删除自动接受/空答；
6. cancel改 `turn/interrupt` 并等待 terminal；
7. model/config/workspace roots按 descriptor映射；
8. 若业务需要 L4，再协同增加 exact context contract，而不是从 thread history猜测。

### 9.2 ACP

ACP 0.10.x/对应 schema提供的强项：

- capability negotiation；
- ContentBlock text/resourceLink baseline，image/audio/embedded optional；
- new/load session MCP；
- tool call/update + client permission；
- client fs/terminal；
- cancel；
- modes/config options；
- unstable usage/model/list/fork/resume/close；
- ext method/notification。

主要缺口：

- 无 standard system/developer channel；
- 无 arbitrary direct host tool schemas；
- 无 steer；
- 无 generic user input elicitation；
- 无 exact context import/export/activation/compaction；
- 无 AgentDash inner hook/turn-boundary contract。

这些可以通过 MCP broker、outer coordinator 或协同 ext method演进。不要把 embedded Resource 当成 system instruction，也不要把 load_session streamed history当 exact model context。

### 9.3 企业 Remote Agent

企业 Agent 属于可协同服务，不需要照搬 ACP 的最低公分母。建议复用同一 typed driver semantics，wire format可以版本化迭代：

- service descriptor声明 profile；
- binding固定 descriptor digest/generation；
- tools/context/surface apply都有 revision + ack；
- command/event带 canonical/native identity mapping；
- context activation可恢复；
- transport仅透明承载。

实现可从 L1/L2 开始，只声明已实现能力；需要 AgentDash workflow/hook/compaction的企业 Agent再逐步补相应 profile，不引入永久 fallback。

## 10. Conformance 用例

本文不主张重型生态认证；以下是 driver/module seam 的行为测试，用于防止 capability 声明与实际实现漂移。

### Input / instruction

1. image 输入经过 adapter 后仍是 image，不出现 placeholder text；
2. system/developer/additional context分别到达对应 channel；
3. 不支持 channel时 command/admission明确拒绝，不能 flatten；
4. context delivery order与 declared fidelity一致。

### Tools / MCP

5. platform tool schema被driver确认 revision/digest；
6. tool call保留 session/turn/item/tool identity；
7. broker端重新执行 capability、permission、VFS检查；
8. result/timeout/cancel能收敛到 terminal item；
9. duplicate tool call idempotent；
10. tool update只在声明的 boundary生效；InitialOnly driver不得返回假成功；
11. MCP transport/readiness与profile一致；不可达时tool availability关闭；
12. native tool无法绕过声明的host-enforced policy，否则profile只能标 unverified。

### Approval / user control

13. approval request不会自动接受；
14. duplicate/conflicting decision按item revision处理；
15. requestUserInput等待真实回答或cancel，不自动空答；
16. steer按active turn和sequence排序；
17. interrupt最终产生acknowledged terminal或明确best-effort fault。

### Hooks / mailbox

18. outer hooks在driver command前/terminal commit后运行；
19. brokered BeforeTool可以阻止实际执行；
20. 不支持inner hook的driver不会声称 BeforeProvider/BeforeStop；
21. mailbox新turn与same-loop continuation使用不同semantic flag；
22. terminal后auto-resume只启动一个幂等新turn。

### Context

23. EventProjected snapshot明确不等于Exact；
24. native resume/fork不被误标为exact ContextRevision restore/fork；
25. native compact telemetry不推进platform checkpoint；
26. managed compaction验证candidate commit、driver activation、head CAS三个crash point；
27. binding generation变化后旧driver event被隔离。

### Usage / config / placement

28. usage单位和cumulative/per-turn语义明确；
29. model/config改变获得driver ack并更新snapshot；
30. `service profile ∩ transport profile ∩ host policy` 求交正确；
31. Relay断线只改变placement health，不改变service provenance。

## 11. Module ownership

| Module | 拥有内容 | 不应拥有 |
| --- | --- | --- |
| Application / AgentRun | 产品授权、run-agent-session binding、提交command、消费availability/journal | connector分支、tool装配、context flatten、restore/compact细节 |
| Business Agent Context | AgentFrame/Capability Pack展开、ContextFrame、ToolCatalog、VFS/MCP/Skill surface、context snapshot | Codex/ACP JSON |
| Managed Agent Runtime | session/turn/item state、bound profile、command admission、mailbox、approval、context/compaction coordinator | driver-specific config解析 |
| Driver Contract | protocol-neutral command/event、capability profile、apply ack/fidelity | AgentRun/product IDs |
| Executor Adapters | Native/Codex/ACP/Enterprise wire mapping | capability业务决策、checkpoint持久化 |
| Platform Tool Broker | tool schema/call/result、MCP façade、permission/VFS enforcement、call identity | Agent UI/workflow orchestration |
| Hook Policy | hook rule evaluation与effects | transport调用 |
| Protocol | Codex/ACP/remote typed wire extensions | application service/repository |
| Infrastructure | event/outbox/checkpoint/binding persistence、secret refs、mount/materialization adapters | context/compaction policy |
| Relay Transport | placement discovery、route、stream/replay transport guarantee | Agent service identity、feature semantics、profile夸大 |

这个划分允许先清理 module ownership，再逐个提升 adapter。矩阵中的 Unsupported 不要求在第一次重构全部补齐。

## 12. 推荐落地顺序

1. **先定义 typed profile 与 availability**：替换 bool connector capabilities/default no-op；
2. **把 AgentFrame 展开收进 Business Agent Context module**：产出 protocol-neutral ContextEnvelope/ToolCatalog/VFS requirement；
3. **Native adapter成为 reference behavior**：固化真实 L4 contract tests；
4. **修 Codex 无损输入与 instruction channel**；
5. **接 Codex dynamic tool callback、approval/user-input、turn interrupt**；
6. **实现 Platform Tool Broker + MCP façade**，为 ACP/外部 Agent提供安全工具路径；
7. **将 Relay 改为透明 placement transport**，传统一 driver command/event/descriptor；
8. **建立 ACP adapter或generic remote adapter**，先覆盖 L1/L2 + MCP/approval；
9. **按真实产品需求协同提升企业 driver**：inner hooks、tool hot update、context L4；
10. **删除 prompt flatten / false success / hardcoded executor availability**。

## 13. 最终判断

外部 Agent 集成的核心不是找到一个“所有 feature 都能降级成文本”的万能协议，而是把 AgentDash feature 分成可验证的语义原语：input channel、tool ingress/callback、workspace enforcement、interaction control、lifecycle hook、context fidelity 和 durable activation。

Native Pi 当前覆盖最完整，可作为 reference adapter；Codex App Server 的协议能力显著高于现有 bridge，应优先修复 adapter 的 flatten、auto-approval 和未接 dynamic tools；ACP 非常适合作为 L1/L2 互操作与 MCP tool broker通道，但不应被误认为天然支持 AgentDash inner runtime/L4 context；企业 Remote Agent 可与 AgentDash协同演进到L4，无需被当前最低公分母永久限制。

最重要的产品规则是：

- 有 callable/ack/identity/policy channel，才算 tool capability；
- 有 exact export/import/digest/recovery，才算 transactional context；
- 有对应内部时点，才算 inner hook；
- terminal后另起turn，不等于same-loop mailbox continuation；
- prompt text可以承载普通内容，但不能伪装system/developer、tools、permissions、Skill lifecycle或context restore。

用这套矩阵驱动 profile 和 UI availability 后，项目可以先把 module seam 收敛干净，再按 Native/Codex/ACP/企业 Agent 的真实需求逐步扩展 driver，不需要兼容层，也不会用假能力掩盖差异。
