# 模块：视图组织（View）

## 定位

提供Story的用户自定义组织方式，支持多种维度的视图呈现。

## 职责

- 支持用户自定义Story的分组和层级关系
- 提供多种视图模式（列表、树状、看板等）
- 视图不影响核心状态和执行流程
- 支持视图的保存和共享

## 核心概念

### 视图（View）
- 用户对Story集合的某种组织呈现
- 完全由用户定义，不影响底层状态
- 一个Story可属于多个视图

### 分组（Group）
- 视图中Story的组织单元
- 可按项目、模块、时间、优先级等维度分组
- 支持层级嵌套

### 筛选器（Filter）
- 根据条件筛选显示的Story
- 支持状态、时间、标签等多种条件
- 可保存为常用筛选

## 视图类型

```
列表视图（List）
- 平铺展示Story
- 支持排序和筛选
- 适合快速浏览

树状视图（Tree）
- 展示Story的分组层级
- 用户自定义父子关系
- 适合项目规划

看板视图（Kanban）
- 按状态分栏
- Story在栏间拖拽移动
- 适合跟踪进度

时间线视图（Timeline）
- 按时间轴展示
- 适合查看历史和发展
```

## 接口定义（概念层面）

```
ViewManager {
  createView(name, type): View
  addStoryToView(viewId, storyId, groupId?): void
  removeStoryFromView(viewId, storyId): void
  organizeStories(viewId, structure): void
  getView(viewId): View
  listViews(userId): View[]
}

View {
  id: string
  name: string
  type: "list" | "tree" | "kanban" | "timeline"
  owner: string
  structure: ViewStructure
  filters: Filter[]
}

Group {
  id: string
  name: string
  parentId: string | null
  storyIds: string[]
}
```

## 关键设计决策（待讨论）

- [ ] 视图的持久化策略
- [ ] 视图的共享机制（团队/个人）
- [ ] 默认视图的定义
- [ ] 视图与权限的关系

## 暂不定义

- 前端视图渲染实现
- 视图的实时同步机制
- 视图模板的预设
- 视图导入导出功能

---

*状态：概念定义阶段*  
*更新：2026-02-21*
