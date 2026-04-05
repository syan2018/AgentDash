# Session 会话事件数据库化与可靠重放 — 执行计划

## 总体策略

这项工作不要在现有 JSONL replay 链路上继续打补丁，而是分四层重建：

1. **数据库事实层**：`sessions` + `session_events`
2. **数据库投影层**：`session_turns` + `session_tool_calls`
3. **服务端读取层**：历史查询接口 + 增量流接口
4. **前端消费层**：history hydrate + live delta + 幂等 reducer

整体推进原则：

- 先把“事件如何正确记录”做对
- 再把“事件如何稳定重放”做对
- 最后替换 UI 侧的消费与副作用逻辑

---

## Phase 1：重建后端事件事实层

### 目标

- 用 SQLite 持久化 session 元数据与事件日志
- 把 `event_seq` 确立为会话内稳定顺序

### 主要改动

- 新增 `session_repository`
- 新增 `session_event_repository`
- 为 `sessions` 表补齐：
  - `last_event_seq`
  - `last_execution_status`
  - `last_turn_id`
  - `last_terminal_message`
  - executor / companion 元信息
- 新增 `session_events` 表：
  - `session_id`
  - `event_seq`
  - `occurred_at_ms`
  - `committed_at_ms`
  - `session_update_type`
  - `turn_id`
  - `entry_index`
  - `tool_call_id`
  - `notification_json`

### 关键产出

- 稳定的 `append_event(session_id, notification)` 事务写入口
- `PersistedSessionEvent` 结构

### 完成标准

- 所有 session 事件都能写入数据库
- `event_seq` 单调递增且不重复

---

## Phase 2：建立 turn / tool call 投影

### 目标

- 不再靠扫历史推断当前状态

### 主要改动

- 新增 `session_turns`
- 新增 `session_tool_calls`
- 在事件写入事务中同步刷新投影

### 投影规则

- `turn_started`：
  - 更新 `sessions.last_execution_status = running`
  - upsert `session_turns`
- `turn_completed` / `turn_failed` / `turn_interrupted`：
  - 更新 `sessions.last_execution_status`
  - 关闭对应 `session_turns`
- `tool_call` / `tool_call_update`：
  - upsert `session_tool_calls`
  - 记录 `first_event_seq` / `last_event_seq`
  - 派生 `is_pending_approval`

### 完成标准

- `get_session_state()` 等查询改为基于数据库投影
- 不再需要通过 JSONL 或 in-memory runtime 扫描历史推断状态

---

## Phase 3：重做历史查询与增量流协议

### 目标

- 会话页面重载先读历史，再接增量
- stream cursor 只基于稳定 `event_seq`

### API 设计

#### 1. 历史接口

`GET /api/acp/sessions/{id}/events?after_seq=<u64>&limit=<u32>`

返回：

- `events`
- `snapshot_seq`
- `has_more`
- `next_after_seq`

#### 2. 增量流接口

保留：

- `/api/acp/sessions/{id}/stream`
- `/api/acp/sessions/{id}/stream/ndjson`

但语义改成：

- 输入 cursor：`after_seq`
- 输出 envelope：
  - `event`
  - `connected`
  - `heartbeat`

其中：

- `event.event_seq` 必须稳定
- `connected.last_seq` 必须表示“本次已实际送达的最后事件”

### 实现策略

推荐做法：

1. 查询当前 `snapshot_seq`
2. 先补发 `(after_seq, snapshot_seq]`
3. 再声明 `connected.last_seq = snapshot_seq`
4. 再转发 live events（要求其 `event_seq > snapshot_seq`）

### 完成标准

- reconnect 不丢消息
- replay 中途断流后能够准确续传

---

## Phase 4：重构前端会话消费模型

### 目标

- 让会话页以数据库事件流为输入，而不是以本地随机 entry 为主键

### 主要改动

- 引入 `PersistedSessionEvent` 前端类型
- `useAcpStream` 改为：
  - `hydrateHistory`
  - `connectLive`
  - `applyEvent`
- store/hook 状态至少包含：
  - `raw_events`
  - `last_applied_seq`
  - `entries`
  - `token_usage`

### reducer 原则

- `event_seq <= last_applied_seq` 直接丢弃
- tool call 聚合主键使用 `tool_call_id`
- 系统事件副作用主键使用 `event_seq`
- React key 尽量使用稳定业务键，不依赖 `Date.now() + random`

### 完成标准

- 重开 session / reconnect 不会重复渲染
- `SessionChatView` 中 hook runtime / canvas / companion 副作用只触发一次

---

## Phase 5：清理旧链路并补测试

### 目标

- 明确 JSONL 退出主链路
- 建立完整回归保护

### 清理项

- 删除或下线：
  - `subscribe_with_history()` 的“历史数组 + receiver”主逻辑
  - 前端 `sessionStateCache` 作为事实源的职责
  - 以数组下标作为 cursor 的实现

### 测试矩阵

- 后端：
  - append_event 顺序测试
  - replay + reconnect 无丢失测试
  - connected cursor 语义测试
  - 投影正确性测试
- 前端：
  - 同一 session 二次挂载不重复渲染
  - reconnect 后不重复 apply 历史
  - system event 副作用不重复触发
- E2E：
  - 执行一次带 tool call 的会话
  - 多次打开同一 session
  - 验证展示稳定

---

## 推荐实施顺序

1. 后端 schema / repository
2. 统一 append_event 服务
3. 状态投影
4. 历史查询 API
5. 增量流协议
6. 前端 hydrate + live delta
7. UI 副作用去重
8. 清理旧逻辑与补测试

---

## 风险与注意点

### 风险 1：一边保留旧 cursor 语义，一边引入新 `event_seq`

后果：

- 前后端会出现双语义并存
- replay bug 更难定位

建议：

- 直接把 session stream cursor 统一切到 `event_seq`

### 风险 2：仍允许前端 cache 作为事实源

后果：

- 页面刷新与同页切换行为继续不一致

建议：

- cache 只缓存已应用的 `last_applied_seq` 与派生视图，不能决定事实

### 风险 3：只记录原始事件，不做投影

后果：

- session 状态查询继续依赖扫历史

建议：

- 本次改造顺手补上 `session_turns` / `session_tool_calls`

### 风险 4：流协议只修 NDJSON，不修 SSE

后果：

- fallback 到 SSE 时仍会复现相同问题

建议：

- SSE / NDJSON 共同使用一套 `event_seq` 语义
