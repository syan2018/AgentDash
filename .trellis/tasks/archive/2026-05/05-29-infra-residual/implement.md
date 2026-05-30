# Infra Residual 执行计划

## 阶段 1：准备与基线

1. 记录基线：
   - `rg "sqlite|SqliteSessionRepository|SqlitePool" crates/agentdash-local/src crates/agentdash-infrastructure/src crates/agentdash-infrastructure/Cargo.toml -n`
   - `rg "created_at TEXT|updated_at TEXT" crates/agentdash-infrastructure/migrations -n`
   - `rg "fn parse_pg_timestamp_checked" crates/agentdash-infrastructure -n`
   - `rg "to_rfc3339" crates/agentdash-infrastructure -n`
   - `rg "io::Result" crates/agentdash-spi/src/*session_persistence* crates/agentdash-infrastructure/src/persistence/session_core.rs crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs -n`
2. 确认 `cargo check -p agentdash-infrastructure -p agentdash-local` 当前基线通过。

## 阶段 2：本机 runtime 切 embedded PostgreSQL

1. 给 `PostgresRuntime` 增加显式 data root/service dir 构造入口。
2. `agentdash-local::runtime` 使用 `PostgresRuntime` + `PostgresSessionRepository` 初始化 session persistence。
3. 在本机 config/runner 生命周期中持有 `PostgresRuntime`，避免 embedded 实例提前 drop。
4. 删除 sqlite repository module、re-export、local runtime sqlite imports 与 sqlite db path helper。
5. 清理 `session_core.rs` sqlite row impl。
6. 验证：
   - `rg "SqliteSessionRepository|SqlitePool|SqliteConnectOptions" crates/agentdash-local/src crates/agentdash-infrastructure/src -n` = 0
   - `Test-Path crates/agentdash-infrastructure/src/persistence/sqlite` = `False`
   - `cargo check -p agentdash-infrastructure -p agentdash-local`
   - `cargo test -p agentdash-infrastructure session_repository`

## 阶段 3：SessionStoreError 类型化

1. 在 `agentdash-spi/src/session_persistence.rs` 增加 `SessionStoreError` / `SessionStoreResult<T>`。
2. 批量替换 session persistence trait 返回值。
3. 改 `session_core.rs` helper 与 Postgres repository 实现，移除 `io::ErrorKind` 判定。
4. 更新 application/session、API/bootstrap、本机 runtime 等调用边缘错误映射。
5. 验证：
   - `rg "io::Result" crates/agentdash-spi/src/*session_persistence* crates/agentdash-infrastructure/src/persistence/session_core.rs crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs -n` = 0
   - `cargo check -p agentdash-spi -p agentdash-infrastructure -p agentdash-application -p agentdash-api -p agentdash-local`
   - session persistence 相关测试通过。

## 阶段 4：TIMESTAMPTZ migration 与 repository 时间类型

1. 新增 migration，将 TEXT timestamp 列转换为 `TIMESTAMPTZ`。
2. 修改历史 migrations 中 timestamp 列声明，保证新库直接得到正确 schema。
3. 逐 repository 将 timestamp row 字段改为 `DateTime<Utc>` / `Option<DateTime<Utc>>`。
4. 替换 bind 侧 `.to_rfc3339()` 为直接 bind。
5. 删除 `parse_pg_timestamp_checked`，保留必要的非 DB timestamp 字符串序列化残留清单。
6. 验证：
   - `rg "created_at TEXT|updated_at TEXT" crates/agentdash-infrastructure/migrations -n` = 0
   - `rg "fn parse_pg_timestamp_checked" crates/agentdash-infrastructure -n` = 0
   - `rg "to_rfc3339" crates/agentdash-infrastructure -n` < 20，并记录残留原因
   - migration up 通过
   - `cargo check --workspace`

## 阶段 5：收尾

1. 更新 `.trellis/spec/backend/database-guidelines.md` 与 session persistence 相关 spec。
2. 更新 wave2 checklist 证据。
3. 归档 `05-29-infra-residual`。

## 回滚点

- 阶段 2 独立提交；若 embedded PG 本机生命周期有问题，只回退 local runtime 切换，不影响 timestamp 与 SessionStoreError 规划。
- 阶段 3 独立提交；若调用边缘过多，先提交 SPI + postgres repo 编译闭环，再分边缘映射。
- 阶段 4 可按 repository 分批提交，但每批必须保持 `cargo check -p agentdash-infrastructure` 通过。
