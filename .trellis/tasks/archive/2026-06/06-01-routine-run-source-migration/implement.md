# 执行计划

## 顺序

1. **定义 Routine dispatch intent 构造器**
   - 在 `agentdash-application/src/routine/` 下新增 `dispatch.rs`。
   - 提供 `build_routine_execution_intent()` 函数，输入 `Routine` + `RoutineExecution` + rendered prompt。
   - `SessionStrategy` → `run_policy` + `agent_policy` 映射逻辑在此实现。
   - 输出 `ExecutionIntent { source=routine, subject_ref=SubjectRef(RoutineExecution), ... }`。

2. **扩展 `RoutineExecution` domain entity**
   - 新增 `dispatch_refs: Option<RoutineDispatchRefs>` 字段，包含 `run_id`, `agent_id`, `frame_id`。
   - 新增 `mark_dispatched(refs)` 方法，替代现有 `mark_completed()` 在 dispatch 阶段的使用。
   - 保留 `mark_completed()` 但改为只能由 projection/callback 调用，代表真正 terminal。
   - 更新 `RoutineExecution` migration / repository（新增 `dispatch_run_id`、`dispatch_agent_id`、`dispatch_frame_id` 列）。

3. **重写 `execute_with_session` → `execute_with_dispatch`**
   - 构造 `ExecutionIntent` → 调用 `LifecycleDispatchService::dispatch()`。
   - 从 `ExecutionDispatchResult` 提取 refs → `execution.mark_dispatched(refs)`。
   - 删除 `resolve_session_id()` 的调用。
   - 删除 `LaunchCommand::routine_executor_input()` 的调用。
   - prompt 注入方式改为 `ExecutionIntent.initial_prompt` 或由 frame builder 处理。

4. **删除 `resolve_session_id` 与 `SessionStrategy` 直接 session 操作**
   - 删除 `RoutineExecutor.resolve_session_id()`（session 创建/复用/per_entity 的直接 session 操作）。
   - `SessionStrategy` enum 保留但改为 `DispatchStrategy`，只作为 intent policy 映射源。
   - 删除 `RoutineExecutor` 对 `SessionCoreService` / `SessionLaunchService` 的直接依赖。

5. **实现 Routine terminal projection**
   - 新增 `RoutineExecutionProjection` 结构：`terminal_status`、`terminal_summary`、`agent_status`、`source_refs`。
   - 投影链路：`RoutineExecution.dispatch_refs → LifecycleAgent.status + LifecycleRun.status`。
   - 提供 `project_routine_execution()` 函数供 API / frontend 使用。
   - terminal status mapping：agent running → execution running；agent terminal(completed/failed) → execution completed/failed。

6. **更新 Routine API routes**
   - `fire_scheduled` / `fire_webhook` / `fire_plugin` response 包含 dispatch refs。
   - routine execution list 返回 projection view 而非 raw status。
   - 同步更新 `agentdash-contracts` 中 routine 相关 DTO。

7. **清理旧依赖**
   - 从 `RoutineExecutor` 构造函数删除 `SessionCoreService` / `SessionLaunchService` 参数。
   - 删除 `RoutineLaunchSource` 在 session launch 中的使用。
   - 删除 `RoutineExecution.session_id` 字段（migration）。

## 质量门

- `RoutineExecutor` 不直接构造或操作 session。
- routine start 走 `LifecycleDispatchService::dispatch()`。
- `RoutineExecution.status=completed` 在 dispatch 层只代表"已派发"；真正 terminal 来自 projection。
- routine dispatch 结果说明创建的是独立 run 还是 same-run graph instance。
- `resolve_session_id` / `LaunchCommand::routine_executor_input` 被删除。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-routine-run-source-migration`
- `cargo build -p agentdash-application -p agentdash-api`
- `rg -n "resolve_session_id|LaunchCommand::routine_executor_input|RoutineLaunchSource" crates/agentdash-application/src/routine/`
- `rg -n "session_id" crates/agentdash-application/src/routine/executor.rs`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `frontend-actor-subject-views` 将 routine execution 页面从 session view 切到 dispatch refs + projection view。
- `session-first-api-demotion` 删除 routine API 中残余的 session-first fields。
