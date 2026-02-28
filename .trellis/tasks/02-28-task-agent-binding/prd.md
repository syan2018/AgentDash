# Task-Agent 绑定与执行流程

## 背景与目标

当前 AgentDash 的 Task 和 Agent 执行是分离的：
- Task 实体有 `AgentBinding` 结构，但仅用于展示，没有完整的绑定流程
- Session（ACP 会话）是独立的，不与 Task 关联
- 用户需要手动在 Session 页面输入 Prompt，无法直接从 Task 启动执行

本任务要建立 Task 与 Agent 进程的绑定关系，实现从 Task 直接启动执行的完整流程。

## 目标

1. 实现 Task 与 Session 的关联（一个 Task 对应一个 Session）
2. 在 Task 中预设 Agent 类型、提示词模板、上下文
3. 实现从 Task 详情页一键启动执行
4. 执行状态回写到 Task，产物收集到 Task.artifacts

## 非目标

- 不实现多 Agent 并行执行（P4-02）
- 不实现复杂的编排策略（P5 系列）
- 不替换现有的独立 Session 页面（保留自由会话能力）

## 当前状态分析

### 已有基础
- ✅ Task 实体有 `AgentBinding` 结构体（`agent_type`, `agent_pid`, `preset_name`）
- ✅ ExecutorHub 提供 Session 管理和 Prompt 执行能力
- ✅ 前端 Task Drawer 显示 agent_binding 信息
- ✅ 前端有完整的 ACP 会话渲染能力

### 需要补齐
- ❌ Task 与 Session 的关联字段
- ❌ 从 Task 启动执行的 API 链路
- ❌ 执行状态同步到 Task 状态
- ❌ Task 预设配置（提示词模板、上下文）

## 需求规格

### FR-1: Task-Session 关联

在 Task 实体中增加 `session_id` 字段，建立与 Session 的一对一关系：

```rust
pub struct Task {
    // ... existing fields ...
    /// 关联的 Session ID（执行时创建）
    pub session_id: Option<String>,
}
```

规则：
- Task 创建时 `session_id` 为 null
- 首次启动执行时创建 Session，写入 `session_id`
- 后续执行复用同一个 Session（保持上下文连续性）
- Task 删除时可选是否保留 Session 历史

### FR-2: Task 预设配置

扩展 `AgentBinding` 支持预设提示词和上下文：

```rust
pub struct AgentBinding {
    /// Agent 类型（如 "claude-code", "codex"）
    pub agent_type: Option<String>,
    /// Agent 进程标识
    pub agent_pid: Option<String>,
    /// 使用的预设名称
    pub preset_name: Option<String>,
    /// 预设提示词模板（用户可编辑）
    pub prompt_template: Option<String>,
    /// 初始上下文（注入到 Session 的第一条消息）
    pub initial_context: Option<String>,
}
```

前端 Task Drawer 增加配置面板：
- Agent 类型选择器（下拉框：claude-code / codex / gemini / custom）
- 提示词模板编辑器（textarea，支持占位符如 `{{task_title}}`）
- 初始上下文编辑器（textarea，可选）

### FR-3: 一键启动执行

前端 Task Drawer 增加"启动执行"按钮：

1. 首次启动：
   - 调用 `POST /tasks/{task_id}/start`
   - 后端创建 Session，绑定到 Task
   - 发送初始 Prompt（渲染后的模板 + 初始上下文）
   - 打开 Session 页面（嵌入或跳转）

2. 继续执行（已有 Session）：
   - 调用 `POST /tasks/{task_id}/continue`
   - 复用已有 Session
   - 打开 Session 页面

### FR-4: 执行状态同步

执行过程中，将关键状态同步到 Task：

| Session 事件 | Task 状态更新 |
|-------------|--------------|
| Agent 开始响应 | `Running` |
| Tool Call 开始 | 记录到 `artifacts`（类型 `ToolExecution`）|
| Tool Call 完成 | 更新对应 artifact |
| Session 完成 | `AwaitingVerification` |
| Session 错误 | `Failed` |

### FR-5: API 接口

新增后端 API：

```rust
// POST /tasks/{task_id}/start
// 启动 Task 执行（创建新 Session）
struct StartTaskRequest {
    /// 覆盖默认的提示词（可选）
    override_prompt: Option<String>,
    /// 执行器配置（可选，默认使用 Task 配置）
    executor_config: Option<ExecutorConfig>,
}

struct StartTaskResponse {
    task_id: Uuid,
    session_id: String,
    status: TaskStatus,
}

// POST /tasks/{task_id}/continue
// 继续已有 Session 的执行
struct ContinueTaskRequest {
    /// 追加提示词
    additional_prompt: Option<String>,
}

// GET /tasks/{task_id}/session
// 获取 Task 关联的 Session 状态
struct TaskSessionResponse {
    session_id: Option<String>,
    status: TaskStatus,
    last_activity: Option<DateTime<Utc>>,
}
```

## 技术方案

### 数据流

```
┌─────────────┐     POST /tasks/{id}/start      ┌─────────────┐
│   前端页面   │ ───────────────────────────────>│   API 层    │
│  Task Drawer │                                │             │
└─────────────┘                                └──────┬──────┘
                                                    │
                                                    ▼
                                           ┌─────────────────┐
                                           │  1. 创建 Session │
                                           │  2. 更新 Task.   │
                                           │     session_id   │
                                           │  3. 发送初始 Prompt│
                                           └────────┬────────┘
                                                    │
                                                    ▼
                                           ┌─────────────────┐
                                           │   ExecutorHub   │
                                           │  (现有能力复用)  │
                                           └─────────────────┘
```

### 状态机

```
Task 生命周期（执行相关部分）：

Pending ──start──> Running ──完成──> AwaitingVerification
    │                    │
    └──────失败──────────┴──────> Failed
```

### 数据库变更

```sql
-- 添加 session_id 列
ALTER TABLE tasks ADD COLUMN session_id TEXT;

-- 可选：添加索引
CREATE INDEX idx_tasks_session_id ON tasks(session_id);
```

## 验收标准

- [ ] 在 Task Drawer 可配置 Agent 类型和提示词模板
- [ ] 点击"启动执行"创建 Session 并发送初始 Prompt
- [ ] Session 页面显示 Task 关联的上下文
- [ ] 执行完成后 Task 状态变为 `AwaitingVerification`
- [ ] 可多次继续同一个 Task 的 Session
- [ ] 执行产物（Tool Call 记录）保存到 Task.artifacts

## 依赖与风险

### 依赖
- 需要 ExecutorHub 的 Session 管理能力（已具备）
- 需要 ACP 会话流稳定（已完成重构）

### 风险
- R1: Session 与 Task 生命周期不一致可能导致状态错乱
  - 缓解：明确 Session 由 Task 创建，但独立管理生命周期
- R2: 提示词模板渲染需要安全的占位符替换机制
  - 缓解：使用简单的字符串替换，避免复杂模板引擎

## 参考文档

- `docs/modules/02-state.md` - State 模块设计
- `docs/modules/05-execution.md` - Execution 模块设计
- `docs/system-flow.md` - Task 生命周期流程
