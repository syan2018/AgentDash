# Canonical Runtime 会话展示链路设计

## 1. 设计结论

采用“schema-generated AgentDash-owned 完整同构会话协议 + 独立 durable Runtime envelope + 原前端组件图直接恢复”的三层结构。

```text
Codex App Server JSON-RPC / Native Agent / Enterprise Agent
                      ↓ adapter
AgentDash-owned Conversation Protocol
  full-fidelity item/event/interaction + typed extensions
                      ↓
Managed Runtime Envelope
  canonical IDs + sequence + revision + operation + terminal + recovery
                      ↓ API snapshot + NDJSON events
features/session transport/envelope adapter
                      ↓
原 SessionChatStream / useSessionFeed / SessionEntry presentation
```

当前 `RuntimeItemContent` 的八类简化 union 不再承担完整会话 payload。它不能通过继续增加若干可选字段修补，因为其 `tool_call/tool_result + JsonValue` 抽象已经丢失 command、file change、MCP、Companion、reasoning channel、usage、typed error 与过程 delta 的 discriminant。

## 2. 边界与依赖方向

### 2.1 AgentDash-owned Conversation Protocol

复用并收束 `agentdash-agent-protocol` 为 dependency-light 跨层合同：

- 标准Codex session/item/event类型由固定版本上游exporter输出的JSON Schema机械生成，不人工复制Rust字段；
- 生成完整 typed `AgentDashThreadItem`标准子集、conversation event、delta、usage、error与interaction request；AgentDash extension保持手写typed union；
- 标准子集与固定 Codex App Server protocol revision 的 JSON shape 同构；
- 继续由 Rust 生成 `packages/app-web/src/generated/backbone-protocol.ts` 或等价单一 generated contract；
- 不依赖 Application、Managed Runtime、driver host、数据库或 React；
- 移除对 Codex vendor crate、ACP runtime crate与宽泛 `agentdash-agent-types` 的不必要依赖，协议所需值对象归本 crate 或更小 shared contract。

“同构”由生成链与conformance证明，不通过re-export/vendor alias或人工同步实现。标准variant必须能与对应Codex payload做JSON等价roundtrip；AgentDash variant使用明确namespace/discriminant，不伪装成标准variant。

生成链：

```text
pinned codex-app-server-protocol 0.144.1 (rust-v0.144.1)
  -> upstream generate_json/generate_ts
  -> pinned v2 schema bundle + fixture tree
  -> AgentDash Rust codegen
  -> committed generated standard Rust types
  -> AgentDash extension composition
  -> project TypeScript contract generation
```

生成器必须有write/check两种模式：write更新产物；check在临时目录重新生成并diff。普通Runtime编译不运行上游generator，也不依赖Codex crate；只有protocol-codegen工具作为开发依赖接触vendor crate。

### 2.1.1 工具链选型

新增workspace内专用`agentdash-agent-protocol-codegen`工具crate（最终名称可按目录规范调整）：

- pinned `codex-app-server-protocol 0.144.1`（`rust-v0.144.1`）：调用上游`generate_json_with_experimental`/`generate_ts_with_options`；
- pinned `typify = 0.7.0`：通过builder interface从JSON Schema生成可提交的Rust source；
- `serde/serde_json`：canonical schema与strict transcode fixtures；
- `sha2`：记录schema bundle digest；
- `tempfile/similar`或项目既有diff helper：实现check mode临时生成与可读diff；
- `rustfmt`：格式化generated Rust；前端继续使用项目既有formatter/check。

不依赖`cargo install cargo-typify`或其它全局CLI，原因是生成流程必须由workspace lockfile固定且可在CI复现。`typify`仅属于codegen工具依赖，不进入`agentdash-agent-protocol`、Runtime或production binary依赖图。

### 2.1.2 生成目录与锁定清单

建议产物：

```text
schemas/upstream/codex-app-server-v2.schemas.json
schemas/upstream/codex-app-server-v2.typescript/...
crates/agentdash-agent-protocol/protocol-codegen.lock.json
crates/agentdash-agent-protocol/src/generated/codex_v2.rs
packages/app-web/src/generated/backbone-protocol.ts
```

`protocol-codegen.lock.json`至少记录：Codex crate version/tag/commit、experimental=false、upstream schema SHA-256、选取的root type清单、typify version与AgentDash extension schema revision。该文件是升级审查入口，不作为运行时协商协议。

### 2.1.3 生成步骤

```text
1. Codex exporter输出完整非experimental v2 JSON Schema/TS fixtures到临时目录
2. canonicalize并计算schema digest
3. 根据显式root allowlist裁剪session/item/event/interaction及其传递依赖
4. typify生成owned标准Rust types，增加项目要求的serde/TS derives与替换映射
5. 与手写AgentDash extension组合为最终conversation contract
6. 使用现有Rust -> TypeScript生成入口输出前端单一contract
7. rustfmt/前端formatter
8. write写入目标；check与仓库产物逐文件diff
```

root allowlist是唯一需要人工维护的“标准协议选择”配置；字段、variant与嵌套结构不得人工复制。若上游新增root family，升级审查显式决定是否纳入；已纳入family的schema变化自动进入diff。

### 2.1.4 命令与CI

计划提供：

```powershell
cargo run -p agentdash-agent-protocol-codegen -- write
cargo run -p agentdash-agent-protocol-codegen -- check
```

根`package.json`/quality gate可增加稳定入口，但仍委托给同一Rust工具。CI运行`check`并在schema hash、文件集合或内容不一致时失败；失败输出必须列出upstream schema diff、generated Rust diff和generated TypeScript diff中的对应阶段。

### 2.1.5 Codex升级流程

1. 更新workspace全部Codex Rust/npm/protocol revision pin；本任务首个基线为`rust-v0.144.1`。
2. 运行codegen `write`，审查schema/lock/root dependency diff。
3. 修复method admission、extension discriminant冲突与strict transcode tests。
4. 运行codegen `check`、protocol conformance、Runtime与frontend parity gates。
5. 在同一提交更新Cargo.lock、schema snapshot、generated Rust/TS与lock manifest。

如果`typify`对实际Codex schema无法生成可编译、可roundtrip的类型，W1停在feasibility gate并回到设计阶段评估其它机械生成方式；不得手写镜像，也不得把schema任意压缩成通用JSON结构。

### 2.2 Codex Integration Adapter

`agentdash-integration-codex` 是 Codex vendor 终止点：

- 直接依赖 pinned `codex-app-server-protocol`；
- JSON-RPC method、source IDs、request IDs 与 native process lifecycle 只存在于 adapter；
- 使用vendor typed deserialization，禁止当前`Value`字段猜测与`_ => AgentMessage(item.to_string())`；
- 标准payload通过vendor serialize -> generated owned deserialize的严格serde transcode无损转换；schema不匹配立即typed failure，不降级为文本；
- JSON-RPC method与event family admission仍使用穷举typed dispatch，避免新方法静默忽略；
- reverse projection/conformance helper 为未来 Codex-compatible server façade留空间，本任务不发布 endpoint。

Native 与企业 adapter直接产生 owned conversation events，不构造 Codex vendor DTO。

### 2.3 Managed Runtime Contract

`agentdash-agent-runtime-contract` 继续保持 dependency-light，并依赖 owned conversation protocol，而非 Codex vendor crate。

Runtime envelope拥有：

- canonical Thread/Turn/Item/Interaction IDs；
- durable sequence、revision与cursor；
- operation acceptance/terminal；
- binding、context、hook、recovery事实；
- 完整 typed conversation event/payload。

Runtime lifecycle与conversation payload分层但不形成双事实：item/turn lifecycle在同一 journal event中携带完整 payload；read-side只从journal/snapshot派生。Snapshot transcript保存完整 final `AgentDashThreadItem`，不保存压缩后的 `RuntimeItemContent`。

过程事件必须有 typed variant：agent message delta、reasoning text/summary delta、item started/updated/completed、command output、file change、MCP progress、plan、usage、error与interaction。若某 adapter无法提供字段，通过 profile/fidelity声明能力强度，不伪造空字段或文本 fallback。

### 2.3.1 Durable 与 Live Transient

Runtime stream必须显式区分：

- durable event：有`EventSequence`，进入journal，可按cursor恢复；
- live transient event：不冒充durable事实，但有`stream_generation + transient_sequence/event_id`，在同一active turn live buffer内可去重和有界replay；
- final item/turn terminal：durable且authoritative，覆盖过程delta投影。

当前`sequence=null`的generic `ItemDelta`不足以支持旧UI。W2必须定义generated `RuntimeStreamEnvelope`等价物，使浏览器能同时维护durable cursor与live transient cursor；target、binding generation或active turn变化时隔离旧transient state。有限durable batch不能通过自动重连模拟live stream。

## 2.4 Connector 与 Tool Projection

协议完整不等于producer已经正确。所有driver和tool owner必须进入同一projection conformance。

### 2.4.1 Driver 边界

| Producer | 投影责任 |
| --- | --- |
| Codex Integration | vendor typed JSON-RPC -> generated owned标准payload strict transcode；source/canonical ID映射；完整delta/interaction/status |
| Native Agent Integration | Agent Core message/reasoning/tool/provider事件 -> owned conversation event；不构造Codex vendor DTO |
| Remote Runtime/Relay | typed Runtime Wire envelope原样转发并只替换placement/generation坐标；不得重新解释payload |
| Future/Enterprise Integration | descriptor声明conversation projection profile并通过共享driver conformance harness |

`AgentRuntimeDriverContribution/Descriptor`必须能证明projection profile：支持的item/event/interaction family、delta fidelity、usage/error fidelity与extension revision。未通过required family conformance的offer不能承载要求这些presentation语义的AgentFrame。

### 2.4.2 Tool Owner Projection

当前`AgentToolResult { content, is_error, details: JsonValue }`和`ToolBrokerResult { output: JsonValue }`可以继续作为执行内部结果，但不能直接成为conversation protocol。每个`ToolContribution`增加显式protocol projector/descriptor，由tool owner负责：

```text
Tool invocation + typed owner metadata
  -> item started payload
Tool update callback
  -> typed progress/update/delta
Tool terminal result
  -> typed completed/failed payload
```

Projector family至少覆盖：

- command/shell -> `CommandExecution`或AgentDash `ShellExec`，保留cwd、actions、process、output、exit code、duration与execution mode；
- file write/edit/apply patch -> `FileChange`，保留typed changes/diff/status；
- fs read/grep/glob -> AgentDash typed extension，保留路径、pattern、分页/limit、bounded output与success；
- MCP -> `McpToolCall`，保留server/tool/plugin/resource URI、progress/result/error/duration；
- explicitly dynamic tool -> `DynamicToolCall`，保留namespace、content items、success/duration；它是声明的family，不是unknown fallback；
- Workspace Module/Canvas -> typed AgentDash extension，保留operation/presentation/resource identity与diagnostic；
- Companion/collaboration -> typed dispatch/request/result/status/source refs；
- Task/Wait/其它产品工具 -> 对应typed extension与原UI需要的view/details。

Business Surface compile遍历最终Tool Catalog并验证每个contribution都有projector。缺失projector是typed admission failure；禁止中央`match tool_name`推断kind，也禁止自动选择DynamicToolCall。

### 2.4.3 行为基线与审计

以`af21f9d7c^:crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`作为Native投影行为oracle，但不原样恢复中央mapper。旧mapper覆盖的item、delta、usage、compaction、approval与error必须进入新projector matrix；实现归各driver/tool owner，共享builder只承载重复协议构造。

## 3. Frontend 恢复策略

以 `af21f9d7c^` 为行为和组件基线，直接恢复：

- `useSessionStream`、`streamTransport`、NDJSON validator与platform dispatcher；
- `sessionStreamReducer`、`useSessionFeed`、turn segmentation、thinking/context/tool aggregation；
- `SessionChatStream -> SessionEntry -> ToolCallCardShell/toolCardRegistry`；
- round actions、fork、token usage、Companion、terminal/system event projection与原测试。

不整体回滚 `SessionChatView`：保留当前 canonical command availability、interaction response、AgentRun product projection与后续 runtime 修复，只把消息 presentation subtree接回。

允许修改的 frontend seam只有：

1. NDJSON endpoint/cursor与新 Runtime envelope validation；
2. Runtime envelope中conversation event到原 `SessionEventEnvelope`/feed reducer输入的无损解包；
3. generated owned type的机械引用变化。

删除 `AgentRuntimeFeed`、`useAgentRuntimeFeed` 及其平行 `role + text + status` view model。不得新建替代 renderer。

## 4. 数据流

```text
Driver typed event
  -> exhaustive adapter conversion
  -> Runtime journal commit
  -> Runtime snapshot / durable NDJSON event
  -> frontend envelope validator
  -> conversation event projector
  -> existing session reducer/feed
  -> existing typed renderer
```

Product/resource events只触发对应projection invalidate或原有明确副作用，不推进Runtime state。Command availability只来自Runtime snapshot；恢复旧UI不恢复旧command authority。

## 5. 协议版本与迁移

- 固定受支持 Codex protocol revision并在 owned contract中记录 conformance baseline。
- Codex依赖升级必须先更新fixture/schema diff，再补穷举conversion；未映射variant编译或测试失败。
- Runtime event/snapshot schema升级使用新的contract revision；项目不提供旧schema reader、dual write或fallback。
- 若数据库JSON payload已存在且无法按新shape解释，新增migration清理/重建预研Runtime journal/projection数据；不得保留兼容反序列化分支。

## 6. 方案比较

| 方案 | 结论 | 原因 |
| --- | --- | --- |
| Runtime直接使用Codex vendor types | 不采用 | vendor依赖与版本变化进入durability kernel；Native/企业adapter被迫构造vendor领域 |
| 当前最小`RuntimeItemContent` | 删除 | 信息有损，无法驱动原UI，也无法证明未来Codex兼容 |
| 人工维护owned同构镜像 | 不采用 | 字段同步成本高，升级时容易漏字段或语义漂移 |
| schema-generated owned同构协议 + adapter conformance | 采用 | 无手抄、Runtime稳定、前端无损、vendor隔离、未来标准wire可投影 |
| 同时保留旧Backbone feed与新Runtime feed | 不采用 | 形成双事实与长期分叉 |

## 7. 验证策略

- 上游schema/TS fixture与AgentDash生成产物有write/check drift gate。
- codegen fresh-workspace test证明无需全局CLI即可从lockfile重建同一文件树。
- Codex vendor ↔ generated owned protocol代表性JSON等价fixtures覆盖所有标准item/event/interaction family。
- Adapter method admission无wildcard/catch-all；strict transcode失败返回typed unsupported/protocol mismatch。
- Driver conformance覆盖Codex/Native/Remote相同source事实产生的owned event family与terminal保证。
- Tool Catalog conformance枚举所有贡献，缺失projector失败；每个projector有call/update/result golden tests。
- Runtime reducer/snapshot/replay覆盖完整item、typed delta、duplicate cursor、gap、target isolation和terminal。
- Runtime stream覆盖durable cursor + transient generation/sequence、active-turn有界replay、reconnect去重与final item覆盖。
- Frontend恢复原测试，并增加使用新Runtime envelope驱动原renderer的contract tests。
- UI parity覆盖command、diff、MCP、Companion、reasoning、plan、context、usage、error、interaction与round actions。
- route ledger、generated contract check、Rust/TypeScript检查和代表性AgentRun workspace E2E共同验收。

## 8. 未来 Codex Frontend 空间

未来若发布Codex-compatible server façade，只需在interface层把owned标准子集投影为Codex JSON-RPC，并通过capability negotiation决定是否暴露AgentDash extension。本任务只保证该映射无损且边界存在，不实现或发布endpoint。
