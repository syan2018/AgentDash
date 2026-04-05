# Session 会话事件数据库化与可靠重放

## 背景

当前会话页的重放链路依赖：

- 本机 `.agentdash/sessions/{session_id}.jsonl` 历史文件
- `SessionHub.subscribe_with_history()` 返回的“历史数组 + broadcast receiver”
- 前端 `useAcpStream` 的模块级 `sessionStateCache`
- 会话流接口以“历史数组下标”作为 resume cursor

这套实现带来了几类根本性问题：

1. 页面重新打开同一 session 时，前端会把已有 `entries` 从内存 cache 恢复出来，但新的 transport 又会从历史头部 replay，导致会话内容重复渲染，tool call / system event 在页面底部持续堆积。
2. 后端读取历史再订阅实时流，中间存在 race window；若在两者之间刚好写入新事件，该事件既不会出现在历史里，也不会被新订阅者收到。
3. NDJSON `connected.last_event_id` 在 replay 真正发完之前就对外宣称“已经追平历史末尾”，一旦 replay 中途断线，客户端会错误跳过尚未收到的尾部历史。
4. 前端系统事件副作用依赖随机生成的本地 entry id 去重，而不是稳定的服务端事件标识，因此 replay 同一段历史时会再次触发 hook runtime 刷新、canvas 打开等副作用。

这些问题的共同根因不是某一段 reducer 写错，而是：

- **会话事件没有数据库级稳定事实源**
- **会话重放没有稳定 cursor 语义**
- **前端没有基于稳定事件 id 做幂等消费**

## 目标

把 session 重载 / replay / reconnect 改造成一套基于数据库状态的稳定实现：

1. 会话事件以数据库中的 append-only event log 作为唯一事实源。
2. 每个 session 内的事件顺序由稳定、自增、可持久化的 `event_seq` 表达。
3. 页面重载时，先从数据库读取历史事件，再建立增量流，不再依赖前端 cache 作为事实源。
4. 实时流只负责补发 `after_seq` 之后的事件和后续增量，不再使用数组下标或随机 id 作为 cursor。
5. 前端按稳定 `event_seq` 幂等应用事件；同一条历史 replay 多次也不会重复渲染、重复聚合、重复触发副作用。
6. Session 当前状态、turn 状态、tool call 状态可通过数据库投影稳定查询，而不是靠扫 JSONL 或内存 map 推断。

## 核心设计

### 1. 事实源

- `sessions`：会话元数据与最新状态
- `session_events`：append-only 会话事件日志，保存原始 `SessionNotification`
- `session_turns`：turn 级投影
- `session_tool_calls`：tool call 级投影

其中：

- `session_events` 是 replay 的唯一事实源
- `session_turns` / `session_tool_calls` 是快速查询投影，不替代原始事件日志

### 2. 顺序定义

每个 session 内维护单调递增的 `event_seq`：

- `UNIQUE(session_id, event_seq)`
- 由后端在持久化事务内分配
- 前端和流协议都基于 `event_seq` 做 resume / dedupe / side-effect gating

### 3. 写入原则

所有会话通知统一经由数据库事务写入：

1. 分配 `event_seq`
2. 写 `session_events`
3. 更新 `sessions` / `session_turns` / `session_tool_calls`
4. 事务提交
5. 向内存 broadcast 发出“已落库事件”

**先落库，后广播**，保证数据库永远是权威事实源。

### 4. 重载原则

页面打开 session 时：

1. 通过历史接口从数据库加载事件页
2. 记住当前 `last_applied_seq`
3. 建立增量流并携带 `after_seq`
4. 对于 `event_seq <= last_applied_seq` 的事件直接忽略

### 5. 副作用原则

系统事件的 UI 副作用必须按稳定事件序号去重，而不是按客户端随机 entry id。

## 范围

### 后端持久化

- 新增 Session SQLite 持久化结构
- 新增 `session_events` / `session_turns` / `session_tool_calls`
- 引入 session event append service / repository
- 废弃 JSONL 作为主事实源

### 后端会话流

- 重做 `/api/acp/sessions/{id}/stream`
- 重做 `/api/acp/sessions/{id}/stream/ndjson`
- 新增 session 历史查询接口
- 明确 `connected` / `event` / `heartbeat` 的稳定 cursor 契约

### 前端会话页

- 会话页改为“history hydrate + live delta”
- `useAcpStream` 改造为基于稳定 `event_seq` 的幂等 reducer
- `SessionChatView` 系统事件副作用改为按稳定事件序号去重
- tool call / chunk / system event 聚合层改造为消费“带 event_seq 的原始事件”

### 测试

- 后端 repository / stream / resume 测试
- 前端 replay / reconnect / side-effect dedupe 测试
- 至少一条“页面重开不重复渲染”的端到端验证路径

## 非目标

- 不继续兼容 JSONL 作为线上主事实源
- 不为旧错误 cursor 语义做长期双协议兼容
- 不为当前预研阶段额外设计复杂的数据迁移回滚方案
- 不扩展 ACP 协议内容本身，只重构 AgentDash 自身的持久化与流 envelope

## 验收标准

- [ ] 重新打开同一 session 页面，不会再出现消息重复追加、tool call 卡片堆积、system event 多次触发的问题
- [ ] reconnect 期间不会因历史读取与订阅之间的 race 丢失事件
- [ ] 中途断流后，客户端能够从“已真正收到的最后 event_seq”继续补发
- [ ] tool call 在 UI 中始终按 `tool_call_id` 呈现单一当前态，不会因 replay 变成新的孤立卡片
- [ ] session 当前执行状态、turn 终态、tool call 当前态可直接通过数据库投影查询
- [ ] 后端 stream 与前端 reducer 都有覆盖 replay / reconnect / dedupe 的自动化测试

## 验证建议

- 后端：
  - `cargo test` 覆盖 session event append / replay / reconnect
  - `cargo check`
- 前端：
  - `pnpm --dir frontend exec vitest`
  - `pnpm --dir frontend exec tsc --noEmit`
- 联调：
  - 打开同一 `/session/:id`
  - 执行一次完整 tool call + agent response
  - 反复刷新 / 切换回来 / 手动 reconnect
  - 确认消息、tool call、system event 不重复且顺序稳定
