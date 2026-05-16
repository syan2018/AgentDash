# Implementation Plan: Batch 7 Final Convergence

## Resume Protocol

每次上下文压缩、暂停或重新领取任务时，只读三份权威文件：

1. `.trellis/tasks/05-14-session-launch-refactor-assessment/prd.md`
2. `.trellis/tasks/05-14-session-launch-refactor-assessment/design.md`
3. 本文件

然后执行第一个状态不是“已完成”的 commit slice。不要创建 child task，不要重新开一份计划，不要把已经删除的旧结构作为迁移基础复活。

终态只认这一条生产主链路：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext projection
```

`LaunchCommand` 只表达 source intent。`SessionConstructionPlan` 是 owner / workspace / VFS / MCP / capability / context / identity / projection / trace 的事实源。`LaunchExecution` 是本次 prompt、lifecycle、restore、hook、follow-up、runtime command、terminal effect、connector input 的执行计划。`ExecutionContext` 只在 connector 边界投影。

## Commit Map

本 batch 总计 10 次提交完成。前 8 次已经落地；剩余 2 次必须按顺序一次性推进，不再拆更小提交，也不创建 child task。

| Commit | Status | Message | Scope |
|---|---|---|---|
| 1 | 已完成 | `refactor(session): 校准 launch source 与 construction provider 边界` | 删除旧 augmenter/seed 命名，校准 source intent 与 construction provider 边界 |
| 2 | 已完成 | `refactor(session): 删除 construction facts 生产传递层` | 删除 `SessionConstructionFacts` production handoff，provider 直接返回 `SessionConstructionPlan` |
| 3 | 已完成 | `refactor(session): 统一 context 查询与 construction plan 投影` | context/query/audit/inspector 不再拥有 route-local construction 主线 |
| 4 | 已完成 | `refactor(session): 将 prompt pipeline 收缩为 launch execution 执行器` | pipeline 只执行 `LaunchExecution`，connector accepted 后才提交成功副作用 |
| 5 | 已完成 | `refactor(session): 拆分 session 业务能力服务` | 拆出 core / eventing / runtime / control 等能力服务，并迁移 API/local 的直接调用点 |
| 6 | 已完成 | `refactor(session): 迁移 session launch 调用至能力服务` | 把 API/task/routine/workflow 的 launch/hook/effects/capability 调用迁到具体服务；Hub 只在装配、runtime tool handle 与内部实现中残留 |
| 7 | 已完成 | `refactor(session): 删除 hub facade 调用残留` | 删除已迁出的 Hub facade 方法，迁移 companion / hook auto-resume / tests / title 调用点到具体能力服务 |
| 8 | 已完成 | `refactor(session): 移除 runtime tools 的 hub 服务定位器` | runtime tool provider、companion/canvas/workflow tools 改用具体 service bundle |
| 9 | 已完成 | `refactor(session): 解除 launch effects 与 hub 依赖` | 让 launch planner/executor、terminal effects 依赖明确服务/依赖包，不再以 Hub 作为执行期参数 |
| 10 | 已完成 | `test(session): 完成 effects pending persistence 验证收口` | 核验 durable effects、pending runtime command、store boundaries、migration 与父任务文档最终收口 |

## Execution Rules

- 每次重新领取任务时，从 Commit Map 找第一个未完成项继续。
- 一个 commit slice 内可以“大刀”改完同一能力边界，不要为了显得谨慎拆成多个细碎提交。
- 该 slice 验证通过后立即提交，再进入下一 slice。
- 发现前一 slice 遗留错误时，在当前 slice 内直接修正，不新增旁路、不保留双主线。
- 不创建 subagent。
- 不做兼容旧内部 API 的双主线。
- 不新增只转发旧 payload 的 wrapper。
- 不把 resolved VFS / MCP / capability / context / hook / effect / working_dir 塞进 `LaunchCommand`。
- 不把 `SessionHub` 包一层继续当业务 facade 使用。

## Completed Slices

### Commit 1: 校准 source intent 与 construction provider 边界

完成事实：

- `PromptRequestAugmenter` / prompt augmenter 命名替换为 `SessionConstructionProvider`。
- `SessionConstructionSeed` 类型名删除。
- task / companion source payload 命名脱离 `PromptAugment*`。
- API bootstrap 文件名脱离 `prompt_augmenter` / `session_launch_augmenter`。
- `SessionLaunchPlannerInput` 接收 `LaunchCommand` 原件。
- `prompt_pipeline` 不再重组 source contract、identity、follow-up、local relay workspace root、local relay MCP declarations。

### Commit 2: 删除 `SessionConstructionFacts` production handoff

完成事实：

- `SessionConstructionProvider::build_construction` 直接返回 `SessionConstructionPlan`。
- `SessionConstructionPlan.prompt` 承载 prompt blocks / env projection。
- executor profile 进入 `execution_profile`。
- API bootstrap、assembler、pipeline、planner 不再传递 facts tuple。
- companion dispatch 使用本次 child session construction plan，parent session 只作为 source policy 解析 parent facts。

### Commit 3: 让 context/query/audit/inspector 与 launch 同源

完成事实：

- Task / Story / Project session detail 入口改为调用 `build_session_context_plan`。
- 详情入口不再直接调用 `SessionConstructionPlanner`、`SessionOwnerResolver` 或 `build_surface_summary`。
- runtime surface、VFS、context snapshot 均从 `SessionConstructionPlan.context_projection` 投影。

### Commit 4: 收缩 `prompt_pipeline` 为执行器

完成事实：

- pending runtime context transition 改为先生成待应用结果，不在 connector.prompt 前持久化 applied 事件或 context frame。
- connector.prompt 接受后再持久化 pending capability events、context frames、bootstrap meta、pending applied 与 title generation。
- connector.prompt 失败路径保持清理 turn、写 failed terminal，不提交 bootstrap/pending/title 成功副作用。

## Remaining Slices

### Commit 5: 拆分 session 业务能力服务

状态：已完成。

本提交先把“业务入口不再都从 Hub 进”的骨架一次性拆出来，避免继续在 `SessionHub` facade 里堆职责。

必须完成：

- 新建或补齐具体能力服务：
  - `SessionCoreService`：meta CRUD、execution projection、owner bootstrap state、startup recovery 查询。
  - `SessionEventingService`：event append、history/page、broadcast、compaction enrichment、transcript projection。
  - `SessionRuntimeService` 或等价服务：active turn 查询、stall detection、cancel control、connector live session 区分。
  - `SessionControlService` 或等价服务：tool approval / rejection、companion response 这类 connector/control plane 操作。
- API / local route handler 不再通过 `state.services.session_hub` 访问 core/eventing/runtime/control 能力。
- Application 外围服务若只需要 core/eventing/runtime/control，也改依赖具体服务；只有 launch、hook、effects 尚未迁出前可以临时持有 Hub。
- 删除 Hub facade 中已迁出的业务实现；若保留同名方法，只允许测试或尚未迁移的内部旧调用临时存在，并必须在 tracker 标成 Commit 6 删除项。
- 更新 `SessionHub` 模块注释，不能再宣称它是职责门面。

退出检查：

```powershell
rg -n "session_hub\\s*\\.\\s*(get_session_meta|get_session_metas_bulk|create_session|create_session_with_title_source|list_sessions|inspect_execution_states_bulk|inspect_session_execution_state|delete_session|mark_owner_bootstrap_pending|inject_notification|subscribe_after|subscribe_with_history|list_event_page|build_projected_transcript|cancel|approve_tool_call|reject_tool_call|respond_companion_request|recover_interrupted_sessions|find_stalled_sessions)" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-application/src
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::hub
git diff --check
```

完成定义：

- API/local 的 CRUD、stream/event、state query、cancel、tool approval、companion response 不再经 Hub；application 的 boot reconcile、terminal cancel、stall detector 也不再经 Hub 执行 cancel/recovery。
- Hub 剩余命中只属于 launch/hook/effects/pending/tool-builder 装配或测试，且全部列入 Commit 6 删除清单。
- 不用 facade 包一层冒充拆分。

### Commit 6: 迁移 session launch 调用至能力服务

状态：已完成。

本提交把调用方可见的 launch / hook / effects / capability 入口迁到具体服务，避免 API、task、routine、workflow orchestrator 继续把 Hub 当业务服务使用。

必须完成：

- `SessionLaunchService` 接管 API/task/routine/workflow/local prompt 的 launch 调用。
- `SessionHookService` 接管 API/workflow 的 hook runtime 读取与确保。
- `SessionEffectsService` 接管 AppState 启动期 outbox replay。
- `SessionCapabilityService` 接管 workflow phase apply、runtime MCP/capability 查询与 construction parent facts。
- task / routine / workflow orchestrator 不再保存 `SessionHub` 字段。
- AppState `ServiceSet` 暴露具体服务，route handler 优先依赖具体服务。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
rg -n "session_hub" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-application/src/task crates/agentdash-application/src/workflow crates/agentdash-application/src/routine crates/agentdash-application/src/reconcile crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
git diff --check
```

完成定义：

- API/task/routine/workflow orchestrator 的 launch/hook/effects/capability 主调用不再依赖 Hub。
- 剩余 `session_hub` 命中只允许是 AppState/local 装配、runtime tool provider handle、advance-node 工具服务定位、session 模块内部实现或测试 fixture。

### Commit 7: 删除 Hub facade 调用残留

状态：已完成。

本提交只做已经明确归属的调用迁移和 facade 删除，不再把这些已拆出的能力留在 `SessionHub` 上假装兼容。完成后，外部调用点不应通过 Hub 调用 launch/control/runtime/effects/title/capability 入口；Hub 的残留必须只剩装配、内部尚待 Commit 8 下沉的实现、local runtime holder 或测试 fixture。

必须完成：

- 删除 Hub 上已经迁移到 `SessionLaunchService` / `SessionRuntimeService` / `SessionControlService` / `SessionEffectsService` / `SessionTitleService` 的同名方法。
- companion、hook auto-resume、session tests、terminal effects tests、API/local 入口全部调用具体服务。
- 删除无调用的 `hub/cancel.rs` 模块。
- title 手动设置只经 `SessionTitleService`；Hub 不再暴露 `set_user_title`。
- 更新执行文档，明确 Commit 8 才处理内部实现去 Hub，不把当前提交冒充最终态。

退出检查：

```powershell
rg -n "pub async fn launch_command|pub async fn launch_command_with_outcome|pub async fn respond_companion_request|pub async fn replay_terminal_effect_outbox|pub async fn set_user_title|pub async fn cancel|pub async fn approve_tool_call|pub async fn reject_tool_call|pub async fn find_stalled_sessions" crates/agentdash-application/src/session
rg -n "\.launch_command\(|\.launch_command_with_outcome\(|\.respond_companion_request\(|\.replay_terminal_effect_outbox\(|\.set_user_title\(|\.cancel\(|\.approve_tool_call\(|\.reject_tool_call\(" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
git diff --check
```

完成定义：

- Hub facade 中没有已迁出业务入口。
- 对外 route / task / routine / workflow / companion 的调用目标是具体 service。
- 剩余 `SessionHub` 命中全部列入 Commit 8 的“内部实现去 Hub”范围。

### Commit 8: 移除 runtime tools 的 Hub 服务定位器

状态：已完成。

本提交删除 runtime tool provider 里的 Hub handle，不再把 Hub 延迟塞进 companion / canvas / workflow runtime tools。工具层只接收具体 `SessionToolServices` bundle：core / eventing / control / launch / hooks / capability / companion wait registry。

完成事实：

- `SharedSessionHubHandle` 删除，替换为 `SharedSessionToolServicesHandle`。
- `RelayRuntimeToolProvider` 不再保存 Hub，只保存 service bundle handle。
- `CompleteLifecycleNodeTool` 直接接收 `SessionToolServices`，不再通过 Hub 现取 orchestrator 依赖。
- companion tools 使用 core/eventing/control/launch/hooks/capability/companion wait registry。
- canvas tools 使用 core/eventing 更新 session meta 与事件，不再读取 Hub。
- `RuntimeSessionMcpAccess` 的实现从 Hub 移到 `SessionCapabilityService`。

退出检查：

```powershell
rg -n "SharedSessionHubHandle|session_hub_handle|impl RuntimeSessionMcpAccess for SessionHub" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
git diff --check
```

完成定义：

- runtime tool provider 不再是 Hub 服务定位器。
- companion/canvas/workflow runtime tools 不再持有 Hub。

### Commit 9: 解除 launch / effects 与 Hub 依赖

状态：已完成。

本提交处理真正的架构收口：把 Hub 从“内部业务实现参数”降为装配阶段依赖来源。不是把 `SessionHub` 改名成另一个 service locator，而是让每条业务主线拿到自己需要的依赖包或具体服务。

必须完成：

- `SessionLaunchService` 持有明确 launch deps；`SessionLaunchExecutor` 不再接收 `&SessionHub`。
- `SessionLaunchPlanner` 持有明确 planner deps；owner/context/VFS/MCP/capability 只来自 `SessionConstructionPlan`、runtime facts 和明确依赖，不从 Hub 侧重建。
- hook runtime 解析、hook trigger dispatch、auto-resume 调度进入 `SessionHookService` 或明确 hook deps。
- runtime capability / MCP / live transition / pending transition 进入 `SessionCapabilityService`，不再通过 Hub 方法转发。
- `SessionTerminalEffectDispatcher` 由 `SessionEffectsService` 创建并接收 effects deps；不再读取 Hub。
- `SessionTurnProcessor` 依赖 eventing/runtime/effects 等明确服务或 deps，不再持有 Hub。
- local relay 的长期 runtime holder 如果仍需保存装配对象，必须命名为 assembly/runtime handle，并且 handler 立即投影为具体 services。

退出检查：

```powershell
rg -n "SessionLaunchExecutor::new\\(&.*hub|SessionLaunchPlanner::new\\(.*hub|SessionTerminalEffectDispatcher::new\\(&.*hub|SessionTurnProcessor::spawn\\(\\s*hub|SharedSessionHubHandle|session_hub_handle|impl RuntimeSessionMcpAccess for SessionHub" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "impl SessionHub" crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
git diff --check
```

完成定义：

- `SessionHub` 仍可作为构造函数和 ready gate 的装配对象存在，但业务执行器、planner、effect dispatcher、runtime tools 不再把 Hub 作为依赖参数或服务定位器。
- `LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext projection` 的生产主线没有旁路 payload。
- Context 查询、launch、companion、local relay、hook auto-resume 都从同一 construction fact source 投影。

### Commit 10: Effects / pending / persistence 验证与任务收口

状态：已完成。

本提交不再新增架构壳，只做最终语义确认、测试补齐、迁移核验和文档闭环。若 Commit 8 发现缺口，必须先在 Commit 8 修掉，不能在 Commit 9 写兼容旁路。

必须完成：

- terminal event 先落库，effect 进入 durable outbox；handler 有 durable identity 或 typed handler。
- effect 支持 retry、dead-letter、replay 与审计。
- pending runtime command 覆盖 requested / applied / failed，具备 apply-once 和失败恢复测试。
- 新增业务逻辑依赖 meta / event / outbox / runtime-command store 边界，不再扩张大 `SessionPersistence`。
- PostgreSQL / SQLite migration 覆盖旧字段删除/迁移；确认没有 `pending_capability_state_transitions_json` 残留。
- 父任务 tracker、closure checklist、session startup spec 与代码事实一致。

最终验证：

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-infrastructure
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
cargo test -p agentdash-application session::hub
cargo test -p agentdash-application session::terminal_effects
cargo test -p agentdash-application session::runtime_commands
cargo test -p agentdash-application session::memory_persistence
cargo test -p agentdash-application session::path_policy
cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions
rg -n "\\b(PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|PromptAugmentInput|SessionConstructionFacts|SessionConstructionSeed)\\b|LaunchCommand::.*_prepared" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

完成定义：

- 父任务 closure checklist 全部通过。
- final convergence tracker 中没有“过渡边界仍在生产主线”的未完成项。
- 可以标记父任务完成。
