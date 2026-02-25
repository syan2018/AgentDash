# 跨层思考指南

> **目的：** 在实现跨层功能前，系统性地思考数据流和模块边界。

---

## 核心问题

**大多数 Bug 发生在层边界处**，而不是在层内部。

AgentDashboard 的特有挑战：
- Story 状态在后端维护，前端只是展示（不能自行推断状态）
- Task 执行是异步的，状态变更通过推送到达前端
- 多个模块共同影响 Story/Task 的最终状态
- 策略可插拔意味着接口稳定但实现可变

---

## AgentDashboard 的层边界地图

```
前端 Client
    │
    │ HTTP API / WebSocket
    ▼
API 层（路由和处理器）
    │
    │ 服务调用
    ▼
编排层 Orchestration (模块04)
    │              │
    │              │
    ▼              ▼
注入层 (模块06)   验证层 (模块07)
    │              │
    └──────┬───────┘
           │
           ▼
执行层 Execution (模块05)
           │
           ▼
工作空间层 Workspace (模块03)
           │
      Agent执行（外部）
           │
           ▼（状态回写）
状态层 State (模块02)
    │
    │ 状态变更推送
    ▼
前端 Client（更新展示）
```

---

## 实现跨层功能前

### 第1步：追踪完整数据流

以"Task执行完成→Story状态更新→前端刷新"为例：

```
Agent完成 → ExecutionManager.reportCompletion(taskId)
         → StateManager.updateTask(taskId, {status: 'completed'})
         → StateManager.recordChange(StateChange{...})
         → ValidationManager.validate(taskId, rules)
         → [验证通过] OrchestrationEngine.checkStoryProgress(storyId)
         → [所有Task完成] StateManager.updateStory(storyId, {status: 'validating'})
         → WebSocket推送 → 前端更新Story状态显示
```

每个箭头处询问：
- 数据格式是什么？
- 什么可能出错？
- 谁负责验证？

### 第2步：识别 AgentDashboard 特有的边界

| 边界 | 常见问题 |
|------|---------|
| Agent ↔ ExecutionManager | Agent输出格式不统一、执行超时 |
| ExecutionManager ↔ StateManager | 状态回写时机、失败状态处理 |
| Orchestration ↔ State | 编排策略不能直接修改状态（必须通过StateManager） |
| Injection ↔ Task | 注入内容过大、注入时机（执行前还是创建时） |
| Validation ↔ Orchestration | 验证失败后的重试vs暂停决策 |
| Backend ↔ Frontend | 实时状态推送协议、断线重连 |
| Connection ↔ State | 多后端数据隔离、会话失效 |

### 第3步：定义接口契约

对每个边界：
- 输入格式是什么？
- 输出格式是什么？
- 可能发生哪些错误？
- 谁负责错误处理？

---

## AgentDashboard 特有的跨层错误模式

### 错误模式1：编排层绕过状态层

**问题：**
```
// 错误：编排层直接修改Task数据
orchestrationStrategy.execute() {
  task.status = 'running'  // 绕过StateManager！
}
```

**正确：**
```
// 编排层通过StateManager更新状态
orchestrationStrategy.execute() {
  stateManager.updateTask(taskId, {status: 'running'}, reason: '编排引擎启动执行')
}
```

**为何重要：** 绕过StateManager会导致状态历史缺失，无法追溯和审计。

---

### 错误模式2：前端自行推断状态

**问题：**
```javascript
// 错误：前端根据UI交互自行推断Task状态
const isCompleted = task.artifacts.length > 0  // 这只是猜测！
```

**正确：**
```javascript
// 以后端推送的status字段为准
const isCompleted = task.status === 'completed'
```

**为何重要：** Task状态由后端验证层决定，前端不掌握完整验证逻辑。

---

### 错误模式3：策略泄漏到接口

**问题：**
```
// 错误：Workspace接口暴露了策略细节
WorkspaceManager.createWorktree(gitRepo, branch)  // worktree是实现细节！
```

**正确：**
```
// 接口只表达意图，不暴露实现
WorkspaceManager.createWorkspace(config: IsolationConfig)
```

**为何重要：** 接口稳定是模块可替换的基础。

---

### 错误模式4：Story视图关系影响执行

**问题：**
```
// 错误：删除视图中的分组时意外影响了Story的执行状态
ViewManager.deleteGroup(groupId) {
  stories.forEach(s => s.status = 'cancelled')  // 视图操作不应影响状态！
}
```

**正确：**
```
// 视图操作只影响视图结构，不影响Story状态
ViewManager.deleteGroup(groupId) {
  view.removeGroup(groupId)  // 只修改视图配置
}
```

**为何重要：** 核心设计原则：Story间关系是视图层概念，不影响执行流程。

---

## 跨层功能实现检查清单

### 实现前

- [ ] 画出完整的数据流（从触发到最终结果）
- [ ] 识别所有经过的模块边界
- [ ] 确认每个边界的接口定义（见 `docs/modules/`）
- [ ] 决定每层的错误处理责任
- [ ] 确认不会绕过 StateManager 进行状态修改
- [ ] 确认视图操作不会影响 Story/Task 核心状态

### 实现后

- [ ] 测试边缘情况（null、空值、超时、网络断开）
- [ ] 验证错误在正确的层被捕获
- [ ] 验证 StateChange 历史完整记录
- [ ] 验证前端状态与后端状态一致

---

## 何时需要创建流程文档

以下情况需要在 `docs/` 中创建详细流程文档：
- 功能跨越 3个以上模块
- 涉及 Agent 与系统的交互协议
- 数据格式复杂（多层转换）
- 该功能之前出现过 Bug
