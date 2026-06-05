# [child-4] 技术设计：双模追加消息 — 排队 / Steer

## Phase B1: 后端 pending 队列

### BR1 pending 领域实体

在 session 维度维护一个进程内有序 pending 消息队列。

**数据结构** (`agentdash-application`):

```rust
pub struct PendingMessage {
    pub id: String,           // UUID
    pub input: Vec<UserInputBlock>,
    pub executor_config: Option<AgentConfig>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct PendingMessageQueue {
    runtime_session_id: String,
    messages: Vec<PendingMessage>,
}
```

**存储**: 进程内 `DashMap<String, PendingMessageQueue>` 由 `PendingQueueService` 管理。
不做持久化（重启清空可接受，由前端事件投影重现状态即可）。

### BR2 服务端自动派发

监听 turn_completed 事件：
- turn_completed → 取队首 → 走 message 路径派发（携带 executor_config）
- turn_failed / turn_interrupted → 保留队列不自动派发，前端展示等待用户决策

状态机：
```
Queue + turn_completed → Dequeue(0) → LifecycleAgentMessageCommand → Running
Queue + turn_failed    → 保留（标记 paused_at_failure）
Queue + turn_interrupted → 保留（标记 paused_at_interruption）
```

### BR3 命令 API

新增 API routes（在 `sessions.rs` 下）：

| Method | Path | 动作 |
|--------|------|------|
| POST | `/sessions/{id}/pending-messages` | enqueue — 排队 |
| GET | `/sessions/{id}/pending-messages` | list — 列出 |
| DELETE | `/sessions/{id}/pending-messages/{msg_id}` | delete — 删除 |
| POST | `/sessions/{id}/pending-messages/{msg_id}/promote` | promote-to-steer — 立即引导 |

### BR4 事件投影

pending 队列变更通过 `SessionRuntimeControlView.pending_messages` 字段暴露：
- 前端轮询 runtime-control 时获取当前 pending 列表
- 新增字段 `pending_messages: Vec<PendingMessageView>` 到 `SessionRuntimeControlView`

```rust
pub struct PendingMessageView {
    pub id: String,
    pub preview: String,      // 首段文本截断
    pub has_images: bool,
    pub created_at: String,   // ISO8601
}
```

### BR5 action 模型扩展

在 running 态下：
- `send_next` 改为 `enqueue`（排队），始终可用（不受 delivery_running 互斥）
- `steer` 保持原有逻辑
- 新增 `enqueue` action 到 `SessionRuntimeActionSetView`

```rust
pub struct SessionRuntimeActionSetView {
    pub send_next: SessionRuntimeActionAvailabilityView,
    pub enqueue: SessionRuntimeActionAvailabilityView,  // 新增
    pub steer: SessionRuntimeActionAvailabilityView,
    pub cancel: SessionRuntimeActionAvailabilityView,
}
```

## Phase B2: 前端投影

在 B1 后端 API 就绪后实现。详见 plan 中的 FR1-FR5。
