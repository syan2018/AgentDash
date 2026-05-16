# 前端目录结构

> AgentDashboard 前端代码的组织方式。

---

## 设计原则

前端采用 monorepo 多包结构，核心应用按**功能模块（FSD 风格）**组织：
- 各模块独立，通过共享类型交互
- 视图组件（`ui/`）和状态逻辑（`model/`）分离
- 全局状态收敛到 `stores/`
- 通用 UI 基础件抽取到独立 `@agentdash/ui` 包

---

## Monorepo 包结构

```
packages/
├── app-web/           # 主 Web 应用（React 19 + TypeScript 5.9 + Vite 7 + Tailwind v4）
├── app-tauri/         # Tauri 桌面端入口
├── ui/                # 共享 UI 基础件（Button / Select / Notice 等）
├── core/              # 共享核心逻辑（local-runtime 等）
└── views/             # 共享视图组件（LocalRuntimeView 等）
```

---

## 主应用布局（packages/app-web/src/）

```
packages/app-web/src/
├── api/                            # API 调用层
│   ├── client.ts                   # 统一 HTTP 客户端
│   ├── mappers.ts                  # API 响应映射
│   ├── eventStream.ts              # SSE 事件流
│   ├── origin.ts                   # API_ORIGIN / buildApiPath
│   └── streamRegistry.ts           # 流连接追踪（HMR 安全）
├── components/                     # 共享 UI 组件
│   ├── acp/                        # 会话内容可视化组件（content-block / tool-call / plan / confirmation）
│   ├── layout/                     # 布局组件（WorkspaceLayout、侧边栏）
│   ├── ui/                         # 通用 UI（StatusBadge, detail-panel 等）
│   └── context-config-editor.tsx   # 上下文配置编辑器
├── features/                       # 功能模块（FSD 风格：model/ + ui/）
│   ├── session/                    # 会话流展示（stream / chat view / event cards）
│   ├── executor-selector/          # 执行器/模型选择器
│   ├── project/                    # 项目管理（selector / agent view / preset）
│   ├── workspace/                  # 工作空间管理
│   ├── workspace-panel/            # 工作空间面板（tab system / context overview）
│   ├── story/                      # Story 管理
│   ├── task/                       # Task 管理
│   ├── workflow/                   # Workflow + Lifecycle DAG 编辑器
│   ├── agent/                      # Agent 管理（tab view / active sessions）
│   ├── assets-panel/               # 资产面板（MCP / Workflow 分类）
│   ├── canvas-panel/               # Canvas 运行时预览与绑定
│   ├── session-context/            # 会话上下文展示
│   ├── file-reference/             # 文件引用（prompt 附件选择）
│   ├── vfs/                        # VFS 浏览器
│   ├── context-source/             # 上下文来源配置
│   ├── mcp-shared/                 # MCP Server 配置共享组件
│   └── routine/                    # Routine 自动触发
├── hooks/                          # 全局 hooks
│   └── use-theme.ts                # 主题切换
├── pages/                          # 页面组件
│   ├── DashboardPage.tsx           # Project 驱动的看板页
│   ├── SessionPage.tsx             # 会话页（ExecutorSelector + Backbone 事件流）
│   ├── StoryPage.tsx               # Story 详情页
│   ├── ProjectSettingsPage.tsx     # 项目设置页
│   ├── SettingsPage.tsx            # 全局设置页
│   ├── LifecycleEditorShellPage.tsx # Lifecycle 编辑页
│   └── LoginPage.tsx               # 登录页
├── services/                       # 服务层（API 封装）
│   ├── executor.ts                 # ExecutorConfig + promptSession API
│   ├── session.ts                  # Session 管理 API
│   ├── workflow.ts                 # Workflow 管理 API
│   ├── vfs.ts                      # VFS API
│   ├── browseDirectory.ts          # 目录浏览 API
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
│   ├── activeSessionsStore.ts      # 活跃会话追踪
│   ├── llmProviderStore.ts         # LLM Provider 管理
│   ├── routineStore.ts             # Routine 管理
│   ├── authStore.ts                # 认证状态
│   ├── sidebarSessionsStore.ts     # 侧边栏会话列表
│   └── workspaceTabStore.ts        # 工作空间标签页状态
├── desktop/                        # Tauri 桌面端相关
├── generated/                      # 自动生成代码
│   └── backbone-protocol.ts        # Backbone Protocol TS 绑定（由 generate_backbone_protocol_ts 生成）
├── types/
│   ├── index.ts                    # 全局类型（Project/Workspace/Story/Task 等）
│   └── skill-asset.ts              # Skill 资产类型
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

- **功能目录**：小写短横线，如 `session/`
- **组件文件**：PascalCase，如 `SessionEntry.tsx`
- **Hook 文件**：camelCase with `use` 前缀，如 `useSessionStream.ts`
- **Store 文件**：camelCase，如 `workflowStore.ts`
- **Service 文件**：camelCase，如 `workflow.ts`
- **类型文件**：`types.ts`（Feature 内）或 `index.ts`（全局）

---

## 技术栈

- [x] 前端框架：React 19
- [x] 状态管理：Zustand 5
- [x] 实时通信：SSE/NDJSON（fetch 优先，SSE 降级）
- [x] 构建工具：Vite 7
- [x] 样式方案：Tailwind CSS v4
- [x] 路由：React Router 7
- [x] 测试：Vitest 4
- [x] UI 组件：自研组件 + shadcn 模式（`@agentdash/ui` 包）

---

*更新：2026-05-16 — 对齐 monorepo packages 结构、实际 features/stores/pages 清单*
