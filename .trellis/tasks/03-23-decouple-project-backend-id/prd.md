# 从 Project / Story 移除 backend_id，建立清晰继承链

## 目标

建立一条清晰、可追溯的 backend 决策链，移除诡异的隐式 fallback，让每一级的
backend 绑定意图都显式可见。

---

## 设计：清晰继承链

```
Task 执行时 backend_id 解析顺序（从具体到通用，逐级显式继承）：

  1. Task.workspace_id 已设置
       → Workspace.backend_id                    ← 最优先，最具体

  2. Task 未绑定 Workspace，Story 有默认 Workspace
       → Story.default_workspace_id
           → Workspace.backend_id                ← Story 级继承

  3. Story 也没有默认 Workspace，Project 有默认 Workspace
       → Project.config.default_workspace_id
           → Workspace.backend_id                ← Project 级继承

  4. 以上均无 → Error（"Task 需要绑定 Workspace 才能执行"）
```

**关键原则**：backend_id 的解析永远经过 Workspace 实体，而不是直接跳过 Workspace
用 Project/Story 上的 backend_id 字符串兜底。Project 和 Story 只能持有
`default_workspace_id`，不再持有 `backend_id`。

**未来扩展**：多用户场景下，解析链在步骤 2/3 之前可以插入
"用户偏好 Workspace 覆盖"——用户把自己 local backend 下的 Workspace 绑到账户上，
系统在继承链的早期命中它。这套机制自然容纳，不需要再回头改继承逻辑。

---

## 当前问题（诡异之处）

| 继承点 | 当前代码 | 问题 |
|--------|----------|------|
| Project → Workspace 创建 | `workspaces.rs:117` `project.backend_id` 赋给新 Workspace | Workspace 没有独立选 backend 的机会 |
| Project → Story 创建 | `DashboardPage.tsx:142` 把 `project.backend_id` 传给 Story 表单 | Story 无需感知 backend |
| Story.backend_id → Task 执行 | `task_execution_gateway.rs:1311` | 绕过 Workspace 直接用 backend 字符串路由 |
| Project.backend_id → Task 执行 | `task_execution_gateway.rs:1325` | 掩盖了 Task/Story 未绑 Workspace 的配置缺失 |

---

## 需要做的事

### 后端

**1. `Project` 实体：移除 `backend_id`，确认 `config.default_workspace_id` 已存在**

- `crates/agentdash-domain/src/project/entity.rs`
  - 移除 `pub backend_id: String` 字段
  - `Project::new()` 移除 `backend_id` 参数
- `crates/agentdash-infrastructure/.../project_repository.rs`
  - CREATE TABLE 去掉列（inline schema）
  - INSERT / SELECT / UPDATE 全部去掉 `backend_id`
  - `ProjectRow` 结构体去掉字段
  - 加 backward-compat：`ALTER TABLE projects DROP COLUMN IF EXISTS backend_id`
- `crates/agentdash-api/src/routes/projects.rs`
  - `CreateProjectRequest` 移除 `backend_id: String`
  - `UpdateProjectRequest` 移除 `backend_id: Option<String>`
  - 对应 handler 逻辑同步移除
- `crates/agentdash-api/src/dto/project.rs`
  - `ProjectResponse` 移除 `backend_id`

**2. `Story` 实体：移除 `backend_id`，新增 `default_workspace_id`**

- `crates/agentdash-domain/src/story/entity.rs`
  - 移除 `pub backend_id: String`
  - 新增 `pub default_workspace_id: Option<Uuid>`（Story 级默认 Workspace）
  - `Story::new()` 移除 `backend_id`，新增 `default_workspace_id: None`
- `crates/agentdash-infrastructure/.../story_repository.rs`
  - `stories` 表：去掉 `backend_id NOT NULL`，加 `default_workspace_id TEXT`
  - 移除 `list_by_backend` 查询方法（前端无 UI 调用，死代码）
  - `state_changes.backend_id`：改为 nullable（`TEXT` 不加 NOT NULL），
    填充逻辑改为从关联 Workspace 解析，无 Workspace 时填 null
  - `StoryRow` 同步更新
- `crates/agentdash-domain/src/story/repository.rs`
  - 移除 `list_by_backend` trait method
- `crates/agentdash-api/src/routes/stories.rs`
  - `CreateStoryRequest` 移除 `backend_id`，可选加 `default_workspace_id`
  - `UpdateStoryRequest` 移除 `backend_id`，可选加 `default_workspace_id`
  - `ListStoriesQuery` 移除 `backend_id` 过滤参数
  - cascade delete 里 `append_change` 的 backend_id 参数改为从 workspace 解析或 null
- `crates/agentdash-api/src/dto/story.rs`
  - `StoryResponse` 移除 `backend_id`，加 `default_workspace_id: Option<Uuid>`

**3. `resolve_task_backend_id`：重写为清晰继承链**

`crates/agentdash-api/src/bootstrap/task_execution_gateway.rs:1283-1326`

```
旧逻辑（移除）：
  Workspace.backend_id → Story.backend_id → Project.backend_id

新逻辑：
  if task.workspace_id.is_some():
      workspace = load(task.workspace_id)
      return workspace.backend_id

  story = load(task.story_id)
  if story.default_workspace_id.is_some():
      workspace = load(story.default_workspace_id)
      return workspace.backend_id

  project = load(story.project_id)
  if project.config.default_workspace_id.is_some():
      workspace = load(project.config.default_workspace_id)
      return workspace.backend_id

  Error("Task 执行需要绑定 Workspace：请为 Task、Story 或 Project 配置默认 Workspace")
```

**4. Workspace 创建改为显式传入 `backend_id`**

`crates/agentdash-api/src/routes/workspaces.rs:16-20`
- `CreateWorkspaceRequest` 增加 `backend_id: String`（必填）
- 移除从 Project 继承的代码（`project.backend_id` 引用将不再存在）
- `resolve_container_and_git` 的 backend_id 改取自 `req.backend_id`
- 移除加载 Project 的 DB 查询（不再需要）

**5. LLM context 清理**

- `crates/agentdash-application/src/project/context_builder.rs` — 移除 `backend_id` 行
- `crates/agentdash-application/src/story/context_builder.rs` — 移除 Project 部分的 `backend_id` 行

**6. MCP relay server**

`crates/agentdash-mcp/src/servers/relay.rs:220-225`
- `create_story` handler 目前用 `project.backend_id` 创建 Story，改为不传 backend_id

---

### 前端

**7. 类型 & Store**

- `frontend/src/types/index.ts`
  - `Project` 接口：移除 `backend_id`
  - `Story` 接口：移除 `backend_id`，加 `default_workspace_id?: string`
- `frontend/src/stores/projectStore.ts`
  - `createProject` / `updateProject` 移除 `backendId` 参数和 POST body 字段
- `frontend/src/stores/storyStore.ts`
  - `createStory` 移除 `backendId` 参数
  - 移除 `fetchStoriesByBackend`（死代码）
  - `mapStory` 移除 `backend_id` 字段映射
  - `canMapStoryFromPayload` 不再要求 `backend_id`

**8. Project UI**

- `frontend/src/features/project/project-selector.tsx`
  - 移除 create Drawer 的 backend 下拉选择器
  - 移除 detail Drawer "基础信息" tab 的 `backend_id` 编辑字段
  - 修复空 description fallback 显示（不再显示 UUID `后端: ${project.backend_id}`）

**9. Story 创建 UI**

- `frontend/src/pages/DashboardPage.tsx:142`
  - 移除 `backendId={currentProject?.backend_id ?? ""}` 传参
- `frontend/src/features/story/story-list-view.tsx`
  - 移除 `backendId` prop
  - 移除 `!backendId` 创建阻断校验
  - Story 创建不再传 `backend_id` 到 API

**10. Workspace 创建 UI**

- `frontend/src/features/workspace/workspace-list.tsx`
  - 创建表单增加 backend 选择器（下拉，从 `coordinatorStore.backends` 取列表，必填）
  - 表单提示：`container_ref` 输入框旁显示已选 backend 名称，让用户清楚填的是哪台机器的路径
- `frontend/src/stores/workspaceStore.ts`
  - `createWorkspace` 增加 `backendId: string` 参数，加入 POST body

**11. 死代码清理**

- `frontend/src/stores/coordinatorStore.ts`
  - 移除 `currentBackendId` 状态
  - 移除 `selectBackend` action
- `tests/e2e/*.spec.ts`
  - 移除 project/story fixture 中的 `backend_id` 字段

---

## 不受影响的部分（不要动）

- `Workspace.backend_id` 字段本身及所有相关 SQL / DTO / 前端类型
- `ExecutionMount.backend_id`（运行时 mount 对象）
- `StateChange.backend_id`（改为 nullable，但保留列）
- `BackendRegistry` / relay dispatch 逻辑
- Address Space 路由层（`RelayAddressSpaceService` 读 `mount.backend_id`）
- 第三方 Agent 执行路径（已经要求 Workspace，不受影响）

---

## Acceptance Criteria

- [ ] `Project` 实体无 `backend_id` 字段，现有数据库迁移不丢数据
- [ ] `Story` 实体无 `backend_id` 字段，有 `default_workspace_id: Option<Uuid>`
- [ ] `state_changes.backend_id` 改为 nullable，现有数据不受影响
- [ ] 创建 Project 无需选 backend
- [ ] 创建 Workspace 必须显式选 backend
- [ ] 创建 Story 无需指定 backend
- [ ] `resolve_task_backend_id` 继承链：Task → Story.default_workspace → Project.default_workspace → Error
- [ ] `coordinatorStore.currentBackendId` / `selectBackend` 已删
- [ ] `storyStore.fetchStoriesByBackend` 已删
- [ ] Workspace 创建表单：backend 选择器旁显示路径提示
- [ ] 全量 Rust 编译通过，全量 TS 类型检查通过

## 参考文件

- `.trellis/spec/backend/address-space-access.md`
- `crates/agentdash-domain/src/project/entity.rs`
- `crates/agentdash-domain/src/project/value_objects.rs` — `ProjectConfig.default_workspace_id` 已存在
- `crates/agentdash-domain/src/story/entity.rs`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs:1283-1326`
- `crates/agentdash-api/src/routes/workspaces.rs:16-20,90-129`
- `crates/agentdash-mcp/src/servers/relay.rs:220-225`
