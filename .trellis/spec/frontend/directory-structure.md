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
### 实际目录布局（React + TypeScript + Vite + Tailwind v4）

```
frontend/src/
├── api/                         # API 调用层
│   ├── client.ts
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
│   ├── executor-selector/       # 执行器/模型选择器（新增）
│   │   ├── model/               # types, useExecutorDiscovery, useExecutorConfig
│   │   └── ui/                  # ExecutorSelector（下拉选择器 + 高级选项）
│   ├── story/                   # Story 卡片/抽屉/列表视图
│   └── task/                    # Task 卡片/抽屉/列表
├── hooks/                       # 全局 hooks
├── pages/                       # 页面组件
│   ├── DashboardPage.tsx
│   └── SessionPage.tsx          # 集成 ExecutorSelector + ACP 会话流
├── services/                    # 服务层
│   └── executor.ts              # ExecutorConfig 类型 + promptSession API
├── stores/                      # 全局 Zustand stores
├── styles/                      # Tailwind v4 主题变量（HSL 色系）
├── types/
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

前端需要与后端共享以下核心类型（详见 `docs/modules/02-state.md`）：

```typescript
// 以下为概念性类型定义，技术栈确定后调整

type StoryStatus = 'created' | 'context_injected' | 'decomposed'
  | 'orchestrating' | 'validating' | 'completed' | 'failed'

type TaskStatus = 'pending' | 'assigned' | 'running'
  | 'validating' | 'completed' | 'failed'

interface Story {
  id: string
  title: string
  status: StoryStatus
  taskIds: string[]
  // context 详情按需加载
}

interface Task {
  id: string
  storyId: string
  status: TaskStatus
  agentBinding: AgentBinding | null
}
```
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

> 技术栈未确定，上述为概念性目录设计。
> 确定技术栈后，在此文件更新实际目录结构。

**需要讨论决定：**
- [ ] 前端框架（React / Vue / Svelte / ...）
- [ ] 状态管理方案（Redux / Zustand / Pinia / ...）
- [x] 实时通信方案（SSE - Server-Sent Events）
- [ ] UI组件库选型
<!-- PROJECT-SPECIFIC-END -->
