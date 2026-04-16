# Agent Knowledge Inline FS

## Goal

为 Agent 新增跟随 `ProjectAgentLink` 的 inline_fs 知识库，使 agent 在同一 Project 下的所有 session 中能够积累、读取和更新通用知识文件。知识按 Project × Agent 隔离，同一 Agent 在不同 Project 下积累独立的知识。

**动机**：当前 Agent session 是无状态的 — 每次启动只能拿到 Project/Story 级的静态 context containers，session 结束后 agent 学到的知识无处持久化。这导致：
- Agent 每次都要重新学习项目的约定、常见问题、历史决策
- 人工只能通过手动维护 project-level inline_fs 来给 agent 补充知识
- 不同 agent（如 code-reviewer vs implementer）无法各自积累专属知识

## Dependencies

- **前置任务**：`04-16-inline-fs-storage-refactor` — inline_fs 独立存储表 + `InlineFileRepository` + 泛化 `owner_kind` 路由
- 本任务的文件存储直接使用 `inline_fs_files` 表，`owner_kind = "project_agent_link"`

## What I Already Know

### 当前 Agent 数据模型

```rust
// crates/agentdash-domain/src/agent/entity.rs
pub struct Agent {
    pub id: Uuid,
    pub name: String,           // "code-reviewer"
    pub agent_type: String,     // "PI_AGENT", "claude-code"
    pub base_config: serde_json::Value,
    pub created_at / updated_at,
}

pub struct ProjectAgentLink {
    pub id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub config_override: Option<serde_json::Value>,
    pub default_lifecycle_key: Option<String>,
    pub is_default_for_story: bool,
    pub is_default_for_task: bool,
    pub created_at / updated_at,
}
```

- Agent 是全局实体，ProjectAgentLink 是 Project × Agent 的绑定
- `merged_config()` 做 base_config + config_override 深度合并
- 目前没有任何 knowledge/container 存储

### Project-Agent Session 创建流程

```
POST /projects/{id}/agent-links/{agent_id}/session
  → open_project_agent_session() (project_agents.rs:208)
    → SessionHub.create_session()
    → SessionBinding { owner_type: Project, label: "project_agent:{uuid}" }
    → bootstrap_state = Pending
    → 首次 prompt 时 SessionBootstrapPlan 注入 address_space
```

`BootstrapOwnerVariant::Project` 已有 `shared_context_mounts` 字段，用于注入 project 级 context containers。

### Address Space 组装

```
build_derived_address_space(project, story?, workspace?, agent_type, target)
  → workspace mount (relay_fs)
  → effective_context_containers (project + story merge)
  → [gap: 无 agent knowledge mounts]
  → lifecycle / canvas mounts
```

## Requirements

### R1: 后端 — ProjectAgentLink 新增 `knowledge_containers`

```rust
pub struct ProjectAgentLink {
    // ... 现有字段 ...
    pub knowledge_containers: Vec<ContextContainerDefinition>,  // 新增
}
```

- serde default 为空 Vec，对现有数据零影响
- 复用现有 `ContextContainerDefinition` 类型
- 每个 container 的 `provider` 必须是 `InlineFiles`（不支持 ExternalService，知识必须 inline）
- 文件内容存储在 `inline_fs_files` 表（`owner_kind = "project_agent_link"`, `owner_id = link.id`）

### R2: 后端 — Agent Knowledge 默认 Container 自动创建

当 `ProjectAgentLink` 创建时（或首次设置 knowledge_containers 时），如果 `knowledge_containers` 为空，自动创建一个默认 container：

```rust
ContextContainerDefinition {
    id: "knowledge",
    mount_id: "agent-knowledge",
    display_name: "{agent.name} 知识库",
    provider: InlineFiles { files: vec![] },
    capabilities: vec![Read, Write, List, Search],
    default_write: false,  // 不作为 session 的默认写入目标
    exposure: ContextContainerExposure {
        include_in_project_sessions: true,
        include_in_task_sessions: true,
        include_in_story_sessions: true,
        allowed_agent_types: vec![agent.agent_type.clone()],  // 仅对自己可见
    },
}
```

### R3: 后端 — Address Space 注入 Agent Knowledge Mounts

在 session bootstrap 时，将 `ProjectAgentLink.knowledge_containers` 构建为 mounts 注入 address space：

```
build_derived_address_space(...)
  → workspace mount
  → project/story context containers
  → agent knowledge containers     ← 新增
  → lifecycle / canvas mounts
```

- mount metadata 标注 `owner_kind: "project_agent_link"`, `owner_id: link.id`
- 利用独立存储重构后的泛化路由，persist 自动走 `InlineFileRepository`

需要在 session bootstrap 链路中传入 `ProjectAgentLink` 信息：
- `open_project_agent_session()` 已有 link 信息
- Task session 需要解析 task.agent_binding → 对应的 ProjectAgentLink（如有）

### R4: 后端 — Exposure 过滤

Agent knowledge containers 的 `exposure.allowed_agent_types` 限制为该 agent 的 type。

在 `container_visible_for_target()` 中，当 session 的 agent_type 不匹配时，该 container 不会被挂载。这确保：
- code-reviewer 的知识对 implementer 不可见
- 同一 Agent 在不同 session type（project/story/task）中都能访问自己的知识

### R5: 后端 — API

**ProjectAgentLink CRUD 扩展**：

- `GET /projects/{id}/agent-links` — 响应中包含 `knowledge_containers` 字段
- `PUT /projects/{id}/agent-links/{agent_id}` — 支持更新 `knowledge_containers`
- 创建 link 时自动初始化默认 container（R2）

**专用知识管理端点（可选，视前端需要）**：

- `GET /projects/{id}/agent-links/{agent_id}/knowledge` — 列出知识文件
- `PUT /projects/{id}/agent-links/{agent_id}/knowledge/{container_id}/files/{path}` — 直接写入文件
- 这些端点是 VFS 之外的管理入口，方便前端在非 session 上下文中管理知识

### R6: 前端 — 类型更新

```typescript
export interface ProjectAgentLink {
  // ... 现有字段 ...
  knowledge_containers: ContextContainerDefinition[];
}
```

### R7: 前端 — Agent 知识管理 UI

在 Project Agent 设置页面中新增知识管理区域：

- 展示当前 agent 的 knowledge containers 列表
- 复用 `ContextConfigEditor` 组件编辑 container 元信息
- 内联文件列表 + 内容编辑（复用现有 inline files editor）
- 支持新建/删除文件

### R8: 后端 — 容量保护

防止知识库无限膨胀：

- 单 container 文件数上限：100（可配置）
- 单文件大小上限：64KB
- 单 link 下所有 containers 总文件数上限：500
- 在 `InlineContentOverlay.write()` 或 persister 中校验
- 超限时返回明确错误信息

## Acceptance Criteria

- [ ] `ProjectAgentLink` 含 `knowledge_containers` 字段，可正常 CRUD
- [ ] 创建 link 时自动初始化默认 knowledge container
- [ ] Project-Agent session bootstrap 包含 knowledge mounts
- [ ] Agent 在 session 中可通过 VFS read/write 操作 knowledge files
- [ ] 知识按 Project × Agent 隔离：不同 project 的知识互不可见
- [ ] 知识按 Agent Type 隔离：不同 agent type 的知识互不可见
- [ ] 前端可查看和编辑 agent knowledge containers
- [ ] 容量保护生效
- [ ] 现有 ProjectAgentLink 功能不受影响（空 knowledge_containers 兼容）

## Definition of Done

- `cargo build` 通过
- `cargo clippy` 无 warning
- 前端 `npm run build` 通过
- Project-Agent session 中可读写 knowledge files
- 知识跨 session 持久化（新 session 可读取旧 session 写入的知识）

## Out of Scope

- Agent 全局知识（跨 project 共享的基础知识）— 可后续扩展
- 知识自动摘要/压缩/过期机制
- 知识版本控制/变更历史
- 知识搜索引擎（超出 VFS search_text 的语义搜索）
- Task session 的 knowledge 注入（需要 task.agent_binding → link 的映射，可后续扩展）

## Technical Notes

### 需修改文件

**Domain 层：**
1. `crates/agentdash-domain/src/agent/entity.rs` — `ProjectAgentLink` 新增 `knowledge_containers`
2. `crates/agentdash-domain/src/agent/repository.rs` — trait 不变（JSONB 透传）

**Infrastructure 层：**
3. `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` — 序列化新字段

**Application 层：**
4. `crates/agentdash-application/src/address_space/mount.rs` — 新增 `build_agent_knowledge_mounts()`
5. `crates/agentdash-application/src/session/bootstrap.rs` — bootstrap plan 注入 knowledge mounts

**API 层：**
6. `crates/agentdash-api/src/routes/project_agents.rs` — link CRUD 扩展 + session 创建注入

**前端：**
7. `frontend/src/types/index.ts` — `ProjectAgentLink` 类型
8. `frontend/src/features/project/` — agent 知识管理 UI

### 与 inline-fs-storage-refactor 的依赖关系

```
inline-fs-storage-refactor (Phase 1-4)
  │
  ├── inline_fs_files 表 ─────────────→ agent knowledge 文件存入此表
  ├── InlineFileRepository ───────────→ 直接复用，无需额外 repo
  ├── owner_kind 泛化路由 ────────────→ 新增 "project_agent_link" 枚举值即可
  └── mount metadata 不嵌入 files ───→ agent knowledge mount 同样走 DB 读取
  │
  v
agent-knowledge-fs (本 task)
  └── 只需关注 ProjectAgentLink 数据模型 + session bootstrap 注入 + 前端 UI
```

重构后新增 owner 类型的成本极低：只需定义 `owner_kind` 常量 + 在 mount 构建时传入正确的 `owner_kind` / `owner_id`。

### 兼容性

- `ProjectAgentLink.knowledge_containers` serde default 为空 Vec
- 现有数据库中的 link 行反序列化时自动填充为空
- 空 knowledge_containers = 无知识挂载，session 行为不变

## Implementation Phases

### Phase 1: 数据模型

- ProjectAgentLink 新增 knowledge_containers 字段
- 序列化/反序列化验证
- 编译通过

### Phase 2: Session Bootstrap 注入

- build_agent_knowledge_mounts() 函数
- bootstrap plan 注入 knowledge mounts
- 端到端：session 中可读取 knowledge files

### Phase 3: 默认 Container + 容量保护

- Link 创建时自动初始化默认 knowledge container
- 写入容量校验
- API 扩展

### Phase 4: 前端 UI

- 类型更新
- Agent 知识管理 UI
- 文件编辑交互
