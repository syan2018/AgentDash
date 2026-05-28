# Session Startup Pipeline

本 appendix 定义 session 构建与 prompt launch 的生产主线。模块不变量见 [Session Architecture](./architecture.md)。

## Pipeline

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchPlan
  -> PreparedTurn
  -> ConnectorAcceptedTurn
  -> CommittedTurn
  -> AttachedTurn
```

`LaunchCommand` 表达来源意图；`SessionConstructionPlan` 是构建事实源；`LaunchPlan` 是单轮启动决策；后续 stage types 表达 accepted 前准备、connector accepted、accepted 后 commit 与 stream attach。`ExecutionContext` 只在 connector 边界投影。

## Stage Responsibilities

| 阶段 | 输入 | 输出 | 职责 |
| --- | --- | --- | --- |
| Source adapter | HTTP / Task / Workflow / Routine / Companion / Hook / Local relay 请求 | `LaunchCommand` | 保留来源身份、请求意图、source policy、prompt payload、executor override、follow-up hint |
| Construction | `LaunchCommand` + session/domain/runtime facts | `SessionConstructionPlan` | 解析 owner、workspace、working dir、VFS、MCP、capability、context bundle/frame、identity、query/audit/inspector projection、resolution trace |
| Launch planning | `LaunchCommand` + `SessionConstructionPlan` + runtime facts | `LaunchPlan` | 解析 resolved prompt payload、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input |
| Turn preparation | `LaunchPlan` | `PreparedTurn` | claim/activate turn，准备 runtime tools、MCP tools、hook runtime、context frames、pending runtime context application 与 connector `ExecutionContext` |
| Connector start | `PreparedTurn` | `ConnectorAcceptedTurn` | 调用 `connector.prompt`，以返回 `ExecutionStream` 作为 accepted 边界；setup 失败时释放 turn/hook 并记录 failed terminal |
| Accepted commit | `ConnectorAcceptedTurn` | `CommittedTurn` | 提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation |
| Stream ingestion | `CommittedTurn` | `AttachedTurn` | spawn `SessionTurnProcessor` 与 stream adapter，并登记 processor tx / adapter abort handle |
| Terminal | connector terminal / stream terminal | terminal event + outbox effect | 持久化终态，清理 active turn，把业务副作用写入 durable outbox |

`Turn` 边界保持很薄：reservation、active、cancel、hook runtime handle、processor/adapter supervision、terminal release。

## Source Adapter Contract

Source adapter 只做来源语义转换，不能预先组装最终运行事实。

| 来源 | `LaunchCommand` 应携带 |
| --- | --- |
| HTTP prompt | request DTO、auth identity、prompt payload、executor override |
| Task service | task id、phase/override/additional prompt source hint、task source identity |
| Workflow orchestrator | workflow/lifecycle source identity、activity activation intent |
| Routine executor | routine source identity，系统身份来自 `AuthIdentity::system_routine(routine.id)` |
| Companion dispatch / parent resume | parent session id、dispatch/slice/target binding/source policy |
| Hook auto-resume | hook trigger identity、resume intent、follow-up hint |
| Local relay | workspace root、原始 MCP declaration、relay source identity |

`working_dir` 是 construction 解析结果，不属于用户 prompt input。resolved VFS、resolved MCP、capability state、context bundle 和 connector input 都由 construction/launch 产出。

Task terminal effect 使用 durable binding 描述，由 construction/effects 解析。command 边界不传内存 `post_turn_handler` 或其它 trait object。

## Construction Contract

`SessionConstructionProvider::build_construction` 直接输出 launch-ready `SessionConstructionPlan`，不是 seed、partial plan 或等待 planner 补齐的中间形态。

`SessionConstructionPlan` 至少覆盖：

- `ResolvedSessionOwner`，owner 解析顺序统一为 `Task -> Story -> Project`。
- workspace 与 typed working directory。`workspace.working_directory` 必须在进入 launch planner 前为 `Some`。
- final VFS、MCP declaration resolution、capability state。
- `SessionContextBundle` 与 continuation/context frames。
- identity、source contract、query/audit/inspector projections。
- resolution trace，用于审计为什么选择某个 owner/workspace/context。

Launch 前必须调用 `SessionConstructionPlan::validate_for_launch()` 或等价 gate：

- 缺少 `workspace.working_directory`、`execution_profile.executor_config`、`surface.vfs`、`projections.capability_state` 时拒绝 launch。
- `projections.capability_state.vfs.active` 必须等于 `surface.vfs`。
- `projections.capability_state.tool.mcp_servers` 必须等于 `projections.mcp_servers`。
- pending runtime command 的 overlay 由 Construction 阶段形成 final capability projection；`requested -> applied` 副作用只能在 connector prompt accepted 后提交。

Construction 可以消费 runtime facts，但这些 facts 一旦进入 `SessionConstructionPlan` 就必须体现在 `resolution` trace 中。LaunchPlanner 不允许再读取 cached profile、hub default VFS、local relay workspace root 或 source MCP declaration 来补齐 VFS/MCP/capability/executor facts。

Context endpoint、权限展示、audit 和 inspector 都投影同一份 `SessionConstructionPlan`。API route 的职责是 auth/permission、DTO 转换、调用 use case、映射 response DTO。

## Capability Projection Normalization

Session runtime surface、VFS、MCP、Skill baseline 与 `CapabilityState` 是同一份 construction projection 的不同维度。

Core entries:

- `derive_session_capability_projection(SessionCapabilityProjectionInput) -> SessionCapabilityProjection`
- `normalize_capability_state_dimensions(&mut CapabilityState, Option<Vfs>, Vec<SessionMcpServer>, &SessionBaselineCapabilities)`
- `build_session_context_plan(...) -> SessionConstructionPlan`

Contract:

- `CapabilityResolver` 只解析 tool / MCP / companion 维度。
- Effective VFS 由 construction finalize 合并 owner/session/runtime-command facts 后确定。
- Skill baseline 与 guidelines 从 effective VFS 派生。
- `CapabilityState.vfs.active` 必须等于 final `plan.surface.vfs`。
- `CapabilityState.tool.mcp_servers` 必须等于 final `plan.projections.mcp_servers`。
- `runtime_surface` 是 query DTO，只从 final `plan.surface.vfs` 生成。

## LaunchPlan And Stage Contracts

`LaunchPlanner::plan` 返回 `LaunchPlan`。planner 输入由 `LaunchPlanningDeps`、`LaunchCommand`、`SessionConstructionPlan` 与 runtime facts 组成。

`LaunchPlan` 承载或引用：

- resolved prompt payload
- `SessionConstructionPlan`
- lifecycle / restore / hook / follow-up plan
- pending runtime command apply plan
- terminal effect plan
- connector input projection
- launch trace

Connector input 的 working directory、executor config、MCP、VFS、identity、capability state 和 context frame 都从 final construction 与 `LaunchPlan` 投影生成。launch stages 执行计划时沿用 construction 事实，保持 owner、context、VFS、MCP 与 capability 的单一来源。

`PreparedTurn` 汇总 connector accepted 前的 turn runtime projection、tools、context frames、hook runtime handle 与 connector-facing `ExecutionContext`。

`connector.prompt` 返回 `ExecutionStream` 是 launch accepted 边界。accepted 之前允许做 turn claim、active runtime projection、hook `SessionStart` context preparation 和 connector context assembly；accepted 之后才提交 user message、`TurnStarted`、context/capability projection event、bootstrap meta、runtime command `applied` 与本地 title derivation。connector setup 失败时释放 turn runtime 并记录失败终态。

`TurnCommitter::commit` 消费 `ConnectorAcceptedTurn`，原因是 accepted 后事实只有在 connector 已接收本轮 prompt 后才有业务意义。`StreamIngestionAttacher::attach` 消费 `CommittedTurn`，原因是 processor/adapter supervision 依赖 accepted 后事实已经落库。

LaunchPlanner 处理 runtime-only planning：

- resolved prompt payload
- lifecycle / restore / hook / follow-up
- requested runtime command apply plan
- terminal effect plan
- connector input projection

## Pending Runtime Commands

Runtime context / capability transition 的事实源是 runtime command event/store。Projection 只服务查询、apply-once 与失败恢复。

状态流：

```text
requested -> applied
requested -> failed
```

connector.prompt accepted 后再标记 applied；connector.prompt 失败时保留 requested/failed 事实供下一轮恢复。

Payload contract:

- persisted payload type: `PendingCapabilityStateTransition`
- transition field: `RuntimeCapabilityTransition { declarations, effects }`
- replay entry: `replay_runtime_capability_transitions(base_state, transitions) -> RuntimeCapabilityReplay`
- payload 语义是 intent，不是 full `CapabilityState` projection
- 写入 runtime command store 前必须通过 `CapabilityDimensionRegistry::validate_transition`

## Freeform Session Lifecycle Ownership

普通自由会话也进入 LifecycleRun 过程归属模型，避免 session 与 workflow 过程形成两套事实源。

Contract:

- `POST /sessions` 创建 project-scoped 业务会话，必须先校验调用者对 `project_id` 有 `Edit` 权限。
- 新 session 必须创建 `SessionBinding(owner_type=Project, owner_id=project_id, label=freeform)`。
- 没有显式 lifecycle 的普通会话必须调用 `ensure_run_for_session`，生成 `LifecycleRun.session_id = session.id`。
- freeform lifecycle 是单 Activity graph：`main_conversation` 使用 `Agent + ContinueRoot` 与 `ActivityCompletionPolicy::OpenEnded`。
- `OpenEnded` 表示普通 prompt terminal 不会自动完成 activity。

## Ready Gate

云端 `AppState::new_with_plugins` 返回前必须完成 session 主链路依赖绑定：

- runtime tool provider
- MCP relay provider
- terminal callback
- session construction provider
- context audit bus

Ready gate 的职责是保证运行期看到完整依赖图。
