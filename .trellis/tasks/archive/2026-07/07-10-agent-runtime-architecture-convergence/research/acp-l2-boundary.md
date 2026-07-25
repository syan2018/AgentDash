# ACP 能否作为 L2 ConversationRuntime seam：专项调研

> 调研对象：当前仓库锁定的 Rust `agent-client-protocol`、前端 TypeScript SDK、Cargo registry source、仓库 reference implementation，以及当前 relay / local / connector 主链。
>
> 本文只写研究结论，不修改生产代码。ACP 在这里被视为 adapter 词汇和能力候选，不被预设为永久的 AgentDash 外部标准；优先目标是让 module ownership、依赖与状态事实源收敛干净。
>
> **范围更新（2026-07-10）：** 首期已取消ACP Agent Driver/L2 Adapter交付。本文保留为协议能力档案；当前正式结论见`acp-event-projection.md`：ACP未来至多作为canonical Runtime events之后的可选read-side presentation projection，首期不实现。

## 一、结论

**不建议定义 `ACP = L2 ConversationRuntime`。建议定义“ACP 是 L2 候选 adapter/profile”，由具体 Agent + Host 保持的实际 guarantee 决定它最终只能到 L1，还是可以进入 L2。**

原因分三层：

1. ACP 原生有很好的 conversation 词汇：`session/new`、`session/load`、`session/resume`、`session/list`、`session/prompt`、`session/cancel`、`session/update`、permission、structured content、tool call、MCP、filesystem和terminal。这使它明显比当前 `AgentConnector::prompt` 更适合做外部Agent adapter。
2. ACP 原生没有 L1/L2 所需的全部 guarantee：没有 canonical turn/item/event sequence、没有 durable command idempotency、没有 reconnect cursor、没有 active-turn status query、没有 exactly-once delivery、没有 exact context snapshot/read，也没有协议级 steer、hot tools revision 或 compaction。
3. L1/L2 的部分缺口可以由 AgentDash Host 低成本保持；另一部分必须由具体 Agent 行为保证或通过轻量 AgentDash ACP profile协同补齐。若具体Agent不能在真实进程restart后以同一个 `SessionId` load/resume并保留上下文，Host不能仅凭ACP类型把它提升为L2。

推荐归属：

```text
application / AgentRun
  -> AgentDash AgentRuntime interface（canonical L1-L4）
      -> ACP Conversation Adapter（候选 L1/L2）
          -> ACP ClientSideConnection / Agent peer

Relay
  -> remote placement transport
  -> 透明承载 normalized AgentRuntime frame
  -> 不成为 ACP Agent、Agent service 或 executor
```

ACP 应在 driver adapter 内终止，ACP DTO不能进入 application、AgentRun repository或 canonical runtime journal。这样未来企业 Agent Core、Host 和协议可以协同调整，而不需要把 ACP 的连接级限制永久写入业务模型。

严格判定建议：

- **ACP baseline + Host wrapper**：可以成为 L1 candidate；通过 single-active-turn、durable command receipt、arrival-order journal、terminal收敛和disconnect=>Lost测试后才声明L1。
- **ACP Agent支持并通过restart后的 `session/load` 或 `session/resume`行为测试**：可成为L2 candidate；Host另需持久化session binding并明确read fidelity。
- **只支持 `session/new/prompt/cancel/update`**：只能L1，不能因为有`SessionId`就叫ConversationRuntime。
- **只在同一进程内能load/resume**：仍不能L2；L2要求真实driver/process restart后的continuation。
- **ACP原生能力不能达到L4**：它没有exact/verified model-visible snapshot、context activation或durable compaction合同。

## 二、仓库依赖与 source 事实

### 2.1 Rust 依赖

workspace声明：

```toml
agent-client-protocol = { version = "0.10.2", features = ["unstable"] }
```

证据：`Cargo.toml:130-131`。

`Cargo.lock:57-79` 实际锁定：

- `agent-client-protocol 0.10.2`；
- `agent-client-protocol-schema 0.11.2`。

registry package metadata把source指向：

- Rust SDK：`https://github.com/agentclientprotocol/rust-sdk`；
- Schema：`https://github.com/agentclientprotocol/agent-client-protocol`。

`agent-client-protocol 0.10.2` 对 schema使用精确 `=0.11.2`，因此下文Rust协议事实以 `agent-client-protocol-schema-0.11.2` 为准。workspace启用了聚合 `unstable` feature，所以Rust编译面同时包含unstable auth、cancel request、fork、close、model、resume、usage、message id和boolean config。

### 2.2 TypeScript 依赖与版本漂移

前端声明 `@agentclientprotocol/sdk ^0.14.1`：`packages/app-web/package.json:21`，当前安装包版本也是0.14.1。

这与Rust 0.10.2/schema 0.11.2不是同一版本。已观察到的具体漂移：

- Rust + `unstable` 暴露 `session/close`；TS 0.14.1的 `ClientSideConnection`/`Agent` interface中没有close方法。
- Rust trait把`list_sessions`作为方法、注释仍标UNSTABLE；TS以`unstable_listSessions`命名。
- Rust feature gate与TS SDK生成面的稳定/unstable分类不完全同步。

当前没有生产ACP connection，因此该漂移暂未表现为wire故障；但新adapter不能同时拿Rust和TS类型当canonical合同。建议ACP adapter只选择一个实现SDK，application继续依赖项目自有Runtime Contract。

### 2.3 当前生产代码对ACP的真实使用非常窄

Rust侧唯一直接消费为：

- `agentdash-agent-protocol`依赖ACP：`crates/agentdash-agent-protocol/Cargo.toml:13`；
- 只re-export `ContentBlock`、`EmbeddedResourceResource`、`TextContent`：`crates/agentdash-agent-protocol/src/lib.rs:35`。

没有生产crate创建 `ClientSideConnection`、`AgentSideConnection`，也没有实现ACP `Agent` / `Client` trait。全仓生产Rust对`agent_client_protocol::`的直接使用只有上述re-export。

前端也只在文件引用构造器导入SDK `ContentBlock`：`packages/app-web/src/features/file-reference/buildPromptBlocks.ts:1-37`。与此同时，UI又维护了一份手写、裁剪后的 `ContentBlock` / `ToolCall` / `SessionUpdate`：`packages/app-web/src/types/acp.ts:1-48`。

项目canonical用户输入实际是Codex `UserInput`别名：`crates/agentdash-agent-protocol/src/backbone/user_input.rs:10-16`。ACP `ContentBlock`只保留了到Codex输入的转换器：同文件`:261-307`。所以“项目已经使用ACP”只在content vocabulary层面成立，在conversation connection层面不成立。

### 2.4 仓库 references

仓库有两个非生产参考实现：

- `references/AgentDispatch/packages/acp`：依赖TS SDK 0.14.1；`connection-factory.ts`只做process start→initialize→newSession并返回内存handle，`pass-runner.ts`串行调用prompt。
- `references/Actant/packages/acp`：同样依赖TS SDK 0.14.1；`connection.ts:102-103`明确描述spawn→initialize→session/new→prompt→cancel→close，另有可选load wrapper。

这些reference证明ACP适合做进程Agent adapter，但它们都没有提供AgentDash L2所需的durable binding、restart conformance、event cursor或command idempotency；不能把reference implementation存在当作L2保证。

## 三、ACP 完整 method / notification 词汇

以下以锁定Rust schema 0.11.2 + workspace `unstable` feature为准；TS 0.14.1差异单独标注。

### 3.1 Client → Agent：request

| Wire method | 请求 / 响应 | 核心语义 | 稳定性/能力门 |
| --- | --- | --- | --- |
| `initialize` | `InitializeRequest/Response` | 协议版本、client/agent capabilities、auth methods、implementation info | baseline |
| `authenticate` | `AuthenticateRequest/Response` | 选择initialize返回的auth method | baseline |
| `session/new` | `NewSessionRequest/Response` | 以cwd+MCP创建conversation并由Agent返回`SessionId` | baseline MUST |
| `session/load` | `LoadSessionRequest/Response` | 恢复session和history，并通过update重放完整conversation history | optional `loadSession` |
| `session/list` | `ListSessionsRequest/Response` | cwd过滤、cursor分页，返回session id/cwd/title/updatedAt | optional；SDK仍标unstable |
| `session/fork` | `ForkSessionRequest/Response` | 从现有session创建独立session | unstable capability |
| `session/resume` | `ResumeSessionRequest/Response` | 恢复session但不重放history | unstable capability |
| `session/close` | `CloseSessionRequest/Response` | cancel ongoing work并释放session资源 | Rust unstable；TS 0.14.1缺失 |
| `session/set_mode` | `SetSessionModeRequest/Response` | 切换Agent mode；可在active turn调用 | optional，由session modes体现 |
| `session/set_config_option` | request/response | 更新一个session配置并返回完整配置集合 | optional |
| `session/set_model` | request/response | 切换session model | unstable |
| `session/prompt` | `PromptRequest/Response` | 一个完整prompt turn，过程中以update报告输出，response返回stop reason | baseline MUST |
| `_<namespace/method>` | `ExtRequest/Response` | namespaced extension request | extension |

方法常量源码：`agent-client-protocol-schema-0.11.2/src/agent.rs:3490-3573`。Schema明确baseline Agent必须支持 `session/new`、`session/prompt`、`session/cancel`、`session/update`：同文件`:3100-3150`附近的 `SessionCapabilities` 注释。

### 3.2 Client → Agent：notification

| Wire notification | Payload | 语义 |
| --- | --- | --- |
| `session/cancel` | `CancelNotification { session_id }` | 取消该session当前ongoing prompt；没有RPC response |
| `_<namespace/method>` | `ExtNotification` | extension one-way message |

ACP cancel没有turn id、expected revision或ack response。协议要求Agent停止model/tool work、发送pending updates，并让原始`session/prompt`最终返回`StopReason::Cancelled`。真正的终止确认来自prompt response，不来自cancel notification发送成功。

### 3.3 Agent → Client：request

| Wire method | 请求 / 响应 | 语义 |
| --- | --- | --- |
| `session/request_permission` | `RequestPermissionRequest/Response` | 针对tool call向用户请求permission |
| `fs/read_text_file` | read request/response | Agent通过client读取文本文件 |
| `fs/write_text_file` | write request/response | Agent通过client写入文本文件 |
| `terminal/create` | create request/response | client启动命令并返回`TerminalId` |
| `terminal/output` | output request/response | 读取当前terminal output/exit status |
| `terminal/wait_for_exit` | wait request/response | 等待terminal退出 |
| `terminal/kill` | kill request/response | kill但保留terminal handle |
| `terminal/release` | release request/response | kill-if-needed并释放terminal handle |
| `_<namespace/method>` | extension request/response | Agent调用client extension |

方法常量源码：`agent-client-protocol-schema-0.11.2/src/client.rs:1663-1714`。

### 3.4 Agent → Client：notification / event vocabulary

ACP只有一个核心notification method：`session/update`。Payload为：

```rust
SessionNotification {
    session_id: SessionId,
    update: SessionUpdate,
    meta: Option<Meta>,
}
```

`SessionUpdate` variants完整列表：

| Discriminant | Payload | 语义 |
| --- | --- | --- |
| `user_message_chunk` | `ContentChunk` | user消息chunk，load replay也可使用 |
| `agent_message_chunk` | `ContentChunk` | assistant输出chunk |
| `agent_thought_chunk` | `ContentChunk` | reasoning/thought chunk |
| `tool_call` | `ToolCall` | tool call创建/当前完整状态 |
| `tool_call_update` | `ToolCallUpdate` | tool call字段增量覆盖 |
| `plan` | `Plan` | Agent plan完整投影 |
| `available_commands_update` | commands | 可用Agent commands变化 |
| `current_mode_update` | mode id | current mode变化 |
| `config_option_update` | full config options | session配置变化 |
| `session_info_update` | title/updatedAt patch | session显示元信息变化 |
| `usage_update` | context used/size/cost | unstable usage |

证据：`agent-client-protocol-schema-0.11.2/src/client.rs:20-105`；TS 0.14.1生成type包含相同11个variants：`packages/app-web/node_modules/@agentclientprotocol/sdk/dist/schema/types.gen.d.ts:2240-2269`。

ACP没有独立`turn_started`、`turn_terminal`、`item_started`、`item_terminal`或event sequence。turn lifecycle的终点是原始`session/prompt` response。

## 四、核心类型与语义

### 4.1 ID

ACP原生ID：

- `SessionId`：opaque string，表示Agent拥有的conversation；schema说明session维护自己的context/history/state。
- JSON-RPC `RequestId`：string/number/null，只在连接内关联request/response；Rust SDK每个新connection从数字0重新计数。
- `ToolCallId`：session内tool call identity。
- `TerminalId`：client创建的OS terminal identity。
- `PermissionOptionId`：permission选项identity，不是approval request identity。
- `SessionModeId`、config/model/auth等选择ID。
- unstable `message_id`：PromptRequest可带UUID，Agent SHOULD在PromptResponse回显；它是user message id，不是turn id或幂等合同。

ACP缺少：

- `TurnId`；
- 通用`ItemId`；
- `CommandId` / idempotency key；
- durable approval request id；
- event id/sequence/cursor；
- session/turn revision；
- snapshot/context head/compaction id。

因此ACP ID不能直接作为AgentDash canonical IDs。adapter必须保存：

```text
RuntimeSessionId <-> ACP SessionId
RuntimeTurnId    <-> active session/prompt JSON-RPC RequestId（connection-local）
RuntimeItemId    <-> ACP ToolCallId或host生成message item id
RuntimeApprovalId<-> ACP permission RPC RequestId + ToolCallId
```

### 4.2 Prompt与structured input

`PromptRequest`包含`session_id + Vec<ContentBlock> + meta`。`ContentBlock`为：

- `Text`；
- `Image { base64 data, mime_type, optional uri }`；
- `Audio { base64 data, mime_type }`；
- `ResourceLink { uri, name, description/mime/size/title }`；
- `Resource { embedded text/blob resource + uri/mime }`。

baseline Agent MUST支持Text和ResourceLink；Image、Audio、Embedded Resource通过`PromptCapabilities`声明。证据：`agent-client-protocol-schema-0.11.2/src/content.rs:25-55`和`agent.rs:3340-3417`。

这比纯文本prompt强，但它仍只有user prompt role。ACP没有system/developer/additional-context authority channel；embedded resource表达“本次user message引用的context”，不能等价于AgentDash model-visible context snapshot。

### 4.3 MCP、filesystem、terminal与tools

session new/load/fork/resume都可携带MCP server definitions：

- stdio：所有Agent必须支持；包含command/args/env；
- HTTP / SSE：通过MCP capability声明；包含URL和headers。

Agent还可反向调用client的filesystem read/write和terminal create/output/wait/kill/release。工具执行向UI报告为`ToolCall`/`ToolCallUpdate`：

- kind：read/edit/delete/move/search/execute/think/fetch/switch_mode/other；
- status：pending/in_progress/completed/failed；
- content：ContentBlock、Diff或Terminal；
- locations、raw input/output。

但ACP没有通用tool schema discovery、`tools/replace`、`tools/update`、`ToolsRevision`或applied ack。它表达的是：

1. session创建时把MCP连接配置交给Agent；
2. Agent自己拥有并执行tools；
3. Agent把tool UI状态报告给client。

因此ACP适合`DriverOwnedTools`，不能原生满足L3的hot tool revision guarantee。另一个安全事实是ACP MCP DTO直接携带env/header值；AgentDash目标adapter应在local credential broker处物化，不能让secret DTO进入cloud journal。

### 4.4 Permission

`RequestPermissionRequest`包含：

- `session_id`；
- 一个`ToolCallUpdate`；
- `PermissionOption[]`。

option kind为allow once/always、reject once/always。Response outcome为`Selected(option_id)`或`Cancelled`。若client取消prompt，所有pending permission requests MUST以Cancelled响应。证据：`agent-client-protocol-schema-0.11.2/src/client.rs:520-735`。

优点是ACP原生有真实双向approval RPC，不需要轮询。缺口是request本身没有durable approval id、canonical turn id、expected revision或policy provenance；Host可在收到JSON-RPC request时生成`RuntimeApprovalId`并持久映射，但跨连接恢复pending approval不是ACP原生能力。

### 4.5 Terminal语义

不要混淆两种terminal：

- `terminal/*`是client托管的OS命令终端；
- Agent turn terminal是`session/prompt`的JSON-RPC response。

`PromptResponse.stop_reason`只有：

- `end_turn`；
- `max_tokens`；
- `max_turn_requests`；
- `refusal`；
- `cancelled`。

没有`failed`或`lost` stop reason；方法失败走JSON-RPC Error，connection中断则没有response。建议adapter映射：

```text
end_turn          -> TurnTerminal::Completed
max_tokens        -> TurnTerminal::LimitReached(MaxTokens)
max_turn_requests -> TurnTerminal::LimitReached(MaxRequests)
refusal           -> TurnTerminal::Refused
cancelled         -> TurnTerminal::Interrupted
RPC Error         -> TurnTerminal::Failed(typed mapped error)
EOF / timeout     -> TurnTerminal::Lost
```

正常连接上，一个JSON-RPC request只能匹配一个response，这为terminal提供了很好的单次相关性；但协议没有保证网络断开后的remote outcome可查询，也不能证明response之前所有update已durable消费。

Tool terminal也只是`ToolCallStatus::Completed/Failed`字段更新，没有独立terminal event或exactly-once规定。Host必须验证tool状态机并对prompt terminal时仍未终止的tool item做明确收敛。

### 4.6 Error

ACP采用JSON-RPC `Error { code, message, data }`。锁定schema定义：

- -32700 ParseError；
- -32600 InvalidRequest；
- -32601 MethodNotFound；
- -32602 InvalidParams；
- -32603 InternalError；
- -32800 RequestCancelled（unstable）；
- -32000 AuthRequired；
- -32002 ResourceNotFound；
- 其它numeric code。

这比当前`ConnectorError::Runtime(String)`更适合adapter映射，但仍没有retryable、terminal scope或safe/redacted details；这些由Host的typed RuntimeError补充。

## 五、Ordering、Idempotency、Reconnect 与 Recovery

### 5.1 ACP wire与SDK能保证什么

ACP基于JSON-RPC 2.0，通常为newline-delimited JSON over stdio，但SDK允许任何AsyncRead/AsyncWrite双向stream。单一byte stream本身保留frame读取顺序，request/response由JSON-RPC id关联。

锁定Rust SDK实现事实：

- 每个connection的request id从0开始递增：`agent-client-protocol-0.10.2/src/rpc.rs:33-37,118-124`；
- outgoing/incoming queue为unbounded；没有业务backpressure：同文件`:55-64`；
- EOF结束IO future并清空pending responses，等待者得到`server shut down unexpectedly`：`:67-79,180-183`；
- SDK没有reconnect、cursor、ack journal或request replay；
- write error被`.await.ok()`忽略：`:168-177`；
- incoming request和notification被分别spawn执行，handler完成顺序不由wire顺序保证：`:260-289`；
- `subscribe()`是debug/monitor stream，容量64、`try_broadcast(...).ok()`，lag会报错；不能作为authoritative event source：`stream_broadcast.rs:65-75,100-163,171-190`。

因此“wire line有序”不能直接提升为“业务lifecycle有序”。目标ACP adapter必须让`session/update`先进入单消费者ordered ingest queue，再调用业务handler；不能依赖SDK debug broadcast，也不能让每个notification并发写journal。

### 5.2 Idempotency

ACP没有durable command id/idempotency key。JSON-RPC request id只在当前connection相关；断线重连后数字重置。unstable messageId只要求UUID和SHOULD echo，不规定重复messageId必须去重。

Host可以低成本提供两种L1策略：

1. **安全的at-most-once unknown策略**：Host先durable写command receipt，再发送prompt；若发送/response状态在crash时未知，不重发，直接把turn收敛为Lost。这样不会双执行，但牺牲未知命令恢复。
2. **协同Agent dedupe profile**：在`_meta.agentdash`携带`commandId/runtimeTurnId`，企业Agent持久记录command id并在重复prompt时返回同一结果；这需要具体Agent协同修改，不是ACP原生保证。

不能把普通JSON-RPC重试称为幂等。若Agent不支持dedupe，Host只能选择不重试unknown command。

### 5.3 Reconnect与active turn

ACP没有connection resume、last-seen event sequence、turn/status/read或pending request恢复。connection断开时：

- Host可以为active canonical turn恰好写一个`TurnTerminal::Lost`；
- Host可以保留session binding，并在新connection initialize后尝试`session/load`或`session/resume`；
- Host不能判断失联前prompt在remote是否已完成；
- Host不能从中断点继续同一个active turn；
- Host不能安全重放没有dedupe的prompt。

这不必阻止L2：L2要求conversation在restart后可continuation，不要求active turn透明续跑。正确语义是“当前turn Lost，随后conversation从同一session恢复并开始新turn”。

### 5.4 `load`与`resume`的差别

- `session/load`：Agent应该restore context/history并通过`session/update`重放完整conversation history；LoadResponse是重放完成的request边界。
- `session/resume`：只恢复native context，不返回previous messages；schema明确它适用于能continuation但不保存完整history的Agent。

两者都没有snapshot revision/digest，也没有证明Agent在process restart后仍认识SessionId。L2 eligibility必须用真实restart测试，不只看capability bool。

Read guarantee应明确分级：

```text
HostObservedTranscript  # Host journal中从绑定后观察到的消息/items
AgentReplayTranscript   # session/load重放的display transcript
OpaqueContinuation      # session/resume成功，但无法读取native transcript
ExactModelSnapshot      # ACP不支持
```

L2 common read可以接受HostObservedTranscript或AgentReplayTranscript，但必须明确它们不是exact model-visible context。若产品要求conversation read必须来自Agent replay，则只有`load`通过conformance的Agent能L2；只resume的Agent应降级或使用更窄profile。

## 六、当前 Relay / Local 主链并不是ACP

### 6.1 命名与事实不一致

`ConnectorType::RemoteAcpBackend` 的注释称“远程ACP后端”：`crates/agentdash-spi/src/connector/mod.rs:29-38`；`RelayAgentConnector`返回这个类型：`crates/agentdash-application/src/relay_connector.rs:45-52`。

但实际wire为项目自有`RelayMessage`：

- `command.prompt/cancel/steer/discover`；
- `response.prompt/cancel/steer/discover`；
- `event.session_notification`；
- `event.runtime_session_state_changed`。

证据：`crates/agentdash-relay/src/protocol.rs:65-96,217-267,414-426`。它没有ACP initialize、new/load/resume、prompt response stopReason、permission RPC或session/update DTO。因此`RemoteAcpBackend`是历史误名，不是协议事实。

### 6.2 Session与prompt语义相反

ACP `session/new`由Agent创建并返回SessionId；当前relay `CommandPromptPayload`由cloud直接传`session_id`，另有含糊的`follow_up_session_id`：`crates/agentdash-relay/src/protocol/prompt.rs:19-43`。

local把它重新构造成`LaunchCommand::local_relay_prompt_input(...).with_follow_up(...)`并重跑`SessionRuntimeServices.launch`：`crates/agentdash-local/src/handlers/prompt.rs:260-280`。它没有调用ACP new/load/resume。

`ResponsePromptPayload { turn_id, status:"started" }`只表示local launch成功：`crates/agentdash-relay/src/protocol/prompt.rs:71-80`和`crates/agentdash-local/src/handlers/prompt.rs:299-309`。ACP `session/prompt` response则在turn完成时返回StopReason。两者不能互换。

### 6.3 Event不是ACP SessionNotification

relay `SessionNotificationPayload`是：

```rust
{ session_id: String, notification: serde_json::Value }
```

其中value实际为`BackboneEnvelope`：`crates/agentdash-relay/src/protocol/session_event.rs:3-9`。local把persisted BackboneEnvelope序列化为Value转发，cloud再反序列化：`crates/agentdash-local/src/handlers/prompt.rs:494-518`、`crates/agentdash-api/src/relay/ws_handler.rs:604-640`。

这不是ACP `{ sessionId, update: SessionUpdate }`，注释“ACP会话通知”不成立。

local forwarder来自broadcast；lag时明确跳过事件：`crates/agentdash-local/src/handlers/prompt.rs:520-543`。它不能保持L1 ordered/complete lifecycle。

### 6.4 Terminal不闭环

项目自定义`RuntimeSessionStateChangedPayload`有started/completed/failed/cancelled：`crates/agentdash-relay/src/protocol/session_event.rs:11-28`，cloud可把terminal投递到session sink：`crates/agentdash-api/src/relay/ws_handler.rs:646-684`。

但生产local没有构造`EventRuntimeSessionStateChanged`；全仓除protocol test和cloud consumer外没有producer。current relay normal turn因此没有可靠terminal producer。

disconnect path会向matching session sinks注入Lost，这一点方向正确：`crates/agentdash-api/src/relay/ws_handler.rs:429-447`。但route是内存map，backend unregister会删除pending和session routes：`crates/agentdash-api/src/relay/registry.rs:141-157`，没有reconnect resume/cursor。

cancel同样不是ACP：local cancel response直接返回`status=cancelled`，cloud connector收到后立即release lease为Interrupted并注销sink：`crates/agentdash-local/src/handlers/prompt.rs:334-374`、`crates/agentdash-application/src/relay_connector.rs`的`cancel`实现。它没有等待原prompt返回Cancelled terminal。

### 6.5 当前路径的level判断

当前relay/local链甚至不能以“已经是ACP L1”作为迁移起点：

- 缺typed ACP turn response；
- 缺exactly-one terminal producer；
- authoritative events可丢；
- session ownership只有内存sink；
- 无load/resume/restart continuity；
- custom steer虽有expected turn id，但不是ACP能力；
- `ConnectorCapabilities`仍是aggregate bool。

它应该被目标Runtime Wire/Driver重写，而不是包一层ACP命名继续使用。

## 七、L1 TurnRuntime 严格对照

项目统一taxonomy中的L1最低保证：typed/idempotent turn、ordered lifecycle、exactly-one terminal、typed error。

| L1 guarantee | ACP原生 | Host可保持 | 必须协同Agent / 降级 |
| --- | --- | --- | --- |
| typed start turn | `session/prompt` typed request/response | 生成canonical RuntimeTurnId并绑定当前prompt | ACP无native turn id |
| structured input | Text/ResourceLink baseline，Image/Audio/Resource capability | 映射AgentDash user input和context resources | system/developer authority不能等价映射 |
| idempotent command receipt | 无；JSON-RPC id仅connection-local | durable receipt + duplicate command返回原结果；unknown不重试而Lost | 若要crash后安全重发，Agent用commandId持久dedupe |
| ordered lifecycle | 单byte stream读取有序 | single active prompt；单消费者按arrival写journal | stock Rust SDK handler并发dispatch，adapter需串行化 |
| exactly-one terminal | prompt response有StopReason；RPC error | response/error/EOF/timeout统一收敛一次；late event quarantine | 无法知道断线前remote真实outcome，只能Lost |
| cancel terminal | cancel后prompt应返回Cancelled | Host按active canonical turn gate；等待prompt terminal | cancel notification本身没有ack/turn id |
| typed error | JSON-RPC code/message/data | 映射scope/retryable/terminal并redact | vendor custom code需adapter映射 |
| typed item/tool lifecycle | ToolCallId+status/update | Host生成RuntimeItemId并验证状态 | ACP无item sequence/exact terminal |

**L1结论：ACP不是天然L1，但很容易成为L1 adapter。** Host不需要修改Agent就能采用“unknown不重试、断线Lost”的安全策略；若要求安全重放accepted prompt，则要协同Agent增加持久command dedupe。

## 八、L2 ConversationRuntime 严格对照

L2最低保证：L1 + durable session binding、restart后native resume或platform semantic rehydrate、明确read/continuation语义。

| L2 guarantee | ACP原生 | Host可保持 | 无法仅由Host补齐 |
| --- | --- | --- | --- |
| conversation identity | Agent返回opaque SessionId | 持久`RuntimeSessionId <-> ACP SessionId + service provenance` | Agent是否跨restart保留SessionId不由类型保证 |
| durable binding | 无repository语义 | Host binding repo、generation、placement、lease | native session不存在时mapping没有用 |
| restart continuation | optional load；unstable resume | reconnect后initialize，再按profile调用load/resume | Agent必须真实持久化session/context；Host不能凭空恢复 |
| platform semantic rehydrate | ACP无role-aware history import | Host只能新建session并送user resources | 把历史塞进user prompt不等价；需要Agent扩展import或换driver |
| conversation read | load重放display history；resume不重放 | Host journal提供ObservedTranscript；load replay提供AgentReplayTranscript | exact native/model-visible snapshot不可读 |
| active turn recovery | 无turn status/cursor | 断线把active turn收敛为Lost | 不能继续原turn或查询remote outcome |
| continuation after Lost | load/resume后可new prompt | Host fence旧generation并创建新turn | 取决于Agent restart conformance |
| event replay/dedupe | load replay无event id/sequence | replay mode与live mode分开，Host可按已知message/tool ids去重 | 无稳定ids时不能证明完整/无重复；可协同加meta event id |
| context ownership | ACP session描述为自有context/history/state | descriptor标`DriverOwned` | 不能提升为SharedCheckpointed/PlatformOwned |

**L2结论：只有通过具体行为测试的ACP Agent才是L2。** `load_session=true`或`resume={}`只是候选能力声明，不是durability证据。

建议的L2 predicate：

```text
is_acp_l2 = l1_conformance_passed
          && host_binding_is_durable
          && real_process_restart_keeps_same_acp_session_id
          && (load_replay_passed || resume_continuation_passed)
          && conversation_read_mode_is_explicit
          && disconnect_active_turn_becomes_exactly_one_lost
```

若只resume而产品要求read完整conversation，则该Agent不能获得common `agentdash.conversation.v1`，只能获得更窄的`conversation.opaque-continuation` profile。

## 九、哪些缺口可以轻量补齐

### 9.1 Host单方面即可补齐

1. canonical session/turn/item/approval IDs与source ID mapping；
2. durable session binding和driver generation fencing；
3. single-active-turn admission；
4. per-session authoritative journal sequence；
5. prompt response/error/EOF/timeout到exactly-one platform terminal；
6. command receipt幂等；unknown command不重发、明确Lost；
7. JSON-RPC error到typed RuntimeError；
8. permission RPC到durable ApprovalRequested/ResolveApproval映射；
9. ContentBlock、ToolCall、Plan和session info到canonical items；
10. restart后重新initialize并选择load/resume；
11. HostObservedTranscript read model。

### 9.2 企业Agent/Agent Core可协同轻量补齐

因为本项目不要求把ACP永久冻结为外部标准，企业Agent可支持一个很小的AgentDash ACP behavior profile：

```json
{
  "_meta": {
    "agentdash": {
      "commandId": "...",
      "runtimeTurnId": "...",
      "expectedSessionRevision": 12,
      "eventId": "...",
      "eventSequence": 44
    }
  }
}
```

可协同实现：

- 持久commandId去重，允许Host安全retry；
- update携带稳定event id/sequence，支持load replay dedupe；
- message/tool updates带runtimeTurnId，避免并发或late update误归属；
- process restart后持久SessionId与context；
- load replay完成前先发完整、有稳定ID的history；
- 可选 `_agentdash/session_state` extension query，确认session/revision/active turn状态；
- 可选 `_agentdash/turn_steer`，若未来要进L3；
- 可选typed runtime surface revision ack。

这些是当前adapter行为profile，不需要建立复杂生态认证或长期兼容层；用contract tests锁定当前Host与企业Agent共同实现即可。

### 9.3 不能伪装为L2补丁的能力

以下不属于L2，也不能通过Host猜测得到：

- exact/verified model-visible snapshot；
- system/developer/additional-context authority fidelity；
- platform-managed compaction prepare/activate/checkpoint；
- exact context fork point；
- hot tool schema revision与applied ack；
- active turn跨disconnect透明继续；
- 无Agent持久化时的native restart resume。

若需要这些能力，应扩展项目Runtime Protocol或共同修改Agent driver，使其进入L3/L4；不应继续向ACP `_meta`塞越来越大的隐藏协议后还声称是标准ACP。

## 十、Approval、Steer、Interrupt、Tools 的level归属

### 10.1 Approval

ACP permission是优秀的L3候选：双向RPC、tool call详情、离散选项、cancel联动都已经原生存在。Host需要补canonical approval id、durable state和policy provenance。

但stock ACP pending request是connection-local；断线后没有replay/query。首期可规定disconnect时pending approval=>Cancelled、turn=>Lost，session之后按L2恢复。若要pending approval跨restart恢复，则需要Agent协同扩展，不能自报。

### 10.2 Steer

ACP没有steer。向active session再发一个`session/prompt`不是steer，而且协议没有turn id/ordered insertion invariant。`session/set_mode`只改变mode，也不是steer。

因此ACP driver默认`steer=Unsupported`。企业Agent可以实现 `_agentdash/turn_steer`并通过ordered steer测试后升L3 profile；当前relay custom `command.steer`不能反向证明ACP支持。

### 10.3 Interrupt

`session/cancel`可以适配为interrupt，但条件严格：

- Host强制每session单active prompt；
- expected canonical turn在Host侧校验；
- cancel发送成功只算Requested；
- 原prompt返回Cancelled才算Interrupted；
- connection关闭则Lost；
- pending permission先回应Cancelled。

这能满足“protocol interrupt terminal”，但不能提供turn-scoped native ack。capability descriptor必须写清。

### 10.4 Tools

ACP能很好地展示Agent-owned tool lifecycle，并通过MCP/fs/terminal提供执行设施；它不提供AgentDash assembled tools的热替换合同。建议profile：

```text
tool_ownership = DriverOwned
tool_observation = ToolCallLifecycle
mcp_binding = SessionCreationOrResume
hot_tool_revision = Unsupported
```

## 十一、Context ownership 与 compaction

ACP schema把session定义为“维护自身context、conversation history与state”的conversation，最自然的descriptor是：

```text
context_ownership = DriverOwned
snapshot_fidelity = OpaqueNative
read_fidelity = HostObservedTranscript | AgentReplayTranscript
compaction = DriverNativeOpaque | Unsupported
```

`session/load`重放内容不能提升snapshot fidelity：重放的`SessionUpdate`只覆盖展示消息、tool call、plan和配置，不包含system/developer instructions、native hidden state、model cache、tool schemas、compaction boundary或context digest。

ACP没有compact method/event。若Agent内部自动压缩，Host最多记录native telemetry；不能完成common `CompactContext`，不能推进AgentDash context head，也不能称L4。

## 十二、目标 ACP adapter module

### 12.1 Seam placement

ACP是executor/integration driver host内部的protocol adapter，不是application port：

```text
agentdash-agent-runtime-contract
  <- application/AgentRun
  <- executor/integration host
       <- AcpConversationAdapter
            <- agent-client-protocol SDK / thin RPC peer
```

删除ACP adapter后，ACP initialization、session binding、content/tool/permission转换、terminal/error/reconnect复杂度会重新散落到Host；它是有depth的module。application只看到canonical RuntimeDriver interface，获得leverage与locality。

### 12.2 Adapter外部interface

ACP adapter实现项目统一driver interface，而不是把ACP trait向上re-export：

```rust
struct AcpConversationAdapter<P, B, J> {
    peer_factory: P,
    binding_store: B,
    journal: J,
}

impl AgentRuntimeDriver for AcpConversationAdapter {
    async fn negotiate(&self, host: RuntimeHostHello) -> Result<RuntimeDescriptor>;
    async fn bind(&self, request: BindRuntimeSession) -> Result<BindingAcceptance>;
    async fn execute(
        &self,
        command: RuntimeCommandEnvelope,
        sink: RuntimeEventSink,
    ) -> Result<CommandAcceptance>;
    async fn inspect(&self, query: RuntimeInspectionQuery) -> Result<RuntimeInspection>;
}
```

ACP-specific types只存在于implementation。内部peer seam可保持很小：initialize/new/load/resume/prompt/cancel + incoming Client callbacks；fork/config/model等作为可选ACP operation，不进入common L2 interface。

### 12.3 ACP profile descriptor

```rust
struct AcpConversationProfile {
    protocol_version: u16,
    continuation: AcpContinuation,
    read: ConversationReadGuarantee,
    command_delivery: CommandDeliveryGuarantee,
    update_ordering: UpdateOrderingGuarantee,
    prompt_content: PromptContentCapabilities,
    permission: PermissionGuarantee,
    interrupt: InterruptGuarantee,
    tools: DriverOwnedToolGuarantee,
    context_owner: ContextOwnership, // fixed DriverOwned for standard ACP
}

enum AcpContinuation {
    None,
    LoadReplay,
    ResumeOpaque,
    LoadAndResume,
}
```

Host根据behavior tests导出L1/L2；不直接把`agent_capabilities.load_session=true`映射为L2。

## 十三、调用顺序

### 13.1 Activation

```text
spawn/connect ACP Agent
  -> initialize(protocolVersion, clientCapabilities, clientInfo)
  -> validate selected version
  -> map agentCapabilities/authMethods
  -> authenticate when explicitly configured
  -> produce candidate ACP profile
```

### 13.2 Open session

```text
Host reserve Pending RuntimeSessionBinding
  -> session/new(cwd, mcpServers)
  -> receive ACP SessionId
  -> atomic persist source binding + SessionBound
  -> Active
```

ACP SessionId由Agent生成；不能把platform session id直接塞给Agent伪装new。

### 13.3 Resume session

```text
new process/connection
  -> initialize/authenticate
  -> fence old driver generation
  -> load(sessionId, cwd, mcp)  # replay mode
       or resume(sessionId, cwd, mcp) # opaque mode
  -> ordered ingest replay updates
  -> response marks replay/resume complete
  -> binding generation Active
```

### 13.4 Turn

```text
Host durable accept RuntimeTurnId/CommandId
  -> ensure one active turn
  -> session/prompt(sessionId, ContentBlocks, agentdash meta)
  -> ordered session/update -> canonical journal items
  -> permission request <-> durable approval response as needed
  -> PromptResponse | RPC Error | EOF/timeout
  -> exactly-one canonical TurnTerminal
  -> command terminal
```

### 13.5 Disconnect

```text
connection closes
  -> fence generation
  -> active turn => Lost exactly once
  -> pending approval => Cancelled
  -> session binding => Suspended if L2, Lost if L1-only
  -> no prompt retry unless persistent Agent dedupe profile
```

## 十四、轻量 conformance cases

这里的conformance用于验证当前Host/Agent行为，不建立复杂认证生态。

### 14.1 L1 cases

1. initialize版本不匹配时明确拒绝，不继续new session。
2. new session返回source SessionId，Host原子持久binding。
3. 一个StartTurn只发送一个prompt；重复canonical command返回同一receipt，不产生第二个prompt。
4. content Text/ResourceLink baseline无损；capability允许时Image/Audio/Resource无损。
5. update arrival order写入journal；并发SDK callback不得重排同session updates。
6. prompt正常response映射一次terminal；duplicate/late response不产生第二terminal。
7. JSON-RPC error映射Failed；EOF和timeout映射Lost；都恰好一次。
8. cancel只先返回Requested，最终等待PromptResponse Cancelled；pending permission先响应Cancelled。
9. tool call pending→in_progress→completed/failed正确；terminal后tool late update被拒或quarantine。
10. unknown command state在Host crash恢复后不重发，收敛Lost。

### 14.2 L2 cases

1. 创建session并完成turn，真实终止Agent process。
2. 创建新process/connection，initialize后以相同ACP SessionId调用load或resume。
3. 新turn能引用restart前conversation事实，证明native contextcontinuation，不只方法返回成功。
4. `load`模式在response前重放完整history；Host区分replay/live，重放不会创建新turn terminal。
5. Host restart后从binding repository恢复platform↔ACP SessionId/service provenance/generation。
6. active prompt期间断线只让该turn Lost；session成功load/resume后可以开始新turn。
7. source SessionId不存在时binding进入Lost/Failed，不偷偷session/new。
8. read mode明确：HostObserved、AgentReplay或Opaque；ExactSnapshot query必须Unsupported。
9. 两个session updates不串线；所有event带canonical session mapping。
10. 若声明persistent command dedupe，断线后重复commandId不会重复模型/tool side effect。

### 14.3 L3候选cases

- permission request产生durable approval item，selected option正确回到原RPC；
- cancel期间所有pending approvals收到Cancelled；
- optional steer extension按expected RuntimeTurnId有序applied；
- interrupt只有prompt Cancelled response后才terminal；
- tools update若无applied revision则capability必须Unsupported。

## 十五、Relay placement 建议

推荐ACP在local driver host终止，再把normalized Runtime Contract经Relay传cloud：

```text
local ACP Agent process
  <-> ACP adapter/client（local filesystem/terminal/credentials）
  <-> canonical Runtime Driver events
  <-> Relay placement transport
  <-> cloud Runtime Host
```

理由：

- ACP client反向提供filesystem/terminal，天然靠近local workspace；
- Host可以在local adapter处做ordered ingest与credential mediation；
- Relay只需保持canonical command/event sequence，不需要理解ACP optional/unstable methods；
- cloud持久化真实Agent service provenance + ACP source session binding + remote placement，不生成“relay ACP executor”。

不推荐把原始ACP NDJSON盲隧道到cloud再把client fs/terminal请求折返local。那会放大connection-scoped ID、permission和disconnect问题，也会让Relay与Agent protocol ownership重新耦合。

最终bound level仍为：

```text
ACP service behavior guarantees
  ∩ Relay placement transport guarantees
  ∩ Host policy
```

Relay不能把L1 Agent提升为L2；缺少ordered replay/ack的Relay也可以把local L2 service的remote placement降级。

## 十六、迁移建议

1. 将`RemoteAcpBackend`重命名/删除；当前Relay不是ACP，connector type不应宣称协议。
2. 不再从`agentdash-agent-protocol`公开re-export ACP types；canonical runtime content由项目自有contract拥有，ACP adapter内部转换。
3. 若首期接企业ACP Agent，新建独立 `AcpConversationAdapter`，不要复用当前`RelayAgentConnector::prompt`。
4. 先只承诺L1：new/prompt/update/cancel、Host terminal收敛、structured content、tool observation。
5. 对具体企业Agent运行restart load/resume测试，通过后再把该service descriptor提升L2。
6. Host持久化binding和journal；不要求ACP Agent承担AgentRun/product repository。
7. ACP在local终止，Relay改承载canonical Runtime Wire；删除薄prompt DTO和Value Backbone转发。
8. approval/steer/tools/compaction按独立profile开放；不因ACP有permission/tool DTO就整体宣称L3/L4。

## 十七、最终建议

### 明确回答

**选择“ACP是L2候选transport/profile”，不选择“ACP=L2”。**

更精确的措辞是：ACP是一个有conversation词汇的Agent-facing protocol adapter；`AcpConversationAdapter`在Host补齐canonical IDs、binding、journal和terminal后，可以提供L1。只有具体Agent证明session在真实restart后可load/resume、并明确read fidelity，它才提供L2。

### 为什么这比直接采用ACP seam更干净

- application不学习ACP unstable methods、`_meta`或connection failure；interface更深。
- AgentDash Host拥有durable binding与canonical state；ACP Agent只拥有native conversation state，ownership清楚。
- Relay保持placement transport locality，不再冒充Agent协议。
- 企业Agent Core可以与adapter协同增加少量dedupe/event-id行为，不需要维护永久兼容标准。
- 将来若ACP演进或被另一协议替换，只改adapter和conformance tests，不改AgentRun。

### 绝对不能过度声明的点

即使ACP Agent通过L2，也默认仍是：

```text
context_owner = DriverOwned
read <= AgentReplayTranscript
snapshot = OpaqueNative
compaction = Unsupported | DriverNativeOpaque
steer = Unsupported（除非扩展）
hot_tools_revision = Unsupported
active_turn_recovery = LostThenContinueConversation
```

这些限制并不削弱ACP作为L2候选的价值；它们恰好让ConversationRuntime与ManagedContextRuntime之间的seam保持诚实。

## Caveats

- 本调研以仓库实际锁定的Rust 0.10.2/schema 0.11.2和已安装TS 0.14.1源码为准，没有假设ACP官网未来版本行为。
- `session/load`“重放完整conversation history”是SDK/schema interface注释要求；是否真正跨process持久化只能由具体Agent conformance验证。
- references下的AgentDispatch/Actant ACP实现不参与当前生产构建，只用来验证常见adapter形状，没有把其行为当作AgentDash保证。
- 本文没有设计长期协议认证、兼容矩阵或动态plugin体系；conformance仅是本项目当前Host/Agent实现的行为测试。
