# 数据库规范

> PostgreSQL + SQLx（云端），SQLite（本机会话元数据）。

---

## 存储分层

| 层 | 技术 | 职责 |
|----|------|------|
| 云端 | PostgreSQL + SQLx | 业务数据（Project/Story/Workspace/Session 等） |
| 本机 | SQLite | 本机会话持久化（`SqliteSessionRepository`） |

---

## 核心约定

- 基础设施层错误必须转换为 `DomainError`，不泄露 `sqlx::Error`
- 数据库列名和 JSON 序列化统一 `snake_case`
- 复杂值对象以 JSON 文本存入 `TEXT`
- 时间字段存 `TEXT`，读取时做健壮解析
- Repository 实现模式详见 [repository-pattern.md](./repository-pattern.md)

---

## 事务规则

- **单一聚合**：事务边界由对应 Repository 负责（如 `WorkspaceRepository` 内部同事务写 `workspaces` + `workspace_bindings`）
- **跨聚合**：使用显式 Command Port 或 Unit of Work，不要硬塞进单一 Repository trait
- Story aggregate 的 Task CRUD 走 `StoryRepository::update` 整体写回

---

## Schema 变更与迁移（必读）

**新增列不等于只改 `CREATE TABLE IF NOT EXISTS`** — 表已存在时新列不会自动添加。

### PostgreSQL

在 `crates/agentdash-infrastructure/migrations/` 下新增递增编号 `.sql` 文件。已提交的 migration 文件不可修改（sqlx 校验和机制）。

### SQLite

在 `initialize()` 中追加 `ALTER TABLE ADD COLUMN`，忽略 duplicate column 错误。

### Checklist

- [ ] 更新 `CREATE TABLE IF NOT EXISTS`（保证新建库完整）
- [ ] PostgreSQL 新增 migration 文件（不修改已有的）
- [ ] SQLite 在 `initialize()` 中追加 ALTER（忽略错误）
- [ ] 更新 INSERT/SELECT/UPSERT 语句和 `map_*_row` 函数
- [ ] 更新测试代码

### 删除旧列

- Repository 主线不再读写旧列
- PostgreSQL 新增 migration 用 `DROP COLUMN IF EXISTS`
- SQLite 可保留历史列但 Repository 不依赖
- 历史 migration 不可修改

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
