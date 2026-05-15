# Current To Target Migration：从现状迁移到目标态

## 结论

目标路径不是再包一层 `LaunchService`，也不是把现有 `PromptSessionRequest` 改名。可行迁移必须按下面顺序收口：

```text
current source-specific request assembly
  -> SessionConstructionPlan becomes the shared construction fact
  -> LaunchExecution becomes the only per-launch execution plan
  -> Runtime / Eventing / Effects / Pending split away from SessionHub
  -> PromptSessionRequest and business SessionHub responsibilities are deleted
```

目标主链路保持精简：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

其中 `LaunchCommand` 只表达来源意图，`SessionConstructionPlan` 承载可查询、可审计、可复用的 session 构建事实，`LaunchExecution` 承载一次 launch 的短生命周期执行策略。connector input 是 `LaunchExecution` 内部字段组，`ExecutionContext` 只在 connector 边界投影。

## 当前事实链路

| 环节 | 当前事实 | 代码证据 | 与目标态的背离 |
|---|---|---|---|
| 用户输入 | `UserPromptInput` 已经是纯用户输入 | `crates/agentdash-application/src/session/types.rs:12` | 这是可复用迁移基础，无需重写 |
| 半成品请求 | `PromptSessionRequest` 包含 user input、MCP、VFS、capability、context、hook trigger、identity、post-turn handler | `crates/agentdash-application/src/session/types.rs:30` | 把 command、construction、launch、effects 语义混在一个可变壳里 |
| 来源意图 | `SessionLaunchIntent` 只表达 source / strictness / preparation / follow-up | `crates/agentdash-application/src/session/launch_intent.rs:35` | 只是过渡分发策略，不承载构建事实或 launch 策略 |
| 组装中间产物 | `PreparedSessionInputs` 是 compose 输出，仍要合并进 `PromptSessionRequest` | `crates/agentdash-application/src/session/assembler.rs:94` | 构建事实没有独立事实源，只是 request mutation 的素材 |
| finalizer | `finalize_request` 合并 prepared 与 base request | `crates/agentdash-application/src/session/assembler.rs:141` | finalizer 是半成品请求时代的胶水，不是目标边界 |
| HTTP augment | API route 内有 owner 选择、lifecycle 判断、context/VFS/capability 组装 | `crates/agentdash-api/src/routes/acp_sessions.rs:950` | route 层持有 application 业务，且和 context 查询分裂 |
| route finalizer | HTTP route 还有一套 `finalize_augmented_request` | `crates/agentdash-api/src/routes/acp_sessions.rs:1080` | 与 assembler finalizer 并存，合并语义可能漂移 |
| context query | `/sessions/{id}/context` 在 route 层按 binding 分支重建 context | `crates/agentdash-api/src/routes/acp_sessions.rs:576` | context endpoint 没有投影同一份 construction fact |
| owner priority | context query 按 project -> story -> task 选 primary binding | `crates/agentdash-api/src/routes/acp_sessions.rs:748` | launch augment 当前按 task -> story -> project，owner 语义分裂 |
| hub launch wrapper | 所有来源最终走 `launch_prompt_with_intent` 再进入旧 pipeline | `crates/agentdash-application/src/session/hub/facade.rs:413` | 已经有收敛入口，但仍传 `PromptSessionRequest`，不能阻止外围预组装 |
| 核心 pipeline | `start_prompt_with_follow_up` 同时做 payload、turn claim、meta、pending、VFS/MCP/capability fallback、hook/restore、ExecutionContext、connector、processor | `crates/agentdash-application/src/session/prompt_pipeline.rs:30` | 规划、校验、状态变更、副作用、运行态监督混在一起 |
| pending command | pending capability transition 藏在 `SessionMeta` 并由下一轮 prompt `take` 消费 | `crates/agentdash-application/src/session/types.rs:293`, `crates/agentdash-application/src/session/prompt_pipeline.rs:75` | hidden queue，不可审计、难恢复、apply-once 语义不清 |
| terminal effects | processor 直接执行 hook effects、post-turn handler、terminal callback、auto-resume | `crates/agentdash-application/src/session/turn_processor.rs:170`, `crates/agentdash-application/src/session/turn_processor.rs:199`, `crates/agentdash-application/src/session/turn_processor.rs:207` | terminal fact 与副作用派发耦合，缺 durable outbox |
| AppState 注入 | `SessionHub` 先构造，再延迟注入 terminal callback / prompt augmenter | `crates/agentdash-api/src/app_state.rs:362`, `crates/agentdash-api/src/app_state.rs:483` | ready 状态靠运行时约定，不是类型约束 |
| path policy | `resolve_working_dir` 仍直接 `mount_root.join(rel)` | `crates/agentdash-application/src/session/path_policy.rs:7` | working dir 没有类型化、规范化、拒绝 `..`/绝对路径 |

## 迁移原则

- 不先引入空壳 `SessionLaunchService`。如果它只是把 `PromptSessionRequest` 原样传给 `SessionHub`，就是多一层噪音。
- 先做 construction 事实源，再迁移 launch。否则 launch 仍会在执行阶段临时 fallback owner/VFS/MCP/capability/context。
- context endpoint 必须尽早接到 `SessionConstructionPlan` 投影。否则 launch 与 query 仍然两路重建。
- `LaunchExecution` 不能复制一份长期 session fact。它只引用或消费同一次 `SessionConstructionPlan`，并承载本轮 launch 策略。
- 每个迁移切片都要删除一个旧分叉或旧可变壳；只新增 wrapper、不减少旧路径的切片不算完成。

## 阶段 0：当前行为锁定与风险护栏

目标：先把现有分叉行为记录成可验证矩阵，避免重构时靠感觉迁移。

要锁定的现状：

- HTTP / Task / Workflow / Routine / Companion dispatch / Companion parent resume / Hook auto-resume / Local relay 的入口输入字段。
- owner priority 的现状差异：launch augment 是 Task -> Story -> Project；context query 是 Project -> Story -> Task。
- VFS / MCP / capability / executor / working_dir / follow-up / hook reload / repository restore 的 fallback 来源。
- connector.prompt 失败时 runtime、meta、bootstrap、pending command 的现状。
- terminal effects 的实际触发顺序。

退出条件：

- 文档中每个现状背离点都有代码证据。
- 后续实现任务有最小回归矩阵，不依赖人工记忆。

## 阶段 1：Owner 与 SessionConstructionPlan

目标：先形成唯一 session 构建事实源，收掉 launch/context 双路径。

迁移动作：

1. 定义 `ResolvedSessionOwner` 与 owner 解析规则，统一 launch、context query、权限展示使用的 owner 语义。
2. 定义 `SessionConstructionPlan`，字段覆盖 owner、source contract、workspace、typed working dir、VFS、MCP、capability、executor profile、context plan、identity、query/audit projection、trace。
3. 把 `SessionAssemblyBuilder` / `PreparedSessionInputs` 中真正有价值的 composer 逻辑迁入 construction builder。
4. 把 `/sessions/{id}/context`、project/story/task context response 改为投影 `SessionConstructionPlan`。
5. 删除 route-local context/VFS/capability 重建路径，删除 `finalize_augmented_request`。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| API route 参与 owner/context/VFS/capability 组装 | route 只做 auth、DTO、调用 use case |
| owner priority 分裂 | 单一 owner resolver |
| context query 与 launch 各自组装 | 都投影 `SessionConstructionPlan` |
| `PreparedSessionInputs` 只是 request 合并素材 | construction plan 成为事实源 |

不能提前删除的旧物：

- `PromptSessionRequest` 此时可暂时存在于旧 pipeline 边界，但不应继续向新入口扩散。
- `SessionAssemblyBuilder` 可作为 construction builder 的迁移素材，但不作为最终边界保留。

退出条件：

- context endpoint 与 launch construction 使用同一 builder。
- route 层不再有 owner/task/story/project 分支组装 request 的主线。
- construction trace 能解释 VFS/MCP/capability/executor/context 的来源。

## 阶段 2：LaunchExecution 从 pipeline 中抽出

目标：把 `start_prompt_with_follow_up` 中的 launch 策略解析移出执行阶段。

迁移动作：

1. 定义 `LaunchExecution`，消费 `LaunchCommand + SessionConstructionPlan + runtime facts`。
2. 从 pipeline 外移 prompt payload、lifecycle、restore、hook reload/refresh、follow-up、pending runtime command、terminal effect plan、connector input。
3. connector input 保持为 `LaunchExecution` 字段组，不先抽成独立主链路 DTO。
4. `ExecutionContext` 只在 connector boundary 由 `LaunchExecution` 投影。
5. `start_prompt_with_follow_up` 改名或拆为 `execute_launch(execution)`，只保留 reservation、event write、connector prompt、supervision 的副作用。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| pipeline 内临时 fallback VFS/MCP/capability/executor | construction/launch trace 前置解析 |
| pipeline 内判断 owner bootstrap / repository rehydrate / hook reload | `LaunchExecution` lifecycle / restore / hook plan |
| pending transitions 在 prompt 开始时隐式 `take` | `LaunchExecution` 只生成 apply plan，不直接把 meta 当队列 |
| connector input 只能执行到中段才知道 | connector input 在 connector.prompt 前完整可打印、可测 |

退出条件：

- connector.prompt 前可以输出完整 launch summary。
- 执行函数不再读取 request/meta/profile 做多级策略 fallback。
- connector.prompt 失败不会提前 commit owner bootstrap。

## 阶段 3：入口 Adapter 与 PromptSessionRequest 删除

目标：所有来源只构造 `LaunchCommand`，不再构造半成品 request。

迁移动作：

1. HTTP prompt：`UserPromptInput + identity` -> `LaunchCommand::http_prompt`。
2. Task：task domain 只提供 task step launch spec、post-turn effect intent；不直接塞 handler 到 request。
3. Workflow AgentNode：orchestrator 只提供 lifecycle node launch spec；terminal callback 改走 effects。
4. Routine：routine session strategy 保留在 routine domain，但 launch 输入变成 source contract，不再组装 request。
5. Companion：dispatch / parent resume 变成 companion source contract；child context 写入纳入 launch/effect 事务边界。
6. Hook auto-resume：不再构造 bare `PromptSessionRequest` 后 strict augment，而是构造 `LaunchCommand::hook_auto_resume`。
7. Local relay：只传 local workspace source、MCP override、follow-up，不直接构造 VFS request。

背离点收口：

| 来源 | 当前入口背离 | 目标入口 |
|---|---|---|
| HTTP | `PromptSessionRequest::from_user_input` 后交 hub strict augment | `LaunchCommand::http_prompt` |
| Task | `compose_story_step -> finalize_request -> launch_task_prompt` | `LaunchCommand::task_step` |
| Workflow | `compose_lifecycle_node_with_audit -> finalize_request -> launch_workflow_prompt` | `LaunchCommand::workflow_node` |
| Routine | routine 内部 build `PromptSessionRequest` + lifecycle 映射 | `LaunchCommand::routine` |
| Companion dispatch | companion compose 后 finalize request | `LaunchCommand::companion_dispatch` |
| Companion parent resume | bare request + strict augment | `LaunchCommand::companion_parent_resume` |
| Hook auto-resume | bare request + strict augment | `LaunchCommand::hook_auto_resume` |
| Local relay | request 直接带 VFS/MCP/follow-up | `LaunchCommand::local_relay` |

退出条件：

- 生产主链路不再出现 `PromptSessionRequest`。
- `SessionLaunchIntent` 被吸收到 `LaunchCommand` / source policy 或删除。
- `launch_*prompt` wrapper 不再承载业务分发。

## 阶段 4：Runtime Registry 与 TurnSupervisor

目标：把运行态从 `SessionHub.sessions` 和 processor 私有状态中拆出。

迁移动作：

1. `SessionRuntimeRegistry` 负责 reserve、activate、release、cancel、active turn 查询。
2. `TurnSupervisor` 持有 adapter task、processor task、cancel token、stall tracking。
3. connector live executor session 与 app active turn 分开命名：`has_live_executor_session` / `has_active_turn`。
4. cancel/stall/delete 不再扫 `SessionHub.sessions` 内部字段。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| `SessionHub.sessions` 同时是 runtime cache、hook store、turn state | runtime registry / hook runtime store / supervisor 分离 |
| `start_prompt_with_follow_up` 直接 claim/activate | launch executor 使用 registry API |
| processor 私有 task 生命周期不可统一监督 | TurnSupervisor 单点持有后台任务 |

退出条件：

- 并发 prompt 只可能一个 reservation 成功。
- cancel/interrupted/stall 有一致事件与释放路径。
- `SessionHub` 不再是 runtime 内部状态入口。

## 阶段 5：Terminal Event + Durable Outbox

目标：终态事实先落库，副作用持久化派发。

迁移动作：

1. processor 只把 stream terminal 转成 terminal event。
2. terminal event 与 outbox enqueue 在明确事务边界内完成。
3. task post-turn、workflow lifecycle、hook auto-resume、companion parent resume 都变成 typed terminal effect。
4. effect handler 使用 idempotency key，有限 retry，dead-letter 可审计。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| processor 直接执行 post-turn handler | typed effect + outbox handler |
| processor 直接读 `terminal_callback` | workflow lifecycle effect handler |
| processor 直接触发 hook auto-resume | hook auto-resume effect handler |
| effect 失败和 terminal fact 混在内存流程 | terminal fact 不受 effect 失败影响 |

退出条件：

- terminal event 持久化后，即使进程重启也能重放未完成 effects。
- processor 内没有业务 effect dispatch。
- effect 失败不会回滚或污染 session terminal fact。

## 阶段 6：Pending Runtime Command 事件化

目标：删除 `SessionMeta.pending_capability_state_transitions` hidden queue。

迁移动作：

1. 定义 `RuntimeCommandRequested / RuntimeCommandApplied / RuntimeCommandFailed`。
2. 建立可重建 projection，用于 construction/launch 查询待应用命令。
3. `LaunchExecution` 生成 apply plan；真正 applied/failed 由 event 记录。
4. PostgreSQL 与 SQLite migration 删除或迁移旧 meta 字段。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| pending command 是 meta 普通字段 | domain event 是事实源 |
| prompt 开始时 `take` 并靠 save meta 清空 | apply-once 有 requested/applied/failed audit |
| connector.prompt 失败可能造成命令语义不清 | 失败路径记录 failed 或保持 requested 可重试 |

退出条件：

- no live turn 的 runtime command 可审计。
- apply-once 有并发测试。
- migration 覆盖 PostgreSQL 与 SQLite。

## 阶段 7：SessionHub、Persistence、AppState 最终收口

目标：删除 `SessionHub` 的业务 facade 职责，完成持久化和初始化边界。

迁移动作：

1. `SessionHub` 职责拆到 core / ownership / construction / launch / runtime / eventing / hooks / effects / pending / adapters。
2. `SessionPersistence` 语义拆成 meta store、event store、projection store、outbox store、runtime command projection。
3. AppState 使用 ready builder，只有必要依赖完成后才暴露服务。
4. working dir 类型化，拒绝绝对路径、`..`、Windows separator 绕过、空 segment。

背离点收口：

| 当前背离 | 目标收口 |
|---|---|
| `SessionHub` 是服务定位器 | 能力服务按职责内聚 |
| persistence 同时做 meta/event/page/list | store 语义拆清 |
| 延迟注入靠运行时约定 | ready builder 类型约束 |
| working dir 裸字符串 join | `SessionWorkingDir` / normalized relative path |

退出条件：

- `SessionHub` 删除；如果某个中间提交仍保留 public shell，它必须无业务逻辑，并且该阶段仍不能作为最终完成。
- AppState 构造后不存在必需依赖为空的服务。
- working dir 安全测试覆盖本地/云端路径策略。

## 最小验证矩阵

每个阶段完成后至少回归：

- HTTP prompt 首轮 owner bootstrap；
- HTTP prompt 普通 continue；
- project/story/task context endpoint 与 launch construction 一致；
- Task start/continue；
- Workflow AgentNode launch 与 successor activation；
- Routine reuse/new/per-entity strategy；
- Companion dispatch 与 parent resume；
- Hook auto-resume；
- Local relay follow-up；
- repository restore：executor state 与 system context 两种路径；
- pending runtime command apply-once 与失败恢复；
- connector.prompt failure；
- concurrent prompt；
- cancel/interrupted/stall；
- working_dir invalid inputs；
- terminal effect retry/dead-letter/idempotency。

## 仍需最终拍板的点

- `SessionConstructionPlan` 字段是否允许持有完整 `SessionContextBundle`，还是只持有 `ContextPlan + connector projection`。
- `LaunchExecution.construction` 在实现中是共享引用、轻量 snapshot，还是只持有 construction id/trace。
- pending runtime command 是纯事件 replay，还是事件 + projection 表；当前建议事件为事实源，projection 仅查询。
- terminal outbox 与 session event append 是否共享事务；当前建议 terminal event 与 outbox enqueue 在同一持久化边界内完成。
- `SessionHub` public shell 的最大生命周期与删除条件；最终态已确认不保留有职责 facade。
