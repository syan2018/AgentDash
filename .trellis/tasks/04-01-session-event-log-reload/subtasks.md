# Session 会话事件数据库化与可靠重放 — 可执行子任务

## 1. `session-schema-and-repositories`

### 目标

- 新增 `sessions` / `session_events` 持久化结构

### 涉及文件

- `crates/agentdash-domain/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`
- `crates/agentdash-api/src/app_state.rs`

---

## 2. `session-event-appender`

### 目标

- 提供统一的 session 事件事务写入口

### 涉及文件

- `crates/agentdash-application/src/session/`

---

## 3. `session-projections-turns-and-toolcalls`

### 目标

- 引入 turn / tool call 投影，并替换状态推断逻辑

### 涉及文件

- `crates/agentdash-application/src/session/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

---

## 4. `session-history-query-api`

### 目标

- 提供 session 历史分页查询接口

### 涉及文件

- `crates/agentdash-api/src/routes/acp_sessions.rs`

---

## 5. `session-stream-protocol-rework`

### 目标

- 让 SSE / NDJSON 都改成基于 `event_seq` 的补发与续传

### 涉及文件

- `crates/agentdash-api/src/routes/acp_sessions.rs`

---

## 6. `frontend-session-timeline-store`

### 目标

- 把前端改成 history hydrate + live delta

### 涉及文件

- `frontend/src/features/acp-session/model/useAcpStream.ts`
- `frontend/src/features/acp-session/model/useAcpSession.ts`
- `frontend/src/features/acp-session/model/streamTransport.ts`

---

## 7. `frontend-session-side-effect-dedupe`

### 目标

- `SessionChatView` 及其父级副作用改为按稳定 `event_seq` 去重

### 涉及文件

- `frontend/src/features/acp-session/ui/SessionChatView.tsx`
- `frontend/src/pages/SessionPage.tsx`

---

## 8. `replay-regression-tests`

### 目标

- 建立 reconnect / replay / duplicate render 回归测试

### 涉及文件

- `crates/agentdash-api/`
- `crates/agentdash-application/`
- `frontend/`
- `tests/e2e/`
