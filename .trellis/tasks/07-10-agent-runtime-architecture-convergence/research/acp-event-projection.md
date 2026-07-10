# ACP 作为 AgentDash 会话事件投影协议：专项评估

> 范围调整：本文不再把 ACP 当作 Agent Runtime Driver、L2 ConversationRuntime boundary 或外部 Agent 的执行协议。这里只评估 AgentDash 已有 canonical Runtime Thread/Turn/Item/Event 在提交后，能否通过 ACP 词汇投影给外部观察者。
>
> 研究基线为 workspace 锁定的 `agent-client-protocol 0.10.2` / `agent-client-protocol-schema 0.11.2`（启用 `unstable`），并继承 `research/acp-l2-boundary.md` 已核验的 schema 和 SDK 事实。本文不修改生产代码。

## 1. 结论

**ACP 不适合作为 AgentDash canonical Runtime Event Stream、Relay wire 或可靠事件总线；它最多适合作为 canonical stream 之后的、可丢失且可重建的 UI/会话投影 adapter。首期建议不实现。**

原因不是 ACP 的消息类型太少，而是它的协议角色和 AgentDash 的传播需求不同：

- ACP 是 Client 驱动 Agent 的双向 JSON-RPC 协议，`session/update` 主要表达 Agent 在处理 `session/prompt` 时向 Client 报告的展示更新。
- AgentDash 要向外部消费者传播的是已经发生的 durable Runtime 事实，需要 event sequence、cursor、replay、ack/gap、terminal、revision 和访问控制。
- ACP 的 update 词汇能很好表达 message chunk、thought、tool progress、plan 和少量 session metadata，但不能表达 canonical Turn terminal、Operation、Binding、Interaction、context checkpoint/activation 或 durable cursor。
- `session/load` 可以把完整 conversation history 以 update 重放给 Client，但没有 `afterSequence`、事件确认、增量恢复或 live subscription 合同；它不是 event log replay API。
- 为了补齐这些缺口而大量使用 `_meta.agentdash` 和 namespaced methods，最终会得到一套披着 ACP 外壳的 AgentDash Runtime Wire。对受控企业消费者，直接使用 owned Runtime Event Stream 更清晰。

推荐位置：

```text
Managed Agent Runtime
  -> authoritative Runtime Event Journal / Snapshot
      -> projection policy + authorization + redaction
          -> optional ACP Session Projection Adapter
              -> external ACP Client / observer

Relay
  -> transports AgentDash-owned Runtime Wire
  -> does not transport ACP as the canonical frame
  -> does not own or rebuild the ACP projection
```

该 adapter 属于 **read-side protocol/presentation Integration**，不属于 Driver Host，不参与 RuntimeBinding，不声明 L1/L2/L4，不影响 Agent service capability，也不能成为控制命令的事实入口。

## 2. 重新表述问题

需要解决的问题不是“怎样让外部系统看起来像 ACP Agent”，而是：

> 外部系统如何在不成为 Runtime owner 的前提下，接收一个 AgentDash Thread 的可授权、可重放、顺序明确的会话状态。

其中有几个不可省略的事实：

1. AgentDash durable Runtime Event Journal 是状态事实源；任何外部协议只是 projection。
2. 外部消费者可能断线、落后、重复接收或只被允许查看部分内容。
3. `TurnTerminal::Failed/Lost/Interrupted`、pending Interaction、context fidelity 等事实会影响消费者对 Thread 状态的正确判断，不能靠“最后一段 assistant 文本”推断。
4. ACP Client 通常预期自己发起 `session/prompt`，并通过对应的 `PromptResponse` 得到该 turn 的停止原因；纯被动观察者没有这条 request/response correlation。
5. 所以，消息 DTO 能映射一部分不等于协议能承载完整 Runtime 保证。

## 3. ACP 原生可表达的投影

锁定 schema 的 `SessionUpdate` union 位于 `agent-client-protocol-schema-0.11.2/src/client.rs:74-105`，共有 11 类 update。下表以 AgentDash canonical event 为输入，区分能够保持语义的 projection、仅展示映射和不可表达项。

### 3.1 Thread 与消息

| AgentDash 事实 | ACP 投影 | 强度 | 说明 |
| --- | --- | --- | --- |
| Thread title / last activity | `session_info_update` | `HostAdaptedBoundary` | 原生只有 title、updatedAt；见 `client.rs:190-244`。|
| Thread started/resumed/forked | 无原生 update | `UnsupportedCurrent` | `session/new/load/fork/resume` 是 Client→Agent request，不是可供被动观察者接收的 lifecycle notification。|
| Thread status / active turn / revision | 无原生 update | `UnsupportedCurrent` | 不能用 title 或 mode 冒充。|
| UserMessage Item 内容 | `user_message_chunk` | `HostAdaptedBoundary` | 可以投影 `ContentBlock`；unstable `messageId` 可关联 chunks，但没有 canonical Item lifecycle/revision。|
| AgentMessage Item 内容 | `agent_message_chunk` | `HostAdaptedBoundary` | 适合流式展示；不能表达 final authoritative Item。|
| Reasoning / thought delta | `agent_thought_chunk` | `HostAdaptedBoundary` | 适合可见 reasoning delta；必须先经过 disclosure/redaction policy。|
| Image/audio/resource message content | 对应 `ContentBlock` | `HostAdaptedBoundary` | 受 ACP ContentBlock 能力和消费者实现限制；不能自动等同于 AgentDash 所有 typed Item。|

`ContentChunk` 的 `messageId` 仍是 unstable，且只说明 chunks 属于同一 message；它不是 RuntimeItemId、TurnId、event sequence 或幂等 key（`client.rs:326` 起）。若 adapter 发送 canonical ID，只能作为 projection metadata，不能要求通用 ACP Client 理解。

### 3.2 Tool、Plan、Usage 与显示配置

| AgentDash 事实 | ACP 投影 | 强度 | 说明 |
| --- | --- | --- | --- |
| ToolCall Item started | `tool_call` | `HostAdaptedBoundary` | 有 session-scoped `toolCallId`、title、kind、status、content、locations、raw input/output。|
| ToolCall Item progress/final | `tool_call_update` | `HostAdaptedBoundary` | status 只有 Pending/InProgress/Completed/Failed；见 `tool_call.rs:420-445`。|
| CommandExecution/FileChange | `tool_call` + Terminal/Diff content | `Observed` | 对 UI 有用，但会丢失 canonical item kind、policy、terminal provenance 和 operation scope。|
| Plan projection | `plan` | `HostAdaptedBoundary` | ACP Plan 是全量替换，适合 projected current plan；见 `plan.rs:23-61`。|
| Usage/cost | unstable `usage_update` | `Observed` | 只有 context used/size/cost，且 schema 标 unstable；不能表达完整 per-turn telemetry。|
| 可用命令 | `available_commands_update` | `Observed` | 只能投影人类可调用的 presentation command；不能把 AgentDash command admission guarantee 简化成命令名称列表。|
| 当前 mode/config | `current_mode_update` / `config_option_update` | `Observed` | 适合显示映射；不能表达 ThreadSettingsRevision、ToolSetRevision 或 capability provenance。|

ToolCall 是 ACP 最接近 RuntimeItem lifecycle 的词汇，但仍有三个边界：

1. `ToolCallId` 只保证 session 内唯一，不携带 RuntimeTurnId 或 event sequence；
2. Tool status 没有 Interrupted、Lost、LimitReached、Refused 等 Runtime terminal；
3. `tool_call_update` 是字段覆盖投影，不是 durable domain event。

因此它可以驱动外部 UI 卡片，不能成为 Tool operation 的审计事实源。

## 4. ACP 无法原生表达的 Runtime 事实

### 4.1 Turn lifecycle 与 terminal

ACP 没有 `turn_started` 或 `turn_terminal` notification。Turn 停止原因只存在于 `session/prompt` 的 `PromptResponse.stopReason`（`agent.rs:2642-2763`）：

```text
end_turn | max_tokens | max_turn_requests | refusal | cancelled
```

这对主动发出 prompt 的 Client 是合理的，但对“只接收平台既有会话状态”的观察者不可用：观察者没有对应的 JSON-RPC prompt request，AgentDash 不能凭空发送一条 response。即使伪造长时间 `session/prompt` 作为订阅，也会制造并不存在的 user turn，并且仍无法表达 Runtime 的 `Failed` 与 `Lost`。

所以以下事实不能通过标准 ACP 保真传播：

- turn accepted / started；
- current active RuntimeTurnId；
- Completed / Interrupted / Refused / LimitReached / Failed / Lost 的完整 terminal；
- exactly-one terminal 和 terminal 之后禁止 delta；
- expected turn / thread revision。

外部 ACP observer 最多从消息停止到来中猜测“似乎空闲”，这种推断不能进入业务逻辑。

### 4.2 Operation、Binding 与协议健康

ACP 没有对应词汇表达：

- `operation/accepted`、idempotency key、operation sequence 与 operation terminal；
- RuntimeBinding、service instance、driver generation、placement；
- binding lost、desynchronized、protocol violation；
- source/canonical ID mapping；
- stale generation fencing。

这些信息通常也不应该全部暴露给普通观察者，但管理面消费者确实需要时，应直接订阅授权后的 AgentDash typed Runtime events，而不是塞入 session update。

### 4.3 Interaction

ACP 有 Agent→Client 的 `session/request_permission` RPC，但这不是观察事件。向外部观察者发起该 RPC，意味着把 permission decision authority 交给它，还要求它返回合法 option；这会改变 Runtime，而不是投影 Runtime。

因此：

- pending/resolved/expired Permission Interaction 不能映射为 ACP permission RPC 用于只读传播；
- UserInputRequest、McpElicitation、DynamicToolExecution 没有通用 `SessionUpdate` 对应物；
- 把 pending interaction 显示成 Pending ToolCall 只能作为弱 UI 提示，不能作为可响应的 Interaction；
- 若外部系统需要回答 Interaction，必须走独立的 AgentDash authorized command API，并校验 RuntimeInteractionId、actor、expected state；不能复用被动 projection channel 暗中获得写权限。

### 4.4 Context、compaction 与 surface revisions

ACP 的 `usage_update.used/size` 是计量信息，不是 model-visible context。它无法表达：

- ContextRecipe / MaterializedContext；
- immutable ContextCheckpoint、digest、provenance；
- ActiveContextHead / ContextRevision；
- compaction candidate、activation、terminal；
- ContextFidelity；
- ToolSetRevision、ThreadSettingsRevision。

也不能把 `agent_thought_chunk`、Plan 或 ToolCall 当作 compaction event。若消费者需要这些管理事实，应使用 canonical snapshot/event schema；普通会话 viewer 通常只需要看到一条经过授权的“context compacted”展示项，且必须明确它是 lossy presentation，而不是恢复依据。

### 4.5 Durable cursor、replay 与 backpressure

ACP `session/update` 没有：

- event ID / per-session sequence；
- `afterSequence` / resume cursor；
- ack；
- gap notification；
- replay window；
- consumer offset；
- protocol-level backpressure。

`session/load` 的标准语义是恢复 session context/history，并把完整 conversation history 通过 notification 重放给 Client；schema trait 注释见 `agent.rs:3627-3640`。它没有 cursor，不能只重放某个 event sequence 之后的数据，也没有说明历史重放与紧接的 live update 之间如何无缝交接。

因此 `session/load` 可以帮助 ACP UI 重建一份 transcript，但不能替代 AgentDash event-log replay。`session/list` 即使启用，也只是 session metadata 分页，不是 event subscription catalog。

## 5. Live 传播的协议角色冲突

标准 ACP 没有“订阅一个已有 session 的外部变化”方法。`session/update` 的文档语义是 Agent 在 prompt processing 期间报告实时 progress/results（`client.rs:20-105`）。虽然 JSON-RPC connection 技术上可以在 `load` 返回后继续发 notification，但通用 ACP Client 是否会把这些通知解释成合法的后台 session update，没有可靠的跨实现合同。

有三个可选方案：

| 方案 | 评价 |
| --- | --- |
| 让 observer 发一个永不结束的 `session/prompt` 作为 subscribe | 不采用；伪造 user turn，破坏 prompt/terminal 语义，disconnect/cancel也会被误解为 turn control。|
| `session/load` 后非标准地持续推 update | 只可作为双方受控 PoC；必须标为 AgentDash profile，不能声称通用 ACP live stream compatibility。|
| 增加 namespaced `agentdash/session/subscribe { afterSequence }` | 技术上可行，但可靠性已由 AgentDash extension 而非 ACP 提供；若消费者受控，直接使用 Runtime Event Stream 更清晰。|

如果未来确有“已有 ACP Client UI，希望低成本展示 AgentDash transcript”的明确需求，第二种可以作为 `BestEffortLive` presentation profile；一旦消费者依赖 cursor、terminal 或交互，应该使用第三种或直接切换到 owned stream。

## 6. 推荐的接入层级

### 6.1 归入 Read-side Projection Integration

建议以后若实现，引入的不是 `AcpConversationAdapter: AgentRuntimeDriver`，而是类似下面的 read-side contribution：

```rust
RuntimeProjectionContribution {
    protocol: AcpSessionProjection,
    supported_projection_profile,
    factory,
}
```

其输入只能来自：

```text
RuntimeSnapshot + RuntimeEventPage/Subscription
```

其输出是：

```text
ACP initialize/session list/session load
+ projected session/update notifications
```

它必须位于 authentication/authorization/redaction 之后，并且：

- 不创建 RuntimeThread；
- 不接受 TurnStart/Steer/Interrupt；
- 不参与 RuntimeBinding；
- 不更新 canonical projection；
- 不保存第二份 authoritative ACP session history；
- 断线重连时从 canonical snapshot/event journal 重建；
- mapping 失败、消费者 lag 或投影能力不足时返回明确 error/关闭流，不让 ACP 状态反向覆盖 Runtime 状态。

这仍然可以通过受信 Integration 系统注册，但 contribution kind 应与 `AgentRuntimeDriverContribution` 分开。两者的生命周期、权限和测试不相同。

### 6.2 与 Application 的关系

Application 负责决定一个外部 actor 能否看见哪个 AgentRun/RuntimeThread，以及 message、reasoning、raw tool input/output、file diff、usage 和 system context 的具体 disclosure policy。

Projection adapter 不应该自行查 AgentRun repository 猜权限，也不应该直接消费 Driver events。合理的数据流是：

```text
Application access scope
  + Managed Runtime snapshot/event stream
  -> authorized projection view
  -> ACP mapper
```

产品 mailbox、AgentFrame 或业务 receipt 若要出现，先由 Application 构造独立 product projection；不能伪装为 Runtime ToolCall 或 message chunk。

### 6.3 与 Driver Host 的关系

没有关系。ACP observer projection：

- 不描述一个 Agent service instance；
- 不执行模型；
- 不拥有 driver generation；
- 不向 Runtime 声明 conversation/context/tool capability；
- 不决定 effective runtime profile。

因此应从当前设计中的 “ACP L2 adapter” 路线移除，避免继续产生 `DriverThreadId <-> ACP SessionId` 的执行绑定语义。Projection endpoint 可以把 RuntimeThreadId编码/映射成 ACP SessionId，但那只是外部 read model coordinate。

## 7. 与 canonical Runtime Event Stream 和 Relay 的关系

### 7.1 Canonical stream 永远在前

ACP projection 必须从 committed durable event和revisioned snapshot派生。推荐内部管线：

```text
1. resolve external principal + Thread access
2. read RuntimeSnapshot at revision R
3. read durable RuntimeEventPage after snapshot watermark
4. map allowed facts to ACP history/update
5. subscribe canonical stream from durable sequence N
6. on gap/lag, abandon live projection and rebuild from snapshot/cursor
```

ACP 自身没有把 N 暴露给通用 Client 的位置。实现可在内部持有 cursor，但消费者不能据此获得 end-to-end delivery guarantee。因此对外 descriptor 必须声明：

```text
HistoryProjection: Rebuildable
LiveDelivery: BestEffort
TurnTerminal: Unavailable
DurableCursor: Unavailable
Interaction: ReadOnlyUnavailable
Context: Unavailable
```

不要因为内部 mapper从可靠 journal 读取，就把 ACP 对外链路声明成可靠事件流。

### 7.2 Relay 继续只运 AgentDash-owned Runtime Wire

Relay 的职责仍是 ordered frame、ack/replay、route、connection health 与 placement。建议保持：

```text
Managed Runtime <-> AgentDash Runtime Wire <-> Relay <-> remote projection gateway
                                                     -> ACP mapper -> ACP client
```

而不是：

```text
Managed Runtime -> ACP session/update -> Relay -> consumer
```

后者会在 transport 边界丢失 terminal、cursor、interaction、context 和 typed error，并迫使 Relay理解 ACP session lifecycle。若 ACP endpoint部署在远端，它可以通过 Relay或普通 Runtime Event API获得 canonical frames，但 ACP 只存在于最外缘。

### 7.3 不建立第二个事件仓库

ACP projection 是确定性的可重建 read model。首期不需要：

- `acp_session_event` 表；
- ACP consumer offset表；
- ACP DTO outbox；
- 把 ACP notification写回 Runtime journal。

若未来有 durable external delivery需求，应先建立通用 `RuntimeProjectionSubscription` / webhook/outbox contract，再让 ACP只是一个 formatter；不要为 ACP单独复制一套可靠消息基础设施。

## 8. Projection allowlist 与安全边界

外部接收“平台会话状态”并不意味着可接收全部 Runtime事实。未来 adapter至少需要按字段分类：

| 内容 | 默认建议 |
| --- | --- |
| User/Agent final message | 可按 Thread read授权投影 |
| transient message delta | 仅 live viewer需要；落后时可丢 |
| reasoning/thought | 默认不暴露；需独立授权与模型政策允许 |
| tool title/kind/status | 可投影 |
| raw tool input/output | 默认过滤；可能含secret、内部endpoint和用户数据 |
| file diff/location | 需 workspace/file access scope |
| terminal output | 需命令输出权限与secret redaction |
| usage/cost | 管理权限 |
| system/developer context | 不通过普通 ACP message projection暴露 |
| context checkpoint/digest/provenance | 仅专用管理API |
| Interaction内容和选项 | 不通过只读 ACP projection暴露或响应 |

Redaction发生在 ACP mapping之前；不能先生成完整 notification再依赖外部 Client隐藏字段。

## 9. 是否进入首期重构

**建议首期跳过 ACP projection实现，也移除“ACP Agent driver/L2 adapter”独立交付项。**

第一阶段真正需要完成的是：

1. AgentDash-owned Runtime Contract 和 durable Event Journal；
2. revisioned snapshot、durable cursor与 transient/durable stream分离；
3. read-side authorization/redaction seam；
4. Relay透明承载 Runtime Wire；
5. Native/Codex等实际执行 adapter收敛。

这些完成后，ACP projection只是一个可插拔 mapper/gateway，不再影响核心模块划分。提前实现会迫使团队在 canonical event 尚未稳定时设计一套 lossy 映射，还可能错误地让 ACP DTO反向塑造 Runtime Item/Event。

未来只有出现以下真实需求时再建立实现任务：

- 已有外部工具只能消费 ACP session/update，替换协议成本明显更高；
- 产品明确接受它只得到 transcript/tool/plan presentation，而不是完整 Runtime guarantees；
- 已决定严格 ACP、`BestEffortLive` AgentDash profile或namespaced reliable extension中的哪一种；
- 已定义 authorization/redaction、断线重建和 gap 行为；
- 已有至少一个真实 ACP Client 做互操作测试。

## 10. 若未来实现，最小行为测试

ACP projection不是 Runtime conformance的一部分，应有独立 projection tests：

1. load同一 Thread可从 canonical snapshot重建相同的message/tool/plan投影；
2. 同一个 canonical final item不会因snapshot+tail交界被重复投影；
3. reasoning、raw tool IO、diff等敏感字段在mapping前按scope过滤；
4. tool状态只能映射到ACP可表达状态，无法表达的Runtime terminal不会被假报Completed；
5. Turn Failed/Lost不通过“assistant输出结束”伪装成EndTurn；
6. pending Interaction不会向只读consumer发permission RPC；
7. consumer lag或canonical cursor gap会触发明确reload/close，而不是静默跳过durable事实；
8. ACP connection断开不改变 RuntimeThread/Turn/Binding 状态；
9. mapper重启不依赖第二份ACP状态，可从canonical snapshot/event重建；
10. Relay路径和直连路径生成相同ACP presentation，不改变Runtime Wire语义。

## 11. 对正式架构文档的调整建议

父设计后续应做以下收敛：

- 将“ACP 作为 L2 候选 Adapter”改为“ACP 作为可选的外部 session projection vocabulary”；
- 删除 ACP restart/load/resume、MCP Broker、permission RPC作为当前 Runtime Driver交付的要求；
- L2仍可保留为 AgentDash-owned `ConversationRuntime` 参考类别，但不再以ACP命名或承诺首批ACP实现；
- `agentdash-agent-runtime-contract` 不依赖或re-export ACP DTO；
- ACP projection若未来实现，归 read-side protocol Integration，依赖canonical snapshot/events，不依赖driver SPI；
- 实施计划中删除 ACP Conversation Runtime Adapter独立交付项，将本文列为未来可选研究输入；
- Relay仍只承载AgentDash Runtime Wire，ACP不成为Relay内部协议。

最终边界可以概括为：

> AgentDash Runtime Event Stream 负责“事实如何可靠传播”；ACP projection只负责“部分会话事实如何被现有ACP UI看懂”。
