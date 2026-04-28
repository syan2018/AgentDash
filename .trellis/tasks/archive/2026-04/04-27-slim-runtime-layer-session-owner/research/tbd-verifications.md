# TBD 核对报告 — 04-27-slim-runtime-layer-session-owner

- **日期**: 2026-04-27
- **范围**: 主线 PRD 5 个 [TBD] 事项事实核对
- **方法**: Grep / Read 代码与 migration，不做修改

---

## [TBD-1] DB 中 binding_kind='task' 的 lifecycle definition 实例

**核对方法**：
- `Grep WorkflowBindingKind::Task` 全仓
- `Grep binding_kind` 扫 `migrations/` 与 `builtins/`
- 读 `migrations/0001_init.sql` 确认 `lifecycle_definitions` 表结构

**现状证据**：

1. **DB seed 路径**：没有独立 `seeds/` 目录。builtin lifecycle 是**运行时惰性写入**：
   - `crates/agentdash-application/src/workflow/builtins/trellis_dag_task.json:5` 显式声明 `"binding_kind": "task"`。
   - 通过 `build_builtin_workflow_bundle()` ([definition.rs:111](crates/agentdash-application/src/workflow/definition.rs)) 在 project 创建或用户触发时写入 `lifecycle_definitions` 表。
   - 调用点：`crates/agentdash-api/src/routes/workflows.rs:225`（需用户主动调 API 注册）+ project 初始化自动安装 `builtin_workflow_admin`（Project kind）+ 存量测试 / 开发 project 可能已注册过 `trellis_dag_task` 模板。

2. **生产代码中 `WorkflowBindingKind::Task` 使用点**（除 domain enum 本体和测试外）：
   - `crates/agentdash-mcp/src/servers/workflow.rs:334` — 解析 MCP 传入字符串 `"task"` → enum（让用户通过 MCP 工具自定义 lifecycle 时仍可指定 task kind）
   - **没有其他生产手动创建点**。其余命中点全是单元测试 fixture（`catalog.rs:540/553/614`, `workflow_contribution.rs:84/103`, `context/workflow_bindings.rs:140/158`, `step_activation.rs:479`, `definition.rs:152`, `entity.rs:712/729`, `value_objects.rs:1241/1260`）。

3. **lifecycle_definitions 表**（`migrations/0001_init.sql:239-253`）：`binding_kind TEXT NOT NULL`，是普通 text 列；`workflow_definitions` 亦然（229）。`lifecycle_runs` 在 `0008_lifecycle_run_session_id.sql:8` 已 **DROP COLUMN binding_kind**（迁到 session_id），所以真正需要关心的只剩 `lifecycle_definitions.binding_kind` / `workflow_definitions.binding_kind`。

**对主线的影响结论**：

- **生产入口仅 2 个**：(a) MCP 字符串 → enum 解析，(b) builtin `trellis_dag_task.json` 惰性注册。前者单点可改/拦截；后者只需改 json 的 `binding_kind` 字段即可。
- **存量 DB 数据风险中等**：开发 / staging / 生产 DB 里如果跑过 builtin 注册或有用户自建的 `kind=task` lifecycle，会留下历史行。数量**取决于部署历史**，代码层不能直接答数字 → `[NEED_FURTHER_INVESTIGATION：需要跑 DB 诊断 SELECT COUNT(*) FROM lifecycle_definitions WHERE binding_kind='task']`。
- **迁移策略复杂度：低**。写一条 `UPDATE lifecycle_definitions SET binding_kind='story' WHERE binding_kind='task'`（+ workflow_definitions 同样）即可；配套改 builtin json。由姊妹任务 `04-27-story-task-cleanup-tail` 已登记为 R16 收尾项，可直接实施。

---

## [TBD-2] StateChange.since_id 游标迁移可行性

**核对方法**：
- 读 `crates/agentdash-domain/src/story/state_change_repository.rs`
- 读 `crates/agentdash-domain/src/story/value_objects.rs`（StateChange）
- `Grep since_id` + 前后端消费点
- 读 `migrations/0001_init.sql:68-76, 120-131`，对比 `state_changes` vs `session_events` schema

**现状证据**：

- **StateChange 游标**：`id BIGSERIAL PRIMARY KEY`（0001_init:69），全局单调；API 侧 `GET /events/since/{since_id}` + `get_changes_since_by_project(project_id, since_id, 1000)` ([state_change_store.rs](crates/agentdash-infrastructure/src/persistence/postgres/state_change_store.rs) + [stream.rs:250-269](crates/agentdash-api/src/stream.rs))；前端在 `frontend/src/api/eventStream.ts` 和 `tests/e2e/story-context-injection.spec.ts` 中消费。
- **ChangeKind 覆盖范围**（value_objects.rs:67-77）：9 个 kind — `StoryCreated/Updated/StatusChanged/Deleted` + `TaskCreated/Updated/StatusChanged/Deleted/ArtifactAdded`。即 StateChange **只承载 Story/Task 业务事件**，不承载 agent session 消息。
- **SessionEvent 游标**：`session_events` 表 PK = `(session_id, event_seq BIGINT)`（0001_init:120-131），**游标是 per-session 单调**，非全局。`acp_sessions.rs` 处 `since_id` query 参数是 `u64` 对应 `event_seq`，逻辑：`if event.event_seq <= seq { continue }`（第 1641 行）。
- **索引**：`state_changes` 有 `idx_state_changes_project`；`session_events` 主键即 `(session_id, event_seq)`，天然按 session 定位。

**对主线的影响结论**：

- **游标无法直接平滑映射**：两者语义不同——`state_changes.id` 是**全局单调 + 按 project 过滤**；`session_events.event_seq` 是**per-session 单调**。如果主线要让 session event stream 成为"唯一审计源"替代 StateChange，需要补：
  1. 跨 session 的全局/project 级游标（新列 `global_seq BIGSERIAL` 或新索引 `(project_id, committed_at_ms, session_id, event_seq)`），否则前端 resume 拉不到"整个 project 最近变更"。
  2. 一种把"Story/Task 业务变更"映射进 `session_events.session_update_type` 的 schema 扩展（当前 session_event 是 ACP notification，业务 CRUD 不在里面）。
- **阻塞性**：**中高**。主线如果只是内部重构、保留 `/events/since` API 不变，需要新加"state_change 视图"或新表；不能简单删除 state_changes。建议主线保留双写兼容期。

---

## [TBD-3] Story session 的 label 值集现状

**核对方法**：
- `Grep owner_type.*Story` / `SessionBinding::new` + `Story`
- 读 `crates/agentdash-domain/src/session_binding/entity.rs` 看 label 字段定义

**现状证据**：

- **字段定义**（`entity.rs:21-22`）："自由标签，约定值: `execution`, `companion`, `planning`, `review`"。类型是 `pub label: String`，**没有 enum，没有 const**，完全 free-form。
- **Story owner 唯一的创建点**：[crates/agentdash-api/src/routes/story_sessions.rs:290,320-326](crates/agentdash-api/src/routes/story_sessions.rs)
  - `let label = req.label.unwrap_or_else(|| "companion".to_string());`
  - 即 API 默认 `"companion"`；客户端若不传也是 `"companion"`。
- **前端唯一传值点**：[frontend/src/features/story/story-session-panel.tsx:98](frontend/src/features/story/story-session-panel.tsx)：`label: "companion"`。测试用例写过 `"reviewer"`（AcpSystemEventCard.test.tsx:135），但指的是 companion_label 子字段，不是 SessionBinding.label。
- **DB 约束**：`session_bindings.label TEXT NOT NULL DEFAULT ''`，无 CHECK 约束。
- **Story owner 的其他读取点**（不写入新值，仅消费）：`canvases.rs:255,300`, `acp_sessions.rs:669,764,806,1008,1836`, `vfs_surfaces.rs:514`, `session-grouping.ts` 前端分组——全部是按 `owner_type == Story` 过滤查 binding，不关心 label 值。
- **Task owner 对比**：硬编码约定 `label="execution"`（`task/mod.rs:28`, `task/gateway/repo_ops.rs:286`, `0006_remove_task_session_id.sql:2`, `entity.rs:12`），通过 `find_by_owner_and_label(Task, id, "execution")` 查。

**对主线的影响结论**：

- **Story owner 的 label 实际值集 = {"companion"}**（单一值）。没有发现任何生产代码写入 `"execution"`/`"review"`/`"planning"` 到 Story binding。
- **可直接实施**：主线若想给 Story session 固定 label（"execution"），把 `story_sessions.rs:290` 的默认值改掉 + 加 DB migration 将存量 `"companion"` 改 `"execution"` 即可。现有单一值的现状反而降低了迁移风险。
- **隐患**：API 允许客户端传任意字符串（`req.label: Option<String>`），没有白名单校验。主线若要枚举化需同时加输入校验。

---

## [TBD-4] LifecycleRunService::activate_step 改造成本

**核对方法**：
- 读 `crates/agentdash-application/src/workflow/run.rs` 全文
- 读 `crates/agentdash-application/src/workflow/error.rs`
- `Grep activate_step` 找调用点

**现状证据**：

- **当前签名**（run.rs:138-147）：
  ```rust
  pub async fn activate_step(&self, cmd: ActivateLifecycleStepCommand) -> Result<LifecycleRun, WorkflowApplicationError>
  // cmd = { run_id: Uuid, step_key: String }
  ```
  内部：`load_run → run.activate_step(step_key) → run_repo.update(&run)`。**不在 transaction 里**，两步 IO 分别提交。
- **`bind_session_and_activate_step`**（run.rs:196-207）：同样两步非事务（`run_repo.update` 一次），但多一次领域层 `bind_step_session`。
- **调用点**（3 个）：
  1. `crates/agentdash-api/src/routes/workflows.rs:343` — HTTP POST /workflows/runs/{run_id}/steps/{step_key}/activate，直接转 ActivateLifecycleStepCommand。
  2. `crates/agentdash-application/src/workflow/orchestrator.rs:395` — `compose_task_runtime` 特例里调用（这是主线要消除的特例分支）。
  3. `crates/agentdash-application/src/workflow/orchestrator.rs:600` — `bind_session_and_activate_step` 被 orchestrator 激活 session 时调用。
- **错误语义**：`WorkflowApplicationError` 四种：BadRequest / NotFound / Conflict / Internal，均可被 API layer 消费（已在 `workflows.rs` 里映射）。`DomainError` 通过 `From` 自动转换。
- **事务边界**：**目前没有显式事务包裹**。`LifecycleRunService` 持有两个 `&dyn Repository`，repo 接口 async 方法各自连池。主线若要把 `activate_story_step` 暴露成 facade service 命令，需要决定是否引入事务（例如同时更新 run + 写 state_change + 更新 Story 状态）。
- **字段契合度**：facade 签名 `activate_story_step(story_id, step_key, user_input)` 需要：
  - `story_id → run_id` 转换：当前 repo 有 `list_by_session(session_id)` 但没有 `find_by_story_id`。Story 合入 session 后，需要新增 `find_active_run_for_story(story_id)`（或通过 Story session 的 SessionBinding → session_id → lifecycle_run 两跳）。
  - `user_input` 当前 `activate_step` 签名里没有 user input 概念，需要新增参数传递（透传到 kickoff prompt 装配器）。

**对主线的影响结论**：

- **改造成本：中等**。
  - 优点：错误/签名简单，已被 API 消费，直接包一层 facade 不困难。
  - 需要补：(a) Story → Run 查询路径（新 repo 方法或两跳），(b) user_input 参数透传，(c) 决定事务边界（至少把 run.update + state_change append 放进同一 tx，现状两步 IO 在并发下有裂缝）。
  - 阻塞项无。可以分步实施：先加 facade 保留非事务语义，再后续加事务。

---

## [TBD-5] Story aggregate 加载性能基线

**核对方法**：
- 读 `migrations/0001_init.sql:52-93`（stories / tasks 表）
- `Grep list_by_story` / `list_by_project` / `list_by_workspace` 调用点
- 读 `crates/agentdash-domain/src/task/repository.rs`
- 读 `crates/agentdash-domain/src/story/entity.rs`（task_count 字段）

**现状证据**：

- **Tasks 表**（`0001_init.sql:78-93`）：`id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding TEXT, artifacts TEXT, created_at, updated_at`。其中 `agent_binding` 和 `artifacts` 已经是 JSON 序列化 TEXT。注意：`session_id`/`executor_session_id` 列已被 `0006_remove_task_session_id.sql` 移除。
- **Stories 表**（52-66）：`task_count INTEGER NOT NULL DEFAULT 0` — **冗余计数列**，由 TaskAggregateCommandRepository 在 create/delete 时事务性维护（`task_repository.rs:256,336`）。
- **`list_by_story(story_id)` 调用点**（4 个）：
  1. `crates/agentdash-api/src/routes/stories.rs:319` — GET /stories/{id} 详情页 **hot path**。
  2. `crates/agentdash-application/src/reconcile/runtime.rs:75` — 启动时 reconcile，每个 story 遍历一次（启动期一次性）。
  3. `crates/agentdash-application/src/project/management.rs:106` — project-level 聚合视图；嵌套在 `list_by_project(stories)` 循环里调用。
  4. `crates/agentdash-application/src/story/management.rs:168` — Story detail 辅助函数，被视图层用。
  5. MCP servers：`task.rs:194`, `relay.rs:286`, `story.rs:471` — 代理工具暴露给 LLM。
- **`list_by_project(project_id)` for Task**（`task_repository.rs:117`）：存在但**在应用层没有被调用**（Grep 所有 `task_repo.list_by_project` 无命中），只有其他 repo 的同名方法被用。
- **`list_by_workspace` for Task**（`task_repository.rs:143`）：同样存在但**未被任何生产代码消费**。
- **没有找到真实 seed / fixture 数据**。无法量化单 Story 下 Task 数量。开发环境典型 Story 观察（基于 UI 与 PRD 描述）通常 3–10 task。

**对主线的影响结论**：

- **Hot path 在 Story 详情**（stories.rs:319）和 **Project 列表聚合**（project/management.rs:104-106，`stories × tasks` 双循环 O(N·M)）。后者已经是 N+1 问题的候选；合入 JSONB 后 `list_by_story` 变成 `story.tasks` 字段直取，**N+1 自动消失**，收益正面。
- **JSONB 列大小**：PostgreSQL JSONB 单列上限 ~1GB（实际建议 <1MB）。按每 task ~1KB（含 description/artifacts），单 story 承载 1000 个 task 约 1MB，即**预期规模下不存在容量顾虑**。
- **Pagination 需求**：
  - Story 详情页可能需要，但前端目前没有任何 task pagination 控件（`stories.rs:319` 直接返回全部 tasks），主线不需要先引入。
  - `TaskRepository::list_by_project` / `list_by_workspace` 既然无生产调用，合入时可直接删除（减负）。
  - `task_count` 冗余列：合入 JSONB 后变成可以改用 `jsonb_array_length(tasks)`，但保留该列对现有 UI 无破坏，**建议保留**。
- **风险项**：
  - MCP 工具路径（`mcp/servers/task.rs:194`, `relay.rs:286`, `story.rs:471`）会被 LLM 调用，**频率不可控**。合入后改为内存字段直取，性能更好；但要注意不能同时做冷启动 Story 全量加载。
  - 如果将来需要 task 跨 story 查询（例如 "全 project 所有 `in_progress` 的 task"），需引入 `jsonb_path_query` 或表级索引，但目前代码完全没有这种查询场景。
- **可直接实施**：**是**。现有 `list_by_project` / `list_by_workspace` 无调用者，`list_by_story` 合入后变 O(1)；typical 规模下 JSONB 内嵌无容量风险；`task_count` 维护逻辑可保留。

---

## 汇总

| TBD | 阻塞? | 额外工作 | 备注 |
|---|---|---|---|
| 1 | 否 | 加一行 UPDATE migration + 改 builtin json | 存量 DB 计数需跑 SQL 查询 |
| 2 | **是（设计分歧）** | 保留 state_changes 双写或新加全局游标到 session_events | 主线不能简单删除 StateChange |
| 3 | 否 | 改默认 label + 可选加校验 | Story label 实际值集单一，迁移简单 |
| 4 | 否 | 加 Story→Run 查询 + user_input 参数 + 可选事务 | facade 结构清晰，渐进式 |
| 5 | 否 | 无需 pagination，可顺手删两个未用方法 | N+1 问题反而被 JSONB 方案根除 |
