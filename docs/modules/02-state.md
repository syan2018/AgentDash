# 模块：状态管理（State Management）

## 核心思想

状态存储是**实现细节**，状态关系才是锚点。

我们不预设"必须用数据库"或"必须用什么格式"，因为：
- 单用户本地使用 vs 多用户远程服务需要不同存储方案
- 调试场景需要可读性，生产场景需要性能
- 技术栈可能随时间变化

系统只保证：**状态完整、可追溯、一致**，但"如何存储"可自由选择。

## 定位

系统的核心存储层，维护所有Story、Task及状态变更的完整历史。

## 职责

- 存储Story和Task的核心状态数据
- 维护状态变更的历史轨迹
- 提供状态查询和检索能力
- 保证状态的一致性和可追溯性

## 核心概念

### 状态容器（State Container）
- 工作空间内的状态存储抽象
- 包含Story、Task、变更历史等所有状态数据
- 不维护Story之间的关系（关系由视图层处理）

### 状态快照（State Snapshot）
- 某一时刻的完整状态记录
- 支持时间点回溯
- 用于恢复和审计

### 变更日志（Change Log）
- 所有状态变更的不可变记录
- 包含变更前状态、变更后状态、变更时间、变更原因

## 存储实体

```
Story {
  id: string
  title: string
  context: Context
  status: StoryStatus
  taskIds: string[]
  createdAt: timestamp
  updatedAt: timestamp
}

Task {
  id: string
  storyId: string
  context: Context
  status: TaskStatus
  agentBinding: AgentBinding | null
  artifacts: Artifact[]
  createdAt: timestamp
  updatedAt: timestamp
}

StateChange {
  id: string
  entityType: "story" | "task"
  entityId: string
  field: string
  from: any
  to: any
  reason: string
  timestamp: timestamp
}
```

## 接口定义（概念层面）

```
StateManager {
  createStory(story): Story
  updateStory(id, changes): Story
  getStory(id): Story
  listStories(query): Story[]
  
  createTask(task): Task
  updateTask(id, changes): Task
  getTask(id): Task
  listTasks(storyId): Task[]
  
  getHistory(entityId): StateChange[]
  getSnapshot(timestamp): Snapshot
}
```

## 关键设计决策（待讨论）

- [ ] 存储介质选择（文件/数据库/混合）
- [ ] 状态序列化格式
- [ ] 变更历史的保留策略
- [ ] 并发写入的处理机制

## 暂不定义

- 具体数据库选型
- 存储性能优化
- 数据压缩策略
- 备份与恢复机制

---

*状态：概念定义阶段*  
*更新：2026-02-21*
