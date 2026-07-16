# Codex App Server Runtime Adapter

## 1. Scope / Trigger

本规范适用于 Codex App Server first-party Integration、JSON-RPC/process adapter、thread/turn/item/interaction映射、dynamic tools、opaque context/compaction与native Hook artifact bridge。修改Codex protocol version、Runtime profile、process pump、mapping、artifact或Interaction处理时必须复核本规范。

## 2. Signatures

```rust
pub struct CodexRuntimeIntegration;

pub fn codex_runtime_contribution() -> AgentRuntimeDriverContribution;

impl AgentRuntimeDriver for CodexRuntimeDriver {
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

Codex Rust protocol、npm package与Integration protocol revision必须使用同一个已审计版本；当前基线为`0.144.1 / revision 144`。Adapter所有vendor DTO、进程与artifact细节封装在`agentdash-integration-codex`或workspace protocol codegen工具；其它production crate只消费generated AgentDash-owned类型。

## 3. Contracts

- Codex通过Integration contribution/factory进入Driver Host；Application/Executor不硬编码构造Codex connector。旧`codex_bridge`不能与新adapter并存。
- JSON-RPC frame可先保留`method + params` transport形态，但admission必须按method把params依次反序列化为vendor typed params与generated owned params。未知method返回typed `UnsupportedMethod`；刻意忽略的hook notification也必须显式admit为typed no-op。
- `ThreadItem`先经vendor typed deserialize，再strict transcode为generated owned item。当前Runtime尚未承载的标准family返回typed `UnsupportedItemFamily`；invalid JSON返回`InvalidItemPayload`，禁止转换为AgentMessage文本。
- 每个Runtime binding拥有独立`Arc<Mutex<CodexSession>>`与持久stdout pump；service instance可并发承载多个binding，不能用全局session锁串行化全部线程。
- Bind intent映射为`thread/start|resume|fork`；dispatch映射turn start/steer/interrupt与Interaction response；inspect映射thread/read。Source thread/turn/item/request coordinates与generation必须完整保留。
- RPC accepted不等于canonical terminal。`turn/interrupt`成功只表示请求被接受，最终Interrupted/Failed/Completed由notification映射。EOF使active turn与pending interactions exactly-once进入Lost。
- Item terminal按Codex item status映射Completed/Failed/Cancelled并保留error，不得无条件报告Completed。
- Runtime以AgentDash-owned、与Codex app-server同构的`UserInputBlock`作为标准用户输入事实；Codex adapter直接序列化完整标准块，不重建第二种text/image/file DTO。`Text.text_elements`、`Image/LocalImage.detail`的缺省与显式`null`、`Skill`和`Mention`的name/path必须保持原始wire语义。`Structured`进入typed additional context；system/developer/additional-context instruction分通道映射。禁止把结构化或多模态内容拍平成prompt。
- Workspace roots合并service config与platform materialized surface，必须为合法绝对路径。声明Workspace Write时thread config必须显式使用对应sandbox保证。
- Start/Resume/Fork都必须携带model/provider/cwd/base+developer instructions/workspace roots/hook config/approval/sandbox。若vendor某方法不支持dynamic tools，surface tools非空时typed Unsupported，不能虚报applied revision。
- Dynamic tools使用`dynamicTools`和`item/tool/call`进入WP03 Host Tool callback，保留binding/generation/thread/turn/item/tool-set revision与image output。未实现tool cancellation时profile不得声明。
- effective route为`VendorStream`时，Codex标准dynamic tool notification是唯一session presentation producer，Broker只提交internal canonical lifecycle；route为`ToolBroker`时mapper抑制对应vendor presentation。两条路径都复用同一callback执行合同与presentation item identity。
- 平台`SurfaceAdopt`与ContextFrame mutation在active tool turn中可以先canonical接受。Codex driver的full surface同步等待该turn terminal后再执行thread resume/rebind；等待不占用session锁，使当前tool result能够回灌并继续final assistant。
- terminal notification只有在presentation sink提交成功后才清理active turn与推进本地terminal fence。terminal sink失败先提交binding-scoped `BindingLost`；若Lost也失败则保留active坐标，允许同terminal重试。
- sink返回`DriverError::Terminalized`表示Managed Runtime已经提交canonical critical terminal；stdout pump必须清理active turns、pending interactions与RPC waiters并停止，不再进入普通terminal sink failure分支或补`BindingLost`。非terminalized sink error仍保留既有retry/坐标语义。
- Approval、user input、MCP elicitation与dynamic-tool interaction都形成durable canonical Interaction。Identity包含稳定source坐标与JSON-RPC request coordinate：同request replay稳定，不同request不碰撞。只有response成功写回Codex后才移除pending并发Resolved。
- Native compaction真实强度为Observed/Opaque：`thread/compacted`只产生opaque observation；ContextCompact不能冒充managed activation；context inspect为Opaque，thread/read为EventProjected。
- Native Hook使用Adapter隔离HTTP callback bridge与digest-addressed immutable plugin artifact。Artifact digest覆盖plugin manifest、hooks manifest、bridge、schema和adapter revision，但不包含ephemeral endpoint token或worktree路径。
- Artifact路径安全、原子并发materialization并校验内容；不使用`bypass_hook_trust`，不覆盖用户项目`.codex/hooks.json`。当前update boundary按Binding/ThreadStart，不虚报hot replace。
- Hook callback通过binding-scoped bounded decision cache（优先hook_run_id）保证重复/并发replay只执行一次canonical callback。`hook/started/completed`仅reconcile，不成为decision事实源。
- Hook profile只声明实际映射的points/actions/strength/failure policy。没有durable approval decision channel时不得声明RequestApproval；未映射Usage/Diagnostics telemetry不得出现在profile。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| Rust/npm/service protocol version不一致 | build/contribution validation失败 |
| notification/request method未知或params不满足0.144.1 typed shape | typed protocol mismatch，不静默忽略 |
| ThreadItem有效但当前Runtime family尚未承载 | `UnsupportedItemFamily`，不文本化 |
| binding A/B并发dispatch | 独立session锁，不互相串行或串事件 |
| Resume/Fork surface含vendor不支持的dynamic tools | typed Unsupported，不虚报ack |
| Text/Image/LocalImage/Skill/Mention/Structured input | 标准UserInput字段逐项保真；Structured独立进入typed additional context，无文本拍平 |
| item status failed/cancelled | canonical Failed/Cancelled并保留message |
| interrupt RPC成功但未收到terminal | 仍active，等待真实notification |
| 不同MCP elicitation request坐标 | 不同Interaction ID |
| Interaction response写回失败 | pending保留，不发Resolved |
| transport EOF | active turn和全部pending Interaction exactly-once Lost |
| active dynamic tool触发SurfaceAdopt | ContextFrame exactly-once先落journal；tool完成并继续final assistant后Codex full adopt生效 |
| terminal presentation sink失败 | 不提前遗忘active operation；BindingLost收敛或保留duplicate terminal重试能力 |
| sink返回`Terminalized` | 清理turn/interaction/waiter并停止pump；不补第二份`BindingLost` |
| VendorStream dynamic tool | Codex presentation start/completed各一次，Broker internal lifecycle各一次，不生成第二张card |
| native compact notification | Opaque observation，不推进managed head |
| duplicate/concurrent hook callback | 返回相同decision，canonical callback执行一次 |
| artifact内容被替换或digest不符 | materialization验证失败 |

## 5. Good / Base / Bad Cases

**Good case:** ThreadStart binding以完整surface启动Codex，structured context和dynamic tools保持typed；stdout pump持续映射items/turns/interactions，tool call进入Broker；native Hook bridge按artifact digest调用canonical Hook一次并把decision翻译回Codex。

**Base case:** 相同JSON-RPC request或Hook callback重放返回缓存receipt/decision，不重复副作用；不同request即便method相同也拥有不同Interaction。

**Bad case:** Adapter把`thread/compacted`当成PlatformExact checkpoint，或把RPC interrupt响应当turn terminal。这会制造虚假context和lifecycle事实，必须由fidelity与notification状态机阻止。

## 6. Tests Required

- Contribution/version/profile测试覆盖真实0.144.1方法与未支持能力不声明。
- 多binding process/session测试覆盖锁隔离、persistent stdout pump、Arc sink、EOF Lost与request idempotency。
- Mapping覆盖start/resume/fork、turn/item全部terminal、source coordinates、typed inspect与error message。
- Input/context测试覆盖`text_elements`、image/local-image detail的absent/null/enum、Skill/Mention、Structured、instruction channels、workspace roots、sandbox与Resume/Fork完整参数。
- Dynamic tool测试覆盖Broker coordinates、image output、denied/completed/interaction-required和unsupported cancellation。
- production tracer覆盖dynamic tool -> active SurfaceAdopt/ContextFrame -> 同ID tool terminal -> final assistant -> idle full rebind，并断言全程single presentation producer。
- terminal sink failure测试覆盖BindingLost成功与失败两种路径，验证已提交terminal不会重复、未提交terminal仍可重试。
- terminalized测试覆盖notification、dynamic tool与EOF清理入口，断言sink只收到原事件、pump停止、waiter收到typed terminalized且没有fallback `BindingLost`。
- Interaction测试覆盖每类server request、request-coordinate identity、replay、response failure与EOF Lost。
- Hook测试覆盖artifact完整digest、path、concurrent materialization、trust、decision映射、duplicate callback single execution和reconcile。
- Codex/first-party/Contract/Host/TestSupport tests、跨包cargo check、strict clippy、contracts generation、fmt与diff check必须通过。
- WP08 production E2E必须通过真实Host activation运行Codex并证明旧connector已删除。

## 7. Wrong vs Correct

```rust
// Wrong: interrupt请求返回success就宣称Turn已终止。
rpc("turn/interrupt").await?;
sink.emit(RuntimeEvent::TurnTerminal { terminal: Interrupted }).await?;

// Correct: receipt只确认command，terminal来自Codex notification。
rpc("turn/interrupt").await?;
return Ok(DriverDispatchReceipt::accepted());
```

```rust
// Wrong: native hook callback每次都重复执行平台rule/effect。
let decision = callback.evaluate(invocation).await?;

// Correct: binding-scoped cache按hook_run/request identity收敛重复与并发调用。
let decision = decision_cache.get_or_evaluate(key, invocation, callback).await?;
```
