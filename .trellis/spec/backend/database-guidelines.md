# 数据库规范

> AgentDashboard 数据库使用规范。当前使用 SQLite + SQLx。

---

## 概览

| 项目 | 说明 |
|------|------|
| 数据库 | SQLite（预研阶段） |
| ORM | SQLx（编译期 SQL 检查） |
| 数据归属 | 业务数据归云端，执行状态归本机 |
| 迁移方式 | 应用启动时 `initialize()` 自动建表 |
| Session 存储 | JSONL 文件（每个 session 一个 `.jsonl`） |

---

## 存储分层

### 云端（SQLite 关系数据库）

业务数据使用 SQLite 持久化，通过 Repository 模式访问：

```rust
// agentdash-infrastructure/src/persistence/sqlite/
├── project_repository.rs
├── workspace_repository.rs
├── story_repository.rs
├── task_repository.rs
├── backend_repository.rs
├── session_binding_repository.rs
└── settings_repository.rs
```

### 本机（JSONL 文件）

Session 执行历史使用 JSONL 格式追加写入：

- **位置**: `{workspace_root}/.agentdash/sessions/{session_id}.jsonl`
- **格式**: 每行一个 `SessionNotification` JSON 对象
- **元数据**: `{session_id}.meta.json` 存储 `SessionMeta`

---

## SQLx 使用约定

### Repository 实现模板

```rust
use sqlx::SqlitePool;
use agentdash_domain::story::{Story, StoryRepository};
use agentdash_domain::common::error::DomainError;

pub struct SqliteStoryRepository {
    pool: SqlitePool,
}

impl SqliteStoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'draft',
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO stories (id, project_id, title, status, context, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(story.id.to_string())
        .bind(story.project_id.to_string())
        .bind(&story.title)
        .bind(serde_json::to_string(&story.status).unwrap_or_default())
        .bind(serde_json::to_string(&story.context).unwrap_or_default())
        .bind(&story.created_at)
        .bind(&story.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        Ok(())
    }
    // ...
}
```

### 错误转换

基础设施层错误必须转为领域错误：

```rust
// ✅ 正确：统一转为 DomainError
.map_err(|e| DomainError::Storage(e.to_string()))

// ❌ 错误：直接暴露 sqlx::Error 给上层
.map_err(|e| e)?  // sqlx::Error 不应泄露到领域层
```

---

## JSON 序列化约定

### 结构化字段存储

复杂值对象以 JSON 文本存入 SQLite TEXT 列：

```rust
// 写入
.bind(serde_json::to_string(&story.context).unwrap_or_default())

// 读取
let context: StoryContext = serde_json::from_str(&row_context)
    .unwrap_or_default();
```

### 字段命名

- **数据库列名**：`snake_case`（如 `project_id`、`created_at`）
- **JSON 序列化**：`snake_case`（与 HTTP DTO 一致）
- **SQLite 不区分大小写**，但建议统一小写

---

## JSONL Session 存储

### 追加写入

```rust
pub async fn append(
    &self,
    session_id: &str,
    notification: &SessionNotification,
) -> std::io::Result<()> {
    tokio::fs::create_dir_all(&self.base_dir).await?;
    let path = self.file_path(session_id);
    let line = serde_json::to_string(notification)?;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    Ok(())
}
```

### Meta 持久化

`SessionMeta` 包含 `last_execution_status` 等运行时状态，必须在 turn 开始/结束时写入，不允许靠扫 JSONL 历史推断。

参考：`backend/quality-guidelines.md` 中的"Session 执行状态持久化规范"。

---

## 迁移策略

### 当前阶段

- 使用 `initialize()` 方法在应用启动时自动建表
- `CREATE TABLE IF NOT EXISTS` 确保幂等
- 数据库文件位于 `{data_dir}/agentdash.db`

### 未来演进

当 schema 变更频繁时，迁移到 SQLx 正式迁移系统：

1. 在 `migrations/` 目录存放 SQL 文件
2. 使用 `sqlx::migrate!()` 自动运行
3. 保留版本追踪

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 不处理文件不存在的情况 | `ErrorKind::NotFound` 时返回空列表或 `None` |
| 一次性读取大 JSONL 文件 | 考虑流式读取 / 分页 |
| 不创建父目录 | 写入前先调用 `create_dir_all` |
| 在领域层直接使用 `sqlx` | 领域层只定义 Repository trait |
| JSON 字段用 `camelCase` | 统一使用 `snake_case` |

---

## 相关规范

- [Repository Pattern](./repository-pattern.md) — Repository 接口定义和依赖注入
- [Quality Guidelines](./quality-guidelines.md) — Session 执行状态持久化规范
- [Error Handling](./error-handling.md) — 错误处理约定

---

*更新：2026-03-29 — 对齐 SQLite + SQLx + JSONL 实际存储架构*
