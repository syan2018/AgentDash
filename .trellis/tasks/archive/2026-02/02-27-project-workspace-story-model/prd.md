# Project/Workspace/Story 领域模型重构 PRD

> **任务**: 补全工作空间业务，建立 Project → Workspace → Story → Task 的完整领域模型
> **优先级**: P1（State 模块核心能力）
> **预计复杂度**: L（3-5天）

---

## 1. 背景与目标

### 1.1 当前问题

当前领域模型存在以下缺失：
- ❌ 没有 `Project` 实体，Story 直接关联 `backend_id`，缺乏项目级别的组织
- ❌ `Task.workspace_path` 是字符串而非外键，无法追踪 Workspace 生命周期
- ❌ 无法支持多 Task 共享 Workspace 的场景
- ❌ 缺乏 Project 级别的配置（Agent 预设、Workspace 模板等）

### 1.2 目标

建立完整的领域模型层次：

```
Project (1) → (*) Workspace  (物理工作空间，多 Task 共享)
Project (1) → (*) Story      (用户价值单元)
Story (1)   → (*) Task       (执行单元)
Workspace (1) ← (*) Task     (多 Task 共享同一 Workspace)
```

### 1.3 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Story 归属 | Project | 与后端(backend)解耦，Project 是业务组织单元 |
| Workspace 概念 | 与 vibe-kanban 对齐 | 复用现有心智模型，container_ref 指向物理目录 |
| Workspace-Task 关系 | 多对多 | 支持多 Task 顺序/并行执行于同一工作空间 |
| Project 作用 | 数据容器 | 配置 Agent、管理 Story/Workspace 列表 |

---

## 2. 领域模型设计

### 2.1 实体定义

#### Project（项目）

```rust
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub backend_id: String,          // 默认后端
    pub config: ProjectConfig,       // 项目配置（JSON）
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct ProjectConfig {
    pub default_agent_type: Option<String>,    // 默认 Agent 类型
    pub default_workspace_id: Option<Uuid>,    // 默认 Workspace
    pub agent_presets: Vec<AgentPreset>,       // Agent 预设配置
}

pub struct AgentPreset {
    pub name: String,
    pub agent_type: String,
    pub config: serde_json::Value,
}
```

#### Workspace（工作空间）

与 vibe-kanban 的 Workspace 概念对齐：

```rust
pub struct Workspace {
    pub id: Uuid,
    pub project_id: Uuid,            // 所属项目
    pub name: String,                // 显示名称
    pub container_ref: String,       // 物理路径（磁盘上的目录）
    pub workspace_type: WorkspaceType,
    pub status: WorkspaceStatus,
    pub git_config: Option<GitConfig>, // Git worktree 配置
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum WorkspaceType {
    GitWorktree,     // Git worktree（基于现有仓库）
    Static,          // 静态目录（现有代码库）
    Ephemeral,       // 临时目录（任务完成后清理）
}

pub enum WorkspaceStatus {
    Pending,         // 待创建
    Preparing,       // 准备中（clone/setup）
    Ready,           // 就绪
    Active,          // 有 Task 正在运行
    Archived,        // 已归档
    Error,           // 错误状态
}

pub struct GitConfig {
    pub source_repo: String,         // 源仓库路径
    pub branch: String,              // 分支名
    pub commit_hash: Option<String>, // 固定 commit
}
```

#### Story（故事）

扩展现有 Story 实体：

```rust
pub struct Story {
    pub id: Uuid,
    pub project_id: Uuid,            // 【新增】所属项目
    pub backend_id: String,          // 执行后端（保留）
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub context: StoryContext,       // 【变更】结构化上下文
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct StoryContext {
    pub prd_doc: Option<String>,     // PRD 文档内容或路径
    pub spec_refs: Vec<String>,      // 规范文档引用
    pub resource_list: Vec<Resource>, // 资源清单
}
```

#### Task（任务）

修改现有 Task 实体：

```rust
pub struct Task {
    pub id: Uuid,
    pub story_id: Uuid,
    pub workspace_id: Uuid,          // 【变更】外键替代 workspace_path
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub agent_binding: AgentBinding, // 【新增】结构化绑定
    pub artifacts: Vec<Artifact>,    // 【变更】结构化数组
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct AgentBinding {
    pub agent_type: String,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>, // 使用的预设名称
}

pub struct Artifact {
    pub id: Uuid,
    pub artifact_type: ArtifactType,
    pub content: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub enum ArtifactType {
    CodeChange,      // 代码变更
    TestResult,      // 测试结果
    LogOutput,       // 日志输出
    File,            // 生成文件
}
```

### 2.2 实体关系图

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│    Project      │────→│    Workspace    │←────│      Task       │
├─────────────────┤  1:* ├─────────────────┤ *:1 ├─────────────────┤
│ id              │     │ id              │     │ id              │
│ name            │     │ project_id      │     │ story_id        │
│ backend_id      │     │ name            │     │ workspace_id    │
│ config          │     │ container_ref   │     │ status          │
│ created_at      │     │ status          │     │ agent_binding   │
└─────────────────┘     │ git_config      │     │ artifacts       │
         │              └─────────────────┘     └─────────────────┘
         │ 1:*
         ↓
┌─────────────────┐
│     Story       │
├─────────────────┤
│ id              │
│ project_id      │
│ backend_id      │
│ title           │
│ status          │
│ context         │
└─────────────────┘
```

---

## 3. Repository 接口

### 3.1 ProjectRepository

```rust
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, project: &Project) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Project>, DomainError>;
    async fn list_all(&self) -> Result<Vec<Project>, DomainError>;
    async fn update(&self, project: &Project) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    // 关联查询
    async fn list_workspaces(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError>;
    async fn list_stories(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
}
```

### 3.2 WorkspaceRepository

```rust
#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Workspace>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError>;
    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: WorkspaceStatus) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;

    // 任务关联
    async fn list_tasks(&self, workspace_id: Uuid) -> Result<Vec<Task>, DomainError>;
    async fn get_active_task_count(&self, workspace_id: Uuid) -> Result<i64, DomainError>;
}
```

### 3.3 扩展现有 Repository

**StoryRepository 新增**:
```rust
async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError>;
async fn update(&self, story: &Story) -> Result<(), DomainError>;
async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError>;
```

**TaskRepository 新增**:
```rust
async fn create(&self, task: &Task) -> Result<(), DomainError>;
async fn get_by_id(&self, id: Uuid) -> Result<Option<Task>, DomainError>;
async fn update(&self, task: &Task) -> Result<(), DomainError>;
async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<(), DomainError>;
async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
async fn list_by_workspace(&self, workspace_id: Uuid) -> Result<Vec<Task>, DomainError>;
```

---

## 4. 数据库 Schema

### 4.1 新建表

```sql
-- projects 表
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    backend_id TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- workspaces 表
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    container_ref TEXT NOT NULL,  -- 物理路径
    workspace_type TEXT NOT NULL DEFAULT 'git_worktree',
    status TEXT NOT NULL DEFAULT 'pending',
    git_config TEXT,              -- JSON: {source_repo, branch, commit_hash}
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_workspaces_project ON workspaces(project_id);
CREATE INDEX idx_workspaces_status ON workspaces(status);
```

### 4.2 修改现有表

```sql
-- stories 表：添加 project_id
ALTER TABLE stories ADD COLUMN project_id TEXT REFERENCES projects(id);
-- 迁移：为已有 story 设置默认 project（需要数据迁移脚本）

-- tasks 表：workspace_path → workspace_id
-- 步骤1：添加新列
ALTER TABLE tasks ADD COLUMN workspace_id TEXT REFERENCES workspaces(id);
-- 步骤2：数据迁移（将 path 映射到 workspace）
-- 步骤3：删除旧列（后续版本）
ALTER TABLE tasks DROP COLUMN workspace_path;

-- 修改 agent 相关字段为 JSON
ALTER TABLE tasks ADD COLUMN agent_binding TEXT;  -- JSON: {agent_type, agent_pid, preset_name}
ALTER TABLE tasks DROP COLUMN agent_type;
ALTER TABLE tasks DROP COLUMN agent_pid;
```

---

## 5. API 设计

### 5.1 Project API

```rust
// 创建项目
POST /api/projects
{
    "name": "AgentDash 开发",
    "description": "AgentDash 看板系统开发",
    "backend_id": "local-vibe-kanban",
    "config": {
        "default_agent_type": "claude-code",
        "agent_presets": [...]
    }
}

// 获取项目列表
GET /api/projects

// 获取项目详情（含 workspaces, stories）
GET /api/projects/:id

// 更新项目
PUT /api/projects/:id

// 删除项目
DELETE /api/projects/:id
```

### 5.2 Workspace API

```rust
// 创建 workspace
POST /api/projects/:project_id/workspaces
{
    "name": "前端代码库",
    "workspace_type": "git_worktree",
    "git_config": {
        "source_repo": "/path/to/frontend-repo",
        "branch": "feature/new-ui"
    }
}

// 获取 workspace 列表
GET /api/projects/:project_id/workspaces

// 获取 workspace 详情（含 tasks）
GET /api/workspaces/:id

// 更新 workspace 状态
PATCH /api/workspaces/:id/status
{
    "status": "ready"
}

// 删除 workspace
DELETE /api/workspaces/:id
```

### 5.3 扩展 Story/Task API

```rust
// 创建 story（改为 project 下）
POST /api/projects/:project_id/stories

// 创建 task（需指定 workspace）
POST /api/stories/:story_id/tasks
{
    "title": "实现登录页面",
    "description": "...",
    "workspace_id": "uuid-of-workspace",  // 必需
    "agent_preset": "claude-code-frontend" // 可选
}

// 获取 task 详情（含 workspace 信息）
GET /api/tasks/:id
```

---

## 6. 实现步骤

### Phase 1: Domain 层（Day 1-2）

1. **创建 Project 模块**
   - `agentdash-domain/src/project/entity.rs`
   - `agentdash-domain/src/project/repository.rs`
   - `agentdash-domain/src/project/value_objects.rs`
   - `agentdash-domain/src/project/mod.rs`

2. **创建 Workspace 模块**
   - `agentdash-domain/src/workspace/entity.rs`
   - `agentdash-domain/src/workspace/repository.rs`
   - `agentdash-domain/src/workspace/value_objects.rs`
   - `agentdash-domain/src/workspace/mod.rs`

3. **扩展现有实体**
   - 修改 `Story`：添加 `project_id`，`context` 改为结构化
   - 修改 `Task`：`workspace_path` → `workspace_id`，`agent_binding` 结构化

4. **扩展 Repository 接口**
   - `StoryRepository`: 添加 `get_by_id`, `update`, `delete`, `list_by_project`
   - `TaskRepository`: 添加完整 CRUD + `list_by_workspace`

### Phase 2: Infrastructure 层（Day 2-3）

1. **SQLite Repository 实现**
   - `agentdash-infrastructure/src/persistence/sqlite/project_repository.rs`
   - `agentdash-infrastructure/src/persistence/sqlite/workspace_repository.rs`
   - 扩展 `story_repository.rs`
   - 扩展 `task_repository.rs`

2. **数据库迁移**
   - 创建 `initialize` 函数中的新表
   - 编写数据迁移逻辑（story.project_id 回填）

### Phase 3: API 层（Day 3-4）

1. **路由实现**
   - `agentdash-api/src/routes/projects.rs`
   - `agentdash-api/src/routes/workspaces.rs`
   - 扩展 `stories.rs`, `tasks.rs`

2. **AppState 更新**
   - 添加 `project_repo`, `workspace_repo`

3. **DTO 定义**
   - Project/Workspace 的 Create/Update/Response DTO

### Phase 4: 测试与文档（Day 4-5）

1. **单元测试**
   - Project/Workspace 实体创建
   - Repository CRUD 测试

2. **集成测试**
   - API 端点测试

3. **文档更新**
   - 更新 `directory-structure.md`
   - 更新 Repository 模式文档

---

## 7. 验收标准

- [ ] `Project` 实体完整，支持 CRUD
- [ ] `Workspace` 实体完整，支持 CRUD 和状态管理
- [ ] `Story` 添加 `project_id`，支持 `list_by_project`
- [ ] `Task` 使用 `workspace_id` 外键替代 `workspace_path` 字符串
- [ ] 所有 Repository 实现 SQLite 版本
- [ ] API 路由完整，支持 Project/Workspace/Story/Task 的完整操作流程
- [ ] 数据库迁移脚本可运行，不丢失已有数据
- [ ] Clippy 检查通过

---

## 8. 技术注意事项

### 8.1 数据迁移策略

已有 Story 数据的迁移：
```rust
// 在 SqliteStoryRepository::initialize 中
pub async fn migrate_add_project_id(&self) -> Result<(), DomainError> {
    // 1. 检查是否需要迁移
    // 2. 创建默认 Project
    // 3. 将所有 story 的 project_id 设为默认 project
}
```

### 8.2 与 vibe-kanban Workspace 的区分

本项目的 `Workspace` 是领域概念，vibe-kanban 的 `Workspace` 是执行概念：
- 本项目的 Workspace = 物理目录配置 + 生命周期管理
- vibe-kanban 的 Workspace = Task 的一次执行上下文

后续可通过 `Workspace.container_ref` 与 vibe-kanban 建立关联。

### 8.3 多 Task 共享 Workspace 的并发控制

当多个 Task 同时运行时：
- Workspace 状态为 `Active`
- Task 启动时检查 Workspace 是否已被其他 Task 占用（根据业务需求决定是否允许并行）
- 建议 Phase 1 先实现简单的"单 Task 独占"模式

---

## 9. 附录：相关代码参考

### vibe-kanban Workspace 模型
见 `third_party/vibe-kanban/crates/db/src/models/workspace.rs`

### 当前 Task 实体
见 `crates/agentdash-domain/src/task/entity.rs`

### Repository 模式规范
见 `.trellis/spec/backend/repository-pattern.md`
