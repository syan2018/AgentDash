# Cross-Layer Thinking Guide

> **Purpose**: Think through data flow across layers before implementing.

---

## The Problem

**Most bugs happen at layer boundaries**, not within layers.

Common cross-layer bugs:
- API returns format A, frontend expects format B
- Database stores X, service transforms to Y, but loses data
- Multiple layers implement the same logic differently

<!-- PROJECT-SPECIFIC-START: AgentDashboard Context -->
> **AgentDashboard 特有挑战：**
> - Story 状态在后端维护，前端只是展示（不能自行推断状态）
> - Task 执行是异步的，状态变更通过推送到达前端
> - 多个模块共同影响 Story/Task 的最终状态
> - 策略可插拔意味着接口稳定但实现可变
<!-- PROJECT-SPECIFIC-END -->

---

## Before Implementing Cross-Layer Features

### Step 1: Map the Data Flow

Draw out how data moves:

```
Source → Transform → Store → Retrieve → Transform → Display
```

For each arrow, ask:
- What format is the data in?
- What could go wrong?
- Who is responsible for validation?

<!-- PROJECT-SPECIFIC-START: Data Flow Example -->
#### AgentDashboard 示例：Task执行完成→Story状态更新→前端刷新

```
Agent完成 → ExecutionManager.reportCompletion(taskId)
         → StateManager.updateTask(taskId, {status: 'completed'})
         → StateManager.recordChange(StateChange{...})
         → ValidationManager.validate(taskId, rules)
         → [验证通过] OrchestrationEngine.checkStoryProgress(storyId)
         → [所有Task完成] StateManager.updateStory(storyId, {status: 'validating'})
         → SSE推送 → 前端更新Story状态显示
```
<!-- PROJECT-SPECIFIC-END -->

### Step 2: Identify Boundaries

| Boundary | Common Issues |
|----------|---------------|
| API ↔ Service | Type mismatches, missing fields |
| Service ↔ Database | Format conversions, null handling |
| Backend ↔ Frontend | Serialization, date formats |
| Component ↔ Component | Props shape changes |

<!-- PROJECT-SPECIFIC-START: AgentDashboard Boundaries -->
#### AgentDashboard 特有的边界

| 边界 | 常见问题 |
|------|---------|
| Agent ↔ ExecutionManager | Agent输出格式不统一、执行超时 |
| ExecutionManager ↔ StateManager | 状态回写时机、失败状态处理 |
| Orchestration ↔ State | 编排策略不能直接修改状态（必须通过StateManager） |
| Injection ↔ Task | 注入内容过大、注入时机（执行前还是创建时） |
| Validation ↔ Orchestration | 验证失败后的重试vs暂停决策 |
| Backend ↔ Frontend | 实时状态推送协议、断线重连 |
| Connection ↔ State | 多后端数据隔离、会话失效 |
| Cloud ↔ Local Address Space | mount 语义不一致、绝对路径泄漏、context 与 runtime tool 分叉 |
<!-- PROJECT-SPECIFIC-END -->

### Step 3: Define Contracts

For each boundary:
- What is the exact input format?
- What is the exact output format?
- What errors can occur?

---

## Common Cross-Layer Mistakes

### Mistake 1: Implicit Format Assumptions

**Bad**: Assuming date format without checking

**Good**: Explicit format conversion at boundaries

### Mistake 2: Scattered Validation

**Bad**: Validating the same thing in multiple layers

**Good**: Validate once at the entry point

### Mistake 3: Leaky Abstractions

**Bad**: Component knows about database schema

**Good**: Each layer only knows its neighbors

<!-- PROJECT-SPECIFIC-START: AgentDashboard Mistakes -->
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
### 错误模式5：ACP 流数据层的 delta 重复发射

**问题：**
```
// 错误：per-index delta 追踪在 provider 创建新 entry index 时重发全文
fn text_delta(prev: Option<&str>, next: &str) -> Option<String> {
    // prev 为 None（新 index）→ 返回 full_content → 重复发射！
}
```

**正确：**
```
// 跟踪全局已发射文本，按 chunk 类型去重
fn emit_deduped(emitted: &mut String, full_content: &str, ...) {
    if full_content.starts_with(emitted.as_str()) {
        let suffix = &full_content[emitted.len()..];
        emitted.push_str(suffix);
        emit_chunk(suffix)  // 只发射真正的增量
    }
}
```

**为何重要：** Provider（如 vibe-kanban/kimi）可能为同一段 Agent 回复创建
多个不同 index 的 NormalizedEntry。仅靠 per-index delta 无法检测跨 index
重复。必须在 Converter 层维护全局已发射文本追踪。

**关联规则：**
- 前端的 `mergeStreamChunk` 应匹配 ABCCraft 标准实现
- `[系统]`/`[用量]`/`[状态]` 等元数据不应作为 `agent_message_chunk` 发射
- 前端需添加安全过滤以兼容旧 JSONL 历史数据

---

### 设计决策6：ACP 事件的 UI 展示策略

**问题：**
```
// 错误：所有 ACP 事件都在主对话流中逐条绘制
// session_info_update（系统消息）和 usage_update（用量）占据大量空间
// 用户不关心这些技术细节，干扰对话阅读体验
```

**正确：**
```
// 分层展示策略：
// 1. 主对话流：只绘制用户关心的内容
//    - user_message_chunk / agent_message_chunk / agent_thought_chunk
//    - tool_call / tool_call_update
//    - plan
// 2. Header 指示器：用量信息通过小圆环实时更新（hover 展示详情）
// 3. 静默保留：session_info_update 数据保留在 entries 中但不绘制
//    - 便于调试和日志追溯
//    - 未来可通过"显示系统消息"开关控制
```

**为何重要：** 行业惯例（Claude/ChatGPT）将上下文用量放在 header 小圆环中，
系统消息不干扰主对话流。数据层保留所有事件，UI 层按需展示。
<!-- PROJECT-SPECIFIC-END -->

---

## Checklist for Cross-Layer Features

Before implementation:
- [ ] Mapped the complete data flow
- [ ] Identified all layer boundaries
- [ ] Defined format at each boundary
- [ ] Decided where validation happens

After implementation:
- [ ] Tested with edge cases (null, empty, invalid)
- [ ] Verified error handling at each boundary
- [ ] Checked data survives round-trip

<!-- PROJECT-SPECIFIC-START: AgentDashboard Checklist -->
#### AgentDashboard 额外检查项

**实现前：**
- [ ] 确认不会绕过 StateManager 进行状态修改
- [ ] 确认视图操作不会影响 Story/Task 核心状态
- [ ] 若功能涉及云端/本机文件访问、上下文注入或多 workspace，先定义统一的 mount / provider / capability 边界，再决定协议与工具面
  参考：`backend/address-space-access.md`
- [ ] 若功能涉及 runtime hook / workflow phase / tool gate / stop gate，先确认“信息获取在 loop 外、控制决策在 loop 边界同步发生”的分层是否成立
  参考：`backend/execution-hook-runtime.md`

**实现后：**
- [ ] 验证 StateChange 历史完整记录
- [ ] 验证前端状态与后端状态一致
- [ ] 若引入了新的 runtime policy / diagnostics / metadata，验证前端看到的是“真实生效的 runtime surface”，而不是静态模板说明
<!-- PROJECT-SPECIFIC-END -->

---

## When to Create Flow Documentation

Create detailed flow docs when:
- Feature spans 3+ layers
- Multiple teams are involved
- Data format is complex
- Feature has caused bugs before

<!-- PROJECT-SPECIFIC-START: AgentDashboard Flow Docs -->
#### AgentDashboard 流程文档规则

以下情况需要在 `docs/` 中创建详细流程文档：
- 功能跨越 3个以上模块
- 涉及 Agent 与系统的交互协议
- 数据格式复杂（多层转换）
- 该功能之前出现过 Bug
<!-- PROJECT-SPECIFIC-END -->
