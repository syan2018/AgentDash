# 前端目录结构

> Monorepo 多包结构 + FSD 风格功能模块组织。

---

## Monorepo 包结构

```
packages/
├── app-web/       # 主 Web 应用（React 19 + TypeScript 5.9 + Vite 7 + Tailwind v4）
├── app-tauri/     # Tauri 桌面端入口
├── ui/            # 共享 UI 基础件（Button / Select / Notice 等）
├── core/          # 共享核心逻辑（local-runtime 等）
├── views/         # 共享视图组件（LocalRuntimeView 等）
└── extension/     # Extension 一体化 SDK：app authoring / host / browser / react / CLI toolchain
```

---

## 主应用布局（packages/app-web/src/）

```
packages/app-web/src/
├── api/                            # API 调用层
│   ├── client.ts                   # 统一 HTTP 客户端
│   ├── mappers.ts                  # API 响应映射
│   ├── eventStream.ts              # Project NDJSON 事件流
│   ├── origin.ts                   # API_ORIGIN / buildApiPath
│   ├── streamRegistry.ts           # 流连接追踪（HMR 安全）
│   ├── auth.ts                     # 认证 API
│   ├── settings.ts                 # 设置 API
│   └── llmProviders.ts             # LLM Provider API
├── components/                     # 共享 UI 组件
│   ├── acp/                        # 会话内容可视化（content-block / tool-call / plan / confirmation）
│   ├── layout/                     # 布局组件（WorkspaceLayout、侧边栏）
│   └── ui/                         # 通用 UI（StatusBadge 等）
├── features/                       # 功能模块（FSD 风格：model/ + ui/）
│   ├── session/                    # 会话流展示（stream / chat view / event cards / ContextFrame）
│   ├── executor-selector/          # 执行器/模型选择器
│   ├── project/                    # 项目管理（selector / agent view / preset）
│   ├── workspace/                  # 工作空间管理
│   ├── workspace-panel/            # 工作空间面板（tab system / context overview）
│   ├── story/                      # Story 管理
│   ├── task/                       # Task 管理
│   ├── workflow/                   # Workflow + Lifecycle DAG 编辑器
│   ├── agent/                      # Agent 管理（tab view / active sessions）
│   ├── assets-panel/               # 资产面板（MCP / Workflow / Canvas 分类）
│   ├── canvas-panel/               # Canvas 运行时预览与绑定
│   ├── session-context/            # 会话上下文展示
│   ├── file-reference/             # 文件引用（prompt 附件选择）
│   ├── vfs/                        # VFS 浏览器
│   ├── context-source/             # 上下文来源配置
│   ├── mcp-shared/                 # MCP Server 配置共享组件
│   └── routine/                    # Routine 自动触发
├── pages/                          # 页面组件
│   ├── DashboardPage.tsx           # Project 看板页
│   ├── AgentRunWorkspacePage.tsx   # AgentRun工作台（canonical Runtime snapshot/events）
│   ├── StoryPage.tsx               # Story 详情页
│   ├── ProjectSettingsPage.tsx     # 项目设置页
│   ├── SettingsPage.tsx            # 全局设置页
│   ├── LifecycleEditorShellPage.tsx # Lifecycle 编辑页
│   └── LoginPage.tsx               # 登录页
├── services/                       # 服务层（API 封装）
│   ├── executor.ts                 # ExecutorConfig + promptSession
│   ├── session.ts                  # Session 管理
│   ├── workflow.ts                 # Workflow 管理
│   ├── vfs.ts                      # VFS
│   ├── browseDirectory.ts          # 目录浏览
│   ├── filePicker.ts               # 文件选择器
│   ├── currentUser.ts              # 当前用户
│   ├── canvas.ts                   # Canvas 管理
│   ├── skillAsset.ts               # Skill 资产
│   ├── mcpPreset.ts                # MCP Preset 管理
│   ├── directory.ts                # 用户/组目录
│   └── contextAudit.ts             # 上下文审计
├── stores/                         # 全局 Zustand stores
│   ├── projectStore.ts             # Project CRUD + 选择
│   ├── workspaceStore.ts           # Workspace CRUD + 状态
│   ├── storyStore.ts               # Story/Task 数据
│   ├── coordinatorStore.ts         # 后端连接管理
│   ├── eventStore.ts               # Project NDJSON 事件流状态
│   ├── workflowStore.ts            # WorkflowGraph 定义态管理
│   ├── lifecycleStore.ts           # Lifecycle 运行态 view projection
│   ├── sessionHistoryStore.ts      # 会话历史
│   ├── settingsStore.ts            # 全局设置
│   ├── currentUserStore.ts         # 当前用户
│   ├── activeSessionsStore.ts      # 活跃会话追踪
│   ├── llmProviderStore.ts         # LLM Provider 管理
│   ├── routineStore.ts             # Routine 管理
│   ├── authStore.ts                # 认证状态
│   ├── sidebarSessionsStore.ts     # 侧边栏会话列表
│   └── workspaceTabStore.ts        # 工作空间标签页状态
├── generated/                      # 自动生成代码
│   └── backbone-protocol.ts        # Backbone Protocol TS 绑定（由 Rust build.rs 生成）
├── types/                          # 全局类型定义
│   ├── index.ts                    # 核心类型（Project / Workspace / Story / Task 等）
│   ├── context.ts                  # 上下文容器类型
│   ├── workflow.ts                 # Workflow 类型
│   ├── session.ts                  # Session 类型
│   ├── acp.ts                      # Artifact 类型
│   ├── canvas.ts                   # Canvas 类型
│   ├── mcp-preset.ts               # MCP Preset 类型
│   ├── skill-asset.ts              # Skill 资产类型
│   └── terminal.ts                 # Terminal 类型
├── desktop/                        # Tauri 桌面端相关
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

## 命名规范

- **功能目录**：小写短横线（`session/`、`workspace-panel/`）
- **组件文件**：PascalCase（`SessionEntry.tsx`、`ContextFrameCard.tsx`）
- **Hook 文件**：camelCase + `use` 前缀（`useSessionStream.ts`）
- **Store 文件**：camelCase（`workflowStore.ts`）
- **Service 文件**：camelCase（`workflow.ts`）
- **类型文件**：`types.ts`（Feature 内）或按领域拆分（全局 `types/`）

---

## 类型组织

全局类型从 `types/index.ts` 导出，按领域拆分到 `types/*.ts` 子文件。字段名直接使用 snake_case 与后端对齐（详见 [type-safety.md](./type-safety.md)）。Feature 私有类型放在 `features/{name}/model/types.ts`。`generated/backbone-protocol.ts` 由 Rust build.rs 自动生成，禁止手动修改。
