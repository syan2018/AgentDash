# 通用 Task 工具集设计

## 背景

Story/Task subject 模型已收束为：

- `LifecycleRun.tasks` 保存 run-scoped Task plan facts。
- Story 只保存主题、上下文和流程状态。
- Story Task projection 从 Story-bound run、linked run、`story_ref` 推导。
- runtime artifacts / latest node / execution evidence 由 SubjectExecution / Lifecycle projection 承担。

通用 Task 工具集需要在这个模型上增加 agent-facing 的高频清单操作体验。参考实现里的清单工具只作为交互模式参考；AgentDash 正式命名必须统一使用 Task。所有工具都应直接读写或投影 `LifecycleRun.tasks`。

## 目标边界

### Run-scoped Task Plan

Run-scoped Task plan 继续由 `LifecycleRun.tasks` 承担，状态语言维持 `open | active | review | blocked | done | dropped`。它代表可跨 session、可投影到 Story、可被 fanout/assignment 消费的持久计划项。

### Task List Tool View

Task tool view 是同一组 Task plan facts 的工具视图。它可以支持模型一次性提交“当前任务清单快照”，也可以支持局部 patch；服务端应把写入解释为 `LifecycleRun.tasks` 的 create/update/reorder/status transition，而不是写入另一个清单 store。

该视图需要解决两个问题：

- 模型既需要快速读回完整 Task 上下文，也需要用一个入口更新 Task 的内容、状态、references 和排序。
- UI 需要稳定展示顺序、状态、owner/subagent 关联、Story/run linkage、context references 和执行投影摘要，而不是从 transient prompt state 猜测。

## 工具集草案

Agent-facing 第一版工具集应优先保持数量最少、操作最内聚。默认只规划两个工具：

- `task_read`
- `task_write`

### `task_read`

`task_read` 负责读取当前 run / agent scope 的 Task view。它必须支持完整读回，而不是只返回薄清单。参考 CLI 的单命令多视图模式，`task_read` 通过 `mode` 参数切换读取形态，而不是拆成多个读工具。

建议读取模式：

- `overview`：默认模式，返回当前 scope 的进度汇总、active/review/blocked 关键项、最近更新时间和 compact Task rows。
- `list`：返回可筛选 Task 列表，支持 `statuses/agent_id/include_archived/story_ref/source_task_id/limit/cursor`。
- `detail`：读取单个或少量 Task 的完整 details，包含 body、context refs、subject linkage、assignment、source/child 关系和 audit/version。
- `context`：围绕 Task 的 context references 读取，按 `slot/required/delivery/priority` 分组，适合 agent 在执行前拉取上下文清单。
- `execution`：返回 Task 关联的 compact execution projection，例如 linked run、SubjectExecution 状态、latest evidence summary；不把 runtime artifacts 变成 Task facts。
- `projection`：从 Story 或 run linkage 角度读取 projection，回答“这个 Story/Run 现在看见哪些 Task，来源是什么”。

所有 mode 都应支持 `format` 或等价参数控制 compact/full，默认 compact；需要机器消费时返回稳定 JSON schema。写工具完成后返回的结果应等价于一次 `task_read`，并可指定 `return_mode`。

建议读回字段：

- scope：`project_id`、`run_id`、`agent_id?`、filter、view revision。
- Task core：`id/title/body/status/priority/order/created_at/updated_at/archived_at`。
- ownership：`created_by_agent_id/owner_agent_id/assigned_agent_id/source_task_id`。
- context：`context_refs[]`，字段沿用 `ContextSourceRef` 的 `kind/locator/label/slot/priority/required/max_chars/delivery`。
- subject linkage：`story_ref`、owning run、linked run / Story projection source summary。
- execution summary：相关 SubjectExecution / linked run 的 compact status，不把 runtime artifacts 写回 Task facts。
- audit/version：每个 Task 的 `updated_at` 或后续 revision，用作写工具 precondition。

`task_write` 是唯一写入口，负责 create/update/status/reorder/drop/reference 变更。它可以有两种 mode：

- `patch`：提交 operations 列表，适合小步更新。operation 可表达 `upsert_task`、`patch_task`、`set_status`、`reorder_tasks`、`drop_task`、`replace_context_refs`。
- `snapshot`：提交当前 scope 的期望 Task 集合，适合模型按完整清单维护。服务端将 snapshot 转成明确 diff，并要求 base revision / updated_at precondition，避免并发覆盖。

单独 `task_set_status` 没有必要。状态推进只是 `task_write` 的一种 operation；把它拆成第三个工具会迫使模型在“改状态”和“改 Task 详细内容”之间切换，降低内聚性。

不在第一版拆出 `task_create`、`task_update`、`task_reorder`、`task_drop`、`task_split`、`task_merge`。这些动作默认归入 `task_write` 的 mutation 语义。只有当权限、审计、冲突处理或 UI 交互证明需要独立命令时，后续才增加单项工具。

`task_assign` / `task_fanout` 属于已有 Task assignment / fanout 能力的衔接点，不作为通用 Task 工具集的重复入口。通用工具只维护 Task plan facts；派发工具消费这些 facts。

工具可以在面向模型的描述中说明“可作为自己的待办清单使用”，但 schema、route、DTO、事件、store 和组件命名必须使用 Task。

写工具返回应复用 `task_read` 的完整 Task view，原因是 agent 调用后需要立即获得一致视图，UI 也可以用同一事件更新。

## CLI 参考原则

CLI 工具给这组 agent-facing tools 的启发是接口数量少、参数表达强：

- Trellis `task.py` 用 `current --source`、`list --status/--mine`、`list-context` 在同一 Task CLI 下表达不同读视图。
- Trellis `get_context.py` 支持 text / JSON 和 phase/package 等上下文模式，说明同一读入口需要面向人和机器两种输出。
- pi coding-agent 暴露 `read/write/edit/bash` 这类少数核心工具，同时用 `--mode json/rpc`、`--tools`、`--exclude-tools` 控制运行模式和工具面。

因此 AgentDash Task 工具不按 CRUD 或状态动作拆散，而是保持 `task_read` / `task_write` 两个核心入口，用 mode、filter、return view 和 precondition 表达厚 Task 的不同读写姿势。

## 事件与持久化

持久事实源固定为 `LifecycleRun.tasks`。工具调用的审计事件可以进入 `LifecycleRun.execution_log` 或现有 state_change / lifecycle event 通道，但不新增第二套清单表。

## UI 投影

前端建议提供两个层级：

- AgentRun workspace 内的 Task 面板：支持清单式快速维护当前 run Task。
- Task plan 面板内的 subagent / assignment summary：显示与 Task 相关的执行者、linked runs、SubjectExecution 证据。

Story 页面仍只显示 Story Task projection，不直接显示另一套清单。

## 权限与审计

Task 工具默认限定在当前 run / owner agent scope。跨 agent 或跨 run 修改需要明确 capability scope。

所有写工具应记录：

- session id
- run id
- agent id
- tool call id 或 turn id
- changed task ids
- link/promote 的 task id

## 待研究确认

- Codex plan/update_plan 的状态语言和使用约束。
- Claude Code `TodoWrite` / `TodoRead` 的字段、调用时机和 UI 行为。
- pi-mono 是否存在可参考机制。
- 参考实现哪些清单式交互适合映射成 Task snapshot / patch 工具。
- 两工具模型是否足够覆盖第一版 agent 高频路径；任何新增工具都必须说明不能并入 `task_read` / `task_write` 或既有 assignment/fanout 命令的原因。
