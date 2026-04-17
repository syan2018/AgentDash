# 前端目录结构

> AgentDashboard 前端代码的组织方式。

---

## 设计原则

前端目录结构按**功能模块（FSD 风格）**组织：
- 各模块独立，通过共享类型交互
- 视图组件（`ui/`）和状态逻辑（`model/`）分离
- 全局状态收敛到 `stores/`

---

## 实际目录布局（React 19 + TypeScript 5.9 + Vite 7 + Tailwind v4）

```
frontend/src/
├── api/                            # API 调用层
│   ├── client.ts                   # 统一 HTTP 客户端（GET/POST/PUT/PATCH/DELETE）
│   ├── eventStream.ts              # SSE 事件流（Dashboard 用）
│   ├── origin.ts                   # API_ORIGIN / buildApiPath
│   ├── settings.ts                 # Settings API
│   └── streamRegistry.ts           # 流连接追踪（HMR 安全）
├── components/                     # 共享 UI 组件
│   ├── acp/                        # ACP 协议可视化组件
│   ├── layout/                     # 布局组件（WorkspaceLayout、侧边栏）
│   ├── ui/                         # 通用 UI（StatusBadge, ThemeToggle 等）
│   ├── context-config-editor.tsx   # 上下文配置编辑器
│   └── context-config-defaults.ts  # 上下文配置默认值
├── features/                       # 功能模块（FSD 风格：model/ + ui/）
│   ├── acp-session/                # ACP 会话流展示
│   │   ├── model/                  # types, useAcpStream, useAcpSession, agentdashMeta
│   │   └── ui/                     # SessionChatView, AcpSessionEntry, AcpMessageCard,
│   │                               #   AcpToolCallCard, AcpSystemEventCard/Guard,
│   │                               #   AcpTaskEventCard, AcpOwnerContextCard, EventCards 等
│   ├── executor-selector/          # 执行器/模型选择器
│   │   ├── model/                  # types, useExecutorConfig, useExecutorDiscoveredOptions
│   │   └── ui/                     # ExecutorSelector
│   ├── project/                    # 项目管理
│   │   ├── project-selector.tsx    # 项目选择器 + 创建表单
│   │   ├── project-agent-view.tsx  # 项目 Agent 配置视图
│   │   └── agent-preset-editor.tsx # Agent 预设编辑器
│   ├── workspace/                  # 工作空间管理
│   │   ├── workspace-list.tsx      # 工作空间列表 + 创建面板
│   │   └── directory-browser-dialog.tsx # 目录浏览对话框
│   ├── story/                      # Story 管理
│   │   ├── story-tab-view.tsx      # Story 标签页视图
│   │   ├── story-list-view.tsx     # Story 列表视图
│   │   ├── story-detail-panels.tsx # Story 详情面板
│   │   ├── story-session-panel.tsx # Story 会话面板
│   │   ├── create-task-panel.tsx   # 创建 Task 面板
│   │   └── context-source-utils.ts # 上下文来源工具函数
│   ├── task/                       # Task 管理
│   │   ├── task-drawer.tsx         # Task 抽屉面板
│   │   ├── task-agent-session-panel.tsx # Task Agent 会话面板
│   │   └── agent-binding-fields.tsx    # Agent 绑定字段
│   ├── workflow/                   # Workflow 管理
│   │   ├── workflow-tab-view.tsx   # Workflow 标签页
│   │   ├── workflow-editor.tsx     # Workflow 编辑器
│   │   ├── lifecycle-editor.tsx    # Lifecycle 编辑器
│   │   ├── binding-editor.tsx      # Binding 编辑器
│   │   ├── project-workflow-panel.tsx  # 项目 Workflow 面板
│   │   ├── task-workflow-panel.tsx     # Task Workflow 面板
│   │   ├── shared-labels.ts       # 共享标签常量
│   │   └── ui/                    # step-summary, validation-panel
│   ├── agent/                      # Agent 管理
│   │   ├── agent-tab-view.tsx      # Agent 标签页视图
│   │   └── active-session-list.tsx # 活跃会话列表
│   ├── address-space/              # 寻址空间浏览
│   │   ├── vfs-browser.tsx
│   │   └── index.ts
│   ├── file-reference/             # 文件引用（prompt 附件选择）
│   │   ├── FilePickerPopup.tsx
│   │   ├── RichInput.tsx
│   │   ├── useFileReference.ts
│   │   └── buildPromptBlocks.ts
│   ├── session-context/            # 会话上下文展示
│   │   ├── context-panels.tsx
│   │   ├── hook-runtime-cards.tsx
│   │   ├── surface-card.tsx
│   │   ├── utils.ts
│   │   └── index.ts
│   └── context-source/             # 上下文来源配置
├── hooks/                          # 全局 hooks
│   └── use-theme.ts                # 主题切换
├── pages/                          # 页面组件
│   ├── DashboardPage.tsx           # Project 驱动的看板页
│   ├── SessionPage.tsx             # 会话页（ExecutorSelector + ACP 流）
│   ├── StoryPage.tsx               # Story 详情页
│   ├── ProjectSettingsPage.tsx     # 项目设置页
│   ├── SettingsPage.tsx            # 全局设置页
│   ├── WorkflowEditorPage.tsx      # Workflow 编辑页
│   └── LifecycleEditorPage.tsx     # Lifecycle 编辑页
├── services/                       # 服务层（API 封装）
│   ├── executor.ts                 # ExecutorConfig + promptSession API
│   ├── session.ts                  # Session 管理 API
│   ├── workflow.ts                 # Workflow 管理 API
│   ├── vfs.ts            # Address Space API
│   ├── browseDirectory.ts          # 目录浏览 API
│   ├── directory.ts                # 目录服务
│   ├── filePicker.ts               # 文件选择器 API
│   └── currentUser.ts              # 当前用户 API
├── stores/                         # 全局 Zustand stores
│   ├── projectStore.ts             # Project CRUD + 选择
│   ├── workspaceStore.ts           # Workspace CRUD + 状态管理
│   ├── storyStore.ts               # Story/Task 数据
│   ├── coordinatorStore.ts         # 后端连接管理
│   ├── eventStore.ts               # SSE 事件流状态
│   ├── workflowStore.ts            # Workflow 管理
│   ├── sessionHistoryStore.ts      # 会话历史
│   ├── settingsStore.ts            # 全局设置
│   ├── currentUserStore.ts         # 当前用户
│   └── activeSessionsStore.ts      # 活跃会话追踪
├── generated/                      # 自动生成代码
│   └── agentdash-acp-meta.ts       # ACP Meta TS 绑定
├── styles/                         # Tailwind v4 主题变量（HSL 色系）
├── types/
│   └── index.ts                    # 全局类型（Project/Workspace/Story/Task 等）
├── App.tsx                         # 路由根组件
└── main.tsx                        # 入口
```

---

## Feature 模块标准结构（FSD 风格）

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

---

## 关键类型定义

前端类型与后端 Rust 实体完全对齐，使用 **snake_case** 字段名直接映射：

```typescript
// types/index.ts — 核心领域类型
interface Project {
  id: string;
  name: string;
  description: string;
  config: ProjectConfig;
  created_at: string;
  updated_at: string;
}

interface Story {
  id: string;
  project_id: string;
  title: string;
  status: StoryStatus;
  context: StoryContext;
  created_at: string;
  updated_at: string;
}

interface Task {
  id: string;
  story_id: string;
  workspace_id?: string | null;
  title: string;
  status: TaskStatus;
  agent_binding: AgentBinding;
  artifacts: Artifact[];
  created_at: string;
  updated_at: string;
}
```

> **设计决策**：前端类型直接使用 snake_case 与后端对齐，不做 camelCase 转换。
> Store 中的 mapper 仅做状态值归一化，不做字段名转换。

---

## 命名规范

- **功能目录**：小写短横线，如 `acp-session/`
- **组件文件**：PascalCase，如 `AcpSessionEntry.tsx`
- **Hook 文件**：camelCase with `use` 前缀，如 `useAcpStream.ts`
- **Store 文件**：camelCase，如 `workflowStore.ts`
- **Service 文件**：camelCase，如 `workflow.ts`
- **类型文件**：`types.ts`（Feature 内）或 `index.ts`（全局）

---

## executor-selector 模块

提供执行器发现（从后端 `GET /api/agents/discovery` 获取）、
配置管理（localStorage 持久化）、最近使用追踪（LRU 最多 8 条）。

| Hook | 职责 |
|------|------|
| `useExecutorDiscoveredOptions` | 获取可用执行器列表 + 连接器能力 |
| `useExecutorConfig` | 管理选择状态 + 持久化 + 最近使用追踪 |

---

## 技术栈决策

- [x] 前端框架：React 19
- [x] 状态管理：Zustand 5
- [x] 实时通信：SSE/NDJSON（fetch 优先，SSE 降级）
- [x] 构建工具：Vite 7
- [x] 样式方案：Tailwind CSS v4
- [x] 路由：React Router 7
- [x] 测试：Vitest 4
- [ ] UI 组件库选型（当前使用自研组件 + shadcn 模式）

---

*更新：2026-03-29 — 对齐前端实际目录结构、features、stores、pages、services*
