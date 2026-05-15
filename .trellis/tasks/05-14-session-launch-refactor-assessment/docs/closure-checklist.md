# Closure Checklist：最终收口点

## 目的

这份清单用于避免重构进入“差不多完成但尾巴很多”的状态。每个收口点都是最终完成前必须明确确认的架构不变量。

## A. 入口与 Launch 收口

- [ ] 所有生产启动来源只构造 `LaunchCommand`，不直接构造最终 `PromptSessionRequest`。
- [ ] 所有启动来源进入同一条 `LaunchCommand -> SessionConstructionPlan -> LaunchExecution` 数据流。
- [ ] `SessionConstructionPlan` 是唯一解析 owner/workspace/VFS/MCP/capability/executor/context/identity 的位置。
- [ ] `LaunchExecution` 是唯一解析 lifecycle/restore/hook/follow-up/runtime-command/terminal-effect/connector-input launch 策略的位置。
- [ ] `LaunchResolution`、`ExecutionPlan`、`ExecutionProjector` 不作为最终态必需传递层保留。
- [ ] connector prompt 前不再通过 mutable request 补字段。
- [ ] Construction trace 与 launch trace 覆盖所有 fallback source，并能在测试或日志中查看。
- [ ] `SessionLaunchIntent` 要么被吸收到 `LaunchCommand`/`LaunchSourcePolicy`，要么最终删除。
- [ ] `PromptSessionRequest` 从最终生产主链路删除。

## B. Session Construction 与 Context 收口

- [ ] `SessionConstructionPlan` 是 context/VFS/capability/MCP/identity/executor profile 的单一组装结果。
- [ ] `PromptSessionRequest` 不再是内部半成品 plan，也不作为长期 wire DTO 保留。
- [ ] project/story/session context endpoint 都投影 `SessionConstructionPlan`。
- [ ] route-local `finalize_augmented_request` 删除。
- [ ] `SessionContextBundle`、`SessionPlan`、`SessionContextSnapshot` 的关系明确：`SessionConstructionPlan` 是主数据，其余是投影或删除对象。
- [ ] launch 与 context endpoint 的 VFS/capability/context/executor 摘要有一致性测试。

## C. Ownership 收口

- [ ] owner/binding 解析只能通过 `SessionOwnerResolver`。
- [ ] launch、context query、权限展示使用同一个 `ResolvedSessionOwner`。
- [ ] `SessionOwnerCtx` 是 owner 语义的权威表达。
- [ ] owner bootstrap phase 不再和 hook reload/restore plan 混用。
- [ ] owner bootstrap commit 不早于 connector prompt accepted。

## D. Runtime 收口

- [ ] `SessionRuntimeRegistry` 负责 reserve/activate/release/cancel。
- [ ] 并发 prompt 只能有一个 turn reservation 成功。
- [ ] `TurnSupervisor` 持有 adapter task、processor task、cancel token 或等价监督结构。
- [ ] cancel/stall 不再直接扫/改 `SessionHub.sessions` 内部字段。
- [ ] connector live session 与 app turn lifecycle 命名区分清楚。

## E. Eventing / Persistence 收口

- [ ] session event append、broadcast、projection 写入通过 `SessionEventWriter` 或等价单点完成。
- [ ] projection 字段不被普通 meta save 回退。
- [ ] `SessionPersistence` 长期拆分或明确分层为 meta/event/projection store。
- [ ] terminal event 先持久化，再执行 effect。
- [ ] terminal effect 进入 durable outbox 或等价持久化派发结构，具备重试、失败记录和审计。
- [ ] connector prompt 失败路径有事件和状态测试。

## F. Hooks / Effects 收口

- [ ] `HookLaunchPlan` 由 `LaunchExecution` 输出，表达 reload/refresh/none 及原因。
- [ ] `SessionTurnProcessor` 不直接调用 workflow lifecycle callback。
- [ ] `SessionTurnProcessor` 不直接调用 hook auto-resume。
- [ ] task post-turn、workflow lifecycle、hook auto-resume、companion parent resume 都通过 `TerminalEffectRouter` 或等价 effect dispatch。
- [ ] effect 失败不会破坏 terminal event 的事实性。
- [ ] effect dispatcher 不依赖内存即时回调作为唯一执行路径。

## G. Pending Runtime Command 收口

- [ ] pending capability/runtime transition 不再藏在 `SessionMeta` 普通字段。
- [ ] pending command 有显式 store/table 或等价持久化结构。
- [ ] pending command apply once，有 applied/failed audit。
- [ ] connector prompt 失败不会永久丢 pending command。
- [ ] 数据库 migration 覆盖 PostgreSQL 与 SQLite。

## H. API / Adapter 收口

- [ ] API route 只做 auth、DTO 转换、调用 use case。
- [ ] Task/Workflow/Routine/Companion/Local relay 只做 domain 输入整理和 adapter command 构造。
- [ ] 新 source adapter 不需要改 planner 主流程。
- [ ] AppState 初始化有 ready-state 或 builder 约束，避免必要依赖延迟注入后仍可为空。

## I. 安全与路径收口

- [ ] `working_dir` 只接受 mount root 内规范化相对路径。
- [ ] 拒绝绝对路径、`..`、Windows separator 绕过、空 segment。
- [ ] VFS materialization 与 local relay path policy 不产生第二套路径规则。
- [ ] shell/MCP/local relay 路径策略有独立测试。

## J. 最终删除/退化清单

- [ ] 生产路径不再调用 `SessionHub::launch_*prompt` wrapper 作为业务主入口。
- [ ] `SessionHub` 删除，或仅在短期迁移窗口作为无业务逻辑 wrapper 存在。
- [ ] `start_prompt_with_follow_up` 被拆成 plan + execute，最终不承载业务主流程。
- [ ] `augment_prompt_request_for_owner` 不再位于 API route，最终被 construction/adapters 边界吸收或删除。
- [ ] `finalize_augmented_request` 删除。
- [ ] `pending_capability_state_transitions` 从 `SessionMeta` 删除。
- [ ] `terminal_callback` 从 `SessionHub` 中删除或退化为 effect handler 注册。

## K. 验证矩阵收口

最终必须覆盖：

- [ ] HTTP prompt 首轮 owner bootstrap；
- [ ] HTTP prompt 普通 continue；
- [ ] hook auto-resume；
- [ ] Task start/continue；
- [ ] Workflow AgentNode launch；
- [ ] Routine reuse/new/per-entity strategy；
- [ ] Companion dispatch；
- [ ] Companion parent resume；
- [ ] Local relay follow-up；
- [ ] repository restore / system context restore；
- [ ] pending runtime command apply once；
- [ ] connector prompt failure；
- [ ] concurrent prompt；
- [ ] cancel / interrupted；
- [ ] working_dir invalid inputs；
- [ ] context endpoint 与 launch plan 一致性。

## L. 最终人工确认点

这些点需要架构评审时明确拍板。已确认：单一 owner、`PromptSessionRequest` 终态删除、`SessionHub` 终态不保留业务 facade、launch/context 同源、terminal event + durable outbox。

- [ ] pending command store 是否使用新表，表结构如何命名？
- [ ] pending runtime command 是否采用纯事件 replay；若不采用，专表/outbox 的 schema 和 migrate 如何设计？
- [ ] terminal effect outbox 与 session event store 是否共享事务边界？
- [ ] `SessionConstructionPlan` / `LaunchExecution` 字段边界是否已完成业务评审？
- [ ] connector input 是否保持为 `LaunchExecution` 内部字段，而不是独立主链路层？
