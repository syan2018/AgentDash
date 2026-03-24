# PRD：前端信息架构重构 — Agent-First 主页设计

## 背景与问题

当前前端信息架构存在两个核心问题：

1. **叙事错位**：主页以 Story 看板为核心，传达的是"这是一个项目管理工具"，而非项目真正的定位——一个多 Agent、多设备的编排控制平面。Agent 能力（PiAgent、第三方 Executor、Hook Runtime、Companion 协作）全部藏在三级以上的深层级里。

2. **导航维度不对齐**：侧边栏的"看板"和"会话"是不同维度的概念被放在同一层级切换——"看板"是项目视角，"会话"是时间线视角，二者不对等，用户找不到清晰的心智模型。

---

## 目标

重构前端主导航结构，使其：
- 将 Agent 作为一等公民展示在主视图
- 侧边栏职责聚焦：项目选择 + Tab 切换 + 系统状态
- 以 Tab 机制提供可扩展的视角切换（Agent / Story / 后续 Workflow 等）
- 在 Agent Tab 中让用户同时看到"我有哪些 Agent"和"它们在干什么"

---

## 信息架构设计

### 总体布局

```
┌──────────────┬──────────────────────────────────────────┐
│  侧边栏       │  主内容区                                 │
│  (纯导航)    │  (根据 Tab 切换)                          │
└──────────────┴──────────────────────────────────────────┘
```

### 侧边栏（重构）

职责精简为三件事：

```
┌──────────────┐
│ [品牌/Logo]  │
├──────────────┤
│ 项目选择器   │  ← 下拉选择或列表，切换后主内容区刷新
├──────────────┤
│ Tab 导航:    │
│ 🤖 Agent    │  ← 默认选中
│ 📋 Story    │
│ (⚙ Workflow)│  ← 后续扩展，暂不实现
├──────────────┤
│  [底部]      │
│ 后端连接状态 │  ← 保留，折叠展示在线本机列表
│ 设置按钮     │
└──────────────┘
```

**改动说明**：
- 移除原有的"看板" / "会话"两个导航项
- Tab 切换控制主内容区视图，不再控制侧边栏的二级内容
- 侧边栏不再包含 Session 历史列表（迁移到 Agent Tab 主内容区）

---

### Agent Tab（主内容区，双栏布局）

Agent Tab 是新的主视图，分左右两栏：

```
┌─────────────────────┬──────────────────────────────────┐
│  左栏：Agent 定义   │  右栏：活跃 Session 列表          │
│  (ProjectAgent 列表)│  (时间线排序)                     │
│                     │                                  │
│ ┌─────────────────┐ │  ┌──────────────────────────┐   │
│ │ PiAgent 规划者  │ │  │ ● running                │   │
│ │ executor: pi    │ │  │ Task: 实现登录API          │   │
│ │ writeback: 只读 │ │  │ Story: 用户认证            │   │
│ │ [打开] [新对话] │ │  │ Agent: PiAgent 规划者      │   │
│ └─────────────────┘ │  │ 开始: 3分钟前              │   │
│                     │  └──────────────────────────┘   │
│ ┌─────────────────┐ │                                  │
│ │ Claude Code 开发│ │  ┌──────────────────────────┐   │
│ │ executor: cc    │ │  │ ○ idle                   │   │
│ │ writeback: 确认 │ │  │ Story: 前端重构            │   │
│ │ [打开] [新对话] │ │  │ Agent: Claude Code 开发    │   │
│ └─────────────────┘ │  │ 最后活跃: 1小时前          │   │
│                     │  └──────────────────────────┘   │
│ + 新建 Agent 预设   │                                  │
│                     │  ┌──────────────────────────┐   │
│                     │  │ ● running                │   │
│                     │  │ (无 Story/Task 归属)       │   │
│                     │  │ Agent: PiAgent 规划者      │   │
│                     │  │ └ Companion: 代码审查 ●    │   │
│                     │  │   └ 由父会话派发           │   │
│                     │  └──────────────────────────┘   │
└─────────────────────┴──────────────────────────────────┘
```

#### 左栏：Agent 定义列表

展示当前项目下所有 ProjectAgent（来源：`GET /api/projects/{id}/agents`，返回 `ProjectAgentSummary[]`）

**每个 Agent 卡片展示**：
- 活跃状态指示点（绿色 = 5分钟内有活动 / 琥珀色 = 1小时内 / 灰色 = 空闲）
- 显示名（`display_name`）+ 描述（`description`）
- Executor 标签（`executor.executor`）+ variant（如有）
- 写回模式 badge（`writeback_mode`：只读 / 确认后写回）
- 最近活跃的 session 摘要（`session?.session_title`，如有）
- 操作按钮：**打开会话**（续接或新建）/ **强制新对话**
- 预设 Agent 有编辑/删除菜单

**底部**：`+ 新建 Agent 预设` 虚线创建入口

**交互**：点击"打开会话" → 调用 `openProjectAgentSession` API → 右栏焦点切换到对应 Session（如果 session 已在右栏列表中），或在右栏展开新 Session 视图

#### 右栏：活跃 Session 列表

展示当前项目下所有活跃 Session（来源：`GET /api/sessions?owner_type=project` 或后续新增的项目级 session 聚合 API）

**排序**：按 `last_activity` 时间倒序（最近活跃的在最上方）

**每条 Session 卡片展示**：
- 运行状态指示：`● running` / `○ idle` / `✓ completed` / `✗ failed`（来自 `SessionExecutionStatus`）
- **归属层级**（来自 `SessionBindingOwner`）：
  - Task 级：显示 Task 名 + 所属 Story 名
  - Story 级：显示 Story 名
  - Project 级（无归属）：显示"直接对话"
- 使用的 Agent 名（来自 session 绑定的 agent 信息）
- 时间信息：running 状态显示"开始于 X 分钟前"，idle 显示"最后活跃于 X"
- **Companion 嵌套**（如有子会话）：缩进展示子 Session 的状态和类型

**点击行为**：
- 点击 Session 卡片 → 右栏切换为完整 `SessionChatView` 展示
- 右栏顶部显示面包屑：`当前项目 / Agent Tab / Session 标题` + 返回按钮回到列表
- 提供"全屏"按钮 → 导航到独立的 `/session/:id` 路由

**状态**：空状态时显示"当前项目暂无活跃 Session"引导文案

---

### Story Tab（主内容区，基本保留）

保留现有 Story 看板视图（`StoryListView` → `StoryBoard`），以下调整：

- 入口从侧边栏"看板"按钮改为主内容区顶部 Tab
- Task 抽屉内的 `TaskAgentSessionPanel` 点击"全屏"时，导航到 `/session/:id`（现有逻辑保留）
- StoryPage（`/story/:storyId`）路由保持不变

---

## 技术实现要点

### 路由结构

采用 **Layout Route + Nested Routes** 方案，所有页面保留在 `WorkspaceLayout` 外壳内（侧边栏始终可见）。

```
现有：
  / → DashboardPage（Story 看板 + Agent Tab 内部切换）
  /session → SessionPage（空 session 列表）
  /session/:id → SessionPage
  /settings → SettingsPage

目标：
  / → redirect to /dashboard/agent
  /dashboard/agent → AgentTabView        ← 新主页
  /dashboard/story → StoryTabView        ← 原看板
  /story/:storyId → StoryPage            ← 不变
  /session/:id → SessionPage             ← 不变，保留侧边栏（选项B）
  /settings → SettingsPage               ← 不变
```

**路由树（React Router v6）：**

```tsx
<Route element={<WorkspaceLayout />}>
  <Route index element={<Navigate to="/dashboard/agent" replace />} />
  <Route path="/dashboard" element={<DashboardPage />}>
    <Route index element={<Navigate to="agent" replace />} />
    <Route path="agent" element={<AgentTabView />} />
    <Route path="story" element={<StoryTabView />} />
  </Route>
  <Route path="/story/:storyId" element={<StoryPage />} />
  <Route path="/session/:sessionId" element={<SessionPage />} />
  <Route path="/settings" element={<SettingsPage />} />
</Route>
```

**`WorkspaceLayout` 改造要点：**
- 移除 `activeView` prop 和 `onChangeView` 回调，改为 `NavLink` 的 `isActive` 高亮
- 主内容区从 `{children}` 换成 `<Outlet />`
- Tab 高亮根据 `useMatch("/dashboard/agent")` 等自动判断
- `/session/:id` 在 Layout 内渲染，侧边栏 Tab 保持 Agent 高亮，主内容区全宽展示 SessionChatView

### 新增组件

| 组件 | 说明 |
|------|------|
| `AgentTabView` | Agent Tab 主容器，管理左右栏状态 |
| `AgentDefinitionList` | 左栏 Agent 定义列表，复用 `ProjectAgentView` 现有卡片逻辑 |
| `ActiveSessionList` | 右栏 Session 时间线列表 |
| `ActiveSessionCard` | 单条 Session 卡片（状态 + 归属 + Agent + 时间 + Companion 嵌套） |
| `SessionInlineView` | 右栏展开的 SessionChatView 包装（含面包屑） |

### 数据层

**现有可复用**：
- `useProjectStore`：`agents: ProjectAgentSummary[]`
- `openProjectAgentSession` service 方法
- `SessionChatView` 组件（直接复用）

**需要新增**：
- 新 store：`useActiveSessionsStore`，管理当前项目的活跃 session 列表、SSE 实时更新
- 新 service：`GET /api/projects/{id}/sessions`（需后端新增，见下方后端需求）

**SSE 实时更新**：
- 监听全局 SSE 事件流中的 `session_status_changed` 等事件
- 更新 `ActiveSessionList` 中对应卡片的执行状态（running / idle / completed / failed）

### 侧边栏重构

**现有 `WorkspaceLayout` 改动**：
- 移除"看板"/"会话"两个导航按钮及 `WorkspaceView` 类型
- 移除 session 历史列表条件渲染块
- 移除 `activeView` prop 和 `onChangeView` 回调
- 新增 Tab 导航区（`NavLink` 方式）：Agent / Story
- 主内容区 `{children}` 换成 `<Outlet />`
- 保留后端连接面板、设置按钮、主题切换

---

## 后端需求

### 新增端点：项目级 Session 聚合列表

**端点**：`GET /api/projects/{project_id}/sessions`

**作用**：返回当前项目下所有层级（project / story / task）的 session，聚合成单一列表，避免前端多次查询拼接。

**响应结构**（新类型 `ProjectSessionEntry`）：

```typescript
interface ProjectSessionEntry {
  // Session 基础信息
  session_id: string;
  session_title: string | null;
  last_activity: number | null;         // unix timestamp ms

  // 执行状态（直接内联，无需二次查询）
  execution_status: SessionExecutionStatus;  // "idle" | "running" | "completed" | "failed" | "interrupted"

  // 归属层级
  owner_type: "project" | "story" | "task";
  owner_id: string;
  owner_title: string | null;           // task 名 或 story 名
  story_id: string | null;             // 当 owner_type = task 时有值
  story_title: string | null;          // 直接内联 story 名，无需前端反查

  // 使用的 Agent 信息（直接内联）
  agent_key: string | null;            // ProjectAgent 的 key
  agent_display_name: string | null;   // 直接内联显示名，无需前端反查

  // Companion 关系（一期展示占位，后续完善）
  parent_session_id: string | null;    // 非 null 表示这是 Companion 子会话
}
```

**查询参数**：
- `status`（可选）：过滤状态，如 `running,idle`，默认返回全部
- `limit`（可选）：默认 50

**排序**：按 `last_activity` 倒序

### Session 绑定补充 Agent 信息

当前 `SessionBindingOwner` 缺少 agent 信息，需在后端 session 绑定记录中补充 `agent_key` 字段，以便聚合端点可直接内联 `agent_display_name`。

---

## 数据字段对应关系

### ActiveSessionCard 字段来源

所有字段均来自新增的 `ProjectSessionEntry`，无需二次查询：

| 展示字段 | 来源字段 |
|---------|---------|
| 运行状态点 | `execution_status` |
| Task / Story 归属标签 | `owner_type` + `owner_title` + `story_title` |
| Agent 显示名 | `agent_display_name` |
| 时间信息 | `last_activity` |
| Companion 层级 | `parent_session_id`（非 null 则为子会话，一期仅展示占位） |

---

## 验收标准

### 前端
- [ ] 侧边栏仅包含：项目选择器 / Agent·Story Tab 切换（NavLink 高亮）/ 后端状态 / 设置按钮
- [ ] 默认路由 `/` 重定向到 `/dashboard/agent`
- [ ] Agent Tab：左栏正确展示当前项目的 ProjectAgent 列表，点击可打开/创建 Session
- [ ] Agent Tab：右栏展示当前项目的活跃 Session，按时间倒序，展示归属 Story/Task、使用的 Agent 名、执行状态
- [ ] 点击右栏 Session 卡片，右栏原地切换为 SessionChatView，顶部面包屑可返回列表
- [ ] SessionChatView 内"全屏"按钮导航到 `/session/:id`，侧边栏 Tab 保持高亮
- [ ] `/session/:id` 页面在 Layout 内渲染，侧边栏可见
- [ ] Story Tab（`/dashboard/story`）展示原有看板，功能不退化
- [ ] SSE 实时更新：Session 状态变化在右栏卡片实时反映（running ↔ idle ↔ completed）

### 后端
- [ ] `GET /api/projects/{project_id}/sessions` 端点可用，返回 `ProjectSessionEntry[]`
- [ ] 响应内联 `execution_status`、`agent_display_name`、`story_title`，无需前端二次查询
- [ ] Session 绑定记录补充 `agent_key` 字段

---

## 暂不包含（后续迭代）

- Workflow Tab 的实现
- Session 列表按 Story 分组视图
- Companion 嵌套展示（待后端 Companion 功能落地）
- 侧边栏宽度收窄优化
- Agent 配置面板的详细设计（左栏点击 Agent 展开配置，属于独立功能迭代）
