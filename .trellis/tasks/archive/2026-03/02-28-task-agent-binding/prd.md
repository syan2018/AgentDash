# Task-Agent 绑定与执行流程（PRD v2）

## 1. Scope / Trigger

本任务触发了典型跨层契约变更：
- 新增 Task 执行入口 API（`/tasks/{id}/start`、`/tasks/{id}/continue`、`/tasks/{id}/session`）
- Task 数据模型新增字段（`session_id`，`AgentBinding` 扩展）
- Task 执行状态与 Session 事件打通（状态机与 artifact 写回）
- 前后端 TaskStatus 契约统一（补齐 `awaiting_verification`）

目标是把当前“Task 管理”和“Session 会话”两条独立链路，收敛为“Task 驱动执行，Session 承载上下文”的闭环。

## 2. 当前约束（基于现有实现）

1. 后端 `ExecutorHub` 是多轮模型：同一 `session_id` 可反复 `start_prompt`，并非单次会话即终态。
2. 当前只有独立 `/sessions` API，Task 无法一键发起执行。
3. Task 有 `agent_binding` 与 `artifacts`，但无 `session_id`。
4. 全局事件流依赖 `state_changes`，Task 变更写入尚不完整。
5. 前端 TaskStatus 仍是 `queued/succeeded/cancelled` 语义，和后端 `assigned/awaiting_verification` 不一致。

## 3. Goals / Non-Goals

### Goals
- Task 与 Session 建立 1:1 持久绑定（Task 首次执行创建 Session，后续复用）。
- 支持在 Task 配置中定义 `prompt_template` 与 `initial_context`。
- Task Drawer 一键启动与继续执行，自动跳转到对应 Session。
- 执行事件回写 Task 状态，并将 Tool Call 结构化写入 Task artifacts。

### Non-Goals
- 不做多 Agent 并行编排（P4-02）。
- 不重构现有独立 Session 页（仍保留自由会话）。
- 不在本任务引入复杂策略引擎（P5 系列）。

## 4. ADR-lite（核心决策）

### 决策 A：Task 绑定 Session，但状态按“Turn”推进

`Session` 是长期上下文容器；`start/continue` 每次触发的是一个执行轮次（turn）。  
Task 状态推进基于“本轮执行结果”，不是“整个 session 是否结束”。

### 决策 B：新增 Task 执行 API 作为会话 API 的领域包装层

`/tasks/{id}/start|continue` 内部复用现有 `/sessions` 能力，不替代它；  
前者面向业务域（Task），后者面向基础能力（会话）。

### 决策 C：Tool Call 以 `ToolExecution` artifact 持久化

新增 artifact 类型 `ToolExecution`，以 `tool_call_id` 做幂等更新键；  
同一工具调用先“创建 artifact（in_progress）”，后“补全结果（completed/failed）”。

### 决策 D：统一 TaskStatus 语义

前后端统一为：
`pending | assigned | running | awaiting_verification | completed | failed`。  
本任务不再使用 `queued/succeeded/cancelled/skipped` 作为 Task 主状态。

## 5. Signatures（后端/API/DB/前端）

### 5.1 Domain 模型

```rust
pub struct Task {
    pub id: Uuid,
    pub story_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub session_id: Option<String>, // NEW
    pub agent_binding: AgentBinding,
    pub artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct AgentBinding {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>, // NEW
    pub initial_context: Option<String>, // NEW
}
```

```rust
pub enum ArtifactType {
    CodeChange,
    TestResult,
    LogOutput,
    File,
    ToolExecution, // NEW
}
```

### 5.2 API 签名

```rust
// POST /tasks/{task_id}/start
struct StartTaskRequest {
    override_prompt: Option<String>,
    executor_config: Option<ExecutorConfig>,
}
struct StartTaskResponse {
    task_id: Uuid,
    session_id: String,
    turn_id: String,
    status: TaskStatus,
}

// POST /tasks/{task_id}/continue
struct ContinueTaskRequest {
    additional_prompt: Option<String>,
    executor_config: Option<ExecutorConfig>,
}
struct ContinueTaskResponse {
    task_id: Uuid,
    session_id: String,
    turn_id: String,
    status: TaskStatus,
}

// GET /tasks/{task_id}/session
struct TaskSessionResponse {
    task_id: Uuid,
    session_id: Option<String>,
    session_title: Option<String>,
    task_status: TaskStatus,
    last_activity: Option<DateTime<Utc>>,
}
```

### 5.3 数据库变更

> 本项目为预研、未上线，直接演进到正确模型。

```sql
ALTER TABLE tasks ADD COLUMN session_id TEXT;
CREATE INDEX idx_tasks_session_id ON tasks(session_id);
```

`agent_binding` 与 `artifacts` 为 JSON 字段，随结构体序列化扩展，无需新增列。

### 5.4 前端类型签名

```ts
type TaskStatus =
  | "pending"
  | "assigned"
  | "running"
  | "awaiting_verification"
  | "completed"
  | "failed";

interface AgentBinding {
  agent_type?: string | null;
  agent_pid?: string | null;
  preset_name?: string | null;
  prompt_template?: string | null;
  initial_context?: string | null;
}

interface Task {
  // ...
  session_id?: string | null;
}
```

## 6. Contracts（请求/响应/状态/事件）

### 6.1 Prompt 生成契约

优先级：
1. `override_prompt`（start 请求显式覆盖）
2. `additional_prompt`（continue 追加）
3. `agent_binding.prompt_template` 渲染结果
4. 默认兜底模板

模板变量（首版）：
- `{{task_title}}`
- `{{task_description}}`
- `{{story_title}}`
- `{{workspace_path}}`

最终发送给 Session 的 prompt：
`[initial_context]\n\n[rendered_template_or_override]`

### 6.2 状态同步契约（按 turn）

| 触发点 | Task 状态 | 备注 |
|---|---|---|
| `/tasks/{id}/start` 或 `/continue` 成功受理 | `running` | 表示该 task 当前轮次开始执行 |
| 收到本轮终态（成功） | `awaiting_verification` | 不要求 session 结束 |
| 收到本轮终态（错误/异常） | `failed` | 记录错误摘要到 artifact |

### 6.3 Artifact（ToolExecution）契约

```json
{
  "id": "uuid",
  "artifact_type": "tool_execution",
  "content": {
    "session_id": "sess-xxx",
    "turn_id": "t123",
    "tool_call_id": "tool-abc",
    "title": "Read file",
    "status": "in_progress",
    "input_preview": "...",
    "output_preview": "...",
    "started_at": "2026-03-02T12:00:00Z",
    "updated_at": "2026-03-02T12:00:01Z"
  },
  "created_at": "2026-03-02T12:00:00Z"
}
```

幂等键：`(task_id, turn_id, tool_call_id)`。

### 6.4 StateChange 契约

Task 相关变更必须写入 `state_changes`，`payload.reason` 必填：
- `task_status_changed`
- `task_artifact_added`
- `task_updated`（包含 `session_id` 绑定）

## 7. Validation & Error Matrix

| 场景 | 接口 | 错误码 | 错误信息 |
|---|---|---|---|
| task 不存在 | start/continue/session | 404 | `Task {id} 不存在` |
| Task 已有运行中 turn | start/continue | 409 | `该任务已有执行进行中` |
| continue 时尚未绑定 session | continue | 422 | `Task 尚未启动，请先 start` |
| prompt 解析为空 | start/continue | 422 | `生成的 prompt 为空` |
| workspace 不存在或越权 | start/continue | 409 | `Workspace 与 Task 不匹配` |
| executor 配置无效 | start/continue | 400 | `执行器配置无效` |
| connector 运行异常 | start/continue | 500 | `执行启动失败` |

## 8. Good / Base / Bad Cases

### Good
- Task 首次点击“启动执行”：
  - 创建 session
  - 回写 `task.session_id`
  - 置 `running`
  - 跳转 `/session/{session_id}`
  - 轮次完成后置 `awaiting_verification`

### Base
- Task 已绑定 session，点击“继续执行”：
  - 复用同一 session
  - 新增一轮 turn
  - 状态按轮次推进

### Bad
- Task 未 start 直接 continue：
  - 返回 422
  - 不创建 session
  - 不改变 task 状态

## 9. Tests Required（含断言点）

### 后端
1. `start_task_creates_session_and_binds_task`
   - 断言 `task.session_id` 已写入
   - 断言返回 `status=running`
2. `continue_task_reuses_existing_session`
   - 断言 session_id 不变
3. `task_status_transitions_follow_turn_result`
   - 成功轮次 -> `awaiting_verification`
   - 失败轮次 -> `failed`
4. `tool_call_persisted_as_tool_execution_artifact`
   - 断言同 `tool_call_id` 为更新而非重复插入
5. `task_changes_are_written_to_state_changes`
   - 断言存在 `task_status_changed` / `task_artifact_added`

### 前端
1. Task Drawer 显示并可编辑：
   - `agent_type` / `prompt_template` / `initial_context`
2. 点击“启动执行”：
   - 调用 `/tasks/{id}/start`
   - 成功后跳转 `/session/{session_id}`
3. Task 状态渲染：
   - 正确显示 `awaiting_verification`

## 10. Wrong vs Correct

### Wrong
- 以“session 是否结束”驱动 Task 终态；
- 前端继续使用 `queued/succeeded`，后端写 `awaiting_verification`；
- Tool Call 只在 UI 展示，不写 Task artifacts。

### Correct
- 以“turn 终态”驱动 Task 状态；
- 前后端共享同一组 TaskStatus；
- Tool Call 结构化持久化到 `artifacts`，并可回放。

## 11. 验收标准（最终）

- [ ] Task 模型包含 `session_id`，并在首次 start 后持久化。
- [ ] `AgentBinding` 支持 `prompt_template`、`initial_context`。
- [ ] Task Drawer 可配置并保存上述字段。
- [ ] `/tasks/{id}/start` 可创建并绑定 Session，自动启动第一轮执行。
- [ ] `/tasks/{id}/continue` 复用已绑定 Session 启动新轮次。
- [ ] Task 状态可按轮次更新为 `running -> awaiting_verification/failed`。
- [ ] Tool Call 可写入 `ToolExecution` artifacts 并随状态更新。
- [ ] Task 相关状态变化可进入 `state_changes` 事件流。

## 12. 实施拆分（建议）

1. 后端模型与仓储层：`Task.session_id` + `AgentBinding` 扩展 + artifact 类型扩展。
2. Task 执行 API：`start/continue/session` 路由与 service。
3. 执行事件桥接：turn 终态回写 + Tool Call artifact 写回 + state_changes 记录。
4. 前端契约对齐：TaskStatus 统一、Drawer 表单与启动按钮、跳转 Session。
5. 回归测试：后端集成测试 + 前端关键交互测试。
