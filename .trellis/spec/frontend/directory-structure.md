# Directory Structure

> How frontend code is organized in this project.

---

## Overview

<!--
Document your project's frontend directory structure here.

Questions to answer:
- Where do components live?
- How are features/modules organized?
- Where are shared utilities?
- How are assets organized?
-->

<!-- PROJECT-SPECIFIC-START: Frontend Overview -->
> **AgentDashboard 前端代码的组织方式。**
> **注意：当前为概念阶段，技术栈未定，目录结构仅为参考设计。**

### 设计原则

前端目录结构按**功能模块**组织，每个功能模块对应后端的一个核心模块：

- 各模块独立，通过共享的类型定义交互
- 视图组件和状态逻辑分离
- 实时状态订阅封装在 hooks 层
<!-- PROJECT-SPECIFIC-END -->

---

## Directory Layout

```
<!-- Replace with your actual structure -->
src/
├── ...
└── ...
```

<!-- PROJECT-SPECIFIC-START: Directory Tree -->
### 实际目录布局（React 19 + TypeScript + Vite 7 + Tailwind v4）

```
frontend/src/
├── api/                         # API 调用层
│   ├── client.ts                # 统一 HTTP 客户端（GET/POST/PUT/PATCH/DELETE）
│   ├── eventStream.ts           # SSE 事件流（Dashboard 用）
│   ├── origin.ts                # API_ORIGIN / buildApiPath
│   └── streamRegistry.ts        # 流连接追踪（HMR 安全）
├── components/                  # 共享 UI 组件
│   ├── acp/                     # ACP 协议可视化组件
│   ├── layout/                  # 布局组件（WorkspaceLayout）
│   └── ui/                      # 通用 UI（StatusBadge, ThemeToggle）
├── features/                    # 功能模块（FSD 风格：model/ + ui/）
│   ├── acp-session/             # ACP 会话流展示
│   │   ├── model/               # types, useAcpStream, useAcpSession, streamTransport
│   │   └── ui/                  # AcpSessionEntry, AcpMessageCard, AcpToolCallCard...
│   ├── executor-selector/       # 执行器/模型选择器
│   │   ├── model/               # types, useExecutorDiscovery, useExecutorConfig
│   │   └── ui/                  # ExecutorSelector（下拉选择器 + 高级选项）
│   ├── project/                 # 项目管理（新增 v0.2）
│   │   └── project-selector.tsx # 项目选择器 + 创建表单
│   ├── workspace/               # 工作空间管理（新增 v0.2）
│   │   └── workspace-list.tsx   # 工作空间列表 + 创建面板
│   ├── story/                   # Story 卡片/抽屉/列表视图
│   └── task/                    # Task 卡片/抽屉/列表
├── hooks/                       # 全局 hooks
├── pages/                       # 页面组件
│   ├── DashboardPage.tsx        # Project 驱动的看板页
│   └── SessionPage.tsx          # 集成 ExecutorSelector + ACP 会话流
├── services/                    # 服务层
│   └── executor.ts              # ExecutorConfig 类型 + promptSession API
├── stores/                      # 全局 Zustand stores
│   ├── coordinatorStore.ts      # 后端连接管理
│   ├── projectStore.ts          # Project CRUD + 选择（新增 v0.2）
│   ├── workspaceStore.ts        # Workspace CRUD + 状态管理（新增 v0.2）
│   ├── storyStore.ts            # Story/Task 数据（重构 v0.2）
│   └── eventStore.ts            # SSE 事件流状态
├── styles/                      # Tailwind v4 主题变量（HSL 色系）
├── types/
│   └── index.ts                 # 全局类型（Project/Workspace/Story/Task 等）
├── App.tsx
└── main.tsx
```

### Feature 模块标准结构（FSD 风格）

```
features/<feature-name>/
├── index.ts              # 公共 API 导出
├── model/                # 数据层：types, hooks, transport
│   ├── index.ts
│   ├── types.ts
│   └── use*.ts           # React hooks
└── ui/                   # 展示层：组件
    ├── index.ts
    └── *.tsx             # React 组件
```

### executor-selector 模块

提供执行器发现（从后端 GET /api/agents/discovery 获取）、
配置管理（localStorage 持久化）、最近使用追踪（LRU 最多 8 条）。

| Hook | 职责 |
|------|------|
| `useExecutorDiscovery` | 获取可用执行器列表 + 连接器能力 |
| `useExecutorConfig` | 管理选择状态 + 持久化 + 最近使用追踪 |
<!-- PROJECT-SPECIFIC-END -->

---

## Module Organization

<!-- How should new features be organized? -->

<!-- PROJECT-SPECIFIC-START: Key Types -->
### 关键类型定义

前端类型与后端 Rust 实体完全对齐，使用 **snake_case** 字段名直接映射：

```typescript
// types/index.ts — 核心领域类型

interface Project {
  id: string;
  name: string;
  description: string;
  backend_id: string;
  config: ProjectConfig;
  created_at: string;
  updated_at: string;
}

interface Workspace {
  id: string;
  project_id: string;
  name: string;
  container_ref: string;          // 物理目录路径
  workspace_type: WorkspaceType;  // "git_worktree" | "static" | "ephemeral"
  status: WorkspaceStatus;        // "pending" | "preparing" | "ready" | "active" | "archived" | "error"
  git_config?: GitConfig | null;
  created_at: string;
  updated_at: string;
}

interface Story {
  id: string;
  project_id: string;
  backend_id: string;
  title: string;
  description?: string;
  status: StoryStatus;
  context: StoryContext;           // { prd_doc, spec_refs[], resource_list[] }
  created_at: string;
  updated_at: string;
}

interface Task {
  id: string;
  story_id: string;
  workspace_id?: string | null;
  title: string;
  description?: string;
  status: TaskStatus;
  agent_binding: AgentBinding;    // { agent_type, agent_pid, preset_name }
  artifacts: Artifact[];
  created_at: string;
  updated_at: string;
}
```

> **设计决策**：前端类型直接使用 snake_case 与后端对齐，不做 camelCase 转换。
> 原因：减少映射层复杂度，避免序列化/反序列化不一致。Store 中的 `mapStory`/`mapTask`
> 函数仅做状态值归一化（后端旧状态名 → 前端展示状态名），不做字段名转换。
<!-- PROJECT-SPECIFIC-END -->

---

## Naming Conventions

<!-- File and folder naming rules -->

<!-- PROJECT-SPECIFIC-START: Naming Rules -->
> **注意：技术栈确定后，根据所选框架约定调整命名规范。**

- **功能目录**：小写短横线，如 `story-detail/`
- **组件文件**：PascalCase，如 `StoryCard.tsx`
- **Hook文件**：camelCase with `use` 前缀，如 `useStoryStatus.ts`
- **Store文件**：camelCase，如 `dashboardStore.ts`
- **类型文件**：PascalCase，如 `Story.types.ts`
<!-- PROJECT-SPECIFIC-END -->

---

## Examples

<!-- Link to well-organized modules as examples -->

<!-- PROJECT-SPECIFIC-START: Current Status -->
### 当前状态

> 技术栈已确定，v0.2 领域模型适配完成。

**技术栈决策：**
- [x] 前端框架：React 19
- [x] 状态管理：Zustand
- [x] 实时通信：SSE (Server-Sent Events)
- [x] 构建工具：Vite 7
- [x] 样式方案：Tailwind CSS v4
- [ ] UI 组件库选型（当前使用自定义组件）

### 架构演进记录

#### v0.2 — Project/Workspace 领域模型适配

**变更范围**：
- 新增 `projectStore`、`workspaceStore` Zustand Store
- 重构 `storyStore`：从 `backendId` 驱动改为 `projectId` 驱动
- 新增 `features/project/`、`features/workspace/` 模块
- 重写 `types/index.ts`：新增 Project/Workspace 类型，Story/Task 类型对齐后端
- 更新 `api/client.ts`：新增 PUT/PATCH/DELETE 方法
- 更新 `workspace-layout.tsx`：侧边栏加入项目选择器和工作空间列表
- 更新 `DashboardPage.tsx`：从 backendId 切换到 projectId 驱动
- 更新所有 Story/Task 组件：适配新字段结构

**设计决策**：
- 类型字段使用 snake_case 直接映射后端，不做 camelCase 转换
- Store 中的映射函数仅做状态值归一化，不做字段名转换
- Workspace 数据按 projectId 隔离存储（`workspacesByProjectId`）
<!-- PROJECT-SPECIFIC-END -->
