# Connector、Tool 与 Application Producer 恢复清单

## Oracle

- 唯一生产行为oracle：`D:\Projects\AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- 当前实现基线：任务重订前`36c5484e6`。
- W0必须动态刷新本清单；本文是派发起点，不是完成证明。

## 当前结构性缺口

- Codex mapping把完整notification压成较窄`RuntimeEvent`，canonicalize item/interaction identity，遗漏main中的diff/plan/status/title等family。
- Native mapping在Assistant MessageStart创建空`ItemStarted`，message/reasoning共享identity，provider/compaction/approval/usage/error顺序与main不同。
- ToolBroker只提交started/terminal与generic `ToolProgress`，而session stream需要main实际ItemUpdated/typed delta/payload。
- 新`Vfs/RuntimeAction/WorkspaceModule/Companion/Task/Wait/LifecycleComplete/TerminalControl`presentation discriminant来自Runtime taxonomy，不是main既有UI合同。
- frontend `features/session/model/runtimeSessionAdapter.ts`直接认识`RuntimeEvent`并反向构造旧session event，违反不可变presentation边界。
- 原application session producer在production切换中大量消失或迁到internal Runtime facts；hook/title/terminal/rewind/context/control-plane等Platform facts需要按main恢复。

## Driver inventory

| Driver | Main oracle | 必须恢复 |
| --- | --- | --- |
| Codex | main `crates/agentdash-executor/src/connectors/codex_bridge.rs` | 所有supported JSON-RPC notification/request的完整typed payload、source IDs、事件顺序和main interaction行为；`0.144.1`新增root原样 |
| Native Agent | main `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` | message/reasoning独立生命周期、tool lifecycle、provider全phase、usage、diagnostic+error、compaction、approval requested/resolved |
| Remote Runtime/Relay | main relay/runtime interface与当前wire ownership | 完整presentation payload原样穿透，只重包placement/generation wrapper |
| Future/Enterprise | contribution descriptor与共享conformance | 未通过全部required fixture不能admit |

profile/fidelity只能声明capability，不能替代行为fixture。每个driver inventory行必须记录：owner、source event、presentation event、ID map、durable/transient、golden fixture、check result。

## Main tool expression原则

工具恢复目标不是为每个业务工具创造新的ThreadItem类型，而是保持main实际表达：

- main已有Codex/native typed variant继续使用该variant；
- main使用`DynamicToolCall`的产品工具继续使用`DynamicToolCall`；
- resource/presentation/dispatch/task等额外业务语义继续由main对应Platform fact表达；
- Runtime内部tool taxonomy只在carrier/capability层存在，不进入protected presentation body。

## Dynamic Tool Catalog inventory

W6从最终Business Surface/Tool Catalog实际枚举contribution，不使用硬编码名单作为完成判定。每行至少记录：

```text
owner crate
runtime name / capability key
main ThreadItem family
main Platform companion facts
started builder
update/progress builder
completed/failed builder
approval/request correlation
source/runtime identity map
frontend renderer
golden fixture
```

初始family：

| Family | Main表达恢复要求 |
| --- | --- |
| command/shell | `CommandExecution`/`ShellExec`、cwd/actions/process/output/exit/duration与command delta |
| file/apply patch | `FileChange`、逐文件changes/diff/rename/status与file delta |
| fs read/grep/glob | main AgentDash FS extension参数名、bounded output、success |
| MCP direct/relay | main对应MCP/Dynamic item、progress/result/error/duration |
| explicit dynamic | 只有明确声明dynamic的工具使用`DynamicToolCall` |
| Workspace/Canvas/VFS | main实际Dynamic item加workspace/control-plane Platform facts |
| Companion/collaboration | main Dynamic item、dispatch/request/result/source refs与Platform facts |
| Task/Wait/lifecycle | main Dynamic item与task/status/session meta facts |
| terminal/control | main command/shell item、terminal output、PTY和control-plane facts |

任何inventory新增项自动成为W6 required fixture；不存在“其它工具默认generic”分支。

## Application producer inventory

W7搜索main全部生产写入点，包括但不限于：

```powershell
rg -n "persist_notification|emit_user_input_submitted|BackboneEvent|PlatformEvent" D:/Projects/AgentDash-main-reference/crates
```

逐个记录current owner，至少覆盖：

- user prompt/steer与system/workflow/companion delivery；
- turn started/terminal/rewind；
- source title/thread status；
- hook trace/action；
- context change/compaction；
- provider/diagnostic；
- terminal output/PTY；
- control-plane projection；
- mailbox/system message；
- fork marker/lineage/round coordinates。

不存在current owner或fixture即G3失败；不得用API filter把缺失producer隐藏。

## 完成定义

```text
driver contribution count = driver conformance count
tool contribution count = projector count = full fixture count
main production writer count = current owner count = fixture count
```

三组等式全部成立且W0 deep comparator通过，才允许标记W4–W7完成。

## W5 Native / Remote 实施账本

Native vendor stream声明`vendor_stream`是`ToolCallStart/Delta/End`、
`ToolExecutionStart/Update/End`与approval生命周期的唯一presentation emitter。ToolBroker可以
保留Runtime内部事实，但不能为同一Native生命周期再次产生presentation fact。

W5 mapper逐分支来自
`main@957fa9d60:crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`，保留Main可见的
文本/推理delta和独立terminal item、全部provider phase、diagnostic后接typed error、message与
reasoning terminal后的usage、context compaction生命周期、tool start/update/output/terminal及
approval requested/resolved。

当前有意不产生presentation的`AgentEvent`分支是穷尽分类，不是fallback：

- `AgentStart`、`TurnStart`、`TurnEnd`与`AgentEnd`：canonical turn presentation生命周期由
  application producer拥有，Native driver只记录Runtime内部确认。
- `MessageStart`：只建立correlation，因为Main不会为普通assistant message创建空item。
- assistant `TextStart/TextEnd`与`ThinkingStart/ThinkingEnd`：只表达stream framing；可观察内容由
  delta与terminal事件承载。
- assistant `ToolCallEnd`：只更新mapper correlation state；实际terminal由`ToolExecutionEnd`产生。
- user message replay：只建立correlation；user fact由application submit producer拥有。

当前穷尽的Native `AgentEvent`枚举没有需要猜测presentation映射的新增variant。未来新增variant会
形成编译期mapping决策，在产品语义明确前保持pending，不能进入generic text/tool fallback。

Remote Runtime/Relay不拥有presentation语义：它按序serialize、relay、deserialize完整
`RuntimeJournalFact`，只修改allowlist中的placement/generation/correlation wrapper。conformance
fixture包含explicit null与异构数组顺序，避免body漂移被语义比较掩盖。

W5已将Main oracle commit `957fa9d60`固定为两组可执行golden：Native 7个场景由mapper实际产出
后通过W0 ordered comparator比较完整event body与durability；Remote/Relay 3个场景依次经过Runtime
Wire与Relay typed frame roundtrip后使用同一comparator比较。比较没有字段ignore list，timestamp由测试
时钟固定为oracle capture时间，生产仍使用真实时钟。

## W6 Final Tool Catalog 实施账本

生产bootstrap重新通过同一个`SessionRuntimeToolComposer`向Business Surface动态装配六个owner
provider：VFS、lifecycle、companion、task、wait与workspace module。composer的final-catalog
constructor固定六个provider槽位；每次`build_tools()`都遍历实际返回工具，缺owner projector或
main parity fixture立即拒绝，随后Business Surface再次对实际catalog执行schema、唯一runtime
name/tool path、非空且唯一fixture admission。MCP contribution仍由同一turn assembly追加并经过相同
Business Surface admission，不用静态工具名列表推断family。

当前生产owner实现动态审计为23个projector与23个fixture声明。Main展示family仅保留command、
file change、fs read/grep/glob、MCP和explicit dynamic；VFS mount、runtime action、workspace、
companion、task、wait与lifecycle owner均恢复为`DynamicToolCall(namespace: null)`，其额外产品语义由
对应Platform producer拥有。`Vfs/RuntimeAction/WorkspaceModule/Companion/Task/Wait/
LifecycleComplete`不再是Tool protocol presentation discriminant。

最终catalog contribution显式携带唯一`vendor_stream`或`tool_broker` emitter。Native/Codex driver
callback路径使用`vendor_stream`，避免ToolBroker对同一vendor生命周期重复发presentation；
`tool_broker`路径保留独立声明，后续journal/stream接线必须直接提交完整typed presentation fact，
不能从Runtime summary反向重建。
