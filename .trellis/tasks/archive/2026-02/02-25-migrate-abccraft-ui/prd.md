# 迁移 ABCCraft UI 到项目（剔除 Task DAG）

## 目标

将 `references/ABCCraft/src/frontend` 中的 UI 实现迁移到本项目的 `frontend/` 目录，同时**剔除 Task 之间的有向图（DAG）依赖关系**，简化为扁平的 Story-Task 两层结构。

## 背景

### ABCCraft 的复杂设计（需简化）

```
CraftTask (Story)
  └── AgentTask[] (Task)
       ├── dependencies: AgentTaskDependency[]  // 有向图边 ❌ 剔除
       └── executionTrace: SessionUpdate[]
```

### AgentDashboard 的简化设计

```
Story
  ├── context: Context          // 设计信息、规范文档引用
  ├── status: StoryStatus
  └── tasks: Task[]             // 扁平列表，无依赖关系

Task
  ├── storyId: string           // 归属 Story
  ├── agentBinding: AgentBinding | null
  ├── context: Context          // 继承自 Story + 特定注入
  ├── status: TaskStatus
  └── artifacts: Artifact[]     // 执行产物
```

**编排逻辑**：由 Story 的编排策略（Orchestration 模块04）决定 Task 执行顺序，而非 Task 间的硬编码依赖。

---

## 迁移范围

### ✅ 需要迁移的内容

#### 1. 主题与样式系统
- `index.css` - Tailwind CSS 主题变量、浅色/深色模式
- 颜色系统：primary、secondary、success、warning、info、destructive
- 字体：Inter、JetBrains Mono

#### 2. 布局组件
- `workspace-layout.tsx` - 工作区整体布局
- 侧边栏导航
- Drawer 上下文管理

#### 3. Story 相关组件
- **列表视图** `board-list-view.tsx` - 按状态分组展示 Story
- **详情抽屉** `task-drawer.tsx` → 改名为 `story-drawer.tsx`
  - Tab: "上下文" (原 Context)
  - Tab: "任务列表" (替代原 DAG 视图)
  - Tab: "验收" (原 Review)

#### 4. Task 相关组件
- **详情抽屉** `agent-task-drawer.tsx` → 改名为 `task-drawer.tsx`
- 流式执行日志展示
- 执行产物展示

#### 5. 基础组件
- `status-badge.tsx` - Story/Task 状态徽章
- `theme-toggle.tsx` - 主题切换

#### 6. ACP 组件（Agent Client Protocol）
- `content-block.tsx` - 文本/图片/资源渲染
- `tool-call.tsx` - 工具调用渲染
- `plan.tsx` - 执行计划渲染
- `confirmation-request.tsx` - 用户确认卡片

### ❌ 需要剔除的内容

| ABCCraft 文件/功能 | 剔除原因 |
|-------------------|----------|
| `dag-view.tsx` | DAG 可视化，Task 间不再有依赖关系 |
| `AgentTaskDependency` 类型 | 有向图边定义 |
| `AgentTask.dependencies` 字段 | Task 间依赖关系 |
| `computeDagProgress()` | DAG 进度计算，改为简单计数 |

### 🔧 需要修改的内容

| 原内容 | 修改方式 |
|--------|----------|
| `CraftTask` → `Story` | 重命名类型 |
| `AgentTask` → `Task` | 重命名类型 |
| Story 详情 Tab "DAG Workflow" | 改为 "任务列表"，展示扁平 Task 列表 |
| `agentTasks: AgentTask[]` | 改为 `tasks: Task[]` |

---

## 类型定义映射

### ABCCraft (源)

```typescript
interface CraftTask {
  id: string;
  title: string;
  description?: string;
  status: CraftTaskStatus;  // draft | ready | running | review | completed | failed | cancelled
  contextId: string;
  agentTasks: AgentTask[];  // DAG 节点
}

interface AgentTask {
  id: string;
  taskId: string;
  name: string;
  agentType: AgentType;     // session | planner | worker | reviewer | researcher
  status: AgentTaskStatus;  // pending | queued | running | succeeded | failed | skipped | cancelled
  dependencies: AgentTaskDependency[];  // ❌ 剔除
  executionTrace?: SessionUpdate[];
  output?: string;
}

interface AgentTaskDependency {
  agentTaskId: string;
  dependsOnAgentTaskId: string;
}
```

### AgentDashboard (目标)

```typescript
interface Story {
  id: string;
  title: string;
  description?: string;
  status: StoryStatus;      // draft | ready | running | review | completed | failed | cancelled
  context: Context;
  taskIds: string[];        // Task ID 列表
  createdAt: timestamp;
  updatedAt: timestamp;
}

interface Task {
  id: string;
  storyId: string;
  title: string;            // 原 name
  agentType: AgentType;
  status: TaskStatus;       // pending | queued | running | succeeded | failed | skipped | cancelled
  context: Context;
  agentBinding: AgentBinding | null;
  artifacts: Artifact[];    // 原 output 改为 artifacts
  createdAt: timestamp;
  updatedAt: timestamp;
}
```

---

## 组件结构规划

```
frontend/src/
├── components/
│   ├── layout/
│   │   └── workspace-layout.tsx    # 工作区布局
│   ├── ui/                          # 基础 UI 组件
│   │   ├── status-badge.tsx        # 状态徽章
│   │   └── theme-toggle.tsx        # 主题切换
│   └── acp/                         # ACP 渲染组件
│       ├── content-block.tsx
│       ├── tool-call.tsx
│       ├── plan.tsx
│       └── confirmation-request.tsx
├── features/
│   ├── story/
│   │   ├── story-list-view.tsx     # Story 列表（原 board-list-view）
│   │   ├── story-drawer.tsx        # Story 详情抽屉
│   │   └── story-card.tsx          # Story 卡片
│   └── task/
│       ├── task-list.tsx           # Task 列表（替代 DAG 视图）
│       ├── task-drawer.tsx         # Task 详情抽屉
│       └── task-card.tsx           # Task 卡片
├── types/
│   └── index.ts                    # 类型定义
├── styles/
│   └── index.css                   # 全局样式
└── hooks/
    └── use-theme.ts                # 主题状态
```

---

## UI 样式保留清单

### 颜色系统
```css
/* 保留 ABCCraft 的配色 */
--color-primary: hsl(var(--primary));        /* 蓝色系 */
--color-success: hsl(var(--success));        /* 绿色系 */
--color-warning: hsl(var(--warning));        /* 黄色系 */
--color-destructive: hsl(var(--destructive)); /* 红色系 */
--color-info: hsl(var(--info));              /* 蓝色系 */
```

### 状态徽章样式
```typescript
// Story 状态
const storyStatusConfig = {
  draft:     { label: "草稿", className: "bg-muted text-muted-foreground" },
  ready:     { label: "就绪", className: "bg-info/15 text-info" },
  running:   { label: "执行中", className: "bg-primary/15 text-primary" },
  review:    { label: "待验收", className: "bg-warning/15 text-warning" },
  completed: { label: "已完成", className: "bg-success/15 text-success" },
  failed:    { label: "失败", className: "bg-destructive/15 text-destructive" },
  cancelled: { label: "已取消", className: "bg-muted text-muted-foreground" },
};
```

### 布局尺寸
- 侧边栏宽度：待定
- Story Drawer 宽度：max-w-[80rem]
- Task Drawer 宽度：max-w-[52rem]
- 列表行高：约 56px

---

## 验收标准

- [ ] 主题系统迁移完成，支持浅色/深色模式切换
- [ ] 工作区布局组件迁移完成
- [ ] Story 列表视图展示正确，按状态分组
- [ ] Story 详情抽屉有三个 Tab：上下文/任务列表/验收
- [ ] Task 详情抽屉展示执行日志和产物
- [ ] 状态徽章样式与 ABCCraft 一致
- [ ] **无 DAG 相关代码**：无 ReactFlow、无 Dagre、无 dependencies 字段
- [ ] 类型定义与 AgentDashboard 规范一致
- [ ] lint 和类型检查通过

---

## 技术注意事项

1. **依赖库检查**：ABCCraft 使用了 `@xyflow/react` (ReactFlow) 和 `dagre`，这些在迁移后应该移除
2. **类型路径**：原 ABCCraft 有 `services/generated/abccraft.ts` (Orval 生成)，新项目中需要重新生成或手动定义
3. **Mock 数据**：原 `mock-data.ts` 中的数据有 DAG 结构，需要重构为扁平结构
4. **路由**：原 ABCCraft 的路由配置可能需要适配新项目的结构

---

## 参考文件位置

```
references/ABCCraft/src/frontend/src/
├── types/index.ts              # 类型定义
├── features/
│   ├── board/board-list-view.tsx
│   ├── task/task-drawer.tsx
│   ├── agent/agent-task-drawer.tsx
│   └── workflow/dag-view.tsx   # ❌ 剔除
├── components/
│   ├── layout/workspace-layout.tsx
│   └── status-badge.tsx
└── index.css                   # 主题样式
```
