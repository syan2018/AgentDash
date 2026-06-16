# W2 Migration + Repository

## 状态

pending

## 依赖

- W1 done

## 目标

把 `LifecycleRun.tasks` 落到 PostgreSQL migration 和 repository roundtrip，清理 `StoryRepository` 的 Task durable CRUD 职责。

## 输入

- W1 的 domain shape。
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_run_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs`
- `crates/agentdash-domain/src/story/repository.rs`

## 范围

- 新增 migration，为 `lifecycle_runs` 添加 `tasks text DEFAULT '[]'::text NOT NULL`。
- 通过新增 migration 让 `stories.tasks` / 旧 Task child 字段退出主线 schema。
- 更新 LifecycleRun repository INSERT / SELECT / UPDATE / row mapping。
- 坏 JSON 错误上下文使用 `lifecycle_runs.tasks`。
- 清理 `StoryRepository` task CRUD / `find_by_task_id` / JSONB containment 查询。
- 保持普通任务只新增 migration，不修改既有 migration。

## 范围边界

- 当前没有需要保留的旧 Task 数据，migration 直接把 schema 推到目标模型，原因是项目仍处于预研期。
- repository 主线只读写 `LifecycleRun.tasks`，原因是双事实源会重新制造 Story-owned Task 与 Lifecycle-owned Task 的歧义。

## 验收

- `pnpm run migration:guard` 通过。
- clean DB migration 初始化通过。
- repository roundtrip 覆盖默认 `tasks`、更新、坏 JSON 错误上下文。
- 主线 repository 不从 Story aggregate 读写 Task facts。

## 产出记录

- 待填写。

## 风险与交接

- W3 依赖 repository 和 domain 字段稳定。
- W4 依赖 StoryRepository 清理后的新查询边界。
