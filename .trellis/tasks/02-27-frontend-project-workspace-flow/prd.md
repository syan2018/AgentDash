# 前端适配 Project/Workspace 领域模型 PRD

> **任务**: 前端适配新的 Project → Workspace → Story → Task 领域模型，跑通全容器创建流程
> **优先级**: P1（紧跟后端领域模型重构）
> **预计复杂度**: M（2-3天）
> **前置依赖**: `feat(domain): 建立 Project/Workspace/Story/Task 完整领域模型` (commit 026cf37)

---

## 1. 背景与目标

### 1.1 当前问题

后端已完成 Project/Workspace/Story/Task 领域模型重构（commit 026cf37），前端仍使用旧模型：
- ❌ 无 `Project` 概念，Story 直接挂在 Backend 下
- ❌ 无 `Workspace` 概念，Task.agentBinding 使用旧字段 (agentType + workspacePath)
- ❌ Story 类型中无 `projectId`，context 使用旧结构 (items[] 而非 StoryContext)
- ❌ 创建 Story 时无法指定 Project
- ❌ API 客户端缺少 `put` / `patch` 方法

### 1.2 目标

1. **类型对齐**：前端 TypeScript 类型与后端 Rust 实体完全对齐
2. **全流程跑通**：Project → Workspace → Story → Task 的完整创建/查看流程
3. **UI 适配**：侧边栏从"Backend 选择"演进为"Project 选择 + Workspace 管理"
4. **向后兼容**：Backend 选择功能保留，作为 Project 配置的一部分

---

## 2. 后端 API 对照

### 2.1 新增 API 端点

| 方法 | 路径 | 请求体 | 响应 |
|------|------|--------|------|
| GET | `/api/projects` | — | `Project[]` |
| POST | `/api/projects` | `{name, description?, backend_id, config?}` | `Project` |
| GET | `/api/projects/:id` | — | `{project, workspaces, stories}` |
| PUT | `/api/projects/:id` | `{name?, description?, backend_id?, config?}` | `Project` |
| DELETE | `/api/projects/:id` | — | `{deleted: id}` |
| GET | `/api/projects/:project_id/workspaces` | — | `Workspace[]` |
| POST | `/api/projects/:project_id/workspaces` | `{name, container_ref?, workspace_type?, git_config?}` | `Workspace` |
| GET | `/api/workspaces/:id` | — | `Workspace` |
| PATCH | `/api/workspaces/:id/status` | `{status}` | `{updated: id}` |
| DELETE | `/api/workspaces/:id` | — | `{deleted: id}` |

### 2.2 变更的 API 端点

| 端点 | 变更 |
|------|------|
| `POST /api/stories` | 请求体新增 `project_id`（必填） |
| `GET /api/stories` | 支持 `?project_id=` 参数（替代 `?backend_id=`） |
| `GET /api/stories/:id` | 新端点，返回单个 Story |
| Story 响应 | 新增 `project_id`，`context` 为 `StoryContext` 结构 |
| Task 响应 | `workspace_path` → `workspace_id`，`agent_type/agent_pid` → `agent_binding: {agent_type, agent_pid, preset_name}` |

---

## 3. 实现计划

### Phase 1: 基础设施（类型 + API 客户端）

#### 3.1 更新 TypeScript 类型 (`types/index.ts`)

**新增类型**：
```typescript
export interface ProjectConfig {
  default_agent_type?: string;
  default_workspace_id?: string;
  agent_presets: AgentPreset[];
}

export interface AgentPreset {
  name: string;
  agent_type: string;
  config: Record<string, unknown>;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  backend_id: string;
  config: ProjectConfig;
  created_at: string;
  updated_at: string;
}

export type WorkspaceType = "git_worktree" | "static" | "ephemeral";
export type WorkspaceStatus = "pending" | "preparing" | "ready" | "active" | "archived" | "error";

export interface GitConfig {
  source_repo: string;
  branch: string;
  commit_hash?: string;
}

export interface Workspace {
  id: string;
  project_id: string;
  name: string;
  container_ref: string;
  workspace_type: WorkspaceType;
  status: WorkspaceStatus;
  git_config?: GitConfig;
  created_at: string;
  updated_at: string;
}
```

**修改类型**：
```typescript
// Story：新增 project_id，context 改为 StoryContext
export interface StoryContext {
  prd_doc?: string;
  spec_refs: string[];
  resource_list: ResourceRef[];
}

export interface ResourceRef {
  name: string;
  uri: string;
  resource_type: string;
}

export interface Story {
  id: string;
  project_id: string;        // 新增
  backend_id: string;
  title: string;
  description?: string;
  status: StoryStatus;
  context: StoryContext;      // 变更：从 Context 改为 StoryContext
  created_at: string;
  updated_at: string;
}

// Task：workspace_id 替代 workspace_path
export interface AgentBinding {
  agent_type?: string;
  agent_pid?: string;
  preset_name?: string;
}

export interface Task {
  id: string;
  story_id: string;
  workspace_id?: string;      // 新增，替代旧 workspace_path
  title: string;
  description?: string;
  status: TaskStatus;
  agent_binding: AgentBinding; // 变更：从旧字段组合改为结构体
  artifacts: Artifact[];
  created_at: string;
  updated_at: string;
}
```

#### 3.2 扩展 API 客户端 (`api/client.ts`)

新增 `put` 和 `patch` 方法：
```typescript
export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, data: unknown) => request<T>(path, { method: 'POST', body: JSON.stringify(data) }),
  put: <T>(path: string, data: unknown) => request<T>(path, { method: 'PUT', body: JSON.stringify(data) }),
  patch: <T>(path: string, data: unknown) => request<T>(path, { method: 'PATCH', body: JSON.stringify(data) }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
};
```

### Phase 2: Store 层

#### 3.3 新增 `projectStore.ts`

```typescript
interface ProjectState {
  projects: Project[];
  currentProjectId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchProjects: () => Promise<void>;
  createProject: (name: string, description: string, backendId: string) => Promise<void>;
  selectProject: (id: string | null) => void;
}
```

#### 3.4 新增 `workspaceStore.ts`

```typescript
interface WorkspaceState {
  workspacesByProjectId: Record<string, Workspace[]>;
  isLoading: boolean;
  error: string | null;

  fetchWorkspaces: (projectId: string) => Promise<void>;
  createWorkspace: (projectId: string, name: string, opts: CreateWorkspaceOpts) => Promise<void>;
  updateStatus: (id: string, status: WorkspaceStatus) => Promise<void>;
  deleteWorkspace: (id: string) => Promise<void>;
}
```

#### 3.5 更新 `storyStore.ts`

- `fetchStories`: 从 `backend_id` 改为 `project_id`
- `createStory`: 新增 `project_id` 必填参数
- `mapStory`: 适配新的 `StoryContext` 和 `project_id`
- `mapTask`: 适配新的 `workspace_id` 和 `agent_binding` 结构

### Phase 3: UI 组件

#### 3.6 侧边栏改造 (`workspace-layout.tsx`)

当前：Backend 列表 → 选择 Backend → 加载 Story
改为：Project 列表 → 选择 Project → 显示 Workspace 列表 + Story 列表

**布局结构**：
```
┌─ 侧边栏 ──────────────────┐┌─ 主区域 ──────────────────────┐
│ [Project 下拉选择器]       ││                               │
│ ─────────────────          ││  Story 列表 / Session 视图    │
│ 📁 Workspace 列表          ││                               │
│   ├ 主仓库工作区  [Ready]  ││                               │
│   ├ 前端特性分支  [Active] ││                               │
│   └ + 新建 Workspace       ││                               │
│ ─────────────────          ││                               │
│ 🔌 后端连接                ││                               │
│   └ local-dev [在线]       ││                               │
│ ─────────────────          ││                               │
│ [看板] [会话]              ││                               │
└────────────────────────────┘└───────────────────────────────┘
```

#### 3.7 新增组件

| 组件 | 位置 | 职责 |
|------|------|------|
| `ProjectSelector` | `features/project/ui/` | Project 下拉选择 + 创建对话框 |
| `ProjectCreateDialog` | `features/project/ui/` | 创建 Project 表单 |
| `WorkspaceList` | `features/workspace/ui/` | Workspace 列表展示 |
| `WorkspaceCard` | `features/workspace/ui/` | 单个 Workspace 卡片（名称+状态+类型） |
| `WorkspaceCreateDialog` | `features/workspace/ui/` | 创建 Workspace 表单 |

#### 3.8 更新现有组件

| 组件 | 变更 |
|------|------|
| `story-card.tsx` | 移除 `backendId` 显示，可选显示 `projectId` |
| `story-drawer.tsx` | context 展示适配 `StoryContext` 新结构 |
| `task-card.tsx` | agent 显示从 `agentType` 改为 `agent_binding.agent_type` |
| `task-drawer.tsx` | 显示 workspace 信息而非 workspace_path |
| `story-list-view.tsx` | 创建 Story 时需传入 `project_id` |
| `DashboardPage.tsx` | 顶层引入 Project 选择逻辑 |

---

## 4. 全容器创建流程

### 4.1 用户操作流程

```
1. 创建 Project
   → 填写名称、描述、选择默认 Backend
   → POST /api/projects
   → 自动选中新创建的 Project

2. 创建 Workspace
   → 选择类型（Static/GitWorktree/Ephemeral）
   → 填写名称、路径
   → GitWorktree 类型可配置 source_repo + branch
   → POST /api/projects/:project_id/workspaces

3. 创建 Story
   → 自动关联当前 Project
   → 填写标题、描述
   → POST /api/stories {project_id, backend_id, title, description}

4. 创建 Task（后续阶段实现）
   → 选择关联的 Workspace
   → 填写标题、描述
   → POST /api/stories/:story_id/tasks {workspace_id, ...}
```

### 4.2 验收标准

- [ ] 能创建 Project 并在侧边栏切换
- [ ] 能在 Project 下创建 Workspace（三种类型）
- [ ] Workspace 列表正确显示，带状态徽标
- [ ] 能在 Project 下创建 Story（自动关联 project_id）
- [ ] Story 列表按 Project 过滤
- [ ] Task 列表正确显示 workspace 信息和 agent_binding
- [ ] API 客户端支持 PUT/PATCH 方法
- [ ] TypeScript 类型与后端完全对齐
- [ ] 旧有功能（Backend 管理、SSE 事件流、ACP 会话）不受影响

---

## 5. 技术注意事项

### 5.1 数据映射（snake_case → camelCase）

后端返回 `snake_case`，前端使用 `camelCase`。在 store 的 `map*` 函数中统一转换：
```typescript
// 后端返回: { project_id: "...", backend_id: "...", created_at: "..." }
// 前端使用: { projectId: "...", backendId: "...", createdAt: "..." }
```

注意：当前 `storyStore.ts` 中的 `mapStory` / `mapTask` 已有此模式，需扩展支持新字段。

### 5.2 向后兼容

- Backend 选择功能保留在侧边栏
- Project 的 `backend_id` 作为默认后端，创建 Story 时自动填充
- 旧的 `context.items[]` 数据格式需要优雅降级处理

### 5.3 代码参考

| 文件 | 参考内容 |
|------|----------|
| `stores/coordinatorStore.ts` | Store 结构模式（zustand） |
| `stores/storyStore.ts` | 数据映射模式（mapStory/mapTask） |
| `features/story/story-card.tsx` | 功能组件结构模式 |
| `components/layout/workspace-layout.tsx` | 侧边栏布局 |
