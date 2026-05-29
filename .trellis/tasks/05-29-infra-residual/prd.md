# infra 收尾（时间戳 TIMESTAMPTZ / sqlite migration / io::Result 类型化）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 5（H）。类：丙（前轮 `infra-persistence-dedup` 只做 session_repository 去重）。Wave 3，**依赖 `error-model-unify`**。

## Goal

收掉第一波 infra 去重未覆盖的三处结构债：时间戳类型、sqlite schema 演进、session port 错误类型。

## 现状证据

- Postgres 时间戳存 **TEXT**（`0001_init.sql:11/21/35` `created_at TEXT`），写 `.to_rfc3339()`（260+ 处/22 文件），读经 `parse_pg_timestamp_checked`（`postgres/mod.rs:68-108`）试 **6 种** chrono 格式含手补 `+08`→`+08:00`。且 `backend_repository` 的 `created_at`/`last_claimed_at` 已用 native `TIMESTAMP`——**自相矛盾**。后果：无原生时间索引/范围、排序是字典序。
- sqlite `initialize()`（`sqlite/session_repository.rs:33-243`）内联 `CREATE TABLE` + `let _=ALTER ... ADD COLUMN`（:222-234 忽略错误）+ 数据 backfill；postgres 用 `sqlx::migrate!`——**两套 schema 演进机制，sqlite 吞 ALTER 失败**。
- session SPI port（`session_persistence.rs:685+` `SessionMetaStore`/`SessionEventStore`/...）返回 `io::Result`，`NotFound` 靠 `ErrorKind` + 中文串——最事务化的部分错误类型最弱。

## 已拍板决策（2026-05-29）

- **砸掉 sqlite 后端**：`agentdash-local` 改用嵌入式 PG（`postgres_runtime.rs` 已具备）。一并消除病灶 5 的 session repo ~4.7k 行 fork + sqlite schema 双机制 + `let _=ALTER` 吞错。第一波 `infra-persistence-dedup` 跳过的 workflow discriminator fork 也随 sqlite 移除一并消失。

## Scope

1. **移除 sqlite 后端**：删 `crates/agentdash-infrastructure/src/persistence/sqlite/`；`agentdash-local/src/runtime.rs:466` 等改用 `PostgresRuntime` 嵌入式实例；清理 `Cargo.toml` 中 sqlite-only 依赖/feature。
2. Postgres 时间戳列迁 `TIMESTAMPTZ`（走 migration），bind/read `chrono::DateTime<Utc>`，删 `parse_pg_timestamp_checked` 与 260 处 `.to_rfc3339()` 噪音。
3. session port `io::Result` → `SessionStoreError`（与 `error-model-unify` 对齐，边缘映射）。

## 依赖与协调

- **前置**：`error-model-unify`（`SessionStoreError` 与 `ApplicationError`/`DomainError` 协调）。
- sqlite 去留决策若选"砍"，与第一波 `infra-persistence-dedup` 的 de-fork 残留合并考虑。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `ls crates/agentdash-infrastructure/src/persistence/sqlite` 不存在；`rg "sqlite" crates/agentdash-local/src crates/agentdash-infrastructure/Cargo.toml | wc -l` 仅余 sqlx feature 必需项（journal 说明）
- [ ] `rg "created_at TEXT|updated_at TEXT" crates/agentdash-infrastructure` = **0**（列为 `TIMESTAMPTZ`）
- [ ] `rg "fn parse_pg_timestamp_checked" crates/agentdash-infrastructure` = **0**；`rg "to_rfc3339" crates/agentdash-infrastructure | wc -l` 大幅下降（目标 < 20，残留逐条 journal）
- [ ] `rg "io::Result" crates/agentdash-spi/src/*session_persistence* crates/agentdash-infrastructure/src/persistence/session_core.rs | wc -l` = **0**（改 `SessionStoreError`）
- [ ] `cargo check --workspace` exit 0；migration 可 up；infra/session 测试绿
