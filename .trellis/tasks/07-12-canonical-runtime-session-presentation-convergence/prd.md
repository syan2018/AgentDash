# 彻底恢复 Main 会话协议与产品行为

## Goal

以只读参考 worktree `D:\Projects\AgentDash-main-reference` 的 `main@957fa9d60` 为生产行为唯一 oracle，彻底恢复 AgentRun 会话从 connector/tool producer 到 journal/API/frontend 的完整链路。

允许 Managed Runtime 重构内部 operation、binding、context、recovery、persistence 与 transport wrapper；不允许改变 main 已产出的 presentation eventstream 内容。对同一输入，移除明确列入allowlist的Runtime transport wrapper后，事件数量、顺序、类型、完整payload，以及payload内的ID、source、时间、null/omitted形状和前端可见副作用必须与main深度全等。

本任务保持单一主任务，不创建 child task；实施拆成多个有明确文件所有权、独立检查与逐项提交的工作项，在当前本地工作区按依赖关系并发推进。

## Authoritative Baselines

### Production behavior oracle

- Worktree：`D:\Projects\AgentDash-main-reference`
- Commit：`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
- 用途：生产 session event 分类与顺序、journal/history/stream、service shape、command/fork/mailbox/context、`features/session` reducer/feed/renderer、AgentRun workspace 与副作用时序。
- 参考 worktree 全程只读，不在其中提交、生成或格式化文件。

### Protocol revision oracle

- Codex App Server Protocol：`rust-v0.144.1 / 0.144.1`。
- main 与 `0.144.1` 共同存在的 event/item/input/interaction family，必须保持 main 的产品行为并使用 `0.144.1` 官方 wire shape。
- `0.144.1` 新增而 main 不存在的标准 family，必须按官方 schema 原样承载，不得借升级改变 main 已有 family。
- AgentDash extension 以 main 的 typed extension 行为为 oracle；Runtime 内部新事实不得伪装成新的 presentation item。

### Task-scoped contract precedence

本任务实施期间，`main@957fa9d60` 的生产行为与本 PRD 是 session presentation 和 AgentRun 外层可观察行为的验收依据。现有 `backbone-protocol.md`、`frontend-backend-contracts.md`、`frontend/type-safety.md` 与 `frontend/state-management.md` 中描述 Runtime feed 替代原 session feed、移除原 command snapshot/stale guard 的条款，属于待迁移的架构输入，不作为本任务 parity check 的否决依据。

W0 必须建立逐条 spec 冲突账本；W11 在全链路 parity 通过后，把最终 immutable presentation carrier、wrapper 分层与恢复后的产品行为写回 `.trellis/spec/`。其他与本任务目标不冲突的项目规范仍然适用。

## Wrapper Allowlist

只有下列内容可以不同：

1. Runtime 内部、且不会进入 normalized presentation event 的 thread/revision/operation/binding/recovery/persistence metadata。
2. NDJSON 外层 transport frame 的物理结构、endpoint 与 durable/transient cursor 字段。
3. `SessionEventResponse` 与 `BackboneEnvelope` 中用于寻址、提交、trace和投递的 wrapper metadata，包括session/journal sequence、outer timestamps、update type、source/trace coordinates；这些字段必须由typed adapter归一化，不能渗入protected payload。
4. `connected`、`heartbeat` 等非 presentation control frame；其重连和时序行为仍须满足 main 的可观察语义。
5. 数据库表、列、索引与迁移实现，只要读取后的 presentation eventstream 完全一致。

比较前 adapter 只允许剥离或归一化以上字段。protected body 明确定义为每条event的`notification.event`，或新合同中的等价`presentation_event`。以下内容受保护：

- event 数量、顺序、durable/ephemeral 分类；
- `BackboneEvent` discriminant 与完整 payload；
- payload 内所有source、ID、timestamp、nullable/omitted字段、数组顺序、delta与terminal；
- fork prefix、entry/round correlation、cursor/reconnect和side effect的可观察结果。

任何未列入 allowlist 的差异一律按 regression 处理。禁止通过忽略字段、排序、填默认值或语义等价断言放宽 deep equality。

## Requirements

### R1 — 不可变 presentation payload

connector、Native mapper 与 tool owner 在语义事实产生处构造完整 owned presentation event；该 payload 一经产生即不可变地进入 Runtime journal。Runtime reducer可以索引其 IDs 和 terminal 状态，但不得从较窄的 `RuntimeEvent`、snapshot 或 tool result 反向猜回 presentation event。

### R2 — 单一事实，不是双轨兼容

Runtime journal 是唯一持久化事实源。每条会话presentation-bearing record同时包含Runtime内部metadata与一个完整presentation payload；纯operation/binding/context/recovery内部record不进入session presentation stream。不得存在第二套Backbone authoritative runtime、dual-read、fallback或兼容reader。预研数据不兼容时通过migration清理/重建。

### R3 — Main eventstream 完整恢复

必须逐 family 恢复：

- user input 与 system/workflow/companion delivery；
- turn started/completed/failed/interrupted/rewound；
- agent message 与 reasoning text/summary delta、独立 durable terminal；
- item started/updated/completed；
- plan updated/delta、turn diff；
- command/file/MCP/dynamic/fs及workspace/companion/task/wait等工具在main中的实际`ThreadItem + Platform`表达；
- tool progress/output/result/error/approval；
- token usage 与 normalized context usage；
- typed conversation error 与 runtime diagnostic；
- context change/compaction；
- thread status/title；
- hook、terminal、PTY、control-plane、mailbox、fork/round action 相关 Platform facts。

### R4 — Codex 0.144.1 owned protocol 工具链

保留并校正 workspace 内 `0.144.1` codegen。只有 protocol-codegen 与 `agentdash-integration-codex` 可以依赖 vendor crate；Runtime/Application/frontend 使用 generated AgentDash-owned types。标准类型从官方 schema/fixtures 机械生成，nullable/omitted 规则由 schema 决定，禁止手抄镜像与裸 JSON fallback。

Codex adapter 对已支持 method 做 vendor typed deserialize 与 strict transcode，随后保存完整 notification/request payload；不得在 admission 后压成 Runtime 摘要再重建，不得重写 presentation payload 内的 source turn/item/request ID。

### R5 — Native、Remote 与 Tool Catalog 全量桥接

- Native 行为以 main 的 `pi_agent/stream_mapper.rs` 为 oracle，恢复 message/reasoning/tool/provider/usage/error/compaction/approval 的事件数量、顺序、ID 与 payload。
- Remote Runtime/Relay 原样转发完整 typed presentation payload，只更改 allowlist 中的 placement/generation wrapper。
- 每个最终 Tool Catalog contribution 由 owner 显式声明 main-compatible projector，覆盖 started、update/progress、terminal、error、approval 和 identity。
- main 中为 `DynamicToolCall + Platform fact` 的工具继续输出该既有表达；禁止为了 Runtime taxonomy 新增 frontend presentation discriminant。
- 缺 projector 或 parity fixture 的 tool/driver 不能通过 Business Surface admission。

### R6 — Journal、history 与 stream 行为恢复

恢复 main 的 AgentRun journal identity、fork inherited prefix、fork marker、durable/ephemeral backlog、live ordering、heartbeat、resume cursor、lagged/closed 处理和 target isolation。GET page、NDJSON initial replay、live 与 refresh 必须输出同一 presentation facts。

### R7 — Frontend 和 AgentRun 外层完全恢复

- `features/session` 以 main 逐文件对照，只允许 envelope adapter 和 generated type import 的必要差异。
- 普通 User/Agent MessageStart 不进入 ToolCall renderer；只有真实 tool lifecycle 产生 item lifecycle 卡片。
- 恢复 main 的 command snapshot authority、ownership、stale guard、executor/backend selection、accepted refs/redirect。
- 恢复 fork/fork-submit、round actions、mailbox waiting/actions/recall、context projection/compaction、status bar、`onSystemEvent`、lineage 与页面外层行为。
- 不保留 `AgentRuntimeFeed`、第二套 renderer 或 Runtime-aware session reducer。

### R8 — 单工作区并发收束

所有工作项都在 `D:\Projects\AgentDash` 当前分支实施，不创建临时 worktree 或工作项分支。依赖已满足且 ownership paths 不重叠的工作项允许由多个 implement/check agent 并发推进；每次派发必须明确告知 agent 当前工作区存在其他并行修改，要求只触碰声明范围并保留他人成果。主会话按工作项精确暂存并逐项提交。

所有 agent 共享当前工作区的 Cargo target/cache；遇到 Cargo build directory lock 时正常等待，由 Cargo 自身排队，不另建 target、不终止其他 Cargo/rustc/rust-analyzer 进程，也不因此禁止其他不冲突工作并行。任何跨工作项 contract 或 ownership 变更先由主会话协调。

### R9 — 验收不能由声明自证

projection profile、capability descriptor、类型检查、事件 variant 断言或“页面出现文字”都不能证明 parity。唯一主 gate 是 main/current 对同一 fixture 在 wrapper normalization 后的 deep equality；辅 gate 才是 Rust/TS tests、typecheck、lint 和浏览器 E2E。

## Acceptance Criteria

### Eventstream parity

- [ ] 建立 main/current 双运行或固定 main golden harness；normalizer 只含本 PRD wrapper allowlist。
- [ ] 同一 user submit 的首个 presentation fact 是 `user_input_submitted`，随后才是 `turn_started`；source/content/ID 与 main 完全一致，刷新后角色不变。
- [ ] 普通 User/Assistant MessageStart 不产生 `item_started`；真实工具严格按 main 输出 started/update/delta/terminal。
- [ ] agent message、reasoning、plan、diff、usage、error、approval、context、Platform 全 family 的事件数量、顺序与完整 JSON deep equal。
- [ ] 同一 durable event 在 GET、首次 NDJSON、reconnect 与 refresh 中内容恒定，不因读取时间重新生成 timestamp。
- [ ] Codex 同一 item/request 在 start、delta、terminal/response 中保持同一 source identity。
- [ ] nullable 字段按 `0.144.1` schema与main extension合同输出 null/omitted，测试不得删除 null 再比较。

### Producer and storage parity

- [ ] Codex、Native、Remote driver inventory全部通过共享 eventstream conformance。
- [ ] 最终 Tool Catalog 每个 contribution 都有 owner projector 和 main-compatible call/update/result/error/approval fixture。
- [ ] Runtime journal直接保存完整不可变 presentation payload；不存在 `runtime_presentation_event()` 一类反向重建器。
- [ ] Runtime snapshot/reducer只派生内部状态和最终 transcript，不压扁或改写保存的 presentation event。
- [ ] 数据库 migration 已处理当前错误 payload 数据，不保留旧 reader、dual write 或 fallback。

### History and transport parity

- [ ] AgentRun transport identity可使用新的wrapper坐标，但typed adapter必须稳定映射为main等价的AgentRun target/session语义，不能使事件隔离或live side effects失效。
- [ ] fork inherited prefix、fork marker、entry index、round action 坐标和父子 lineage 与 main一致。
- [ ] durable backlog、connected、ephemeral backlog、live、heartbeat、resume、lagged/closed 行为通过 main parity tests。
- [ ] GET page、NDJSON replay、live terminal 与 reconnect 不漏事件、不重排、不重复追加 transient delta。

### Frontend and product parity

- [ ] `features/session` 组件图、feed、reducer、tool registry和视觉行为与 main 一致，仅 envelope adapter/generated type seam有审查过的差异。
- [ ] 用户气泡、assistant/reasoning、plan、typed tools、Companion、context、error、usage与approval均落入原 renderer。
- [ ] fork、mailbox、context projection、command authority、模型/后端选择、status bar、system side effects、lineage与AgentRun页面行为逐项恢复。
- [ ] main/current 浏览器 parity覆盖用户 submit、无 phantom tool card、refresh、tool progress、interaction、fork、mailbox与context compaction。

### Protocol and quality gates

- [ ] Workspace Codex相关依赖、schema、fixture与codegen lock统一为`0.144.1`，无残留旧 pin。
- [ ] protocol codegen write/check可复现，无全局CLI依赖，fresh checkout生成结果一致。
- [ ] vendor `0.144.1` payload与owned payload strict roundtrip deep equal；method/variant admission无静默 catch-all。
- [ ] implement/check agent使用不同清单；每个工作项有独立check结果与符合项目格式的提交。
- [ ] 最终执行协议check、相关Rust tests、frontend tests/typecheck/lint和代表性E2E；绿灯必须包含main deep-parity gate。

## Out of Scope

- 实现或发布对外 Codex App Server JSON-RPC server endpoint。
- 保留旧 RuntimeSession/Backbone runtime 作为兼容路径。
- 旧 schema、旧 API、旧数据库 payload 的兼容 reader或fallback。
- 与 main 行为恢复无关的 AgentRun UI重设计、工具taxonomy重命名或产品交互调整。
- 将 Runtime 内部 operation/context/binding 事实直接绘制为新的会话 item。
