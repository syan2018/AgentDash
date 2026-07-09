# Design: Session 业务耦合收束

## 架构原则

```
┌─────────────────────────────────────────┐
│           AgentRun Scope                │
│  (run_id + agent_id) — 业务生命周期    │
│                                         │
│  ┌─────────┐ ┌──────────┐ ┌─────────┐  │
│  │Terminal  │ │ Title    │ │ Context │  │
│  │Registry │ │ Store    │ │ Audit   │  │
│  └─────────┘ └──────────┘ └─────────┘  │
│                                         │
│  ┌─────────────────────────────────┐    │
│  │    Runtime Session (internal)   │    │
│  │    - model connection           │    │
│  │    - event journal              │    │
│  │    - NDJSON transport           │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

Session 只负责：event 持久化、model 连接、stream 传输。
业务数据（terminal、title、audit）归 AgentRun scope 所有。

---

## Phase 1: Terminal 收束

### 后端

#### 1.1 `AgentRunTerminalRegistry` (替代 `SessionTerminalCache`)

```rust
// crates/agentdash-application-agentrun/src/agent_run/terminal_registry.rs

pub struct AgentRunTerminalRegistry {
    /// (run_id, agent_id) → { terminal_id → TerminalState }
    inner: RwLock<HashMap<AgentRunKey, HashMap<String, TerminalState>>>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct AgentRunKey {
    pub run_id: String,
    pub agent_id: String,
}

pub struct TerminalState {
    pub terminal_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub backend_id: String,
    pub state: String,
    pub exit_code: Option<i32>,
    pub process_id: Option<u32>,
    pub cwd: Option<String>,
    pub created_at: i64,
    pub exited_at: Option<i64>,
}
```

- `register_terminal(run_id, agent_id, terminal_id, backend_id, ...)`
- `list_terminals(run_id, agent_id) → Vec<TerminalState>`
- `get_terminal(terminal_id) → Option<TerminalState>` (全局 terminal_id 唯一，支持反查)
- `update_state(terminal_id, state, exit_code)`
- `handle_backend_disconnect(backend_id) → Vec<String>`

#### 1.2 `ws_handler.rs` 事件路由改造

当前：
```rust
// EventTerminalOutput 到达时
let term_state = terminal_cache.get_terminal(terminal_id);
inject_notification(&term_state.session_id, envelope);
```

改为：
```rust
let term_state = terminal_registry.get_terminal(terminal_id);
let session_id = resolve_active_session(term_state.run_id, term_state.agent_id);
inject_notification(&session_id, envelope);
```

注：`terminal_output` 事件仍需写入 session journal（这是 session 的职责），但路由查找不再从 terminal 的 session_id 字段来，而是通过 AgentRun → active delivery session 动态解析。

#### 1.3 终端输出回查 API

```
GET /agent-runs/:run_id/agents/:agent_id/runtime/terminals/:terminal_id/output
    ?after_seq=0&limit=100

Response: {
  terminal_id: string,
  output: string,          // 拼接所有 terminal_output 事件的 data
  total_bytes: number,
  truncated: boolean,
}
```

实现：从 session journal 过滤 `PlatformEvent::TerminalOutput { terminal_id }` 事件，拼接 data 字段返回。

### 前端

#### 1.4 `useTerminalStore` 扁平化

```typescript
interface TerminalStoreState {
  // 扁平索引：terminal_id 全局唯一
  terminals: Map<string, TerminalInfo>;
  outputBuffers: Map<string, string>;
  outputBufferBaseOffsets: Map<string, number>;
  outputBufferRevisions: Map<string, number>;
  projectedEventKeys: Set<string>;

  registerTerminal: (info: TerminalInfo) => void;
  // ...其余 API 不变，只是移除 sessionId 参数
}
```

`TerminalInfo` 中 `sessionId` 字段移除，替换为可选的 `runId` + `agentId`。

#### 1.5 TerminalView 输出加载

```typescript
// terminal-tab.tsx
useEffect(() => {
  if (output || !agentRunTarget || activeId === "new") return;
  // store 为空时从 API 加载
  fetchTerminalOutput(agentRunTarget, activeId).then((data) => {
    if (data?.output) {
      useTerminalStore.getState().replaceOutput(activeId, data.output);
    }
  });
}, [activeId, output, agentRunTarget]);
```

#### 1.6 `sessionPlatformEventDispatcher` 简化

```typescript
// 不再传 session_id 给 projectOutputEvent
if (platform.kind === "terminal_output") {
  useTerminalStore.getState().projectOutputEvent(
    event.event_seq,           // 去重 key 只需 event_seq（journal 内唯一）
    platform.data.terminal_id,
    platform.data.data,
  );
  return true;
}
```

---

## Phase 2: Title 收束

### 后端

#### 2.1 Workspace Title 独立持久化

在 `AgentRunWorkspaceRecord`（或 workspace 持久层）新增字段：

```rust
pub struct AgentRunWorkspaceTitle {
    pub display_title: String,
    pub title_source: TitleSource,  // Auto | Source | User
    pub updated_at: i64,
}
```

#### 2.2 Title 写入路径

- Auto title: launch commit 时直接写 workspace title，不写 SessionMeta
- Source title: `SourceSessionTitleUpdated` 处理逻辑改为写 workspace title
- User title: `title_service.set_user_title()` 改为写 workspace title

#### 2.3 前端通知

当前 `session_meta_updated` 事件 → 改为 workspace-level 事件通知（或复用现有 workspace state refresh 轮询机制）。

### SessionMeta 清理

`SessionMeta` 中 `title` + `title_source` 字段标记废弃并最终移除。

---

## Phase 3: Context Audit 收束

#### 3.1 索引 key 替换

```rust
pub struct InMemoryContextAuditBus {
    // 改为 AgentRun scope
    inner: RwLock<HashMap<AgentRunKey, VecDeque<ContextAuditEvent>>>,
}
```

`emit()` 和 `query()` 接口参数从 `session_id` → `(run_id, agent_id)`。

---

## Phase 4: Wait Activity + 清理

#### 4.1 Wait Activity exec source

```rust
// 改为通过 AgentRun scope 查终端
fn collect_scope_exec_items(run_id, agent_id) {
    let terminals = terminal_registry.list_terminals(run_id, agent_id);
    // ...
}
```

#### 4.2 Hook Script Engine

```rust
// script_engine.rs context
// 移除: "session_id": ctx.query.runtime_session_id()
// 新增:
"run_id": ctx.agent_run_ref.run_id,
"agent_id": ctx.agent_run_ref.agent_id,
```

#### 4.3 清理项

- 删除 `SessionTerminalCache` 整个文件
- 删除 legacy canvas session endpoint
- Terminal control callback: 入口参数从 `session_id` 改为 `(run_id, agent_id)`
- `SessionMeta.title` / `SessionMeta.title_source` 字段移除

---

## 数据流变更对照

### Terminal Output (Before → After)

**Before:**
```
Backend Worker → EventTerminalOutput
  → ws_handler: terminal_cache.get_terminal(id) → term_state.session_id
  → inject_notification(session_id, PlatformEvent::TerminalOutput)
  → journal[session_id]
  → frontend NDJSON stream → dispatchSessionPlatformEvent
  → useTerminalStore.projectOutputEvent(sessionId, seq, terminalId, data)
```

**After:**
```
Backend Worker → EventTerminalOutput
  → ws_handler: terminal_registry.get_terminal(id) → (run_id, agent_id)
  → resolve_active_session(run_id, agent_id) → session_id
  → inject_notification(session_id, PlatformEvent::TerminalOutput)  [journal 仍 session-scoped]
  → frontend NDJSON stream → dispatchSessionPlatformEvent
  → useTerminalStore.projectOutputEvent(seq, terminalId, data)  [去掉 sessionId]
```

### Title (Before → After)

**Before:**
```
Title change → write SessionMeta.title → emit session_meta_updated
  → frontend session stream → trigger workspace refresh → read-through SessionMeta
```

**After:**
```
Title change → write AgentRunWorkspaceTitle → emit workspace event / trigger refresh
  → frontend workspace state → display_title
```

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| terminal_output 事件仍写入 session journal，session 重建后旧事件在旧 journal | 输出回查 API 从 AgentRun scope 聚合所有关联 session 的 terminal_output 事件 |
| 前端 store 扁平化后丢失 "属于哪个 AgentRun" 信息 | TerminalInfo 保留 runId/agentId 字段；outputBuffers 按 terminal_id 索引不受影响 |
| Title 迁移期间 workspace record 无历史 title | 一次性迁移脚本从 SessionMeta 回填 |
