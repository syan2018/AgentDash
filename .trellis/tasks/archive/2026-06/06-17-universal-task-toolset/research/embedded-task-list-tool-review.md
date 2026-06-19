# Research: embedded task list tool review

- Query: 参考 `references/codex`、`references/claude-code`、`references/pi-mono` 中与 task list / plan / update_plan / TodoWrite / agent 内嵌清单相关的实现，规划 AgentDash 通用 Task 工具集。
- Scope: mixed
- Date: 2026-06-17

## Findings

### 0. Direction correction

**架构前提确认**：AgentDash 项目只有 `Task` 一个业务概念。业务代码、API、DTO、DB、MCP tool name、event kind、store、组件命名都必须使用 Task。外部参考实现中的 `TodoWrite` 等名称只作为研究证据出现；AgentDash 自己最多在面向模型的工具说明中写“Task 工具集可作为自己的待办清单使用”。

**AgentDash 落点**：参考实现中的清单机制应统一映射为 `LifecycleRun.tasks` 上的 Task plan facts。wire contract、事件事实、UI 投影和权限审计都必须使用 Task 语义。

### 1. Files found

- `references/codex/codex-rs/core/src/tools/handlers/plan_spec.rs`：Codex `update_plan` function tool schema，定义 `plan[]`、`step`、`status` 与单一 `in_progress` 约束。
- `references/codex/codex-rs/core/src/tools/handlers/plan.rs`：Codex `update_plan` handler，将工具调用转成 `EventMsg::PlanUpdate`，并禁止在 Plan mode 使用。
- `references/codex/codex-rs/protocol/src/plan_tool.rs`：Codex plan tool 参数类型，`StepStatus = pending / in_progress / completed`。
- `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs`：Codex 把 PlanUpdate 事件投影为 `turn/plan/updated` app-server notification。
- `references/codex/codex-rs/tui/src/history_cell/plans.rs`：Codex TUI 把 plan update 渲染为 checkbox-like history cell。
- `references/codex/codex-rs/tui/src/chatwidget/turn_runtime.rs`：Codex TUI 记录最新计划完成数，用于 status surface。
- `references/claude-code/src/tools/TodoWriteTool/TodoWriteTool.ts`：Claude Code V1 `TodoWrite`，以整体列表覆盖方式更新当前 session/agent 的清单。
- `references/claude-code/src/utils/todo/types.ts`：Claude Code V1 todo item schema，含 `content/status/activeForm`。
- `references/claude-code/src/tools/TodoWriteTool/prompt.ts`：Claude Code 对何时使用 todo、状态更新节奏和完成条件的模型提示。
- `references/claude-code/src/utils/tasks.ts`：Claude Code V2 Task schema、状态、文件持久化、并发锁与 interactive/non-interactive 切换。
- `references/claude-code/src/tools/TaskCreateTool/TaskCreateTool.ts`：Claude Code V2 `TaskCreate`，创建 pending task，支持 `subject/description/activeForm/metadata`。
- `references/claude-code/src/tools/TaskUpdateTool/TaskUpdateTool.ts`：Claude Code V2 `TaskUpdate`，更新状态、owner、依赖、metadata，并在完成时触发 hooks。
- `references/pi-mono/packages/coding-agent/docs/usage.md`：pi-mono 设计原则，核心不内建 plan mode / to-dos，而交给扩展。
- `references/pi-mono/packages/coding-agent/examples/extensions/todo.ts`：pi-mono 清单扩展示例，使用 tool result details 和 session branch 重建状态。
- `references/pi-mono/packages/coding-agent/examples/extensions/plan-mode/index.ts`：pi-mono plan-mode 扩展示例，基于只读工具集、计划提取、`[DONE:n]` 标记和 widget 展示。
- `crates/agentdash-domain/src/workflow/value_objects/task_plan.rs`：AgentDash 当前 Task plan facts 和状态机。
- `crates/agentdash-domain/src/workflow/entity.rs`：AgentDash 当前 `LifecycleRun` aggregate 上的 Task create/update/archive/status transition。
- `crates/agentdash-contracts/src/task/contract.rs`：AgentDash Task plan contract DTO。
- `crates/agentdash-application/src/task/plan.rs`：AgentDash run-scoped Task command、policy hook 与 Story Task projection。
- `crates/agentdash-api/src/routes/task_plan.rs`：AgentDash run / agent-run scoped Task API。
- `packages/app-web/src/stores/taskPlanStore.ts`：AgentDash 前端 run-scoped Task plan store。

### 2. Codex reference: update_plan is a checklist interaction tool, not durable plan mode

**参考实现确认**：Codex `update_plan` 的 schema 很小：每个 item 只有 `step` 和 `status`，状态枚举为 `pending / in_progress / completed`，外层有可选 `explanation` 和必填 `plan`。工具描述明确要求 “At most one step can be in_progress at a time”。证据：`references/codex/codex-rs/core/src/tools/handlers/plan_spec.rs:7-57`。

**参考实现确认**：Codex 通过 Rust 类型收紧输入，`PlanItemArg` 使用 `#[serde(deny_unknown_fields)]`，`UpdatePlanArgs` 有 `explanation: Option<String>` 和 `plan: Vec<PlanItemArg>`。证据：`references/codex/codex-rs/protocol/src/plan_tool.rs:6-28`。

**参考实现确认**：Codex 把 `update_plan` 定位为 “TODO/checklist tool”，并在 Plan mode 禁用，说明它不是计划模式产物，而是执行过程中的进度清单。证据：`references/codex/codex-rs/core/src/tools/handlers/plan.rs:84-92` 和 `references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:1301-1314`。

**参考实现确认**：Codex 的展示边界是事件投影：tool handler 发送 `EventMsg::PlanUpdate`，app-server 转成 `TurnPlanUpdatedNotification`，通知字段是 `thread_id / turn_id / explanation / plan[]`。证据：`references/codex/codex-rs/core/src/tools/handlers/plan.rs:90-95`、`references/codex/codex-rs/app-server-protocol/src/protocol/v2/turn.rs:401-445`。

**参考实现确认**：Codex TUI 将计划更新渲染成历史记录里的 checkbox-like cell，completed 使用勾选和删除线，in_progress 使用 cyan/bold，pending dim。证据：`references/codex/codex-rs/tui/src/history_cell/plans.rs:46-50`、`references/codex/codex-rs/tui/src/history_cell/plans.rs:177-214`。

**参考实现确认**：Codex 会把最新计划进度汇总到状态表面，例如 `completed/total`，但这个汇总来自最近一次 plan update，不是独立领域模型。证据：`references/codex/codex-rs/tui/src/chatwidget/turn_runtime.rs:460-473`、`references/codex/codex-rs/tui/src/bottom_pane/title_setup.rs:128-130`。

**对 AgentDash 的架构推断**：Codex 模式适合借鉴为“Task list bulk sync / replace view”工具，即 agent 可以一次提交当前完整 Task 清单，让后端按 `LifecycleRun.tasks` 做创建、状态推进、重排或软归档。不能照搬为独立清单存储，因为 AgentDash 已有唯一事实源 `LifecycleRun.tasks`。

### 3. Claude Code reference: TodoWrite V1 and Task V2 both optimize agent progress ergonomics

**参考实现确认**：Claude Code V1 `TodoWrite` item schema 是 `content/status/activeForm`，状态为 `pending / in_progress / completed`。`activeForm` 是专门给运行中 spinner 展示的现在进行时文案。证据：`references/claude-code/src/utils/todo/types.ts:4-17`。

**参考实现确认**：V1 `TodoWrite` 输入是完整 `todos` 列表，输出包含 `oldTodos/newTodos`；存储键为 `context.agentId ?? getSessionId()`，全部完成时会清空 AppState 中对应列表。证据：`references/claude-code/src/tools/TodoWriteTool/TodoWriteTool.ts:13-25`、`references/claude-code/src/tools/TodoWriteTool/TodoWriteTool.ts:65-103`。

**参考实现确认**：V1 是 session/AppState 机制，不是 domain task；subagent 结束后还会清理 agentId 对应 todos，避免 orphaned key 泄漏。证据：`references/claude-code/src/state/AppStateStore.ts:217-221`、`references/claude-code/src/tools/AgentTool/runAgent.ts:835-842`。

**参考实现确认**：Claude Code 的提示词非常强调使用时机：复杂多步任务、用户明确要求、多任务输入、收到新指令、开始工作前标 `in_progress`、完成后立即标 `completed`；单步/纯信息任务跳过。证据：`references/claude-code/src/tools/TodoWriteTool/prompt.ts:6-25`。

**参考实现确认**：Claude Code 的管理规则要求 “Exactly ONE task must be in_progress”，完成要立即更新，不批量延后；遇到失败、阻塞、未完成时不得标 completed。证据：`references/claude-code/src/tools/TodoWriteTool/prompt.ts:144-180`。

**参考实现确认**：Claude Code V2 变成 Task 工具：`TaskSchema` 包含 `id/subject/description/activeForm/owner/status/blocks/blockedBy/metadata`，状态仍是 `pending / in_progress / completed`，并用文件锁处理多 agent 并发。证据：`references/claude-code/src/utils/tasks.ts:69-89`、`references/claude-code/src/utils/tasks.ts:91-108`。

**参考实现确认**：V2 `TaskCreate` 创建 `pending` task，支持 `subject/description/activeForm/metadata`，并会触发 created hooks；`TaskUpdate` 支持状态、owner、依赖、metadata，完成时触发 completed hooks。证据：`references/claude-code/src/tools/TaskCreateTool/TaskCreateTool.ts:18-32`、`references/claude-code/src/tools/TaskCreateTool/TaskCreateTool.ts:80-119`、`references/claude-code/src/tools/TaskUpdateTool/TaskUpdateTool.ts:33-65`、`references/claude-code/src/tools/TaskUpdateTool/TaskUpdateTool.ts:212-240`。

**对 AgentDash 的架构推断**：Claude Code V1 的价值主要是交互规则和 `activeForm` 展示字段，不适合作为 AgentDash 独立存储模型。Claude Code V2 更接近 AgentDash 方向，因为它把清单升级成 Task 工具，但 AgentDash 状态和事实源必须保持 `LifecycleRun.tasks`：`open / active / review / blocked / done / dropped`，而不是 Claude 的 `pending / in_progress / completed`。

### 4. pi-mono reference: no built-in task-list core, useful extension patterns only

**参考实现确认**：pi-mono 的核心设计明确不内建 plan mode、to-dos、sub-agents 等工作流特性，而是交给 extensions / skills / prompt templates / packages。证据：`references/pi-mono/packages/coding-agent/docs/usage.md:299-303`。

**参考实现确认**：pi-mono 清单示例是扩展，不是核心机制。它注册 `todo` tool 和 `/todos` command，状态存储在 tool result `details` 中，通过 session branch replay 重建，这样 fork/branch 时状态随历史自然分叉。证据：`references/pi-mono/packages/coding-agent/examples/extensions/todo.ts:1-11`、`references/pi-mono/packages/coding-agent/examples/extensions/todo.ts:105-134`。

**参考实现确认**：pi-mono 清单示例的模型极简：`id/text/done`，工具动作只有 `list/add/toggle/clear`。证据：`references/pi-mono/packages/coding-agent/examples/extensions/todo.ts:18-35`、`references/pi-mono/packages/coding-agent/examples/extensions/todo.ts:135-219`。

**参考实现确认**：pi-mono plan-mode 示例通过扩展实现只读 plan mode、从 `Plan:` 文本提取编号步骤、执行时使用 `[DONE:n]` 标记完成，并用 footer/widget 展示进度。证据：`references/pi-mono/packages/coding-agent/examples/extensions/plan-mode/index.ts:38-74`、`references/pi-mono/packages/coding-agent/examples/extensions/plan-mode/index.ts:158-205`、`references/pi-mono/packages/coding-agent/examples/extensions/plan-mode/index.ts:207-288`。

**对 AgentDash 的架构推断**：pi-mono 对 AgentDash 的主要启发是“工作流特性可以通过 tool/event/UI 投影组合，而不必新增领域对象”。但 AgentDash 已经选择 Task 作为业务概念，因此应把此思路落成 Task 工具事件与 Task UI 投影，不复制外部扩展的 `done` 布尔模型或 `[DONE:n]` 文本解析作为事实源。

### 5. AgentDash current state

**参考实现确认**：AgentDash 当前长期 spec 已明确 Task 是 `LifecycleRun` aggregate 内的计划项事实，保存在 `LifecycleRun.tasks`；Task 不做 Project/global 任务池，也不拥有 runtime truth。证据：`.trellis/spec/backend/story-task-runtime.md:10-11`、`.trellis/spec/backend/story-task-runtime.md:30-33`。

**参考实现确认**：AgentDash 当前 Task 状态为 `open / active / review / blocked / done / dropped`，状态转换由 `TaskPlanStatus::can_transition_to` 控制。证据：`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:9-45`。

**参考实现确认**：AgentDash 当前 Task plan item 已有 `id/title/body/status/priority/created_by_agent_id/owner_agent_id/assigned_agent_id/source_task_id/created_at/updated_at/archived_at/context_refs/story_ref`。证据：`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:57-82`。

**参考实现确认**：`LifecycleRun` aggregate 已提供 `create_task/update_task/archive_task/transition_task_status`，其中 archive 会设置 `status = Dropped` 和 `archived_at`。证据：`crates/agentdash-domain/src/workflow/entity.rs:257-369`。

**参考实现确认**：AgentDash run-scoped command 已存在，`list_run_tasks/create_run_task/update_run_task/archive_run_task/transition_run_task_status` 都围绕 `LifecycleRunRepository` 读取并写回 aggregate。证据：`crates/agentdash-application/src/task/plan.rs:89-209`。

**参考实现确认**：AgentDash API 已有 `/lifecycle-runs/{run_id}/tasks`、`/lifecycle-runs/{run_id}/tasks/{task_id}`、`/status`、`/archive`、以及 `/agent-runs/{run_id}/agents/{agent_id}/tasks`。证据：`crates/agentdash-api/src/routes/task_plan.rs:32-54`。

**参考实现确认**：Story 页面读取 Task projection，不拥有 Task facts；projection 来源包括 Story-bound run、linked run 和显式 `story_ref`。证据：`crates/agentdash-application/src/task/plan.rs:293-374`、`crates/agentdash-api/src/routes/stories.rs:60-63`、`crates/agentdash-api/src/routes/stories.rs:234-278`。

**参考实现确认**：前端已有 `taskPlanStore`，按 run id 缓存 `RunTaskPlanResponse`，提供 fetch/create/update/status/archive/upsert。证据：`packages/app-web/src/stores/taskPlanStore.ts:13-33`、`packages/app-web/src/stores/taskPlanStore.ts:43-135`。

### 6. Recommended AgentDash Task toolset

以下为**架构推断**，均以 `LifecycleRun.tasks` 为唯一事实源。

第一版 agent-facing 工具集应按“数量最少、操作内聚”设计，默认只保留一对读写工具：

1. `task_read`
   - 读取当前 run / 当前 agent scope 的 Task view。
   - 参数：`run_id` 可由 frame/runtime 注入时省略；核心参数是 `mode`，可选 `task_id/agent_id/statuses/include_archived/include_execution_summary/detail/limit/cursor`。
   - 推荐 mode：`overview` 默认 compact 总览；`list` 筛选分页；`detail` 厚读回；`context` 只聚焦 Task context refs；`execution` 返回 linked run / SubjectExecution compact projection；`projection` 从 Story/run linkage 角度读取 Task 来源。
   - 返回：厚 Task DTO，含 `title/body/status/priority/context_refs/story_ref/owner/assignment/source_task_id/created_at/updated_at/archived_at`，以及 progress、Story/run linkage、compact execution summary 和写入 precondition 所需 version。

2. `task_write`
   - 借鉴 Codex `update_plan` 和 Claude `TodoWrite` 的整体列表覆盖体验，让模型一次提交当前清单。
   - 语义是对当前 `LifecycleRun.tasks` 做批量 create/update/status/archive/reorder/reference mutation。
   - 建议参数：`mode: patch | snapshot`、`explanation?`、`base_revision?`、`operations[]?`、`tasks[]?`。每个 Task payload 含 `id? / title / status / body? / priority? / owner_agent_id? / assigned_agent_id? / source_task_id? / context_refs? / story_ref? / active_label? / order?`。
   - `active_label` 对应 Claude `activeForm` 的 UI spinner 文案，可作为 Task presentation metadata 或 projection-only 字段；若要持久化，也应是 Task 字段或 Task view field。
   - 约束：默认最多一个 `active`，除非显式允许 parallel agent scopes；对未知 id 的处理必须明确为 create 或 reject，不能静默生成不可追溯项。

不建议第一版拆出 `task_set_status`。状态推进只是 `task_write` 的 operation：只有真正完成才标 `done`；遇到外部阻塞标 `blocked`；subagent 交付需 owner review 时标 `review`。独立状态工具会降低写入口内聚性。

不建议第一版拆出 `task_create/task_update/task_reorder/task_archive/task_split/task_merge`。这些能力优先由 `task_write` 的 mutation 语义覆盖，避免把模型暴露面扩成一组低内聚 CRUD 工具。只有当权限、审计、冲突处理或 UI 操作证明某个动作需要独立闭环时，后续再拆分。

`task_assign/task_fanout` 应继续作为既有 Task assignment / fanout 能力，而不是并入通用 Task 工具集第一版。通用工具维护 Task plan facts；派发工具消费这些 facts。

### 6.1 CLI reference patterns

**参考实现确认**：Trellis `task.py` 采用单 CLI 多 subcommand 和 flag 组合：`current --source` 展示同一当前任务的来源信息，`list --mine --status` 在同一 list 入口下组合过滤，`list-context` 专门读取 context manifest。证据：`.trellis/scripts/task.py:421-452`。

**参考实现确认**：Trellis `get_context.py` 有 text / JSON 输出，并在 workflow 中通过 mode / step 拉取不同上下文。证据：`.trellis/scripts/get_context.py:1-10`、`.trellis/scripts/task.py:377`。

**参考实现确认**：pi coding-agent 默认核心工具很少，README 描述内建工具是 `read/write/edit/bash`，并用 CLI `--mode json/rpc`、`--tools`、`--exclude-tools` 控制输出与工具面。证据：`references/pi-mono/packages/coding-agent/README.md:96`、`references/pi-mono/packages/coding-agent/README.md:538-580`。

**对 AgentDash 的架构推断**：Task 工具应采用 CLI-like 的少入口、多 mode/flag 设计。`task_read` 不拆成多个读工具，而是通过 mode 表达 overview/list/detail/context/execution/projection；`task_write` 不拆成状态、编辑、排序等工具，而是通过 operation type 和 return view 表达写入意图。

### 7. Status model mapping

**参考实现确认**：Codex/Claude 共同使用三态：`pending / in_progress / completed`。Claude V2 也保持同样三态，但额外有 owner、blocks、blockedBy、metadata。

**架构推断**：AgentDash 不应改成三态，因为现有 Task plan 状态更符合 run/subagent/review/fanout：

| Codex / Claude 状态 | AgentDash Task 状态 | 说明 |
| --- | --- | --- |
| `pending` | `open` | 已列入计划，未开始。 |
| `in_progress` | `active` | 当前处理。默认限制同一 agent scope 最多一个 active。 |
| `completed` | `done` | 已完成。 |
| 无 | `review` | AgentDash 必要扩展，支持 subagent / owner review gate。 |
| 无 | `blocked` | AgentDash 必要扩展，表达外部条件阻塞。 |
| deleted / removed / clear | `dropped` + `archived_at` | 软归档而非删除，保留审计和 Story projection 解释链。 |

状态工具提示应使用 Task 语言；如需帮助模型理解，只在工具说明中写“可作为自己的待办清单使用”。

### 8. Event and persistence boundary

**架构推断**：

- 所有 Task 工具写入都应落到 `LifecycleRun.tasks` aggregate mutation，不写 session-local object。
- 工具调用应生成 activity / event 事实，至少包含 `run_id`、`agent_id/frame_id`、`tool_call_id`、`task_id(s)`、`action`、`before/after status`、`explanation?`。
- `task_write` 应被审计为批量 Task command，而不是隐藏成一次 UI patch。建议保存 per-item diff，便于回放与冲突诊断。
- app/runtime stream 可以借鉴 Codex `turn/plan/updated`，增加 `run/task_plan/updated` 或复用现有 lifecycle projection refresh 事件。事件 payload 可以带 compact summary，但事实仍从 `LifecycleRun.tasks` 重拉。
- subagent 每个 agent scope 可以有自己的 Task 工具视图，但写的仍是同一个 owning run 的 Task 集合，通过 `created_by_agent_id/owner_agent_id/assigned_agent_id` 区分。
- session resume / context compaction 时，直接从 run-scoped Task plan view 注入当前 Task summary。

### 9. UI projection

**架构推断**：

- AgentRun workspace 的 Task plan 面板是主入口，展示 `LifecycleRun.tasks` 当前计划、状态、owner/assigned agent、review/blocked 信息。
- Story 页面继续展示 Story Task projection，并说明来源：owning run / linked run / story_ref。
- 运行中输入区或状态栏可显示 “Task progress x/y”，借鉴 Codex 的 completed/total summary；该 summary 从当前 Task plan projection 派生。
- spinner / active line 可以借鉴 Claude `activeForm`，但命名应贴近 Task，例如 `active_label`、`working_label` 或 projection-level `current_activity_label`。
- subagent 汇总视图按 `assigned_agent_id` / subject association / linked run 聚合，不创建另一套 subagent 清单。
- 清单式 UI 可以显示为 checkbox，但 checkbox 操作必须调用 Task status command：checked = `done`，unchecked/open = `open` 或 `active` 需要明确选择，不能把 checkbox 变成独立 bool。

### 10. Risks

- **命名风险**：业务代码出现 `Todo` 会让后续开发者误以为存在第二个领域概念。正式 tool name / DTO / event / store / route / component 均必须使用 `task_*` 或 Task 命名。
- **批量同步风险**：`task_write` 的 snapshot mode 若用完整列表覆盖，可能误删/归档并发创建的 Task。需要 optimistic version、updated_at precondition、或明确 diff-based sync。
- **单 active 约束风险**：Codex/Claude 的“一次一个 in_progress”适合单 agent；AgentDash 有 subagent 和 assignment，应把约束限定在 agent scope 或 owner scope，而不是整个 LifecycleRun。
- **状态降维风险**：模型熟悉三态清单，可能把 `blocked/review/dropped` 压成 pending/completed。工具 schema 和 prompt 必须暴露 AgentDash 六态，并解释何时用 review/blocked/dropped。
- **执行事实混淆风险**：Task `active/done` 仍是计划状态，不代表 runtime session running/completed。runtime truth 继续走 `SubjectExecutionView`。
- **Story ownership 风险**：Story projection 中出现 Task 不代表 Story 拥有 Task。Task 工具不能通过 Story 页面无 run scope 直接写 Task facts。

## Code Patterns

- Codex schema pattern：小 schema、完整列表、三态、单 in-progress 约束。`references/codex/codex-rs/core/src/tools/handlers/plan_spec.rs:7-57`。
- Codex event pattern：tool call -> core event -> app-server notification -> UI projection。`references/codex/codex-rs/core/src/tools/handlers/plan.rs:90-95`、`references/codex/codex-rs/app-server/src/bespoke_event_handling.rs:1295-1315`。
- Codex UI pattern：history cell 和 status progress 都消费 update event，不成为事实源。`references/codex/codex-rs/tui/src/history_cell/plans.rs:166-238`、`references/codex/codex-rs/tui/src/chatwidget/turn_runtime.rs:460-473`。
- Claude V1 pattern：模型可一次性写完整清单，`activeForm` 单独服务 UI 动作感。`references/claude-code/src/tools/TodoWriteTool/TodoWriteTool.ts:65-103`、`references/claude-code/src/utils/todo/types.ts:8-17`。
- Claude V2 pattern：Task 工具加入 id、owner、依赖、metadata、hook、文件锁，适合多 agent 任务协作。`references/claude-code/src/utils/tasks.ts:69-108`。
- pi-mono extension pattern：状态通过 session entries / tool result details 重建，适合分支历史，但 AgentDash 应改为 Task event + `LifecycleRun.tasks` 重拉。`references/pi-mono/packages/coding-agent/examples/extensions/todo.ts:105-134`。
- AgentDash canonical pattern：Task facts 在 `LifecycleRun.tasks`，命令读写 aggregate，Story 只读 projection。`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:57-82`、`crates/agentdash-application/src/task/plan.rs:89-209`、`crates/agentdash-application/src/task/plan.rs:293-374`。

## External References

- No live web references were used. Research was based on local checked-in reference repositories under `references/` and current AgentDash source/spec files.
- `references/codex` appears to be an OpenAI Codex Rust/TUI/app-server codebase snapshot.
- `references/claude-code` appears to be a Claude Code TypeScript codebase snapshot with V1 `TodoWrite` and V2 Task tools.
- `references/pi-mono` appears to be a pi coding-agent monorepo snapshot whose core intentionally keeps workflow features extension-based.

## Related Specs

- `.trellis/spec/backend/story-task-runtime.md`：Task 是 `LifecycleRun.tasks` 计划项事实；Story 只消费 projection；runtime truth 走 `SubjectExecutionView`。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：Task plan DTO 属于 generated contract，Story 页面消费 Story Task projection DTO，runtime artifacts 留在 `SubjectExecutionView`。
- `.trellis/spec/frontend/state-management.md`：`taskPlanStore` / AgentRun workspace 管 Task plan facts，`storyStore` 管 Story Task projection cache，`lifecycleStore` 管 execution projection。
- `.trellis/tasks/archive/2026-06/06-16-story-task-subject-model-cleanup/prd.md`：上一轮已确认 Task 是 LifecycleRun 内计划项事实，状态为计划语言，不表达 runtime execution truth。
- `.trellis/tasks/archive/2026-06/06-16-story-task-subject-model-cleanup/design.md`：上一轮设计了 Task aggregate、状态机、assignment、Story projection、SubjectExecutionView 边界。
- `.trellis/tasks/archive/2026-06/06-16-story-task-subject-model-cleanup/implement.md`：上一轮实现计划已列出 `LifecycleRun.tasks`、contracts、frontend store、MCP/capability 和 fanout 的风险文件。

## Caveats / Not Found

- 未发现 pi-mono core 内建 task-list / plan 持久模型；只有扩展示例和文档说明核心不内建这些工作流特性。
- 未使用外部网络检索；如果 reference 快照不是最新版本，本研究只对当前仓库内快照负责。
- 本研究不建议新增第二套清单 domain、表、session-local entity、promotion/import/link 层；这些方向与用户明确修正和 AgentDash 当前领域模型冲突。
- `active_label/working_label` 是否持久化为 Task 字段还是仅 UI projection，需要后续 design 决策；无论哪种，业务命名都必须使用 Task 语义。
- `task_write` 的 snapshot mode 需要设计 precondition，否则完整列表覆盖会和多 agent 并发 Task 更新冲突。
