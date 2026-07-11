# Application / Compaction / Context 现状调查

> 调查范围：`agentdash-application`、`agentdash-application-agentrun`、`agentdash-application-runtime-session`、Agent Core、executor 协议适配、session persistence 与相关 migration / tests / 历史任务。
>
> 本文只记录仓库事实、由事实支持的架构判断和目标归属建议，不修改生产实现。

## 1. 结论先行

当前问题并不是“compaction 放错了一个 crate”这么简单，而是同一项 managed Agent 能力被拆成了多段浅模块：

1. `application-agentrun` 接收产品命令、判断运行态并创建 durable request；
2. `application-runtime-session` 在 launch 时消费 request、决定 auto policy、恢复 transcript；
3. Agent Core 执行摘要算法并先修改内存 context；
4. `executor/pi_agent` 把 Core 事件翻译成 Codex / AgentDash 协议事件，同时拼出业务投影所需的 stringly JSON；
5. `application-runtime-session::SessionEventingService` 再解析这份 JSON、提交压缩投影、推进 projection head、写 ContextFrame 和 manual request 终态；
6. infrastructure 只在最末端提供 PostgreSQL 事务。

这导致真正的不变量没有单一所有者。最重要的例子是：

- “压缩成功”在 Agent Core 中等于内存消息已经被替换；
- “压缩成功”在业务 session 中应等于 replacement projection 已原子提交；
- “压缩完成”在协议上又由 executor 额外发送 `ItemCompleted` 表示；
- 但 turn processor 会吞掉持久化错误，因此三者可以分叉。

对用户原始判断需要做一个关键修正：

- **compaction 算法、触发策略、会话 replacement baseline、恢复语义属于业务 Agent module，不属于 infrastructure。**
- **infrastructure 只拥有这些状态的原子存储 adapter、数据库约束和 migration。**
- `application` 应只编排产品用例并依赖业务 Agent 的窄 interface；不应知道 request 消费、compact-only 维护轮、投影 segment/head、transcript fold 或 connector 的恢复能力细节。
- `executor` 应只把统一 Agent Runtime interface 适配到内部 Agent Core 或外部 Codex App Server；不应制造平台压缩投影的业务提交 payload。

从 deep module 角度看，当前 crate 拆分主要是物理拆分，不是信息隐藏。删除任意一个当前“边界”后，复杂度只会原样泄漏到相邻 crate，说明这些 interface 没有形成有效 leverage/locality。

## 2. 当前依赖与职责事实

### 2.1 crate 依赖已经形成双向语义耦合

- umbrella `agentdash-application` 同时依赖 `application-agentrun`、`application-runtime-session`、domain、SPI、protocol，并在 dev dependency 中直接依赖 Agent Core：`crates/agentdash-application/Cargo.toml:8-25,57`。
- `application-agentrun` 直接依赖 `application-runtime-session`、domain、SPI、protocol：`crates/agentdash-application-agentrun/Cargo.toml:8-17`。
- `application-runtime-session` 直接依赖 domain、SPI、protocol：`crates/agentdash-application-runtime-session/Cargo.toml:8-13`。
- executor 依赖 SPI、domain、protocol，并在 `pi-agent` feature 下直接依赖 Agent Core：`crates/agentdash-executor/Cargo.toml:8-11,28,36-44`。
- 自称通用核心的 `agentdash-agent` 仍直接依赖 AgentDash domain：`crates/agentdash-agent/Cargo.toml:9-11`。

**判断：** 当前没有“application -> 通用 Agent interface -> executor adapter -> internal/external implementation”这一条稳定依赖方向。产品、运行时、协议、持久化 DTO 与 Core types 在多层同时可见。

### 2.2 已有 runtime-session seam 并未真正统一

`crates/agentdash-application/src/runtime_session_agent_run_bridge.rs:12-45` 已经为 AgentRun 定义的 session ports 提供了 core/control/eventing/launch bridge，`SessionCoreBridge`、`SessionControlBridge`、`SessionEventingBridge`、`SessionLaunchBridge` 分别在 `:47-193` 做类型映射或透传。

但 manual compaction API 没有走这组 seam：

- API handler 直接拿 `state.services.session_launch` 和 manual request repository，构造 concrete `AgentRunContextCompactionSessionRuntimePort`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1002-1049`。
- 该 adapter 又直接持有 concrete `application_runtime_session::SessionLaunchService`：`crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:183-203`。

**问题：** 一部分 AgentRun 用自有 port + bridge，另一部分直接引用 concrete runtime-session。interface 既没有隐藏实现，也没有统一测试面。

## 3. Compaction 真实端到端调用链

### 3.1 手动压缩入口与产品编排

HTTP route 为 `/agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:173-179`。handler 完成授权和 workspace command policy 后，直接组装 compaction command service：同文件 `:1002-1049`。

`AgentRunRuntimeCommandFulfillmentService::decide_context_compaction` 读取 current delivery 并按运行态分流：

- Running 且有 turn：创建 next-turn request；
- Starting / Cancelling / Lost：拒绝；
- Idle / Completed / Failed / Interrupted：启动 compact-only maintenance turn。

证据：`crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:82-141`。

`AgentRunContextCompactionCommandService::compact_context` 还负责 command receipt 幂等、选择 delivery、创建 request、launch、保存结果：同文件 `:297-349,352-472,602-638`。

**事实归属：** run/agent 鉴权、command receipt 与选择当前 delivery 是 application/product orchestration；“何时 next turn、何时 compact-only、哪些 runtime state 可执行”是 managed Agent runtime 命令语义，当前两者揉在同一文件。

### 3.2 compact-only launch 包含 transport 级轮询策略

`AgentRunContextCompactionSessionRuntimePort::launch_compact_only_turn`：

- 构造英文占位 prompt `Run AgentDash manual context compaction maintenance turn.`；
- 调用 `SessionLaunchService::launch_command_in_task`；
- 以 25ms 间隔轮询 request，最多 750ms；
- 快速完成则返回 Completed / NoEligible / Failed，否则返回 Launched。

证据：`crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:201-261`。

文件还保留 `AgentRunContextCompactionRuntimeTodoPort` / `NotImplemented`：同文件 `:153-173,264-277`。

**问题：** command interface 同时暴露同步结果与异步 launch，调用方必须知道 750ms 这一隐藏时序；receipt 的 observable outcome 会受机器速度影响。项目不需要兼容或 fallback，Todo adapter 和 `NotImplemented` 变体没有继续存在的架构理由。

### 3.3 durable manual request 在 domain、runtime-session、eventing 间漂移

request aggregate 当前位于 workflow domain：

- 状态：Requested / Consumed / Completed / Noop / Failed；
- 模式：NextTurn / CompactOnly；
- 保存 session/run/agent/receipt、参数、consumed turn、compaction refs。

证据：`crates/agentdash-domain/src/workflow/manual_context_compaction_request.rs:7-105`，repository interface 在 `:107-155`。

runtime launch planner 把 `ManualContextCompactionDelegate` 包到 hook compaction delegate 外层：`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:173-195`。

delegate 在每轮 preflight：

- 查找 requested request；
- next-turn request 如果仍处在原 active turn 则 defer；
- 否则先标记 Consumed，再用 manual params 覆盖 auto policy；
- failed/noop 在 delegate 内直接写终态；
- success 不写 Completed，而由 eventing 在投影事务成功后写。

证据：`crates/agentdash-application-runtime-session/src/session/manual_compaction_delegate.rs:42-80,94-145,148-221`。

**判断：** “请求消费 + compaction lifecycle + success checkpoint”是一个业务状态机，却被分成 command service、runtime delegate、eventing 三个 writer。它没有一个可以通过单一 interface 测试的 module。

### 3.4 auto compaction policy 也在 application-runtime-session

`HookRuntimeDelegate` 实现 `RuntimeCompactionDelegate`：

- 从 provider-visible estimate 和 hook token state 计算触发；
- 内置 `keep_last_n=20`、`reserve_tokens=16384`；
- 执行 `BeforeCompact` / `AfterCompact` hooks；
- 保存连续失败次数并在 fuse limit 后停止自动尝试。

证据：`crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:296-470`。

**判断：** 这不是通用 application 编排，也不是 infrastructure；它是 managed Agent 的 compaction policy。hook 只是 policy 的一个输入 adapter，不应成为整个 auto policy 的所有者。

### 3.5 Agent Core 执行算法，但混入 AgentDash 业务摘要策略

普通 turn 在 provider request 前调用 `run_compaction_preflight`：`crates/agentdash-agent/src/agent_loop/streaming.rs:129-196`。preflight：

- 询问 runtime delegate 是否 compact；
- 校验 eligibility；
- 发送 Started / Noop / Failed；
- 执行 summary-prefix；
- **先替换内存 `context.messages` 与 provider request**；
- 再发送 `ContextCompacted` 和 after callback。

证据：同文件 `:741-907`，尤其内存替换与事件发送顺序在 `:841-864`。

compact-only 是 Core 的独立 loop，不追加 prompt，不执行正常 provider answer：`crates/agentdash-agent/src/agent_loop.rs:210-255`。

summary-prefix 的 cut point、boundary refs、消息 replacement、provider summary request 位于 `crates/agentdash-agent/src/compaction/mod.rs:90-225,355-463`。这些是可复用 Core 算法。

但默认 prompt 强制包含 AgentDash 的 “Lifecycle 文件列表索引 / 原文回看索引”，并解析工具写入语义：同文件 `:355-551`。`agentdash-agent` 还依赖 domain。

**建议归属：** Core 保留通用 compact algorithm 与 loop extension point；默认摘要 prompt、Lifecycle recall index、manual/auto metadata policy 应由业务 Agent 注入，Core 不应知道 AgentDash domain 或 protocol。

### 3.6 executor 同时承担协议 adapter 与业务投影 payload 组装

Pi mapper 把 Core 事件映射为协议事件：

- Started -> Codex `ContextCompaction` item started；
- Noop / Failed -> `PlatformEvent::SessionMetaUpdate` string key + JSON；
- Success -> 先发送 key=`context_compacted` 的 JSON，再发送 item completed。

证据：`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1286-1405`。

外部 Codex 的 `thread/compacted` 则只映射为 `BackboneEvent::ExecutorContextCompacted`：`crates/agentdash-executor/src/connectors/codex_bridge.rs:526-533`。协议注释明确该事件只有 executor telemetry、没有 AgentDash-owned replacement provenance：`crates/agentdash-agent-protocol/src/backbone/event.rs:13-18,55-66`。

**问题：** 内部 Agent 的 mapper 知道 durable projection 所需的 `summary`、boundary refs、trigger、request_id 等业务字段，而外部 Agent 只有 telemetry。二者并未在 executor seam 上实现能力等价；它们只是最终都产生 `BackboneEvent`。

`PlatformEvent::SessionMetaUpdate { key, value }` 是无类型 escape hatch：`crates/agentdash-agent-protocol/src/backbone/platform.rs:21-28`。compaction 成功契约因而由 executor 组 JSON、application eventing 再按 key 解析，编译器无法保护字段完整性。

### 3.7 projection commit 与 manual success 发生在 SessionEventingService

`SessionEventingService::persist_notification_inner` 对普通事件调用 append 后再推进 projection head；对 `context_compacted` 则走特殊 commit：`crates/agentdash-application-runtime-session/src/session/eventing.rs:240-335`。

`maybe_commit_compaction_projection` 位于同文件 `:622-923`，负责：

- 按 string key 识别事件并解析 summary / boundary；
- 读取全量 events 与当前 head；
- 计算 projection version；
- 在 transcript 中解析 MessageRef；
- 构造 compaction record、replacement segments、head；
- 调用 atomic persistence commit；
- commit 成功后把 manual request 标为 Completed，失败则标 Failed。

manual request 成功/失败写回在同文件 `:925-1029`。

**判断：** eventing 已经不只是 event transport。它同时实现 managed Agent 的 checkpoint coordinator、context projector、manual request state machine、read model、title projection、rewind 与 ContextFrame 派生。文件超过 3400 行，是典型低 locality 聚合点，而不是 deep module：调用者仍需理解大量事件 key、顺序和 side effects。

## 4. 会话上下文构造与恢复真实链路

### 4.1 launch path 在两个 crate 中重复定义，而且行为已经不同

`application-agentrun` 自己定义：

- `SessionExecutionState`；
- `SessionRepositoryRehydrateMode`；
- `PromptLaunchPath`；
- `RuntimeTraceLaunchState`；
- `resolve_prompt_launch_path`。

证据：`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:19-116`。

`application-runtime-session` 再定义一套相同概念：`crates/agentdash-application-runtime-session/src/session/types.rs:61-156,264-291`。

二者已经产生语义差异：runtime-session resolver 接收 `LaunchSource`，对 ContextCompaction cold launch 强制 `ExecutorState` restore，即使存在 follow-up metadata：同文件 `:108-156`；agentrun resolver 没有 `LaunchSource` 参数：`runtime_session_boundary.rs:94-116`。

frame construction 使用后者：`crates/agentdash-application/src/frame_construction/mod.rs:212-229`；runtime launch planner 使用前者：`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:108-118`。

**直接后果：** 同一个 compact-only launch，frame construction 可能判成 Plain，runtime planner 却判成 `RepositoryRehydrate(ExecutorState)`。context/owner bundle 是否构建与历史消息是否恢复由两个不同判断控制。

### 4.2 `SystemContext` restore 路径目前只剩类型和注释，缺少实际数据流

runtime planner 只在 `ExecutorState` 下调用 `build_projected_transcript` 并构造 `RestoredSessionState`：`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:197-231`。`SystemContext` 仅映射成 `LaunchRestoreMode::SystemContext`，没有构造历史消息或 continuation bundle。

application 中存在 `build_continuation_bundle_from_markdown`：`crates/agentdash-application/src/context/builder.rs:132-171`，但全仓搜索只有定义与 re-export，没有调用。

frame construction 把两种 rehydrate 都设为 `prebuilt_continuation_bundle: None`；SystemContext 还明确 `include_owner_bundle: false`：`crates/agentdash-application/src/frame_construction/mod.rs:355-370`。owner composer 在该组合下返回 `None` context bundle：`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:247-265`。

**推断（高置信）：** 对不支持 repository restore 的 connector，cold rehydrate 的 SystemContext 分支目前没有把 durable history 交给模型。Pi connector 是仓库中明确支持 repository restore 的实现，其他 connector 返回 false，因此该死路径会影响外部/relay 能力对齐。

### 4.3 ContextProjector 与 transcript restore 是业务 Agent 会话语义

`ContextProjector`：

- 无 head 时从 raw events 构造；
- 有 committed compaction 时读取 replacement checkpoint + raw suffix；
- 支持 at-event、指定 compaction、active head 验证。

证据：`crates/agentdash-application-runtime-session/src/session/context_projector.rs:29-152,164-264`。

`compaction_checkpoint.rs` 定义 checkpoint provenance、projection entries、segment 转换、suffix boundary 和校验：`crates/agentdash-application-runtime-session/src/session/compaction_checkpoint.rs:10-404`。

`transcript_restore.rs` 折叠 Backbone events，重建 user / assistant / tool result 消息及 MessageRef：`crates/agentdash-application-runtime-session/src/session/transcript_restore.rs:99-278,525-596`。

**判断：** 这些不是 application 用例，也不是数据库 adapter。它们共同定义“managed Agent 下一次看到什么上下文”，应成为业务 Agent conversation/context module 的深实现。

### 4.4 上下文构造有 execution path 与 read/query path 两套实现

本地 `context::builder` 定义 ContextBuildPhase、Contribution reducer 和 fragment merge：`crates/agentdash-application/src/context/builder.rs:23-128`；project/story/task 又各自构造 contribution。

真正 launch 的 subject assignment 在 `crates/agentdash-application/src/frame_construction/subject_assignment.rs:42-237,281-299`，owner bundle 在 `frame_construction/owner_bootstrap.rs:230-275,535-653`，lifecycle bundle 在 `frame_construction/request_assembler.rs:544-591`。

但 task read model 在 `crates/agentdash-application/src/task/context_builder.rs:37-186` 又直接读 repositories 重建 context，并把多类读取错误降级成 `None`。它不是 launch 入口，却重复派生 capabilities / VFS / workflow context。

**问题：** UI 查询看到的 context 与真正送进 executor 的 context 没有同一事实源，也没有共享 invariant test。应由业务 Agent 暴露一个 context snapshot/read interface，application 不再自行重建。

### 4.5 Context bundle 的 identity 命名误导所有权

`SessionContextBundle` 保存 `bundle_id` 和 `session_id: Uuid`：`crates/agentdash-spi/src/context/bundle.rs:17-45`。实际构造时多处传 `Uuid::new_v4()`，例如 `frame_construction/owner_bootstrap.rs:575-582` 与 `request_assembler.rs:566-576`，并非 string runtime session id。

audit 又称其为 `bundle_session_uuid`：`crates/agentdash-application-runtime-session/src/context.rs:53-65`。

**判断：** 该字段实际是 build/audit correlation id，不是 session identity。命名会让上下文 bundle 与会话事实源的关系更加模糊；重构时应直接改正，不保留兼容字段。

## 5. 关键一致性与并发问题

### P0：压缩投影持久化失败会被吞掉，turn 仍可继续成功

Agent Core 在发送 `ContextCompacted` 前已经替换内存 context：`agent_loop/streaming.rs:841-858`。Pi mapper 随后产生两个独立 envelope：先 `context_compacted`，再 `ItemCompleted`：`pi_agent/stream_mapper.rs:1374-1405`。

turn processor 对每条 notification 调 `persist_notification`，但用 `let _ = ...` 丢弃所有错误：`crates/agentdash-application-runtime-session/src/session/turn_processor.rs:166-185`。

因此可能发生：

1. 内存上下文已经压缩；
2. `context_compacted` 的 atomic projection commit 失败；
3. eventing 尝试把 manual request 标 Failed；
4. processor 继续处理后续 `ItemCompleted` / terminal event；
5. turn 对外仍可完成；
6. 重启后从旧 durable projection 恢复，与 live context 分叉。

这直接违反 `.trellis/spec/backend/session/context-compaction-projection.md:27-42,57` 规定的“projection checkpoint 必须先于 item completed，commit 失败不能替换 active head”。

**目标不变量：** managed Agent runtime 只有在 durable replacement checkpoint commit 成功后，才能发布 compaction completed 并允许 turn success。事件持久化失败必须成为 turn 失败，而不是 telemetry warning。

### P0：普通事件 append 与 projection head advance 不是一个事务

普通事件路径先 `append_event`，再 `advance_model_projection_head`：`eventing.rs:300-306`。head advance 先 read、内存修改、再 unconditional upsert：同文件 `:1078-1107`。

PostgreSQL adapter 的 `upsert_projection_head` 是无条件覆盖；atomic compaction commit 只覆盖 compaction 特殊路径：`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1109-1128,1152-1261,1662-1703`。

**问题：** append 成功、head update 失败时，durable event 已存在但 active projection head 仍旧；并发 persist 时，较晚完成的旧 read-modify-write 还可能把 head_event_seq 回退。

**建议：** persistence port 提供 `append_conversation_event`，在同一 DB transaction 内以 monotonic SQL 条件推进 active projection head；禁止 application 做 read-modify-upsert。

### P1：manual request repository 没有状态迁移保护

PostgreSQL `update_status` 只按 `id` 更新，没有 expected status/version 条件：`crates/agentdash-infrastructure/src/persistence/postgres/manual_context_compaction_request_repository.rs:23-58`。所有 mark 方法都复用它：同文件 `:161-230`。

migration 只保证同 session 最多一个 `requested`，不限制 `consumed`：`crates/agentdash-infrastructure/migrations/0059_manual_context_compaction_requests.sql:56-67`。

**问题：** concurrent turn 可以竞争消费同一 request；terminal 状态可被后到 writer 改写；一个 consumed request 尚未结束时可创建第二个 requested request。

**建议：** 让业务 Agent aggregate 定义合法 transition，adapter 使用 compare-and-set/version；数据库约束应覆盖“session 同时最多一个 active request（requested/consumed）”。如果最终将 manual compaction 统一为 Agent operation，可直接删表并迁移到通用 operation/command journal，不保留双轨。

### P1：command availability 与真正 fulfillment 已发生语义漂移

conversation snapshot 把除 Running/Cancelling 外的 execution state 都映射为 Ready：`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:656-678`，compact availability 对 Ready 或 active running 开启：同文件 `:681-815`。

但 fulfillment 明确拒绝 Lost：`context_compaction_command.rs:126-132`，也没有在 availability 阶段检查已有 requested/consumed operation。

**问题：** UI 可以显示 enabled，提交后才 Blocked；第二个不同 client command 可能直到数据库 unique conflict 才失败。

**建议：** 由业务 Agent 的 command capability query 同时生成 availability 与 command decision，application 只投影它。

## 6. 持久化模型与协议模型

### 6.1 当前 SPI 实际暴露了业务 Agent 的存储形状

`agentdash-spi/src/session_persistence.rs` 定义：

- `PersistedSessionEvent`：`:556-573`；
- compaction status / record：`:591-714`；
- segment / head；
- atomic commit DTO：`:801-814`；
- event/projection stores：`:826-853,910-945`。

这些 interface 暴露表形 DTO、string `projection_kind` 和 compaction row 结构。它不是 executor 通用 SPI，而是 managed Agent conversation persistence port 泄漏到共享 SPI。

**建议：** persistence interface 由业务 Agent module 拥有，以业务 operation/result 表达；PostgreSQL 是 adapter。数据库 row types 保留在 infrastructure 内部。

### 6.2 compaction status 存在未兑现状态

`SessionCompactionStatus` 表达 Started / ProjectionCommitted / Failed，但仓库搜索只有 projection commit 创建 record；Started / Failed 主要留在事件流，没有对应 record transition。

**判断：** 当前 record 同时假装是 lifecycle aggregate 和 committed checkpoint。目标模型应二选一：

- operation journal 表达 requested/running/noop/failed/succeeded；
- committed checkpoint 表只保存不可变成功结果。

不要继续让一张 checkpoint record 表表达从未写入的 lifecycle 状态。

### 6.3 spec 与现库已经漂移

`.trellis/spec/backend/session/context-compaction-projection.md:9-23` 仍写 PostgreSQL + SQLite 和旧 `session_*` 表名；commit `d500c892b` 已移除 SQLite，当前 migration/table 使用 `runtime_session_*`。repository-pattern 与 backend architecture 的 session 决策也仍残留 SQLite 描述。

**建议：** 目标设计确认后同步更新 spec，只记录新的不变量与原因；不要记录旧实现禁令。

## 7. 状态所有权建议

| 状态/行为 | 当前散落位置 | 目标唯一所有者 | 说明 |
| --- | --- | --- | --- |
| run/agent 授权、workspace command policy、command receipt | API + application-agentrun | Application orchestration | 产品用例与幂等入口 |
| Agent command availability、running/idle/lost 下的 compact 决策 | conversation snapshot + compaction command | Business Agent runtime | 必须与真实 command decision 同源 |
| manual/auto compact policy、failure fuse、hook 决策 | runtime-session delegates | Business Agent runtime | hook 是输入，不是状态所有者 |
| conversation history、MessageRef、projection head、replacement checkpoint | runtime-session eventing/projector/SPI | Business Agent conversation module | 定义下一轮模型可见上下文 |
| transcript fold / restore | runtime-session | Business Agent context module | 对协议事件的业务解释 |
| project/story/task/guideline/memory context composition | application/frame construction | Business Agent context composition | application 只提供 subject/config facts |
| cut point、summary-prefix replacement、provider summary invocation | Agent Core | Agent Core | 注入 summarization strategy，移除 AgentDash prompt/index |
| compact-only generic loop | Agent Core | Agent Core | 作为统一 runtime capability |
| internal Core / external Codex command & event adaptation | executor connectors | Executor infrastructure | 只适配，不决定 checkpoint 语义 |
| atomic event/checkpoint/head persistence | PostgreSQL repository | Infrastructure adapter | 事务、CAS、数据库约束、migration |
| Codex App Server + AgentDash extension wire shapes | agent-protocol | Protocol module | 使用 typed extension，不用 string-key JSON |
| AgentRun journal / product timeline projection | application-agentrun journal | Application read projection | 可以消费统一 runtime event，不拥有 conversation state |

## 8. 建议目标 module 与 seam

### 8.1 Application 只依赖一个窄的 Managed Agent interface

建议用一个高 leverage interface 隐藏当前十余个具体 service/port。命名可在 design 阶段决定，语义至少包含：

- `submit_command(agent_session, command) -> operation receipt`；
- `inspect(agent_session) -> managed session snapshot/capabilities`；
- `subscribe(agent_session, cursor) -> typed runtime events`；
- `read_context(agent_session, revision?) -> context snapshot`。

`CompactContext` 是 command 变体，不再由 API 构造 special runtime port。application 不知道 next-turn request 表、maintenance prompt、restore mode 或 750ms polling。

### 8.2 Business Agent module 做深

建议把以下实现收敛到同一业务 Agent module 内部：

- managed session aggregate / operation state machine；
- context source composition 与 delivery plan；
- conversation transcript + typed MessageRef；
- manual/auto compaction coordinator；
- replacement checkpoint / projection / restore；
- hook policy composition；
- internal/external runtime capability negotiation。

这些可以有 private internal seams，但不应把 repository rows、hook delegate、projector、request repository 逐个暴露给 application。

### 8.3 Agent Core 保持纯执行核心

Core 的公开 interface 只理解：context、messages、tools、provider bridge、runtime delegates/capabilities、typed Agent events。它不理解 Lifecycle、AgentRun、数据库、Codex wire payload 或项目 context fragments。

compaction strategy 通过注入 interface 提供摘要 prompt/index；Core 只执行通用 eligibility、cut、summary call 与 replacement result。

### 8.4 Executor 是双实现 adapter seam

这里确实存在两个以上 adapter，seam 是真实的：

- Internal adapter：统一 Agent Runtime command -> Agent Core / Pi agent；
- External adapter：统一 Agent Runtime command -> Codex App Server protocol；
- 测试 adapter：in-memory scripted agent。

capability model 需要明确区分：

- executor-native compact telemetry；
- platform-managed durable compact；
- repository state restore；
- system-context restore；
- branch/fork、steer、cancel、approval、usage 等。

外部 agent 若只发 `thread/compacted`，只能声明 native telemetry capability；若要参与 AgentDash durable replacement，需要扩展协议返回 summary、boundary、replacement provenance，或由平台持有 canonical transcript 后执行 managed compaction。不能把两者伪装成同一成功语义。

### 8.5 Protocol 使用 typed extension event

至少把下列 string-key payload 改为 typed variants：

- `ContextCompactionStarted`；
- `ContextCompactionNoop`；
- `ContextCompactionFailed`；
- `ContextProjectionCommitted`（携带 summary、boundaries、revision、operation id）；
- `ExecutorContextCompacted`（明确 telemetry）。

executor 只翻译 source event；业务 Agent coordinator 决定何时产生 `ContextProjectionCommitted` 和 item completed。

## 9. 直接替换式重构顺序

用户已明确不需要兼容/回退，因此建议按 invariant slice 直接替换，而不是保留双轨：

1. **先修正成功边界**：让 event persistence error 终止 turn；把 `projection commit -> completed event` 收进单一 coordinator/transaction contract；为失败注入写集成测试。
2. **统一 launch classification**：删除两套 `PromptLaunchPath` / resolver，单一 command-aware decision；补齐或删除 SystemContext restore，不能保留死枚举。
3. **引入 typed compaction lifecycle**：删除 executor 组装 `SessionMetaUpdate` JSON 与 eventing 反解析；eventing 不再识别 string key。
4. **建立 Business Agent conversation module**：迁入 projector、checkpoint、transcript restore、manual/auto policy；将 persistence ports 从 shared SPI 迁入该 module。
5. **建立统一 executor capability seam**：internal/external adapter 实现同一 interface；明确 native telemetry 与 managed durable compact 的不同 capability。
6. **收敛 context construction**：execution/query 共用同一个 context snapshot builder；删除未调用 continuation helper 与重复 task read builder。
7. **瘦身 application**：API 只提交 Agent command；AgentRun journal 只做产品 timeline 投影；删除 concrete runtime-session compaction adapter、Todo port 与 750ms poll。
8. **纯化 Agent Core**：移除 domain dependency；把 Lifecycle summary/index 策略注入业务 Agent。
9. **数据库 migration**：按最终 operation/checkpoint 模型重建 `runtime_session_*` 表与约束；项目未上线，可直接迁移并删除旧表/字段，不建兼容 view 或双写。

每一步完成条件都应是旧入口/旧类型已删除，并通过新 module interface 的行为测试；不保留“暂时继续调用旧 service”的 wrapper 链。

## 10. 关键测试现状与缺口

已有重要覆盖：

- Core eligibility / summary replacement：`crates/agentdash-agent/src/compaction/mod.rs:681-1027`；
- compact-only empty context：`crates/agentdash-agent/src/agent_loop.rs:605-643`；
- manual command running/idle/noop/completed/failed/duplicate/starting：`context_compaction_command.rs:929-1157`；
- projector active checkpoint + suffix：`context_projector.rs:669-764`；
- compaction projection validation/manual completion/external telemetry：`eventing.rs:2350-2654`；
- PostgreSQL atomic checkpoint/segment/head：`session_repository.rs:2049-2090`；
- Pi lifecycle mapping：`crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:560-742`。

目标重构前必须新增或提升到新 interface 的测试：

1. projection commit 注入失败后，不发布 completed、不完成 turn、不替换 durable head；
2. append event 与 active head 在失败/并发下保持单调原子；
3. manual operation 在并发 turn 下只消费一次，terminal 状态不可改写；
4. compact-only 与普通 cold resume 使用同一个 launch decision；
5. 每种 executor capability 下，restart 后模型可见 context 与 commit 前后一致；
6. context query snapshot 与真正 delivery frames 来自同一 revision；
7. internal/external adapter 对统一 command/event contract 做 contract tests，同时明确不支持 capability 的 typed result。

旧浅模块 unit tests 应在新 deep module interface tests 建立后删除；不要在旧 tests 上再叠一层 wrapper tests。

## 11. 历史演化解释

相关提交显示当前散布是渐进叠加的结果，而不是一次有意设计的 stable seam：

- `74ab99473`：初始 compaction policy 横跨 agent/application/hooks/executor；
- `596a8d1dc`：加入 Codex lifecycle mapping；
- `cdc2ef059`：provider-visible trigger；
- `c605c452f`：projection、branching、MessageRef、atomic commit；
- `d09eb1e0b`：native message summarization；
- `fe18290d4`：manual compaction 一次扩散到 58 个文件；
- `fafe213bf`：修复 manual lifecycle；
- `94705c692`：物理拆分 application crates，但大量语义原样移动；
- `62f507046`：frame construction 又回到 umbrella application；
- `d500c892b`：移除 SQLite，spec 未完全同步。

归档任务 `.trellis/tasks/archive/2026-07/07-09-manual-context-compaction-execution/` 的设计重点是修复 compact-only 无 durable restore，并明确“managed operation + durable replacement baseline 才是成功边界”。它解决了行为缺口，但没有重划 module seam；本次架构收敛应继承这个成功边界，而不是继承当时为快速接通形成的文件分布。

## 12. 最终判断

现有 implementation 中值得保留的是业务知识与经过验证的算法：MessageRef boundary、replacement checkpoint、raw suffix restore、manual next-turn/compact-only 语义、external compact telemetry 区分、PostgreSQL atomic compaction commit、Core summary-prefix 算法。

不值得保留的是当前 interface 形状：重复 launch types、concrete runtime-session 引用、string-key compaction payload、SessionEventing god module、分散的 request writers、query/execution 双 context builder、Todo fallback 与 polling outcome。

最合理的收敛方向不是把所有 compaction 文件简单挪进 `infra`，而是创建一个真正深的 **Business Agent Runtime / Conversation module**，让它独占 managed session、context 和 compaction 不变量；application、executor、Agent Core、protocol、PostgreSQL 分别围绕清晰 seam 提供编排、适配、纯执行、传输类型和原子存储。
