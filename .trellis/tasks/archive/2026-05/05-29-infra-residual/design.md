# Infra Residual 设计

## 目标边界

本任务收掉 infrastructure 层剩余结构债，但不改变业务语义、API 响应 shape 或 session runtime 行为。

本次包含三条独立但有顺序关系的改造：

- 本机 session persistence 从 sqlite fork 切到 embedded PostgreSQL。
- Postgres timestamp 列与 repository bind/read 改为 `TIMESTAMPTZ` / `DateTime<Utc>`。
- session persistence SPI 从 `io::Result` 改为结构化 `SessionStoreError`。

## 事实确认

- `SqliteSessionRepository` 只通过 `agentdash-infrastructure::SqliteSessionRepository` 被 `agentdash-local/src/runtime.rs` 消费；云端 API 已使用 `PostgresSessionRepository`。
- `PostgresRuntime` 已能启动 embedded PostgreSQL，并且 API 通过持有 `_db_runtime` 保持 embedded 实例生命周期。本机 runtime 切换时也必须持有 runtime 句柄，不能只取 `pool` 后让 `PostgresRuntime` drop。
- sqlite session repository 与 postgres session repository 大量共享 `session_core.rs`，但 sqlite 文件仍保留完整 schema 初始化、legacy backfill 与测试。移除 sqlite 后，`session_core.rs` 的 sqlite row impl 与 sqlite-only 注释也应删除。
- Postgres migration 中大量 `created_at TEXT` / `updated_at TEXT` 仍存在，repository 写入大量 `.to_rfc3339()`，读取依赖 `parse_pg_timestamp_checked`。当前库已在部分新表使用 native `TIMESTAMPTZ`，说明目标类型已被项目接受。
- session persistence SPI 位于 `agentdash-spi/src/session_persistence.rs`，所有 store trait 仍返回 `io::Result`；postgres repository、session_core helper 与 application session 调用边缘都依赖该错误形状。

## 阶段边界

### 1. 移除 sqlite 后端

本机 runtime 使用 `PostgresRuntime` 启动本机 embedded PostgreSQL，并用 `PostgresSessionRepository` 初始化 session persistence。

实现要点：

- `agentdash-local` 的 runtime config 增加一个持有字段，例如 `session_db_runtime: Option<Arc<PostgresRuntime>>`，随 WebSocket config 克隆，保证 embedded PostgreSQL 生命周期覆盖本机进程。
- `PostgresRuntime` 增加可指定 data root / service dir 的构造入口，避免本机 runtime 通过全局 `AGENTDASH_DATA_ROOT` 影响其他 embedded PG 使用者。
- 本机 service name 使用 backend key 派生，数据库目录落在现有 `local_runtime_data_dir()/local_runtime_backend_key()` 下。
- 删除 `crates/agentdash-infrastructure/src/persistence/sqlite/`、`persistence::sqlite` module 与 `SqliteSessionRepository` re-export。
- 删除 `session_core.rs` 中的 `sqlx::sqlite::SqliteRow` impl 与 sqlite-only 注释。

原因：预研阶段无需保留 sqlite 兼容路径；统一到 embedded PostgreSQL 可以同时消除 sqlite schema 双机制和 session repository fork。

### 2. SessionStoreError 类型化

在 `agentdash-spi::session_persistence` 定义 `SessionStoreError`，并将所有 session persistence trait 返回值从 `io::Result<T>` 改为 `Result<T, SessionStoreError>`。

建议变体：

- `NotFound(String)`
- `InvalidInput(String)`
- `InvalidData(String)`
- `Database(String)`
- `Internal(String)`

实现要点：

- `session_core.rs` helper 返回 `SessionStoreResult<T>`。
- `PostgresSessionRepository` 的 `sqlx` 错误通过局部 mapper 转为 `Database`；row/JSON/状态解析错误转为 `InvalidData`；分页、lineage 参数错误转为 `InvalidInput`；缺失依赖记录转为 `NotFound`。
- application/session 调用边缘按既有错误语义映射为 `ApplicationError` 或保留局部 service error；不再依赖 `io::ErrorKind` 和中文错误串判定。

原因：session persistence 是 runtime 事务边界，必须保留 NotFound / InvalidInput / InvalidData / Database 语义，不能继续让上层解析 `io::ErrorKind` 和字符串。

### 3. TIMESTAMPTZ 单源

Postgres schema 与 repository 时间字段统一使用 `TIMESTAMPTZ` 和 `chrono::DateTime<Utc>`。

实现要点：

- 修改历史 migration 中新建表的 `created_at` / `updated_at` / `*_at` timestamp 列类型为 `TIMESTAMPTZ`，满足当前 prelaunch 项目从干净库初始化的正确 schema。
- 增加新 migration，将已有库中的 TEXT timestamp 列显式转换为 `TIMESTAMPTZ`，并处理 `DEFAULT CURRENT_TIMESTAMP`。
- repository bind `DateTime<Utc>` / `Option<DateTime<Utc>>`，不再手动 `.to_rfc3339()`。
- query row 直接读取 `DateTime<Utc>`；删除 `parse_pg_timestamp_checked`。
- 对确实不是数据库 timestamp 的字符串序列化保留 `.to_rfc3339()`，并在 journal/checklist 中说明残留原因。

原因：PostgreSQL 应承担 timestamp 类型、排序、索引与 range 查询语义；应用层不应维护 6 种文本时间解析格式。

## 风险与顺序

1. 先移除 sqlite，减少 session port 类型化时需要同步维护的实现数量。
2. 再改 `SessionStoreError`，让 session persistence 错误语义稳定后再碰 timestamp。
3. 最后改 TIMESTAMPTZ，因为它触及最多 repository 和 migration，必须分批编译与 grep。

## DDD / 分层约束

- `SessionStoreError` 属于 SPI persistence port，不进入 domain DTO 或协议 DTO。
- 本机 embedded PostgreSQL 装配归 `agentdash-local` composition root；repository 实现仍归 infrastructure。
- migration 与 repository 时间类型属于 infrastructure 事实，不回流到 domain 形成数据库协议依赖。
