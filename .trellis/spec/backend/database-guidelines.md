# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

当前使用文件系统（JSONL）进行数据持久化，没有使用传统数据库。

- **Session 存储**: 每个 session 一个 `.jsonl` 文件
- **位置**: `workspace_root/.agentdash/sessions/{session_id}.jsonl`
- **格式**: 每行一个 JSON 对象

---

## Query Patterns

```rust
// 追加写入
async fn append(&self,
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

// 全量读取
async fn read_all(&self,
    session_id: &str,
) -> std::io::Result<Vec<SessionNotification>> {
    let content = tokio::fs::read_to_string(self.file_path(session_id)).await?;
    content.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

---

## Migrations

当前不需要迁移机制。如未来切换到数据库，需要：

1. 使用 `sqlx` 或 `sea-orm` 进行版本管理
2. 在 `migrations/` 目录存放 SQL 文件
3. 应用启动时自动运行未执行的迁移

---

## Naming Conventions

- 文件名使用 snake_case: `{session_id}.jsonl`
- JSON 字段使用 camelCase（与前端一致）
- 目录结构按功能划分: `sessions/`, `logs/`

---

## Common Mistakes

| 错误 | 正确 |
|------|------|
| 不处理文件不存在的情况 | 使用 `ErrorKind::NotFound` 返回空列表 |
| 一次性读取大文件 | 考虑使用流式读取 |
| 不创建父目录 | 先调用 `create_dir_all` |
