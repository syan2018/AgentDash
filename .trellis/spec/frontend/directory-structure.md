# 前端目录结构

> AgentDashboard 前端代码的组织方式。
> **注意：当前为概念阶段，技术栈未定，目录结构仅为参考设计。**

---

## 设计原则

前端目录结构按**功能模块**组织，每个功能模块对应后端的一个核心模块：

- 各模块独立，通过共享的类型定义交互
- 视图组件和状态逻辑分离
- 实时状态订阅封装在 hooks 层

---

## 建议目录布局（参考设计）

```
frontend/
├── features/              # 功能模块（对应后端模块）
│   ├── connection/        # 连接管理（模块01）
│   │   ├── components/    # 连接配置、状态显示组件
│   │   ├── hooks/         # useConnection, useBackendStatus
│   │   └── store/         # 连接状态管理
│   ├── dashboard/         # 主看板（Story展示）
│   │   ├── components/    # StoryCard, TaskBadge等
│   │   ├── hooks/         # useStories, useStoryStatus
│   │   └── store/         # 看板状态（含多后端隔离）
│   ├── story/             # Story详情和管理
│   │   ├── components/    # StoryDetail, ContextEditor
│   │   ├── hooks/         # useStory, useStoryTasks
│   │   └── store/
│   ├── task/              # Task详情和执行状态
│   │   ├── components/    # TaskDetail, AgentOutput（流式）
│   │   ├── hooks/         # useTask, useTaskOutput
│   │   └── store/
│   └── view/              # 视图组织（模块08）
│       ├── kanban/        # 看板视图组件
│       ├── list/          # 列表视图组件
│       └── tree/          # 树状视图组件
├── shared/                # 共享组件和工具
│   ├── components/        # 通用UI组件
│   ├── hooks/             # 通用Hook
│   ├── types/             # 共享类型定义（Story, Task等）
│   └── utils/             # 工具函数
├── api/                   # API调用层
│   ├── client/            # HTTP/WebSocket客户端
│   └── endpoints/         # 各模块API调用封装
└── app/                   # 应用入口
    ├── routes/            # 路由配置
    └── config/            # 应用配置
```

---

## 关键类型定义

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

---

## 命名规范

> **注意：技术栈确定后，根据所选框架约定调整命名规范。**

- **功能目录**：小写短横线，如 `story-detail/`
- **组件文件**：PascalCase，如 `StoryCard.tsx`
- **Hook文件**：camelCase with `use` 前缀，如 `useStoryStatus.ts`
- **Store文件**：camelCase，如 `dashboardStore.ts`
- **类型文件**：PascalCase，如 `Story.types.ts`

---

## 当前状态

> 技术栈未确定，上述为概念性目录设计。
> 确定技术栈后，在此文件更新实际目录结构。

**需要讨论决定：**
- [ ] 前端框架（React / Vue / Svelte / ...）
- [ ] 状态管理方案（Redux / Zustand / Pinia / ...）
- [ ] 实时通信方案（WebSocket / SSE / ...）
- [ ] UI组件库选型
