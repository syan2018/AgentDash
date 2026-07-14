# Main-Parity 会话链路恢复设计

## 1. 设计结论

采用“不可变 owned presentation event + Runtime-only carrier metadata + main 原前端直接消费”的结构。

```text
Codex App Server / Native Agent / Remote Runtime / Tool Owner
                         │
                         │  producer boundary 构造一次
                         ▼
ImmutablePresentationEvent
  durability
  event: BackboneEvent
                         │
                         │  只包裹，不重建
                         ▼
RuntimeJournalRecord
  runtime thread/revision/operation/binding/cursor metadata
  + Presentation(event) | Internal(runtime fact)
                         │
             ┌───────────┴───────────┐
             ▼                       ▼
Runtime reducer/snapshot       Journal/API transport
只派生内部状态                  只筛选/重包 Presentation
                                     │
                                     ▼
frontend envelope adapter
  移除Runtime transport wrapper
                                     │
                                     ▼
main features/session feed/reducer/renderer
```

`ImmutablePresentationEvent.event` 序列化后必须与 main 的 `notification.event` 深度全等。`SessionEventResponse`、`BackboneEnvelope`、session/journal sequence、outer timestamp、trace/source coordinate 均属于 typed wrapper，不得被放进或改写 protected event body。

禁止的方向：

```text
RuntimeEvent summary -> API match/serde_json -> guessed BackboneEvent
```

`runtime_presentation_event()` 类反向投影必须删除。信息一旦在 producer boundary 被压成摘要，后续任何 mapper 都无法恢复 main 的 ID、时间、source、null、事件顺序和完整 payload。

## 2. Oracle 与比较模型

### 2.1 两个基线、不同职责

| 基线 | 职责 |
| --- | --- |
| `D:\Projects\AgentDash-main-reference@957fa9d60` | 唯一生产行为、事件分类/顺序、AgentRun service/UI 与副作用 oracle |
| Codex `rust-v0.144.1` | 标准 Codex payload 的字段、variant、nullable 与 wire shape oracle |

main 已有 family 的产品语义不能被 `0.144.1` 升级改写；`0.144.1` 新 family 按官方 wire 原样加入 owned 标准协议，但不能取代 main 的 AgentDash extension 或把 Runtime 内部 taxonomy 暴露给 UI。

### 2.2 Golden capture

为 main 建只读 fixture capture：

1. 复用 main producer/mapper tests 和 route/service builders，捕获 `Vec<SessionEventResponse>`。
2. 固定 clock、UUID/operation ID、run/agent/session/turn/item/request ID、provider/tool outputs。
3. 每个场景保存 input fixture、main eventstream JSON、预期 UI/side-effect assertion。
4. current 使用同一 input fixture，经过自己的 Runtime wrapper 后输出 presentation eventstream。

main fixture是测试资产，不在参考worktree生成或提交；由当前分支的 parity harness读取已提交 golden。

### 2.3 Wrapper normalizer

normalizer 是显式 typed function，不是递归删除字段的 JSON helper。它只处理：

- current 的 Runtime transport frame与cursor；
- current endpoint/frame命名到统一 test carrier；
- `SessionEventResponse`/`BackboneEnvelope`的session、sequence、outer timestamp、source/trace等wrapper metadata；
- connected/heartbeat control frame的单独比较通道。

normalizer输出有序的`{ durability, presentation_event }`序列。它不得修改、补齐、重排或过滤`presentation_event`；该protected body逐项`serde_json::Value` deep equality，数组顺序和explicit null均参与比较。wrapper的可观察行为由独立history/stream/side-effect断言覆盖。

任何新增 allowlist 字段必须修改 PRD/design 并由用户审阅，不能由实现 agent 临时扩大。

## 3. Owned presentation contract

### 3.1 标准与扩展

`agentdash-agent-protocol` 继续承担 dependency-light owned contract：

- Codex标准 payload从`0.144.1`官方 exporter/schema机械生成；
- AgentDash extension维持显式 typed union；
- `BackboneEvent`/`BackboneEnvelope`是 AgentDash 会话 presentation contract，不等同于 Codex JSON-RPC transport；
- Runtime/Application/frontend不得直接依赖 vendor DTO。

生成工具必须提供：

```powershell
cargo run -p agentdash-agent-protocol-codegen -- write
cargo run -p agentdash-agent-protocol-codegen -- check
```

lock manifest记录Codex tag/version/commit、schema digest、root allowlist、generator版本、extension revision。fresh checkout只能依赖workspace Rust/Node工具链。

### 3.2 Null 与时间

- schema为nullable的字段必须能显式序列化`null`；optional-only字段按官方 fixture决定 omitted/null。
- AgentDash extension的nullable规则以 main JSON fixture为准。
- 时间来自 producer source event或统一注入的 producer clock，并在commit时固化。
- journal GET/replay/stream不得调用`Utc::now()`重造 presentation timestamp。
- 秒/毫秒单位由具体协议字段决定，禁止按字段名猜测或复用不同单位。

### 3.3 Identity

presentation payload 保留 main/source identity：thread、turn、item、request、tool call、entry index。

Runtime canonical identity只存在于carrier metadata：

```text
RuntimePresentationCoordinate {
  runtime_thread_id,
  runtime_turn_id?,
  runtime_item_id?,
  source_thread_id?,
  source_turn_id?,
  source_item_id?,
  interaction_id?,
}
```

coordinate map用于Runtime reducer、routing和response correlation，不得重写payload内ID。Codex source request ID与Runtime interaction ID必须同时保存；前端批准动作使用main卡片所持source/request identity，经API边界解析到Runtime interaction。

## 4. Runtime journal 与 snapshot

### 4.1 单一 journal record

建议合同：

```text
RuntimeJournalRecord {
  carrier: RuntimeCarrierMetadata,
  fact: RuntimeJournalFact,
}

RuntimeJournalFact =
  | Presentation(ImmutablePresentationEvent)
  | Internal(RuntimeInternalEvent)
```

这不是双事实：

- presentation事实只在`Presentation`记录中存在一次；
- operation/binding/context recovery等内部事实只在`Internal`记录中存在一次；
- reducer可以关联两者，但不从一方重新制造另一方；
- API只输出`Presentation`，内部inspect/audit endpoint读取`Internal`。

对于同一业务动作需要多个presentation事件时，producer按main顺序提交多个完整records，例如 user submit必须先提交`UserInputSubmitted`，再提交`TurnStarted`。事务保证顺序和幂等。

### 4.2 Snapshot

Runtime snapshot拥有command availability、active turn、binding、context、interaction与最终transcript索引。最终transcript引用/复制presentation terminal payload中的完整item，不能保存另一套压缩`RuntimeItemContent`后再重建UI。

transient delta使用carrier中的`stream_generation + transient_sequence + event_id`做去重和有界replay；其presentation payload仍是完整main-compatible delta事件。durable terminal覆盖live聚合状态，但不删除历史事实。

### 4.3 Persistence migration

项目未上线，不提供错误payload schema的兼容reader。修改journal/snapshot JSON shape时：

- 新增明确migration，清理或重建已有预研Runtime journal/snapshot/binding projection；
- 更新schema revision和repository tests；
- 禁止dual write、旧字段fallback与`#[serde(default)]`掩盖错误历史数据。

## 5. Producer boundaries

### 5.1 Codex integration

Codex adapter流程：

```text
JSON-RPC method
  -> vendor typed params/request
  -> strict transcode to generated owned payload
  -> main-compatible BackboneEvent wrapper
  -> immutable presentation commit
```

要求：

- method admission覆盖main全部方法与`0.144.1`纳入root allowlist的新方法；unsupported必须typed failure/diagnostic，不能静默丢弃；
- start/delta/terminal保留同一source item ID；
- request ID原样进入approval payload，Runtime interaction ID放carrier；
- total/last/context window/error details/thread status/title/diff/plan/compacted完整保留；
- reverse conformance helper仅用于证明未来可投影为Codex wire，本任务不发布server façade。

### 5.2 Native integration

以main `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`逐事件分支为oracle：

- User replay不重复创建用户fact；
- Assistant MessageStart不创建空item；Text与Reasoning使用独立identity；
- MessageEnd按main顺序输出message terminal、reasoning terminal、usage；
- Tool start/update/end、provider全部phase、diagnostic+error、compaction与approval完整输出；
- 所有ID和时间由确定的mapper state/producer clock产生。

### 5.3 Remote/Relay

wire envelope携带完整presentation payload和Runtime carrier metadata。Relay只验证generation/placement/correlation并转发；不得反序列化成摘要后重建。断线先提交authoritative internal binding fact，再处理pending correlation terminal；presentation事件不被伪造。

### 5.4 Tool owner

每个`ToolContribution`声明：

- main presentation family；
- started/update/terminal/error/approval builders；
- source/runtime identity映射；
- required fields与null策略；
- golden fixture列表。

Business Surface compile遍历最终catalog；缺少任一声明或fixture即admission失败。共享builder只能消除同family重复构造，不能按tool name猜family。

原行为矩阵：

| Tool family | Presentation oracle |
| --- | --- |
| command/shell | main `CommandExecution`/`ShellExec`及output delta、actions、cwd、exit/duration |
| file/apply patch | main `FileChange`、逐文件typed changes、rename/diff/status |
| fs read/grep/glob | main AgentDash extension参数、bounded output、success |
| MCP | main Codex/native对应的MCP或Dynamic表达、progress/result/error |
| dynamic | 只有显式dynamic工具使用`DynamicToolCall` |
| Workspace/Canvas | main DynamicToolCall与Platform presentation facts |
| Companion/Task/Wait | main typed details、source refs、status与Platform facts |
| terminal/control | main terminal output、PTY与control-plane Platform events |

## 6. Journal/API/stream

AgentRun journal service恢复main语义：

1. 解析AgentRun与delivery runtime binding；
2. 合并fork inherited prefix、marker与当前delivery journal；
3. 将Runtime物理identity/sequence通过typed wrapper adapter映射为main等价的AgentRun target/session语义；
4. 事件payload、source、trace、timestamp原样；
5. GET和NDJSON共享同一projection函数；
6. initial stream顺序为prefix → durable backlog → connected → ephemeral backlog → live；
7. heartbeat、resume cursor、lagged/closed与headers恢复main行为。

Runtime inspect/internal events可保留独立endpoint，但不能替代session journal endpoint。

## 7. Frontend 与产品外层

### 7.1 features/session

对`D:\Projects\AgentDash-main-reference\packages\app-web\src\features\session`逐文件比较：

- feed、stream、reducer、turn segmentation、tool registry、renderer、system dispatcher与测试以main为准；
- 允许差异仅为transport frame adapter、generated type import与`0.144.1`新增nullable类型要求；
- envelope adapter输出main `SessionEventEnvelope`，后续session代码不认识`RuntimeEvent`；
- unknown item不可降级成AgentMessage或generic JSON卡片。

### 7.2 AgentRun outer behavior

逐文件恢复main：

- `agentRunRuntime`/mailbox/executor service shape；
- conversation command snapshot authority、ownership、stale guard；
- submit/cancel/compact、accepted refs、redirect、backend/model selection；
- fork/fork-submit、round action与lineage；
- mailbox waiting/action/recall/resume；
- context projection/compaction；
- status bar target、`onSystemEvent`和control-plane副作用；
- AgentRun workspace parent/children/run detail和页面布局行为。

Runtime inspect/capability可以存在于内部调试界面，但不能替换main生产控制面或会话UI。

## 8. 单工作区并发实施模型

```text
W0 Main Oracle/Harness ──┬── W1 Protocol 0.144.1
                         └─────────────┬── G1
                                       ▼
                              W2 Immutable Carrier
                                       ▼
                              W3 Persistence/Migration
                                       ▼
                    ┌──────────┬───────┴────────┬──────────┐
                    ▼          ▼                ▼          ▼
                W4 Codex  W5 Native/Remote  W6 Tools  W7 App Producers
                    └──────────┴───────┬────────┴──────────┘
                                       ▼ G3
                              W8 Journal/History/Stream
                                       ▼
                           ┌───────────┴───────────┐
                           ▼                       ▼
                  W9 features/session      W10 AgentRun Outer
                           └───────────┬───────────┘
                                       ▼
                              W11 Full Parity
```

W0 建立 main golden、严格 comparator 与行为账本，W1 完成 Codex `0.144.1` generated contract；两项 ownership 不重叠，可在同一工作区并行，分别检查并提交后进入 G1。W2 单独冻结 immutable carrier，W3 冻结 repository/UoW 与 migration。

W4–W7 在 W3 提交后按 ownership 并行恢复 Codex、Native/Remote、Tool Catalog 与 application producers；受 agent 槽位限制分批启动，但不人为串行化。G3 等待四项全部独立检查并提交。W8 随后接线 journal/history/stream API；W9 与 W10 在 W8 合同冻结后并行，W11 执行最终 eventstream/browser parity 与 spec 收口。

所有工作都直接发生在当前本地分支，不创建临时 worktree/branch。每个派发 prompt 必须明确说明共享工作区内存在并行改动、该工作项的 ownership paths，以及不得覆盖/格式化/回退其他修改。主会话按 ownership 精确暂存通过检查的工作项并逐项提交；其他 agent 的未暂存修改原样保留。

所有 agent 复用同一 Cargo target/cache。Cargo 锁竞争按正常构建行为等待，不创建独立 target，不杀占锁进程，也不阻止前端、文档或不需要该锁的工作继续并行。任何 parity 失败都回到拥有该 presentation fact 的 producer，禁止在 API/frontend 加猜测补丁。

## 9. 验证策略

### 必须通过的主gate

- main/current eventstream deep equality after typed wrapper normalization；
- GET = initial NDJSON replay = reconnect replay = refresh；
- all driver/tool inventories complete；
- main/current service/route ledger一致或仅有审查通过的内部Runtime增量；
- main/current浏览器行为场景一致。

### 辅助gate

- protocol codegen write/check与vendor-owned roundtrip；
- Rust unit/integration/PostgreSQL tests；
- frontend Vitest/typecheck/lint；
- representative `pnpm dev` browser E2E；
- dependency direction与dead-code audit。

测试不得用以下方式宣称成功：只断言variant、删除null再比较、忽略事件顺序、只检查文字出现、由profile声明full fidelity、把被过滤事件当正确结果。

## 10. 方案取舍

| 方案 | 结论 | 原因 |
| --- | --- | --- |
| 从Runtime摘要反推BackboneEvent | 删除 | 信息不可逆丢失，已经造成当前回归 |
| Runtime直接依赖Codex vendor DTO | 不采用 | vendor进入durability kernel且Native被迫构造vendor领域 |
| generated owned payload作为不可变journal fact | 采用 | 保真、vendor隔离、可升级、可直接驱动原UI |
| presentation与internal facts同一journal不同variant | 采用 | 单一持久化事实源且职责清晰 |
| 双feed/compatibility/fallback | 不采用 | 形成长期分叉，项目未上线无必要 |
| 新建Runtime session renderer | 不采用 | 改变产品行为并悬空原组件图 |

## 11. 未来 Codex App Server 前端空间

owned标准payload必须能按`0.144.1`无损序列化回Codex标准wire；AgentDash extension通过独立namespace/capability negotiation表达。未来server façade只需要增加interface transport，不需要再次改Runtime journal或session renderer。本任务只守住该空间，不实现endpoint。
