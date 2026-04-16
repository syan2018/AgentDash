# Inline FS 独立存储重构

## Goal

将 inline_fs 的文件内容从嵌套在 Project/Story 实体 JSON TEXT 列中的 `ContextContainerFile[]` 数组，下沉到独立的 `inline_fs_files` 表。解决当前每次单文件写入都要整行 read-modify-write 的写放大、并发竞态和扩展性问题，同时为后续 Agent Knowledge FS 等新 owner 类型铺路。

**动机**：
- 当前写入链路：`InlineContentOverlay.write()` → `DbInlineContentPersister.persist_write()` → 加载整个 Project/Story → 在嵌套 `context_containers[i].provider.files[j]` 里改一个文件 → 序列化整个实体写回 TEXT 列
- 并发问题：两个 session 同时写同一 container 的不同文件，last-writer-wins 丢更新
- 扩展性：新增 owner 类型（如 `project_agent_link`）需要在 persister 里加 if-else 分支 + 注入对应 repository

## What I Already Know

### 当前存储结构

```
projects.config (TEXT column, JSON serialized)
  └── ProjectConfig.context_containers: Vec<ContextContainerDefinition>
        └── provider: InlineFiles { files: Vec<ContextContainerFile> }
              └── ContextContainerFile { path: String, content: String }

stories.context (TEXT column, JSON serialized)
  └── StoryContext.context_containers: Vec<ContextContainerDefinition>
        └── (同上)
```

### 当前读写链路

**写入**：
```
Agent tool → RelayAddressSpaceService.write_text()
  → 检测 mount.provider == "inline_fs"
  → InlineContentOverlay.write(address_space, mount, path, content)
    → HashMap 缓存（立即可读）
    → DbInlineContentPersister.persist_write(project_id, story_id?, container_id, path, content)
      → match owner_scope:
          project → ProjectRepo.get_by_id() → upsert_inline_file(containers, ...) → ProjectRepo.update()
          story   → StoryRepo.get_by_id() → upsert_inline_file(containers, ...) → StoryRepo.update()
```

**读取**：
```
Agent tool → RelayAddressSpaceService.read_text()
  → 先查 InlineContentOverlay（session 缓存）
  → miss → InlineFsMountProvider.read_text()
    → inline_files_from_mount(mount) → mount.metadata["files"] → BTreeMap<String, String>
    → BTreeMap.get(path)
```

**Mount 构建**：
```
build_context_container_mount(container)
  → normalize_inline_files(files) → BTreeMap<String, String>
  → Mount { metadata: json!({"files": map}), provider: "inline_fs", ... }
```

### 关键文件清单

| 文件 | 职责 |
|------|------|
| `crates/agentdash-domain/src/context_container.rs:53-65` | `ContextContainerDefinition` struct 定义 |
| `crates/agentdash-domain/src/context_container.rs:12-16` | `ContextContainerFile { path, content }` |
| `crates/agentdash-domain/src/context_container.rs:18-28` | `ContextContainerProvider` enum |
| `crates/agentdash-domain/src/project/value_objects.rs:18` | `ProjectConfig.context_containers` |
| `crates/agentdash-domain/src/story/value_objects.rs:63-66` | `StoryContext.context_containers` |
| `crates/agentdash-application/src/address_space/mount.rs:215-263` | `build_context_container_mount()` |
| `crates/agentdash-application/src/address_space/mount.rs:383-392` | `normalize_inline_files()` |
| `crates/agentdash-application/src/address_space/mount.rs:485-493` | `inline_files_from_mount()` |
| `crates/agentdash-application/src/address_space/provider_inline.rs` | `InlineFsMountProvider` 读取实现 |
| `crates/agentdash-application/src/address_space/inline_persistence.rs:48-172` | `InlineContentOverlay` session 缓存 |
| `crates/agentdash-application/src/address_space/inline_persistence.rs:215-432` | `DbInlineContentPersister` 实体嵌套写回 |
| `crates/agentdash-application/src/address_space/relay_service.rs` | VFS 分发（read/write/list/search） |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | DB schema（TEXT 列） |
| `crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs` | Project 序列化 |
| `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` | Story 序列化 |

## Requirements

### R1: 新建 `inline_fs_files` 表

```sql
CREATE TABLE IF NOT EXISTS inline_fs_files (
    id          TEXT PRIMARY KEY,          -- UUID
    owner_kind  TEXT NOT NULL,             -- 'project' | 'story' | 后续 'project_agent_link' 等
    owner_id    TEXT NOT NULL,             -- 对应实体的 UUID
    container_id TEXT NOT NULL,            -- ContextContainerDefinition.id
    path        TEXT NOT NULL,             -- 归一化文件路径
    content     TEXT NOT NULL,
    updated_at  TEXT NOT NULL,             -- ISO 8601

    UNIQUE(owner_kind, owner_id, container_id, path)
);

CREATE INDEX IF NOT EXISTS idx_inline_fs_files_owner
    ON inline_fs_files(owner_kind, owner_id, container_id);
```

### R2: 新建 `InlineFileRepository` trait

```rust
#[async_trait]
pub trait InlineFileRepository: Send + Sync {
    /// 读取单个文件
    async fn get_file(
        &self,
        owner_kind: &str,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<Option<InlineFile>, DomainError>;

    /// 列出 container 下所有文件（path + content）
    async fn list_files(
        &self,
        owner_kind: &str,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<Vec<InlineFile>, DomainError>;

    /// 写入或更新文件（UPSERT）
    async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError>;

    /// 删除文件
    async fn delete_file(
        &self,
        owner_kind: &str,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), DomainError>;

    /// 删除 container 下所有文件（container 被删除时调用）
    async fn delete_by_container(
        &self,
        owner_kind: &str,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<(), DomainError>;
}
```

`InlineFile` 结构：

```rust
pub struct InlineFile {
    pub id: Uuid,
    pub owner_kind: String,
    pub owner_id: Uuid,
    pub container_id: String,
    pub path: String,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}
```

### R3: 重构 `DbInlineContentPersister`

从当前的「加载整个实体 → 改嵌套 JSON → 写回整个实体」改为直接操作 `InlineFileRepository`：

```rust
pub struct DbInlineContentPersister {
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl InlineContentPersister for DbInlineContentPersister {
    async fn persist_write(&self, ...) -> Result<(), String> {
        self.inline_file_repo.upsert_file(&InlineFile {
            owner_kind, owner_id, container_id, path, content, ...
        }).await
    }

    async fn persist_delete(&self, ...) -> Result<(), String> {
        self.inline_file_repo.delete_file(owner_kind, owner_id, container_id, path).await
    }
}
```

- 不再需要注入 `ProjectRepository` / `StoryRepository`
- owner scope 路由通过 `owner_kind` 字段泛化，无需硬编码 if-else

### R4: 重构 `InlineContentOverlay` scope 解析

当前 `story_scope_for_mount()` 从 mount metadata 解析 `agentdash_context_owner_scope` 来判断写回 project 还是 story。重构为：

- mount metadata 新增 `agentdash_context_owner_kind` + `agentdash_context_owner_id` 两个字段
- `InlineContentOverlay.write()` 直接从 metadata 读取 `owner_kind` + `owner_id`，传给 persister
- 不再依赖 `address_space.source_project_id` / `source_story_id` 做间接推断

### R5: 重构 Mount 构建 — 文件内容不再嵌入 metadata

`build_context_container_mount()` 改为：
- metadata 中不再放 `{"files": {...}}`
- 改为放 `{"owner_kind": "project", "owner_id": "uuid", "container_id": "xxx"}`
- `InlineFsMountProvider` 读取时通过 `InlineFileRepository` 查 DB

### R6: 重构 `InlineFsMountProvider` 读取源

从读 `mount.metadata["files"]` 改为查 `InlineFileRepository`：

```rust
pub struct InlineFsMountProvider {
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl MountProvider for InlineFsMountProvider {
    async fn read_text(&self, mount: &Mount, path: &str, _options: &ReadOptions) -> Result<ReadResult, MountError> {
        let (owner_kind, owner_id, container_id) = parse_mount_metadata(mount)?;
        let file = self.inline_file_repo.get_file(&owner_kind, owner_id, &container_id, &path).await?;
        // ...
    }
}
```

### R7: 重构 `RelayAddressSpaceService` 中的 inline_fs 分支

- `list()` 不再调 `inline_files_from_mount()` + `overlay.apply_to_files()`
- 改为从 `InlineFsMountProvider.list()` 获取 DB 文件列表，再合并 overlay
- `search_inline()` 同理

### R8: Container 元信息保留在父实体

`ContextContainerDefinition` 仍留在 `ProjectConfig` / `StoryContext` 中，但 `InlineFiles` variant 的 `files` 字段变为可选或移除：

```rust
pub enum ContextContainerProvider {
    InlineFiles {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<ContextContainerFile>,  // 仅用于 API 创建/导入时的初始文件批量写入
    },
    ExternalService { ... },
}
```

- API 创建 container 时：如果 `files` 非空，批量写入 `inline_fs_files` 表，然后清空 `files` 再存入父实体
- 运行时不再从父实体读文件内容

### R9: 数据迁移

项目未上线，无需在线迁移。但需要处理开发环境已有数据：

- 新增 migration：建表 + 从现有 `projects.config` / `stories.context` 中的 inline files 提取到新表
- 原有 `files` 字段清空（或保留为空数组做兼容）

## Acceptance Criteria

- [ ] `inline_fs_files` 表创建成功
- [ ] `InlineFileRepository` trait + Postgres 实现
- [ ] `DbInlineContentPersister` 不再依赖 `ProjectRepository` / `StoryRepository`
- [ ] `InlineFsMountProvider` 从 DB 读取文件
- [ ] Mount metadata 不再嵌入文件内容
- [ ] Owner scope 通过 `owner_kind` + `owner_id` 泛化路由
- [ ] 现有 Project/Story 级 inline_fs CRUD 功能不变
- [ ] 前端 context-config-editor 功能不变
- [ ] 编译通过、无 warning

## Definition of Done

- `cargo build` 通过
- `cargo clippy` 无 warning（inline_fs 相关）
- 前端 `npm run build` 通过
- 现有 inline_fs 读写功能正常

## Out of Scope

- Agent Knowledge FS（另一个 task）
- ExternalService provider 的改动
- 前端 container 编辑器的 UI 重设计
- 性能基准测试

## Technical Notes

### 需修改文件（按依赖顺序）

**Domain 层：**
1. `crates/agentdash-domain/src/context_container.rs` — 新增 `InlineFile` struct，`InlineFiles.files` 改为创建时入参
2. 新建 `crates/agentdash-domain/src/inline_file/` — `InlineFile` entity + `InlineFileRepository` trait

**Infrastructure 层：**
3. 新增 migration — `inline_fs_files` 表
4. 新建 `crates/agentdash-infrastructure/src/persistence/postgres/inline_file_repository.rs` — 实现

**Application 层：**
5. `crates/agentdash-application/src/address_space/inline_persistence.rs` — `DbInlineContentPersister` 重写
6. `crates/agentdash-application/src/address_space/provider_inline.rs` — `InlineFsMountProvider` 注入 repo
7. `crates/agentdash-application/src/address_space/mount.rs` — mount 构建不再嵌入 files
8. `crates/agentdash-application/src/address_space/relay_service.rs` — list/search 调整

**API 层：**
9. `crates/agentdash-api/src/routes/projects.rs` — 创建/更新时批量写入初始文件
10. `crates/agentdash-api/src/routes/stories.rs` — 同上
11. Provider registry — `InlineFsMountProvider` 构造注入 repo

**前端：**
12. 基本无改动（API 契约不变，前端仍发 `context_containers` 含 `files`，后端负责拆分存储）

### 兼容性

- `ContextContainerDefinition` 的 JSON 序列化不变（前端 API 契约稳定）
- `InlineFiles.files` 在 API 层面仍可接收（创建时批量入库）
- 运行时 mount metadata 格式变化，但前端不直接读 metadata

### 并发改善

- 文件级 UPSERT 替代实体级 read-modify-write
- `UNIQUE(owner_kind, owner_id, container_id, path)` 约束保证行级操作原子性
- 不同文件的并发写入不再互相影响

## Implementation Phases

### Phase 1: Domain + Infrastructure

- `InlineFile` entity + `InlineFileRepository` trait
- Postgres 实现 + migration
- 编译通过

### Phase 2: Application 层重构

- `DbInlineContentPersister` 改用 `InlineFileRepository`
- `InlineFsMountProvider` 改读 DB
- Mount 构建去掉 files 嵌入
- `InlineContentOverlay` scope 解析简化
- 编译通过

### Phase 3: API 层适配

- Container CRUD 时批量初始文件写入
- Provider registry 注入
- 端到端功能验证

### Phase 4: 清理

- 移除旧 `inline_files_from_mount()` / `normalize_inline_files()` 等废弃函数
- 移除 `DbInlineContentPersister` 对 `ProjectRepository` / `StoryRepository` 的依赖
- 验证无 dead code
