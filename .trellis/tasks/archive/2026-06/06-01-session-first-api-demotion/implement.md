# 执行计划

## 顺序

1. **扫描残留 session-first 路径**
   - 运行 `rg -n "LifecycleRun\.session_id|list_by_session|SessionBinding|binding_id|owner_type|owner_id|active_step_key|lifecycle_step_key|runsBySessionId"` 生成完整清单。
   - 分类为：可直接删除、需要替换为 trace adapter、需要前置任务先完成。

2. **删除 `story_sessions.rs` route 文件**
   - 删除 `crates/agentdash-api/src/routes/story_sessions.rs`。
   - 从 API router 注册中移除 `/stories/{id}/sessions` 路由。
   - 删除 `CreateStorySessionRequest`、`SessionBindingResponse`、`StorySessionDetailResponse`、`UnboundBindingResponse` 等关联 DTO。

3. **删除 `LifecycleRunRepository::list_by_session`**
   - 从 `LifecycleRunRepository` trait 删除 `list_by_session` 方法。
   - 从 Postgres implementation 删除对应实现。
   - 修复所有编译错误（caller 必须已在前置任务中迁移）。

4. **删除 `LifecycleRun.session_id` 读写路径**
   - 从 `LifecycleRun` domain entity 删除 `session_id` 字段。
   - 从 `LifecycleRun` repository 的 INSERT / UPDATE / SELECT 中删除 `session_id` 列。
   - 修复所有编译错误。
   - 编写 DB migration：`ALTER TABLE lifecycle_runs DROP COLUMN session_id`。

5. **清理 `StoryRunOverviewDto`**
   - 从 `StoryRunOverviewDto` 删除 `session_id` 字段（若未在 frontend-views 中完成）。
   - `lifecycle_id` 字段保留为 internal，对外 API 改为 `graph_instances[]`。
   - 更新 `story_runs.rs` 使用 `LifecycleRunView`。

6. **清理 session query 路径**
   - `ListSessionsQuery.owner_type` / `owner_id`：降级为 `project_id`-only filter。
   - 删除 `SessionMetaStore` 中按 `owner_type` / `owner_id` 查询的路径。
   - session list API 只支持 `project_id` filter + `runtime_session_id` exact lookup。

7. **清理 step vocabulary**
   - `EffectiveSessionContract.active_step_key` → `active_activity_key`（若未在 spec-convergence 中完成）。
   - `LifecycleExecutionEntry.step_key` → `activity_key`。
   - `LifecycleExecutionEventKind::StepActivated` / `StepCompleted` → `ActivityActivated` / `ActivityCompleted`。
   - 更新 contracts generator 与 generated TS types。

8. **清理 binding vocabulary**
   - 在全项目范围内删除 `binding_id`、`SessionBinding`、`owner_type`、`owner_id` 的 response/DTO 使用。
   - `attach_story_link_for_session` / `ensure_freeform_lifecycle_run` 等 helper 若仅服务于已删除路由，一并删除。

9. **标记 session routes 为 RuntimeTrace**
   - 保留的 session routes（events / cancel / fork / projection / lineage）添加 `// RuntimeTrace API` 注释标记。
   - 确认这些 routes 不返回 business owner / lifecycle progress / permission scope。
   - 可选：路由前缀从 `/sessions/{id}` 改为 `/traces/{id}`（或保留路径但更新文档）。

10. **最终 contract scan**
    - 重新运行 `cargo run -p agentdash-contracts --bin generate_contracts_ts`。
    - `rg` 验证无残留。
    - `pnpm -F app-web type-check` 验证前端编译。

## 质量门

- `rg -n "LifecycleRun\.session_id|list_by_session|SessionBinding|binding_id|owner_type|owner_id|active_step_key|lifecycle_step_key|runsBySessionId"` 不命中目标事实源路径（trace adapter 除外）。
- `story_sessions.rs` 已删除。
- `lifecycle_runs` 表不含 `session_id` 列。
- session route 不返回 business owner truth。
- workflow / task / story / project route 使用 subject / agent / run contracts。
- `cargo build` 和 `pnpm -F app-web type-check` 通过。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-session-first-api-demotion`
- `cargo build -p agentdash-api -p agentdash-application -p agentdash-contracts -p agentdash-infrastructure`
- `cargo run -p agentdash-contracts --bin generate_contracts_ts`
- `pnpm -F app-web type-check`
- `rg -n "LifecycleRun\\.session_id|list_by_session|SessionBinding|binding_id|owner_type|owner_id|active_step_key|lifecycle_step_key|runsBySessionId" crates/ packages/`
- `rg -n "story_sessions" crates/agentdash-api/src/`
- `git diff --check -- .trellis/tasks`

## 后续交接

- 本任务是 B7 Legacy API And Field Removal 的主任务，完成后整轮 session-first 控制面遗产消除。
- 后续进入 B8 Verification：端到端验证 target state blueprint 的所有不变量。
- `ActivityLifecycleDefinition` / `WorkflowDefinition` 的大范围重命名可锁进显式最终重命名任务，期间不允许新增旧语义使用。
