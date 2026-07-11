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
```

Factory从WP04 Host获得`ActivatedInstance + RuntimeDriverHostPorts`，resolver只解析真实Native bridge；生产composition显式构造Integration，不使用全局静态connector。

## 3. Contracts

- Native service通过与Codex/企业service相同的Integration contribution/factory进入Host。Application/router不按Pi或Native类型分支，service descriptor与conformance是能力事实源。
- Native service instance使用schema-validated `provider`、`model`与显式`credential_scope`。`credential_scope`只能是平台凭据或带非空`user_id`的账户凭据；缺失scope不得解释为平台回退。instance只保存凭据查找坐标，API key/OAuth token仍由repository/secret codec在driver激活时短暂解析。
- Bind intent显式区分Start、Resume与Fork。Resume必须保留source thread；Fork必须导入请求指定的checkpoint并验证checkpoint ID/context digest，不能选择任意最新context。
- Native descriptor只声明实际原生支持的输入与能力。当前Text/Image可进入本地Core；FileReference/Structured不得文本拍平冒充支持，必须在request lock、status event、prompt或任何side effect前typed Unsupported。
- Surface materialization返回真实surface/tool-set/hook plan revision与digest。ToolSetReplace receipt必须携带`DriverToolSetApplyReceipt`；其他命令为None。Host只依据ack开放required dispatch gate。
- Platform tools通过WP03 Direct Callback Broker；Native driver不接收`DynAgentTool`、application delegate、credential或VFS runtime object。Approval使用canonical Interaction。
- AgentCore callback facets只表达真实inner-loop Hook点，业务Hook plan/rule仍由Runtime拥有。Native driver不得查询workflow/project/repository。
- Context read/Thread projection使用typed inspect。Managed compaction只接受Runtime已durable candidate activation，验证activation/checkpoint/digest后幂等应用；Native Core不拥有AgentDash自动压缩策略或checkpoint事实源。
- Turn、steer、interrupt、settings与tool replace按binding/request维度幂等。Active-turn fence在成功、mapper error、sink error、Agent task error与cancel所有路径都必须finally清理；失败turn不能继续被steer/interrupt命中。
- Driver使用`Arc<dyn DriverEventSink>`，streaming和terminal可以异步送达；authoritative sink failure必须向上返回，不能静默丢事件后报告成功。
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
| 失败后steer/interrupt旧turn | Rejected |
| stale binding/generation | fence，不发送Core command/event |
| Core依赖domain/vendor/application | dependency/spec gate失败 |

## 5. Good / Base / Bad Cases

**Good case:** Host用Native contribution激活service，Fork bind从Context Broker取得指定checkpoint并验证digest，surface/tool/hook ack后启动Turn；Direct Callback工具经Broker执行，流式事件通过Arc sink持续进入Runtime，终态清理active fence。

**Base case:** 相同request重放返回原binding/receipt，ToolSet revision和compaction activation不会重复产生副作用。

**Bad case:** Adapter把Structured序列化成普通文本却在profile声明Structured Native，或Core自行根据token阈值压缩context。这会产生虚假能力和双context authority，必须拒绝。

## 6. Tests Required

- Native behavior覆盖contribution/factory、truthful descriptor、Start/Resume/Fork、exact checkpoint/digest、Turn/steer/interrupt/settings/idempotency。
- 覆盖surface/tool/hook applied receipts、hot ToolSetReplace、Direct Callback、approval Interaction与typed inspect。
- 覆盖managed compaction exact activation、wrong digest/checkpoint、duplicate replay和digest选择不依赖map ordering。
- 覆盖unsupported modality在任何副作用前拒绝，以及mapper/sink/task error的active fence清理。
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
