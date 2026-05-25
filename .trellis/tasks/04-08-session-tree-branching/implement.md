# 会话分支与状态投影实施计划

## Phase 0. Dependency Gate

- [ ] 确认父任务 `.trellis/tasks/05-25-context-compaction-architecture-enhancement` 已交付 `session_checkpoints`。
- [ ] 确认 active projection cursor 已存在，或在本任务第一阶段补齐。
- [ ] 确认 `ProjectedTranscript` restore 路径支持 checkpoint + suffix。

## Phase 1. Repository And Migration

- [ ] 新增 `session_lineage` schema。
- [ ] 新增或补齐 `session_projection_heads` schema。
- [ ] 扩展 SPI repository trait：
  - [ ] upsert / insert lineage edge。
  - [ ] list direct children。
  - [ ] list ancestors。
  - [ ] list descendants with stable ordering。
  - [ ] read / update projection head。
- [ ] 同步 PostgreSQL repository。
- [ ] 同步 SQLite repository。

Validation:

```powershell
cargo test -p agentdash-infrastructure session_repository -- --nocapture
```

## Phase 2. Fork Use Case

- [ ] 实现 fork point resolver：支持 `event_seq` / `MessageRef` / checkpoint id。
- [ ] 创建 child session meta。
- [ ] 写入 `session_lineage` edge。
- [ ] materialize child initial checkpoint。
- [ ] 初始化 child projection head。
- [ ] 写入 `session_branch_forked` platform event。

Validation:

```powershell
cargo test -p agentdash-application session fork -- --nocapture
```

## Phase 3. Rollback Use Case

- [ ] 实现 rollback target resolver。
- [ ] 追加 rollback platform event。
- [ ] 更新 `session_projection_heads.model_visible`。
- [ ] 确保 active checkpoint 查询不会返回 rollback head 之后的 checkpoint。
- [ ] 保持 UI event backlog 完整，不删除 `session_events`。

Validation:

```powershell
cargo test -p agentdash-application session rollback continuation -- --nocapture
```

## Phase 4. Branch-aware Restore

- [ ] 更新 continuation / executor restore 入口，统一通过 projection head 找 checkpoint。
- [ ] 覆盖 fork child restore。
- [ ] 覆盖 parent fork 后继续追加事件不影响 child restore。
- [ ] 覆盖 rollback 后 restore。

Validation:

```powershell
cargo test -p agentdash-application continuation -- --nocapture
```

## Phase 5. API And Frontend

- [ ] 新增 branch / lineage API。
- [ ] 前端 session list 改用 lineage grouping。
- [ ] Session detail 增加 fork source / branch status 展示。
- [ ] 增加前端测试。

Validation:

```powershell
pnpm test -- --run session-grouping
pnpm typecheck
```

## Phase 6. Quality Gate

- [ ] `cargo fmt`
- [ ] Rust targeted tests。
- [ ] PostgreSQL / SQLite migration verification。
- [ ] 前端 targeted tests。
- [ ] 用 `trellis-update-spec` 固化 session lineage / projection head 契约。

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
