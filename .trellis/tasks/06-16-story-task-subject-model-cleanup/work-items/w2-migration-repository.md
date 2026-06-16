# W2 Migration + Repository

## 状态

done

## 依赖

- W1 done

## 目标

把 `LifecycleRun.tasks` 落到 PostgreSQL migration 和 repository roundtrip，清理 `StoryRepository` 的 Task durable CRUD 职责。

## 输入

- W1 的 domain shape。
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
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

- 新增 `crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql`：
  - `lifecycle_runs.tasks text DEFAULT '[]'::text NOT NULL`
  - `stories.tasks` / `stories.task_count` 退出主线 schema
- 更新 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`：
  - `RUN_COLS` / `RUN_INSERT_COLS` / INSERT / UPDATE 全量 roundtrip `LifecycleRun.tasks`
  - row mapping 解析 `tasks`，坏 JSON 错误上下文为 `lifecycle_runs.tasks`
  - focused tests 覆盖默认空 `tasks`、更新 roundtrip、坏 JSON 上下文
- 更新 `crates/agentdash-domain/src/story/repository.rs` 与 `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs`：
  - `StoryRepository` trait 只保留 Story 自身 CRUD
  - PostgreSQL Story repository 不再读写 `stories.tasks` / `task_count`
  - 移除 `find_by_task_id`、Task durable CRUD、JSONB containment 查询和 Story-owned Task state change 写入

验证：

- `pnpm run migration:guard` 通过
- `cargo test -p agentdash-infrastructure workflow_repository` 通过
- `cargo check -p agentdash-infrastructure` 通过

## 风险与交接

- W3 可基于稳定的 `LifecycleRun.tasks` repository roundtrip 更新 contract / generated TS。
- W4 需要重接旧 Story / Task API、auth、application task gateway、SubjectContextAssignment 和 projection 调用面；这些调用面不能再依赖 `StoryRepository::find_by_task_id` 或 Story-owned Task CRUD。
- W6 需要迁移 Story / Task MCP 的旧创建、查询、状态推进和 artifact 工具入口，避免继续假设 Task 持久化在 Story aggregate。
- 本节点未做 API route / contract / frontend 迁移，相关编译收口属于 W3-W6。
