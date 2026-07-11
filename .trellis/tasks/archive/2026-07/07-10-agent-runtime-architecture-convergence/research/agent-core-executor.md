# Agent Core 与 Executor 当前架构调查

## 调查范围与结论

本文件只记录 Agent Core、executor、内部 Pi Agent、外部 Codex/relay Agent 及其 application 消费链路的代码事实、问题判断和目标建议。调查基于当前源码、Cargo 依赖、Trellis session 规范、现有测试以及 2026-05/2026-07 的 compaction 历史评估；没有修改生产代码。

核心结论：当前架构不是“application 直接依赖具体 Agent 实现”，但也没有形成真正完整的 Agent Runtime 依赖倒置。

1. **编译层面的倒置部分成立。** `agentdash-application-runtime-session` 通过 `Arc<dyn AgentConnector>` 调用执行器，具体 Pi/Codex 实现在 `agentdash-executor`，application 生产依赖没有反向引用 executor concrete type。`AgentConnector` 位于 `agentdash-spi`，因此主启动链在形式上满足 application 依赖 port、executor 实现 port。
2. **语义层面的倒置失败。** `AgentConnector` 只统一了 prompt/cancel/steer/approval/tool refresh 等窄操作，没有统一 conversation resume/fork/read、manual/auto compaction、checkpoint export、context delivery channel、runtime surface update 等完整语义。compaction 因而另建了 application-agentrun runtime port、application-runtime-session delegate、Pi 特殊 turn mode，并没有通过同一 Agent 能力合同执行。
3. **executor 不是唯一适配边界。** Codex/Pi/Composite 在 executor，但 relay connector 位于 `agentdash-application`；Codex DTO 直接进入 agent-types/SPI/application；relay transport DTO 进入 application-ports；API 还直接调用 executor 的 Pi provider registry。当前适配逻辑散落在 core、protocol、SPI、application、executor、local、API 七处。
4. **内部 Agent 与外部 Agent 只有事件外形对齐，能力和状态机没有对齐。** Pi 收到完整 `ExecutionContext`、runtime delegate、restored messages、assembled tools 并调用内部 Agent Core；远端执行器链路把 turn surface 缩减成 prompt/workspace/env/executor config/MCP，再由 local 重新跑一套 session application pipeline。云端构造的 capability、identity、context projection、delivery plan、runtime delegate 和 compaction mode没有完整跨 relay。
5. **Codex app-server bridge 目前只是首阶段 prompt bridge。** 它支持 start/fork/turn start/steer 和事件映射，但没有使用原生 thread resume、turn interrupt、thread compact start、正式审批响应等协议能力；多模态输入和 context channel 还被拍平成单个 user text。
6. **Agent Core 并不“干净”。** 当前 core 包含 AgentDash compaction prompt、Lifecycle URI、checkpoint boundary ref、runtime delegate facets、provider 重试分类以及 `agentdash-domain::ThinkingLevel`，而 `agentdash-agent-types` 直接依赖 Codex app-server protocol。它是“Pi 风格的 AgentDash 业务 runtime 内核”，不是可独立复用的纯 Agent Loop。
7. **允许大规模重构是合理且必要的。** 目标不应只移动 compaction 文件，而应先建立项目自有、完整、逐 executor 描述的 Agent Runtime contract，再让内部业务 Agent driver 和外部协议 driver 在 executor/router 边界实现同一状态机。

## 一、当前模块与实际依赖

### 1.1 主要模块事实

| 模块 | 当前实际职责 | 关键证据 |
| --- | --- | --- |
| `agentdash-agent-types` | 消息、工具、context、projection、runtime delegate、compaction 参数；同时承载 Codex ThreadItem 扩展 | `crates/agentdash-agent-types/src/lib.rs:8-47`；`crates/agentdash-agent-types/src/protocol.rs:1-21` |
| `agentdash-agent` | stateful Agent、Agent Loop、工具执行、LLM bridge、compaction 执行、Lifecycle tool-result ref | `crates/agentdash-agent/src/agent.rs:85-108`；`crates/agentdash-agent/src/compaction/mod.rs:1-4`；`crates/agentdash-agent/src/tool_result_ref.rs:21-61` |
| `agentdash-spi` | `AgentConnector`、`ExecutionContext`、capability/VFS/MCP/hook/tool 等跨层合同 | `crates/agentdash-spi/src/connector/mod.rs:224-265`、`:944-1087` |
| `agentdash-executor` | Pi/Codex/Composite connector、provider bridge、MCP tool adapter；同时 re-export SPI | `crates/agentdash-executor/src/lib.rs:1-11`；`crates/agentdash-executor/src/connectors/*` |
| `agentdash-application-runtime-session` | launch planning、context/delegate/tool preparation、connector start、event persistence、projection commit、runtime registry、cancel/terminal | `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:61-299`；`.../launch/connector_start.rs:29-93` |
| `agentdash-application-agentrun` | AgentRun mailbox/control/fork，以及 manual compaction command lifecycle 和专用 runtime port | `crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:27-181` |
| `agentdash-application::relay_connector` | 云端远端执行器 adapter，实际上也是 `AgentConnector` infrastructure implementation | `crates/agentdash-application/src/relay_connector.rs:1-45` |
| `agentdash-agent-protocol` | 内部 Backbone wire/event，同时直接复用 Codex notification/ThreadItem/UserInput DTO | `crates/agentdash-agent-protocol/src/backbone/event.rs:13-66`；`.../user_input.rs:10-16` |
| `agentdash-relay` / `agentdash-local` | Cloud/local 命令 wire 与本机重新启动 session runtime/外部 connector | `crates/agentdash-relay/src/protocol.rs:65-96`；`crates/agentdash-local/src/handlers/prompt.rs:81-330` |

### 1.2 Cargo 依赖方向

当前主要生产依赖可以简化为：

```text
agentdash-agent-types
  -> codex-app-server-protocol

agentdash-agent
  -> agentdash-agent-types
  -> agentdash-domain

agentdash-agent-protocol
  -> codex-app-server-protocol
  -> agentdash-agent-types

agentdash-spi
  -> agentdash-agent-types
  -> agentdash-agent-protocol
  -> agentdash-domain

agentdash-application-runtime-session
  -> agentdash-spi / agent-types / agent-protocol / domain / application-ports

agentdash-executor
  -> agentdash-spi / agent-protocol / agent-types / domain / application-ports
  -> agentdash-agent (pi-agent feature)

agentdash-api / agentdash-local (composition roots)
  -> application runtime + executor
```

证据：

- `agentdash-agent-types` 直接依赖 Codex protocol：`crates/agentdash-agent-types/Cargo.toml:8-16`。
- `agentdash-agent` 直接依赖 domain：`crates/agentdash-agent/Cargo.toml:7-12`；实际用于 `ThinkingLevel`：`crates/agentdash-agent/src/types.rs:23`。
- `agentdash-spi` 同时依赖 types、domain 和 agent protocol：`crates/agentdash-spi/Cargo.toml:8-27`。
- executor 反向依赖 application-ports 和 domain，并可选依赖 agent core：`crates/agentdash-executor/Cargo.toml:7-40`。
- application facade 生产依赖 agent-types/protocol/SPI，但只在 dev-dependencies 依赖 `agentdash-agent`：`crates/agentdash-application/Cargo.toml:7-25`、`:53-59`。
- API 生产依赖 executor、SPI、runtime-session、agentrun，且为了 `AgentTool`/`MessageRef` 直接依赖 `agentdash-agent`：`crates/agentdash-api/Cargo.toml:15-35`、`:64-67`。

因此需要区分两种判断：

- **事实：** application 主运行链没有 concrete `PiAgentConnector`/`CodexBridgeConnector` 编译依赖，具体装配在 API/local composition root。
- **问题：** 内层 contract 已被 Codex DTO、relay DTO、domain mega types 和 application transport shape 共同塑形，属于“依赖箭头正确、抽象所有权和语义仍向外泄漏”。

### 1.3 直接与间接消费者

- `agentdash-agent` 的生产主消费者是 executor 的 Pi connector；API 也直接消费其 re-export type。Pi connector 是唯一创建 core `Agent` 的生产位置：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:244-255`。
- `agentdash-agent-types` 的 blast radius 很大：agent、protocol、SPI、application-runtime-session、application-agentrun、application-lifecycle、application-vfs、contracts、executor 都直接依赖它。将 Codex protocol item 放在这里使协议升级影响所有内层包。
- `AgentConnector` 被 runtime-session、API/local composition、executor 和 application relay connector 消费。executor 本身只被 API/local 直接依赖，说明它不是 application 的 compile-time gateway，而是若干 concrete adapters 的集合。

## 二、application 是否真正依赖通用 Agent 抽象

### 2.1 已经成立的部分

`AgentConnector` 是 application runtime 当前消费的抽象：

- `SessionControlService` 只保存 `Arc<dyn AgentConnector>`，并转发 steer/approval 等操作：`crates/agentdash-application-runtime-session/src/session/control.rs:7-69`。
- `ConnectorStarter` 只通过 trait 调用 `prompt(...) -> ExecutionStream`：`.../launch/connector_start.rs:29-57`。
- Launch planner 按 executor 查询 repository restore 支持，不引用 concrete connector：`.../launch/planner.rs:108-118`。
- executor 的 Pi/Codex/Composite 都实现这一 trait：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:471-495`、`.../codex_bridge.rs:568-588`、`.../composite.rs:288-319`。

所以“application 完全没有通用抽象”并不准确；现有抽象是 `agentdash-spi::AgentConnector`。

### 2.2 为什么它还不是目标中的通用 Agent Runtime 抽象

`AgentConnector` 的公共操作只有：discovery、live-session probe、prompt、cancel、steer、approve/reject、tool replace、notification push（`crates/agentdash-spi/src/connector/mod.rs:978-1087`）。它缺少：

- conversation new/resume/fork/read/close 的显式区分；
- turn start/interrupt/terminal 的 typed outcome；
- manual/auto compaction command、compaction ownership 与 checkpoint export；
- context delivery channel/system/developer/additional context 的能力声明；
- runtime surface update 的 typed delta；
- per-executor、per-protocol-revision 能力描述；
- session ownership handle 和结构化 stale/not-found/unsupported/busy 错误。

当前 `ExecutionContext` 反而过宽：session frame 包含工作目录、env、domain `AgentConfig`、MCP、VFS、backend placement、runtime backend anchor、auth identity；turn frame包含 hook runtime、`CapabilityState`、runtime delegates、restored messages、ContextFrame、delivery plan 和 assembled tools（`crates/agentdash-spi/src/connector/mod.rs:61-90`、`:224-255`）。这是 AgentDash launch projection，不是可供内部/外部 driver 一致实现的最小 Agent runtime contract。

**判断：** application 依赖了通用“connector facade”，但 compaction、context ownership、conversation state 和协议能力不在该抽象内，因此它还不能承担目标架构中的 Agent Runtime port。

## 三、内部 Pi 与外部 Agent 的实际运行链

### 3.1 内部 Pi Agent 主链

```text
FrameLaunchEnvelope
  -> LaunchPlanner（恢复、hook/runtime delegate、manual compaction delegate）
  -> LaunchPlan / ExecutionContext
  -> ConnectorStarter -> CompositeConnector -> PiAgentConnector
  -> stateful Agent -> Agent Loop / LlmBridge / tools / compaction
  -> Pi stream mapper -> BackboneEnvelope
  -> Session eventing -> durable event/projection/terminal
```

关键事实：

- Launch planner 组装 Hook runtime 全 facets、mailbox turn boundary、manual compaction wrapper，并恢复 projected transcript：`.../launch/planner.rs:144-229`。
- Launch plan 用 `LaunchSource::ContextCompaction` 生成 Pi 特殊 `ExecutionTurnMode::ContextCompaction`：`.../launch/plan.rs:308-357`。
- Pi connector 消费 `restored_session_state`、assembled tools、context frames、runtime delegates，并根据 mode 调用 `agent.compact_context_only()` 或普通 prompt：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:663-688`、`:806-875`。
- core `Agent` 自己持有 messages/message refs/streaming/tools/error 等 live state：`crates/agentdash-agent/src/types.rs:234-276`；connector 外层再持有按 session 索引的 Agent、tools、system prompt、model selection/window：`.../pi_agent/connector.rs:40-68`。
- core event 经 executor 映射为 Codex-shaped/AgentDash Backbone 事件；compaction completed 被映射为一个 untyped `SessionMetaUpdate(key="context_compacted")` 和一个 `ItemCompleted`：`.../pi_agent/stream_mapper.rs:1354-1405`。

### 3.2 外部 Codex 经 relay 的主链

```text
Cloud LaunchPlan / ExecutionContext
  -> application::RelayAgentConnector
  -> RelayPromptRequest / CommandPromptPayload
  -> local PromptCommandHandler
  -> 重新构造 LaunchCommand::local_relay_prompt_input
  -> local SessionRuntimeServices / CompositeConnector
  -> CodexBridgeConnector
  -> npx codex app-server JSON-RPC
  -> Codex notification -> BackboneEnvelope
  -> local session eventing -> relay broadcast -> cloud session eventing
```

这不是 Pi 链路的 remote transport 等价物，而是“云端 application runtime 嵌套本机 application runtime”。

最高影响证据：

- 云端 `RelayAgentConnector` 从完整 `ExecutionContext` 只发送 input、mount root、working dir、env、executor config 和 MCP：`crates/agentdash-application/src/relay_connector.rs:88-155`。
- `RelayPromptRequest` 没有 turn mode、identity、CapabilityState、context frames/delivery plan、runtime delegates、restored state、assembled tools 或 surface revision：`crates/agentdash-application-ports/src/backend_transport.rs:117-134`。
- wire `CommandPromptPayload` 同样只有上述缩减字段：`crates/agentdash-relay/src/protocol/prompt.rs:17-36`。
- local 收到后重新构造普通 `LaunchCommand::local_relay_prompt_input` 并再次调用 session launch：`crates/agentdash-local/src/handlers/prompt.rs:260-280`。
- relay 的 Agent 命令面只有 prompt/cancel/steer/discover/discover-options：`crates/agentdash-relay/src/protocol.rs:65-96`，没有 compact/resume/fork/read/approval/surface update。

这与当前 spec 明确存在漂移：规范声称 relay 会下发完整 VFS、identity 和 context projection（`.trellis/spec/backend/session/runtime-execution-state.md:53-62`），实际 payload 没有这些字段。

### 3.3 Context delivery 还存在 per-driver 路由错误

- preparation 用 `deps.connector.connector_id()` 计算 context connector profile：`.../launch/preparation.rs:400-417`。
- production 注入的是 `CompositeConnector`：云端 bootstrap `crates/agentdash-api/src/bootstrap/session.rs:296-326`，本机 `crates/agentdash-local/src/runtime.rs:566-569`，因此 connector id 是 `composite`，不是最终 `pi-agent`/`codex-bridge`。
- `agent_consumption_mode_for_frame` 只有 connector id 精确等于 `pi-agent` 才把所有 frame 标为 `Consume`；其它 connector 的 system/developer frame 标为 `SystemAppend`：`.../launch/preparation.rs:850-883`。
- Pi connector 只消费 `mode == Consume` 且 channel 为 System/Developer 的 frame：`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:1166-1179`。
- 当前测试只覆盖直接传入 `pi-agent` 和 `codex-bridge` 的 helper，没有覆盖 production Composite 路由：`.../launch/preparation.rs:1021-1060`、`:1063-1088`。

**问题判断：** Composite 隐藏了最终 driver identity 和 per-executor context capability，application 在路由前就按 aggregate connector 做 context policy；这会让 delivery metadata 与真正 driver 不一致。`context_delivery_plan` 目前没有任何 executor consumer（代码检索只有 application 构建/测试），因此它更像审计记录而不是可执行合同。

## 四、Compaction 当前所有权为何零散

### 4.1 当前所有权分布

| 责任 | 当前位置 | 证据 |
| --- | --- | --- |
| compaction 参数、metadata、delegate contract | agent-types | `crates/agentdash-agent-types/src/runtime/decisions.rs:158-267`；`.../delegate.rs:20-50` |
| eligibility、cut point、summary prompt/LLM 调用、消息替换 | Agent Core | `crates/agentdash-agent/src/compaction/mod.rs:1-4`、`:90-230`、`:243-463` |
| pre-provider lifecycle 与 live context mutation | Agent Core streaming | `crates/agentdash-agent/src/agent_loop/streaming.rs:741-907` |
| 自动策略、失败熔断、Hook 决策 | application-runtime-session Hook delegate | `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:455-465` 等 |
| manual request consume/noop/failed | application-runtime-session manual delegate | `.../manual_compaction_delegate.rs:20-220` |
| AgentRun command receipt、running/idle 决策、maintenance turn | application-agentrun | `.../context_compaction_command.rs:27-181`、`:297-578` |
| Pi event -> protocol projection | executor Pi mapper | `.../pi_agent/stream_mapper.rs:1286-1405` |
| checkpoint/segment/head commit、manual request completed | application-runtime-session eventing | `.../eventing.rs:622-820`、`:936-1018` |
| records/repositories/migrations | domain/SPI/infrastructure | session persistence 与 projection stores |

这些切分中的“策略、算法、持久化分别可替换”并非都错；真正的问题是没有一个 cohesive Agent Runtime command/lifecycle owner 把它们组成原子语义。

### 4.2 现有 internal compaction 成功链

- core 在 provider request 前调用 compaction delegate，执行 eligibility 和 summary：`crates/agentdash-agent/src/agent_loop/streaming.rs:741-839`。
- 成功后 core 立即替换 live `context.messages/message_refs`，然后发 `ContextCompacted`：同文件 `:841-863`。
- executor mapper 把该事件拆成 `SessionMetaUpdate(key="context_compacted")` 与 `ItemCompleted`：`.../stream_mapper.rs:1374-1405`。
- application eventing 对前一个事件提交 checkpoint/segments/head，之后普通流才处理 item completed：`.../eventing.rs:275-296`、`:622-820`。

持久化顺序测试覆盖了 projection commit，但 core event channel 本身是无 ack 的 broadcast：发送忽略错误，接收 lag 时会跳过事件（`crates/agentdash-agent/src/event_stream.rs:9-24`、`:42-69`）。因此：

- **事实：** durable ingestion 内部会按 envelope 顺序先 commit 再处理 item completed。
- **高风险推论：** core 已经激活 compacted live context，且可以继续 provider request，但它没有等待 projection commit acknowledgment；若 event consumer lagged 或 projection commit 失败，live provider context 与 durable projection 仍可能短时或永久分叉。目标架构应让“checkpoint committed”成为 runtime activation 的显式 ack，而不是异步观察者副作用。

### 4.3 external compaction 与 manual command 的断点

- Codex `thread/compacted` 只映射为 `ExecutorContextCompacted` 遥测：`crates/agentdash-executor/src/connectors/codex_bridge.rs:526-533`。
- eventing 明确不让该遥测推进 projection head：`crates/agentdash-application-runtime-session/src/session/eventing.rs:1078-1088`；测试固定了这个行为：同文件 `:2615-2653`。
- 当前 compaction spec 也只把结构性 checkpoint contract 适用于 Pi/native；外部缺少 summary/boundary/replacement 时只做 telemetry：`.trellis/spec/backend/session/context-compaction-projection.md:3-6`。
- Codex reference protocol已经提供 `thread/compact/start`、`thread/resume`、`thread/read`、`turn/interrupt`：`references/codex/codex-rs/app-server-protocol/src/protocol/common.rs:482-486`、`:577-580`、`:632-635`、`:811-814`，但当前 bridge 没有调用这些操作。
- AgentRun manual compaction runtime port只是创建一个带文本的 `LaunchSource::ContextCompaction` maintenance turn并轮询 request 750ms：`.../context_compaction_command.rs:201-260`；它没有先查询 selected executor 的 compaction capability。
- Codex bridge完全不读取 `ExecutionTurnMode`，而 relay payload也不传 mode/runtime delegate。由此可推断：选择外部 executor 时，manual compact-only 不会进入与 Pi 等价的结构性 compaction；它最多启动普通文本 turn或让 request停留在 requested/timeout 后只返回 launched。当前测试只用 fixture runtime验证 command receipt，没有 external Codex/relay 集成测试。

**判断：** compaction 应从“Pi Agent loop 的 delegate 特例”提升为统一 Agent Runtime command。内部 driver 可以用 AgentDash-owned projection；外部 driver必须声明它是 platform-managed、executor-managed-with-checkpoint-export，还是 unsupported。不能继续用一个 `ExecutionTurnMode` 暗示所有 connector都实现相同语义。

## 五、Codex bridge 与扩展 App Server Protocol 的差距

### 5.1 Conversation lifecycle

- 每次 prompt 都启动新的 `npx ... codex app-server` 进程：`crates/agentdash-executor/src/connectors/codex_bridge.rs:620-655`。
- 无 follow-up 时 `thread/start`；有 `executor_session_id` 时调用 `thread/fork`，不是 `thread/resume`：同文件 `:879-913`。
- application planner 自动把上一 `SessionMeta.executor_session_id` 当 follow-up：`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:233-248`。

因此当前“同一 RuntimeSession 的后续 turn”被映射为“基于旧 Codex thread 再 fork 一个新 thread”。这可能保留历史，但它改变了 conversation identity、lineage、title、native state 与 compaction semantics，不是完整 resume。

### 5.2 输入与 context channel

- `build_prompt_text` 对 canonical input 调用 `to_fallback_text()`，再把所有 ContextFrame rendered text拼接成一个字符串：`.../codex_bridge.rs:176-187`。
- `turn/start` 最终只发送一个 `UserInput::Text`：同文件 `:930-943`。
- `compose_prompt_text` 无视 frame 的 System/Developer/Context/User channel，只串接文本：`crates/agentdash-executor/src/connectors/context_frame_render.rs:3-22`。

结果是图片/LocalImage/Skill/Mention、多角色 context、delivery metadata都在 Codex adapter边界退化。Pi connector反而用结构化 `ContentPart` 并按 System/Developer channel组 prompt（`.../pi_agent/connector.rs:677-684`、`:1166-1179`）。

### 5.3 Cancel、approval 与错误

- Codex cancel只 cancel本地 token/杀进程，没有发送原生 `turn/interrupt`：`.../codex_bridge.rs:960-964`。
- server -> client 的 command/file approval 被自动返回 `acceptForSession`，tool user input被自动回空答案：同文件 `:553-565`。
- 但 connector又声明 `supports_permission_policy = true`：`:578-587`，而显式 approve/reject API 返回“尚未接入”：`:1012-1030`。能力声明、真实执行和平台审批面相互矛盾。
- JSON-RPC error只保留 `error.message` 并转成 `ConnectorError::Runtime`：`:768-802`，丢失 code/data/retryability。
- 未识别 notification只写 Debug diagnostic，未知 server request返回 `null` 成功响应：`:542-565`，会掩盖 protocol drift。

### 5.4 Capability 描述不足

`ConnectorCapabilities` 只有 7 个布尔值（cancel/steering/discovery/variants/model override/permission policy/source title）：`crates/agentdash-spi/src/connector/mod.rs:40-50`。`CompositeConnector` 对所有 child做 OR，生成 aggregate capabilities：`crates/agentdash-executor/src/connectors/composite.rs:298-313`；API 再把这份 aggregate能力挂在 composite connector上，同时单独列 executors：`crates/agentdash-api/src/routes/discovery.rs:9-49`。

后果：能力不是 per executor，无法回答“Codex支持、Pi不支持”或“relay backend A支持、B不支持”；也无法表达能力模式、协议版本或 checkpoint ownership。

前端手写 `ConnectorCapabilities` 甚至缺少 Rust 已有的 `supports_steering`：`packages/app-web/src/features/executor-selector/model/types.ts:8-16`。API DTO直接引用 SPI type而非 generated contract：`crates/agentdash-api/src/dto/discovery.rs:3-20`。这是当前 protocol drift 的直接实例。

## 六、状态、错误、取消与事件边界

### 6.1 当前状态所有者

| 状态 | 当前 owner | 评价 |
| --- | --- | --- |
| AgentRun/Frame/delivery binding | application/domain | 应继续作为业务事实源 |
| RuntimeSession events/meta/projection | application-runtime-session + persistence | 应继续作为云端 durable trace/model projection |
| runtime map/active turn/cancelling/ephemeral | application-runtime-session `SessionRuntimeRegistry`/`TurnSupervisor` | live coordination 合理，但不能替代 connector ownership |
| Pi messages/tools/stream/error | core `AgentState` | 是 durable projection 的 live副本，需明确 cache/activation 规则 |
| Pi per-session Agent/model/system prompt | `PiAgentConnector.agents` | executor live driver state |
| Codex child process/thread/active turn | `CodexBridgeConnector.live_sessions` | external native state |
| relay session route/backend lease | application relay connector/transport | transport ownership，目前混入 application |

`SessionRuntimeRegistry` 明确区分 active turn和 runtime entry：`crates/agentdash-application-runtime-session/src/session/runtime_registry.rs:10-91`；spec也要求 runtime map、active turn、connector live session分离。这个原则是正确的。问题是 connector contract只返回 `has_live_session(bool)`，没有 typed handle/revision/owner，也不能查询 native conversation state。

### 6.2 事件可靠性

- core authoritative lifecycle event使用 broadcast channel，容量 65536；send忽略错误，lagged receiver跳过事件：`crates/agentdash-agent/src/event_stream.rs:9-24`、`:42-69`。
- local -> cloud notification forwarder同样在 broadcast lagged时“跳过部分消息”：`crates/agentdash-local/src/handlers/prompt.rs:493-543`。

delta可以允许丢弃并靠终态 SET恢复，但 turn/item/compaction/checkpoint/approval/terminal 不能丢。当前同一流混合了 authoritative lifecycle和observer progress，可靠性 contract不够。

### 6.3 错误边界

- `ConnectorError` 只有 InvalidConfig/SpawnFailed/Runtime/ConnectionFailed/IO/JSON：`crates/agentdash-spi/src/connector/mod.rs:962-976`，没有 Unsupported/NotFound/StaleTurn/Busy/Cancelled/Protocol/Retryable/Terminal。
- runtime delegate error只有 `AgentRuntimeError::Runtime(String)`：`crates/agentdash-agent-types/src/runtime/delegate.rs:14-18`。
- core保留结构化 provider classification，但对 legacy error仍在 core里按字符串猜测网络/HTTP语义：`crates/agentdash-agent/src/bridge.rs:220-290`。

这些错误在 application/Codex/relay边界反复 `.to_string()`，导致 command availability、retry、HTTP/relay映射和 terminal classification无法共用一个稳定事实。

### 6.4 Cancel 路由

- `CompositeConnector::cancel` 对所有 child广播，并把任一 `Ok` 当成功：`crates/agentdash-executor/src/connectors/composite.rs:395-409`。
- Pi/Codex在 session不存在时也返回 `Ok(())`：`.../pi_agent/connector.rs:1039-1052`；`.../codex_bridge.rs:960-964`。
- approve/reject则通过逐个试错路由：`.../composite.rs:412-448`。

因此 Composite没有稳定 session -> driver ownership registry。cancel可能在没有 owner时仍报告成功；approval错误取决于遍历顺序和最后一个错误文本。应以 typed live handle路由，不应广播猜测。

## 七、依赖倒置与协议泄漏判断

### 7.1 事实与判断矩阵

| 问题 | 结论 | 依据 |
| --- | --- | --- |
| application是否直接依赖 concrete executor | 主链没有 | runtime-session只持 `dyn AgentConnector` |
| application是否依赖完整通用 Agent abstraction | 否，只依赖窄 connector facade | compaction另建 port/mode/delegate；conversation能力缺失 |
| executor是否是唯一适配边界 | 否 | relay connector在 application；Codex types在 types/protocol/SPI；local重跑application runtime |
| 是否存在编译层 dependency inversion failure | 局部存在 | agent core/types依赖 Codex/domain；application-ports依赖 relay DTO；API依赖 executor internals |
| 是否存在更严重的语义 inversion failure | 是 | inner port由外部协议/transport shape塑形，业务命令无法按能力派发 |
| 内部/外部 Agent是否真正对齐 | 否 | 只有部分 Backbone event外形对齐，输入、context、compaction、resume、approval、cancel不同 |
| 当前外部 compaction是否可用于AgentDash model projection | 明确不可以 | 被建模为 telemetry且不推进 head |

### 7.2 其它明确泄漏

- `agentdash-agent-types::protocol` 直接把 Codex ThreadItem当内部类型：`crates/agentdash-agent-types/src/protocol.rs:1-21`。
- `UserInputBlock` 只是 Codex `UserInput` alias：`crates/agentdash-agent-protocol/src/backbone/user_input.rs:10-16`，alias并没有真正隔离依赖。
- BackboneEvent payload直接使用大量 Codex notification DTO：`crates/agentdash-agent-protocol/src/backbone/event.rs:21-57`。
- `PlatformEvent::SessionMetaUpdate { key: String, value: Value }` 承担 compaction、approval、terminal、context frame等核心语义：`crates/agentdash-agent-protocol/src/backbone/platform.rs:4-28`；消费者靠字符串 key解析。
- application-ports注释称 relay prompt抽象“不依赖 relay协议”，实际直接 import `agentdash_relay::McpServerRelay`：`crates/agentdash-application-ports/src/backend_transport.rs:3-5`、`:117-134`。
- executor crate说明仍写“AgentConnector trait、SessionHub”，但 trait在SPI、SessionHub已在application-runtime-session：`crates/agentdash-executor/Cargo.toml:1-4`，说明模块定位已漂移。

## 八、建议的目标架构

### 8.1 目标依赖图

```text
Application / AgentRun use cases
  -> agent-runtime-contract（项目自有、协议无关）
      -> AgentRuntimePort + commands + descriptors + typed events/errors

Composition root
  -> ExecutorRouter
      -> InternalBusinessAgentDriver
          -> Business Agent Runtime
              -> Agent Core
              -> context/compaction/checkpoint ports
      -> CodexAppServerDriver
          -> Codex protocol adapter + process transport
      -> RemoteExecutorDriver
          -> versioned relay runtime protocol

API / NDJSON / Relay / Codex JSON-RPC
  -> protocol adapters
  -> 不进入 Agent Core 或 application command model
```

建议模块边界：

1. **`agentdash-agent-core`（由当前 `agentdash-agent` 净化）**
   - 只拥有 provider-neutral turn loop、tool loop、structured message/content和 engine event。
   - 不依赖 `agentdash-domain`、Codex protocol、Backbone、Session/AgentRun repository、Lifecycle URI。
   - `LlmBridge` 改为结构化 provider client port，request携带 cancellation/deadline，error不做字符串猜测。
   - core接受一次 turn的 context snapshot并返回 outcome；stateful message cache只能是可丢弃cache，不是恢复事实源。

2. **`agentdash-business-agent-runtime`（新增 cohesive业务 Agent层）**
   - 内部 Agent conversation/turn owner，组合 context materialization、AgentFrame surface、tool/admission、mailbox boundary、compaction policy/lifecycle和checkpoint activation。
   - compaction summary prompt、Lifecycle recall index、MessageRef boundary、projection commit ack、manual/auto trigger从 core移到这里。
   - 它实现与外部 driver相同的 `AgentRuntimeDriver`，内部再调用 Agent Core。

3. **`agentdash-agent-runtime-contract`（可由精简后的SPI独立出来）**
   - application只依赖该合同，不依赖 Codex/relay/Backbone DTO。
   - 命令至少包含：`OpenConversation`、`ResumeConversation`、`ForkConversation`、`StartTurn`、`SteerTurn`、`InterruptTurn`、`CompactConversation`、`ResolveApproval`、`UpdateRuntimeSurface`、`ReadConversationState`、`CloseConversation`。
   - 返回 typed accepted/terminal outcome和稳定 `RuntimeConversationHandle`/`RuntimeTurnHandle`，不再用 `follow_up_session_id: Option<&str>` 同时暗示 resume/fork。

4. **`agentdash-executor`**
   - 成为唯一 driver registry/router和物理执行 adapter边界；relay connector从 application移入此层。
   - 按 executor id返回 concrete descriptor并路由，不再暴露 Composite OR capabilities，也不广播 cancel/approval。
   - Codex process、provider HTTP、remote relay属于 driver adapters；provider catalog use case/API DTO不应由 executor public module直接暴露。

5. **`agentdash-agent-protocol`**
   - 项目自有的“扩展 Codex App Server Protocol”应是 versioned owned types，不是 re-export外部 crate DTO。
   - Codex v2、Backbone NDJSON、relay WebSocket分别是 adapter；内部 runtime event先使用项目 typed event，再映射 wire。
   - 删除核心路径的 `SessionMetaUpdate(key, Value)`，为 compaction started/noop/committed/failed、approval、terminal、context delivery等建立 typed variant。

### 8.2 能力模型不能继续用布尔集合

建议每个 executor返回自己的 descriptor：

```rust
struct AgentRuntimeDescriptor {
    executor_id: ExecutorId,
    protocol_revision: RuntimeProtocolRevision,
    conversation: ConversationCapabilities,
    turn: TurnCapabilities,
    context: ContextDeliveryCapabilities,
    compaction: CompactionCapability,
    approval: ApprovalCapability,
    surface_update: SurfaceUpdateCapabilities,
}

enum CompactionCapability {
    PlatformManaged,
    ExecutorManaged { checkpoint_export: CheckpointExportCapability },
    Unsupported,
}
```

这里必须表达“谁拥有模型上下文”和“能否导出可恢复 checkpoint”，而不仅是 `supports_compaction: bool`。内部业务 Agent应为 `PlatformManaged`；Codex如果通过扩展协议返回 replacement/checkpoint，可为 `ExecutorManaged { checkpoint_export: Full }`；拿不到 summary/boundary/replacement时不能宣称与平台 projection对齐。

Context capabilities也应声明 System/Developer/User/AdditionalContext/Native channel和结构化 image/file input，不应由 application根据 connector id字符串猜测。

### 8.3 Compaction 的正确事务/激活边界

内部业务 Agent：

```text
materialize durable model context
  -> evaluate/execute compaction candidate
  -> generate summary/replacement projection
  -> commit checkpoint + head
  -> acknowledge activation
  -> replace live runtime context
  -> emit typed CompactionCommitted + ItemCompleted
```

外部 Codex：

```text
CompactConversation command
  -> thread/compact/start（或项目扩展 operation）
  -> executor returns checkpoint/revision/replacement projection extension
  -> platform validates and commits normalized checkpoint
  -> command completed
```

若外部 runtime只发 deprecated `thread/compacted(thread_id, turn_id)`，它只能是 `ExternalContextChanged` telemetry，不能让 AgentRun manual compact命令返回成功，也不能用于resume/fork projection。

### 8.4 Relay 的目标

Relay不应再传一个缩水版 prompt然后让 local重跑 application runtime。建议传 versioned `PreparedRuntimeCommand`/`PreparedTurnSpec`：

- 已解析的 executor/driver identity；
- conversation command（new/resume/fork/compact/start/steer/interrupt）；
- immutable runtime surface revision和允许的context channels；
- input、MCP/VFS/runtime tool refs、permission/admission contract；
- command id、expected conversation/turn revision、cancel token identity；
- executor capability revision。

local只拥有物理 process/driver live state和必要的native conversation cache；云端继续拥有 AgentRun、RuntimeSession durable events和model projection。这样不会出现cloud/local双重 launch planner、双重 session persistence和context丢失。

### 8.5 事件与错误

- authoritative lifecycle使用有背压、不可丢的单消费者通道或 command/event journal；observer delta可以broadcast/evict。
- driver error统一成 typed code/category：unsupported、not_found、stale_turn、busy、cancelled、protocol、transport、provider、invalid_input、internal，并携带retryable/terminal/details。
- cancel返回 `Requested/AlreadyTerminal/NotFound/Unsupported`，之后由driver terminal event确认物理结束；application业务终态仍由AgentRun delivery binding收敛。
- approval request必须产生typed request和response correlation id；禁止adapter自动接受后同时宣称平台审批已支持。

## 九、建议的重构切片

以下是架构迁移顺序，不建议保留旧API兼容层或长期双轨：

1. **合同先行：** 建立 per-executor descriptor、typed command/event/error和conversation/turn handle；用 contract tests固定内部Pi、Codex、relay所需语义。
2. **协议去污染：** 将 Codex DTO从 agent-types/SPI/application contract移到adapter；把 compaction/terminal/approval核心 `SessionMetaUpdate` 变成typed event。
3. **净化 Core：** 把 compaction business、Lifecycle refs和AgentDash persistence concern移入业务Agent runtime；core只保留loop/tool/provider primitive。
4. **内部 Driver 收敛：** 以业务Agent runtime实现完整driver，修复context delivery按最终executor descriptor执行，并让checkpoint commit ack控制live activation。
5. **Codex Driver 完整化：** 接入native resume/read/interrupt/compact/approval；保留structured UserInput和system/developer/additional context；明确external checkpoint extension。
6. **Relay 硬切：** relay传完整runtime command/spec，local停止重跑application session pipeline；移动RelayAgentConnector到executor/transport adapter层。
7. **删除旧缝：** 删除旧 `AgentConnector` mega trait、`ExecutionTurnMode::ContextCompaction` 特例、manual compaction专用SessionRuntimePort、Composite aggregate capability、字符串meta解析和broadcast authoritative event。

每一切片都应以内部Pi + direct Codex + cloud→relay→local Codex三套contract test验收，尤其覆盖new/resume/fork、structured input、context channel、manual/auto compaction、checkpoint、steer、interrupt、approval、reconnect和error mapping。

## 十、现有测试与历史结论

### 已有正向保障

- Agent Core eligibility测试覆盖empty context、ref mismatch、boundary missing和true noop：`crates/agentdash-agent/src/compaction/mod.rs:681-770`。
- compact-only失败测试确保结构性错误不是noop：`crates/agentdash-agent/src/agent_loop.rs:605-672`。
- eventing测试确保external compact telemetry不推进projection head：`crates/agentdash-application-runtime-session/src/session/eventing.rs:2615-2653`。
- 2026-07 manual compaction任务已经把“成功边界是projection checkpoint”写入设计并修复Pi/native链：`.trellis/tasks/archive/2026-07/07-09-manual-context-compaction-execution/design.md`。
- 2026-05 review已经识别Codex native compaction与AgentDash projection断开；当前代码通过重命名为`ExecutorContextCompacted`明确了telemetry属性，而不是假装提交projection：`docs/reviews/2026-05-26-compaction-session-branching-review/03-findings.md`。

### 关键测试空白

- 没有 external Codex/relay manual compact command test。
- 没有 `thread/resume`、`thread/compact/start`、`turn/interrupt`、正式approval response测试，因为生产bridge尚未实现。
- 没有 production `CompositeConnector` 下context delivery profile测试。
- 没有 per-executor capability contract；discovery只测试/返回aggregate connector能力。
- 没有“projection commit失败前禁止live context activation”的ack测试。
- 没有 authoritative event consumer lagged时turn/compaction/terminal不可丢测试；当前实现明确允许跳过。

## Caveats

- 本调查没有运行真实Codex进程或端到端浏览器流；关于external manual compact“退化为普通turn/request不被消费”的结论是根据mode/delegate没有跨relay、Codex bridge忽略mode以及缺少compact operation三项代码事实做出的高置信推论，应在后续设计前用一个最小integration test复现并锁定。
- 本文件聚焦生产主链和最高影响边界，没有枚举所有 integration connector、MCP tool adapter和前端事件consumer。它们应在目标contract确定后做机械消费者迁移清单。
- 当前spec中有多处current baseline滞后，例如relay完整context透传和executor职责描述；后续设计不应把这些baseline文字当作既有实现事实，应以本调查代码证据修订。
