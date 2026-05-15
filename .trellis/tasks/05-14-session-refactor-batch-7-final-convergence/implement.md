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

本 batch 总计 7 次提交完成。前 4 次已经落地；剩余 3 次必须按顺序一次性推进，不再拆更小提交。

| Commit | Status | Message | Scope |
|---|---|---|---|
| 1 | 已完成 | `refactor(session): 校准 launch source 与 construction provider 边界` | 删除旧 augmenter/seed 命名，校准 source intent 与 construction provider 边界 |
| 2 | 已完成 | `refactor(session): 删除 construction facts 生产传递层` | 删除 `SessionConstructionFacts` production handoff，provider 直接返回 `SessionConstructionPlan` |
| 3 | 已完成 | `refactor(session): 统一 context 查询与 construction plan 投影` | context/query/audit/inspector 不再拥有 route-local construction 主线 |
| 4 | 已完成 | `refactor(session): 将 prompt pipeline 收缩为 launch execution 执行器` | pipeline 只执行 `LaunchExecution`，connector accepted 后才提交成功副作用 |
| 5 | 已完成 | `refactor(session): 拆分 session 业务能力服务` | 拆出 core / eventing / runtime / control 等能力服务，并迁移 API/local 的直接调用点 |
| 6 | 未开始 | `refactor(session): 删除 session hub 业务门面` | 把 launch/cancel/hooks/effects/pending/tool approval/companion 迁出 Hub，调用点依赖具体服务；Hub 只剩装配壳或删除 |
| 7 | 未开始 | `refactor(session): 完成 effects pending persistence 收口验证` | durable effects、pending runtime command、store boundaries、migration 与父任务文档最终收口 |

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

### Commit 6: 删除 session hub 业务门面

本提交把剩余业务职责从 Hub 中移出。完成后 `SessionHub` 若仍存在，只能是依赖装配对象、测试 handle 或被具体服务内部短暂持有的依赖集合；它不再是调用方可见的业务能力入口。

必须完成：

- `SessionLaunchService`：接管 `launch_command` / `launch_command_with_outcome` / test-only constructed launch。
- `SessionHookService`：接管 hook runtime rebuild、hook trigger dispatch、hook auto-resume scheduling、context update injections。
- `SessionRuntimeService`：接管 cancel、stall scan、active turn control、runtime MCP/capability hot update。
- `SessionEffectsService`：接管 terminal effect outbox enqueue/replay/dispatch handler resolution。
- `SessionPendingService`：接管 runtime command enqueue/apply/fail/projection 查询。
- `SessionControlService`：接管 approve/reject tool call、push notification、companion wait response。
- workflow / routine / task / reconcile / local runtime 只依赖所需具体服务，不再保存有职责 `SessionHub`。
- `impl SessionHub` 中除 factory/ready-gate/test handle 外的业务方法删除。
- 如果 `SessionHub` 类型仍存在，字段与方法名称必须表达 assembly/dependency container，而不是业务 facade；若无法保持无职责，就删除该类型并以 services struct 替代。

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

- `SessionHub` 不再是业务能力入口。
- 调用点按能力依赖具体服务。
- 剩余 `session_hub` 命中若存在，只能是装配期变量名或测试 fixture；不得承载业务判断。

### Commit 7: Effects / pending / persistence 最终验证

本提交不再重构入口形态，只做运行语义、迁移、测试与文档收口。

必须完成：

- terminal event 先落库，effect 进入 durable outbox；handler 有 durable identity 或 typed handler。
- effect 支持 retry、dead-letter、replay 与审计。
- pending runtime command 覆盖 requested / applied / failed，具备 apply-once 和失败恢复测试。
- 新增业务逻辑依赖 meta / event / outbox / runtime-command store 边界，不再扩张大 `SessionPersistence`。
- PostgreSQL / SQLite migration 覆盖旧字段删除/迁移。
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
rg -n "PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|LaunchCommand::.*_prepared|PromptAugmentInput|SessionConstructionFacts|SessionConstructionSeed" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

完成定义：

- 父任务 closure checklist 全部通过。
- final convergence tracker 中没有“过渡边界仍在生产主线”的未完成项。
- 可以标记父任务完成。
