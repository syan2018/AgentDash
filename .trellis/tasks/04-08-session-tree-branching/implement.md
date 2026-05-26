# 会话分支与状态投影实施计划

## Phase 0. Dependency Gate

- [x] 确认父任务 `.trellis/tasks/archive/2026-05/05-25-context-compaction-architecture-enhancement` 已交付 `session_compactions`、`session_projection_segments`、`session_projection_heads`。
- [x] 确认 `ContextProjector` restore 路径支持 active compaction + segments + suffix。
- [x] 确认 fork child initial projection 需要的 segment / replacement envelope 表达方式。

## Phase 1. Repository And Migration

- [x] 新增 `session_lineage` schema。
- [x] 复用已存在的 `session_projection_heads` schema，并补齐 branch/fork 读写场景。
- [x] 扩展 SPI repository trait：
  - [x] upsert / insert lineage edge。
  - [x] list direct children。
  - [x] list ancestors。
  - [x] list descendants with stable ordering。
  - [x] read / update projection head。
- [x] 同步 PostgreSQL repository。
- [x] 同步 SQLite repository。

Validation:

```powershell
cargo test -p agentdash-infrastructure session_repository -- --nocapture
```

## Phase 2. Fork Use Case

- [x] 实现 fork point resolver：支持 `event_seq` / `MessageRef` / checkpoint id。
- [x] 创建 child session meta。
- [x] 写入 `session_lineage` edge。
- [x] materialize child initial compaction checkpoint。
- [x] 初始化 child projection head。
- [x] 写入 `session_branch_forked` platform event。

Validation:

```powershell
cargo test -p agentdash-application session fork -- --nocapture
```

## Phase 3. Rollback Use Case

- [x] 实现 rollback target resolver。
- [x] 追加 rollback platform event。
- [x] 更新 `session_projection_heads.model_visible`。
- [x] 确保 active compaction / suffix 查询不会越过 rollback 后的 projection head。
- [x] 保持 UI event backlog 完整，不删除 `session_events`。

Validation:

```powershell
cargo test -p agentdash-application session rollback continuation -- --nocapture
```

## Phase 4. Branch-aware Restore

- [x] 更新 continuation / executor restore 入口，统一通过 projection head 找 active compaction。
- [x] 覆盖 fork child restore。
- [x] 覆盖 parent fork 后继续追加事件不影响 child restore。
- [x] 覆盖 rollback 后 restore。

Validation:

```powershell
cargo test -p agentdash-application continuation -- --nocapture
```

## Phase 5. API And Frontend

- [x] 新增 branch / lineage API。
- [x] 前端 session list 改用 lineage grouping。
- [x] Session detail 增加 fork source / branch status 展示。
- [x] 增加前端测试。

Validation:

```powershell
pnpm test -- --run session-grouping
pnpm typecheck
```

## Phase 6. Quality Gate

- [x] `cargo fmt`
- [x] Rust targeted tests。
- [x] PostgreSQL / SQLite migration verification。
- [x] 前端 targeted tests。
- [x] 用 `trellis-update-spec` 固化 session lineage / projection head 契约。

## Risky Files

- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
- `crates/agentdash-infrastructure/migrations/*`
- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-api/src/routes/project_sessions.rs`
- `packages/app-web/src/features/agent/session-grouping.ts`
- `packages/app-web/src/components/layout/session-shortcut-rows.ts`

## Rollback Points

- Phase 1 只新增仓储能力，可独立验证。
- Phase 2 fork 成功边界必须包含 lineage + child initial checkpoint。
- Phase 3 rollback 只移动 projection head，不物理删除历史。
- Phase 5 前端切换到 lineage API 时需要与后端 DTO 同步合入。
