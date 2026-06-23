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
| `UserInputSubmitted` | 按 `turn_id + item_id` upsert 为用户输入 entry | 是 |
| `TurnPlanUpdated` / `PlanDelta` | 直接添加/更新 | 是 |
| `Platform(HookTrace)` | 直接添加 | 选择性可见（Guard 决定） |
| `Platform(SessionMetaUpdate)` | 按 key 分发 | 选择性可见 |
| `TokenUsageUpdated` | 更新 `tokenUsage` state | 否（header 圆环） |

`tokenUsage` 聚合后必须保留当前上下文与累计消耗的语义差异：header 圆环和上下文窗口百分比使用 `currentContextTokens / effectiveContextWindow`，累计统计使用 `cumulativeTotalTokens` 或 provider `total` breakdown。上下文查看窗口消费 session projection 返回的 `context_usage` 分类与 token usage state 的窗口信息；这样 UI 负责渲染，不重新实现后端的 token estimate 口径。

### isPendingApproval 终态保护

终态（completed/failed/canceled/rejected）→ `false`；`approval_state=pending` → `true`；`in_progress` → `false`。

### Platform 事件可见性

`Platform(HookTrace)` 和 `Platform(SessionMetaUpdate)` 不一律静默，交由 `SessionTaskEventGuard` 和 `SessionSystemEventGuard` 判定。

### 聚合范畴 — 同类内部，绝不跨类

`useSessionFeed::classifyEntry` 把每个 entry 划成四类。**两个独立的聚合 lane：tool_burst 与 CTX side group。两者互不混。**

| 分类 | 触发 | 行为 |
|---|---|---|
| `tool_like` | 终态 item_started/completed for tool kinds（commandExecution / fileChange / mcpToolCall / dynamicToolCall / fsRead / fsGrep / fsGlob / 等） | 累积进活跃 tool group，连续 ≥2 条折成 tool burst |
| `active_tool` | `status === "inProgress"` 的工具 entry | flush 已完成 tool group，自身作为单卡展示；终态事件到达后重新进入 `tool_like` 并参与 burst |
| `hard_boundary` | 非空 agent_message_delta / reasoning_* / approval / error / user_input_submitted / 可渲染 hook_event / `context_frame` 等 | flush tool group；非 CTX 自身作为单 entry 入 result；CTX 进 CTX side group 内部聚合（连续多条折成 "Nx" CTX 组） |
| `neutral` | turn_started/completed / token_usage_updated / 静默 platform 等 | 完全透明 |

> **关键约定 1（合并范畴）**：合并只对同类内部生效——tool_like 跟 tool_like 合并，CTX 跟 CTX 合并。**两个 lane 之间永不混**。CTX 出现在工具序列中间会截断工具 burst，反之 tool 也不会进 CTX 组。
>
> **关键约定 2（不聚合 reasoning）**：reasoning_text_delta/summary 同 itemId 已在 `useSessionStream` 层累积成单条 entry，到 `aggregateEntries` 这层不会出现"连续多条 thinking entry"，因此 thinking 不需要聚合 lane。
>
> **关键约定 3（hard / neutral）**：判定一个 platform 事件是否打断工具 burst，看它是否改变用户或 Agent 可见上下文。
> - 如果是 agent 说话、用户输入、错误提示、审批，或 `context_frame` 表达 identity / capability / tool schema 等运行期上下文变化 — hard。
> - 如果只是 telemetry / lifecycle observation（如 `hook:before_provider_request:observed`）或空文本 thinking，它不表达新的会话内容 — neutral。
>
> **关键约定 4（active tool 先独立）**：执行中的工具不进入已折叠 burst。它保持单卡可见，原因是运行态需要展示进度、审批和展开状态；终态事件到达后聚合由完整 entries 重新计算，工具会自然并入相邻 burst。
>
> **关键约定 5（bounded output 单卡可见）**：终态工具或 command 带有 bounded/truncation marker 时不进入 tool burst。它保持单卡可见，原因是用户需要直接看到输出被裁切、omitted bytes 与 lifecycle path；完整输出读取由后端 lifecycle VFS / `fs_read` 合同承载。

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
