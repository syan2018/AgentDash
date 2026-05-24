# Hook Guidelines

> 前端 Hook 开发规范。

---

## Hook 分类

| 类型 | 位置 | 命名 | 示例 |
|------|------|------|------|
| 基础 Hook | `src/hooks/` | use + 功能 | `useTheme` |
| Feature Hook | `features/{name}/model/` | use + 领域 | `useSessionStream` |
| 流式 Hook | `features/{name}/model/` | use + Stream/Feed | `useSessionFeed` |

---

## 流式 Hook 规范（Fetch NDJSON）

参考 `useSessionStream` + `streamTransport` 实现。

### 必备功能

- 连接状态追踪（`isConnected`）
- transport 抽象：业务层不直接处理 `ReadableStream`
- NDJSON transport 支持 `x-stream-since-id` 续传
- 消息缓冲与批量刷新
- 清理函数 + HMR dispose 时统一关闭（防连接累积）

### 依赖卫生

长连接 Hook 的连接 effect 不能把"每次 render 新建"的数组/对象放进依赖里：

```ts
// ❌ 每次 render 生成新引用，导致 effect 持续 teardown / reconnect
useEffect(() => { connect(); return disconnect; }, [initialEntries]);

// ✅ 用模块级常量 + ref + 仅依赖 source key
const EMPTY: Entry[] = [];
const initialRef = useRef(initialEntries ?? EMPTY);
useEffect(() => { connect(); return disconnect; }, [sessionId, endpoint, connectKey]);
```

### NDJSON Envelope 契约

NDJSON envelope 属于 cross-layer contract，类型应由 Rust contract 生成。Hook 只消费解析后的 envelope，并把业务聚合交给 reducer。

- `connected`：`last_event_id: number`
- `event`：`session_id`, `event_seq`, `occurred_at_ms`, `committed_at_ms`, `session_update_type`, `turn_id?`, `entry_index?`, `tool_call_id?`, `notification`（`BackboneEnvelope`）
- `heartbeat`：`timestamp: number`

前端必须对未知 `type` 安全忽略；业务层只消费解析后的 envelope。

---

## 事件聚合契约（useSessionFeed）

`useSessionFeed` 将 `BackboneEvent` 变体聚合为 UI 可渲染的 feed entries：

| BackboneEvent 变体 | 聚合策略 | 主流绘制 |
|---|---|---|
| `ItemStarted` / `ItemCompleted` | 按 item id 反向查找 upsert | 是 |
| `AgentMessageDelta` / `ReasoningTextDelta` | 同 turn_id 合并相邻 chunk | 是 |
| `TurnPlanUpdated` / `PlanDelta` | 直接添加/更新 | 是 |
| `Platform(HookTrace)` | 直接添加 | 选择性可见（Guard 决定） |
| `Platform(SessionMetaUpdate)` | 按 key 分发 | 选择性可见 |
| `TokenUsageUpdated` | 更新 `tokenUsage` state | 否（header 圆环） |

### isPendingApproval 终态保护

终态（completed/failed/canceled/rejected）→ `false`；`approval_state=pending` → `true`；`in_progress` → `false`。

### Platform 事件可见性

`Platform(HookTrace)` 和 `Platform(SessionMetaUpdate)` 不一律静默，交由 `SessionTaskEventGuard` 和 `SessionSystemEventGuard` 判定。

---

## Workflow Artifact Type Mapping

- `WorkflowRecordArtifactType` 前后端必须同步扩展
- Workflow 面板 type chip 直接显示 API 返回值，不允许组件层硬编码映射

---

## 参考实现

- `features/session/model/useSessionStream.ts` — 流管理
- `features/session/model/useSessionFeed.ts` — 事件聚合
- `features/session/model/streamTransport.ts` — NDJSON 会话流
- `features/session/model/platformEvent.ts` — PlatformEvent 解析
