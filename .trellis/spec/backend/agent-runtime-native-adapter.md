# Native Agent Runtime Adapter and Clean Agent Core

## 1. Scope / Trigger

本规范适用于 first-party Native Agent service contribution、Native `AgentRuntimeDriver`、Agent Core依赖边界，以及Managed Runtime Surface/Context/Tool/Hook能力到本地Agent loop的映射。修改Native descriptor、bind/dispatch/inspect、exact context/compaction、Core delegate或旧Pi切换时必须复核本规范。

## 2. Signatures

```rust
pub fn native_agent_contribution(
    resolver: Arc<dyn NativeBridgeResolver>,
) -> AgentRuntimeDriverContribution;

pub struct NativeAgentRuntimeIntegration { /* explicit resolver */ }

impl AgentRuntimeDriver for NativeAgentDriver {
    async fn describe(&self) -> Result<RuntimeDescriptor, DriverError>;
    async fn bind(&self, request: DriverBindRequest)
        -> Result<DriverBinding, DriverError>;
    async fn dispatch(
        &self,
        request: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError>;
    async fn inspect(&self, request: DriverInspectRequest)
        -> Result<DriverInspectResult, DriverError>;
}

DriverCommandEnvelope {
    request_id,
    binding_id,
    generation,
    source_thread_id,
    runtime_turn_id: Option<RuntimeTurnId>,
    command,
}
```

Factory从WP04 Host获得`ActivatedInstance + RuntimeDriverHostPorts`，resolver只解析真实Native bridge；生产composition显式构造Integration，不使用全局静态connector。

## 3. Contracts

- Native service通过与Codex/企业service相同的Integration contribution/factory进入Host。Application/router不按Pi或Native类型分支，service descriptor与conformance是能力事实源。
- Native service instance使用schema-validated `provider`、`model`与显式`credential_scope`。`credential_scope`只能是平台凭据或带非空`user_id`的账户凭据；缺失scope不得解释为平台回退。instance只保存凭据查找坐标，API key/OAuth token仍由repository/secret codec在driver激活时短暂解析。
- Bind intent显式区分Start、Resume与Fork。Resume必须保留source thread；Fork必须导入请求指定的checkpoint并验证checkpoint ID/context digest，不能选择任意最新context。
- Native descriptor只声明实际原生支持的输入与能力。当前Text/Image可进入本地Core；FileReference/Structured不得文本拍平冒充支持，必须在request lock、status event、prompt或任何side effect前typed Unsupported。
- Surface materialization返回真实surface/tool-set/hook plan revision与digest。ToolSetReplace receipt必须携带`DriverToolSetApplyReceipt`；其他命令为None。Host只依据ack开放required dispatch gate。
- Platform tools通过WP03 Direct Callback Broker；Native driver不接收`DynAgentTool`、application delegate、credential或VFS runtime object。Approval使用canonical Interaction。
- Native tool callback通过Platform Tool Broker执行并提交canonical internal lifecycle；Native Agent Core vendor stream按binding的effective `VendorStream` route发布唯一session-visible ItemStarted/update/terminal。Broker不为同一调用再发布presentation。只有effective route明确为`ToolBroker`时，Native mapper抑制vendor presentation并由Broker投影展示。
- Native从durable Runtime journal的presentation与internal facts共同恢复provider transcript，覆盖user、assistant、paired tool-call/result、shell/fs/MCP/native typed item与compaction tail。session-scoped identity allocator从durable presentation ID恢复水位，使rebind后的tool/command/readable ref继续单调递增。
- AgentCore callback facets只表达真实inner-loop Hook点，业务Hook plan/rule仍由Runtime拥有。Native driver不得查询workflow/project/repository。
- Context read/Thread projection使用typed inspect。Managed compaction只接受Runtime已durable candidate activation，验证activation/checkpoint/digest后幂等应用；Native Core不拥有AgentDash自动压缩策略或checkpoint事实源。
- Turn、steer、interrupt、settings与tool replace按binding/request维度幂等。Active-turn fence在成功、mapper error、sink error、Agent task error与cancel所有路径都必须finally清理；失败turn不能继续被steer/interrupt命中。
- `ThreadStart`/`TurnStart` 的 canonical Turn identity 由 Managed Runtime 根据 accepted operation分配，并通过`DriverCommandEnvelope.runtime_turn_id`交给Driver。Native只生成`DriverTurnId`作为source coordinate；普通事件、Tool callback与Hook callback都必须同时保留这两套坐标，不能把source ID转换成Runtime ID。
- `TurnStart` acceptance已经把canonical `TurnStarted`写入Runtime projection；Driver回报相同`runtime_turn_id`的`TurnStarted`是同身份ack，不新增第二条lifecycle transition。不同identity仍属于critical protocol violation。
- Driver一旦已经发送`TurnTerminal`，底层Agent task返回同一失败只能形成成功的dispatch completion；否则durable outbox会把已终态命令当成“acceptance前拒绝”重派。只有尚未产生authoritative terminal的Rejected/Lost才向dispatch caller返回错误。
- Driver使用`Arc<dyn DriverEventSink>`，streaming和terminal可以异步送达；authoritative sink failure必须向上返回，不能静默丢事件后报告成功。
- Provider retry/status只映射为完整ephemeral `PlatformEvent::ProviderAttemptStatus` presentation；Native mapper不生成第二份internal transient summary。该状态不推进durable Runtime revision或cursor。
- event sink返回`DriverError::Terminalized`表示Managed Runtime已原子提交critical terminal；Native accepted-turn pump必须立即停止并清理active-turn fence，不再发送后续terminal或fallback `BindingLost`。其它sink error继续按其原有失败语义传播。
- Clean Agent Core只拥有provider-neutral inference/stream/tool loop。它不依赖Application、Domain、Codex/Backbone/vendor DTO、AgentDash lifecycle prompt、runtime compaction policy或repository。
- Provider-specific DTO放在protocol/adapter；`ThinkingLevel`是provider-neutral Core type。Core不公开RuntimeCompactionDelegate，也不执行pre-provider/compact-only/manual AgentDash policy。
- API旧Pi生产构造入口在Native阶段删除。Provider registry从legacy Pi源码抽离、Pi物理删除与runtime-session dead compaction SPI删除随WP08唯一cutover完成，不保留双轨或fallback。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| Start/Resume/Fork缺少或错用source coordinate | typed bind error，无session side effect |
| user credential scope缺失或user_id为空 | typed configuration error，不尝试平台全局凭据 |
| Fork broker返回非请求checkpoint/digest | reject，不激活context |
| FileReference/Structured输入 | side effect前Unsupported |
| surface/tool/hook applied digest不匹配 | Host gate保持未应用/失败 |
| duplicate ToolSetReplace | 返回相同revision/digest receipt，不重复替换 |
| compaction activation重复 | exact idempotent receipt |
| compaction activation digest不匹配 | reject，不改变live context |
| mapper/sink/Agent task失败 | error传播且active-turn fence清理 |
| Provider retry/status | 仅一份ephemeral presentation；无internal fact、durable revision或binding loss |
| sink返回`Terminalized` | accepted-turn pump停止并清理fence，不追加`BindingLost` |
| Turn命令缺少`runtime_turn_id` | side effect前critical protocol error |
| Tool/Hook callback把source turn作为Runtime turn | Runtime transition拒绝；不得写第二套坐标 |
| Native与Broker分别投影同一tool started payload | contract violation；只允许binding effective presentation route选中的owner提交 |
| effective route为`VendorStream`的Native platform tool | Broker只提交internal lifecycle；Native vendor start/update/terminal使用同一presentation ID并继续provider loop |
| cold rebind存在历史tool pair与presentation ID | provider transcript保留配对调用/结果；下一ID高于durable水位 |
| canonical Turn已accepted后Driver回报同identity `TurnStarted` | `Observed` ack；revision/cursor不推进 |
| 已发送`TurnTerminal`后Agent task返回错误 | dispatch成功收口并ack outbox，不重派命令 |
| 失败后steer/interrupt旧turn | Rejected |
| stale binding/generation | fence，不发送Core command/event |
| Core依赖domain/vendor/application | dependency/spec gate失败 |

## 5. Good / Base / Bad Cases

**Good case:** Host用Native contribution激活service，Fork bind从Context Broker取得指定checkpoint并验证digest，surface/tool/hook ack后以Managed分配的Turn ID启动；Direct Callback工具经Broker执行，Runtime/source坐标同时保留，流式事件通过Arc sink持续进入Runtime，终态清理active fence并ack outbox。

**Base case:** 相同request重放返回原binding/receipt，ToolSet revision和compaction activation不会重复产生副作用。

**Bad case:** Adapter把Structured序列化成普通文本却在profile声明Structured Native，或把`DriverTurnId`直接作为`RuntimeTurnId`发给Tool/Hook callback。这会产生虚假能力或第二Turn identity，必须拒绝。

## 6. Tests Required

- Native behavior覆盖contribution/factory、truthful descriptor、Start/Resume/Fork、exact checkpoint/digest、Turn/steer/interrupt/settings/idempotency。
- 覆盖surface/tool/hook applied receipts、hot ToolSetReplace、Direct Callback、approval Interaction与typed inspect。
- Direct Callback测试必须让source/runtime/presentation item ID不同，并覆盖ApplyPatch、shell control及重复调用经Broker执行且不发生idempotency conflict；`VendorStream`组合场景断言Broker internal与Native presentation各自恰好一次。
- 覆盖managed compaction exact activation、wrong digest/checkpoint、duplicate replay和digest选择不依赖map ordering。
- 覆盖unsupported modality在任何副作用前拒绝，以及mapper/sink/task error的active fence清理。
- 覆盖Provider retry/status只有ephemeral presentation，以及`Terminalized`在至少一次成功emit后停止pump、零fallback `BindingLost`。
- Runtime interface覆盖matching Driver `TurnStarted`只得到`Observed`且revision不变；Native工具轮次覆盖Tool/Hook使用canonical Turn、terminal后task error不触发同request重派。
- recovery测试从真实durable journal重建完整user/assistant/tool-call/tool-result transcript，并覆盖compaction边界后的tail replay、typed shell/fs/MCP item与readable ID水位。
- Contract/Wire/TestSupport/Host conformance与generated TS/schema check必须通过。
- Agent Core dependency tree与source scan必须证明无Application/Domain/Codex/Backbone/repository依赖；Core/Native strict clippy与tests通过。
- WP08必须验证provider registry抽离后legacy Pi与dead runtime-session compaction SPI物理删除、生产Host composition使用Native Integration。

## 7. Wrong vs Correct

```rust
// Wrong: profile声称Structured，但adapter只是format成文本。
RuntimeInput::Structured { value, .. } => ContentPart::text(value.to_string())

// Correct: 未实现保持语义的ingress时，在任何副作用前typed拒绝。
RuntimeInput::Structured { .. } => return Err(DriverError::Unsupported(...))
```

```rust
// Wrong: `?`提前返回留下active turn。
self.active_turn.insert(turn_id.clone());
run_agent(...).await?;
self.active_turn.remove(&turn_id);

// Correct: 所有成功/失败路径统一清理fence，再返回原结果。
let result = run_agent(...).await;
self.active_turn.remove(&turn_id);
result
```

```rust
// Wrong: source coordinate成为第二套Runtime identity。
let turn_id = RuntimeTurnId::new(source_turn_id.to_string())?;

// Correct: Runtime identity由accepted operation分配，source coordinate只用于Driver映射。
let turn_id = envelope.runtime_turn_id.clone().ok_or(DriverError::ProtocolViolation { .. })?;
tool_callback.invoke(DriverToolInvocation {
    turn_id,
    source_turn_id,
    ..
}).await?;
```
