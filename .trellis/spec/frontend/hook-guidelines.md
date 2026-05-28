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

`tokenUsage` 聚合后必须保留当前上下文与累计消耗的语义差异：header 圆环和上下文窗口百分比使用 `currentContextTokens / effectiveContextWindow`，累计统计使用 `cumulativeTotalTokens` 或 provider `total` breakdown。上下文查看窗口消费 session projection 返回的 `context_usage` 分类与 token usage state 的窗口信息；这样 UI 负责渲染，不重新实现后端的 token estimate 口径。

### isPendingApproval 终态保护

终态（completed/failed/canceled/rejected）→ `false`；`approval_state=pending` → `true`；`in_progress` → `false`。

### Platform 事件可见性

`Platform(HookTrace)` 和 `Platform(SessionMetaUpdate)` 不一律静默，交由 `SessionTaskEventGuard` 和 `SessionSystemEventGuard` 判定。

### Tool burst 边界分类（soft vs hard）

`useSessionFeed::classifyEntry` 把每个 entry 划成四类，决定它对工具组合并的影响：

| 分类 | 触发 | 对 tool burst |
|---|---|---|
| `tool_like` | item_started/completed for tool kinds | 加入活跃 tool group |
| `hard_boundary` | 非空 agent_message_delta / reasoning_* / approval / error / user_message_chunk / 可渲染 hook_event 等 | flush tool group 并自身入 result |
| `soft_boundary` | `context_frame`（侧轨身份/能力切换） | **不**flush tool group，仅入自身 side group |
| `neutral` | turn_started/completed / token_usage_updated / 静默 platform 等 | 完全透明 |

> **关键约定**：判定一个 platform 事件是 hard 还是 soft，看它是不是"agent/用户实际产出的可见内容"。
> - 如果是 agent 说话、用户输入、错误提示、审批 — hard。
> - 如果只是后端往会话里塞的"侧轨上下文"（identity/capability/tool schema 切换等），渲染时只是参考信息，不打断主交互 — soft。
>
> **不要把可渲染但属于侧轨性质的事件归为 hard_boundary**——会出现"幽灵 boundary"：肉眼看不到分隔，工具组却被打散。曾经因把 `context_frame` 默认归为 hard 导致连续工具调用全被拆开（见 commit `b787c6bb`）。

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
