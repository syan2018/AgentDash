# 执行计划

## 顺序

1. **定义 Task dispatch intent 构造器**
   - 在 `agentdash-application/src/task/` 下新增 `dispatch.rs`，提供 `build_task_execution_intent()` 函数。
   - 输入：`task_id`、`project_id`、`phase(start/continue)`、`prompt`、`executor_config`、`identity`。
   - 输出：`ExecutionIntent { subject_ref: SubjectRef(kind=Task, id=task_id), run_policy, agent_policy, ... }`。
   - start → `run_policy=append_graph`（如果 Story 已有 active run）或 `create_linked_run`；continue → `run_policy=reuse_existing`。

2. **替换 `resolve_execution_session_id` 为 `resolve_task_execution_refs`**
   - 新函数通过 `LifecycleSubjectAssociationRepository::find_by_subject(SubjectRef(Task, task_id))` 查找 active run/agent。
   - 返回 `Option<TaskExecutionRefs { run_id, agent_id, frame_id }>` 而非 `Option<String>`。
   - 删除旧的 `find_task_execution_session_id()` 函数。

3. **重写 `start_task` / `continue_task`**
   - `start_task_inner`：构造 intent → 调用 `LifecycleDispatchService::dispatch()` → 从 `ExecutionDispatchResult` 提取 refs → 返回 `TaskExecutionResult`。
   - `continue_task_inner`：查找 active refs → 构造 intent(reuse_existing) → dispatch → 返回 result。
   - 删除 `gw_create_task_session()`、`bind_session_to_owner()`、`LaunchCommand::task_service_input()` 调用路径。

4. **重写 `cancel_task`**
   - 通过 `resolve_task_execution_refs()` 获取 agent_id → 从 `AgentFrameRepository` 获取 current frame → 从 frame 取 `runtime_session_refs` → cancel runtime session。
   - 不再直接使用裸 `session_id`。

5. **更新 `TaskExecutionResult` / `TaskSessionResult` DTO**
   - `TaskExecutionResult`：`run_ref`, `agent_ref`, `frame_ref`, `trace_ref?` 替代 `session_id`。
   - `TaskSessionResult` → `TaskExecutionView`：包含 `execution_status`（from projection）、`agent_ref`、`trace_ref`、`context_sources`。
   - 删除 `TaskSessionResult.agent_binding` 字段。

6. **创建 `TaskProjection` 投影**
   - 新增 `TaskProjection` 结构：`execution_status`、`execution_artifacts`、`current_agent_ref`、`current_activity_key`、`source_refs(run_id, agent_id, graph_instance_id, activity_key, attempt)`。
   - 投影链路：`SubjectAssociation → LifecycleAgent → AgentAssignment → ActivityAttemptState → outputs`。
   - 提供 `project_task_execution()` 函数供 API / frontend view 使用。

7. **删除 Task runtime 字段**
   - 从 `Task` domain entity 删除 `lifecycle_step_key`。
   - 将 `Task.agent_binding` 重命名为 `Task.agent_preference`，语义降级为 dispatch hint。
   - `Task.status` 保留为 spec-level（用户/业务流设置），不再由 runtime 回写。
   - `Task.artifacts` 保留为 spec-level（用户添加），execution artifacts 从 projection 读取。
   - 更新 migration（若有 DB schema 变化）。

8. **更新 API routes**
   - `start_task` / `continue_task` API response 使用新 DTO。
   - `get_task_session` → `get_task_execution`，返回 `TaskExecutionView`。
   - 同步更新 `agentdash-contracts` 与 generated TS types。

9. **清理 gateway 层**
   - 删除 `gw_create_task_session`、`gw_get_session_overview`（在 task context 中的使用）。
   - 删除 `bridge_task_status_event_to_envelope` 中的 session-first event bridge（改为通过 lifecycle event）。
   - 删除 `StoryStepActivationService` 对 `SessionCoreService` / `SessionLaunchService` 的直接依赖。

## 质量门

- `StoryStepActivationService` 不再直接构造 session 或 binding。
- `Task.lifecycle_step_key` 在 domain entity 中不存在。
- `resolve_execution_session_id` / `find_task_execution_session_id` 被删除。
- `TaskExecutionResult` 不以 `session_id` 作为主返回字段。
- Task continue 路径不经过 `LifecycleRun.session_id`。
- `TaskProjection` 所有字段都带 source refs。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-task-subject-execution-migration`
- `cargo build -p agentdash-application -p agentdash-api -p agentdash-contracts`
- `rg -n "resolve_execution_session_id|find_task_execution_session_id|lifecycle_step_key|gw_create_task_session|bind_session_to_owner" crates/agentdash-application`
- `rg -n "TaskSessionResult|agent_binding" crates/agentdash-application/src/task/ crates/agentdash-api/src/routes/`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `frontend-actor-subject-views` 将前端 Task 页面从 session-first 切到 `TaskExecutionView` / `SubjectExecutionView`。
- `session-first-api-demotion` 删除 task route 中残余的 session-first response fields。
- `companion-gate-lineage-migration` 的 companion-for-task 路径可以复用同一 dispatch intent 追加 companion graph。
