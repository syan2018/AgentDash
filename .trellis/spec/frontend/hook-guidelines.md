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

### History Hydrate 与 Live 副作用边界

`useSessionStream` 暴露的历史事件用于重建 feed、turn segment、projection refresh key 等本地展示状态；会改变用户工作台意图的控制面副作用（workspace panel open、task plan refresh、module presentation action）只消费初始 `historyReplayBoundarySeq` 之后的新 durable 事件。这样做的原因是历史分页表达的是既有事实回放，而不是新的用户或 Agent 意图；同一条历史 `workspace_module_presented` 不能在每次打开长 session 时重新触发外部打开动作。

AgentRun Runtime feed 的 `turn_started`、`turn_terminal`、`interaction_requested`与`interaction_terminal`是Runtime inspect的失效信号：feed保留可展示的event identity/terminal，但命令可用性仍通过重新读取canonical snapshot获得，不直接修改本地command state。有限durable replay即使在页面打开后才消费到terminal，也必须触发同一刷新，原因是composer的`turn_start/turn_steer`与interaction response只能由最新`command_availability`裁决。

### Terminal Platform Event Projection

Terminal platform event 是展示投影，不是聊天 feed entry，也不是控制面副作用。`useSessionStream`
必须在 history hydrate 与 live stream 两条路径上都调用同一个 terminal projector，并保持 reducer
纯净。

#### 1. Scope / Trigger

- Trigger: `BackboneEvent::Platform(terminal_output | terminal_state_changed)` 需要重建 terminal tab 可见状态。
- Scope: `useSessionStream` history paging、`dispatchSessionPlatformEvent` live dispatch、`useTerminalStore` terminal output/state projection。

#### 2. Signatures

```ts
export function dispatchSessionPlatformEvent(
  event: SessionEventEnvelope,
  onError?: (error: Error) => void,
): boolean;

export function projectSessionTerminalPlatformEvents(
  events: readonly SessionEventEnvelope[],
  onError?: (error: Error) => void,
): void;
```

#### 3. Contracts

- History hydrate applies `projectSessionTerminalPlatformEvents(page.events, onError)` before or alongside `reduceStreamState`.
- Live stream applies `dispatchSessionPlatformEvent(event, onError)` before reducer enqueue.
- `terminal_output` writes bounded terminal output to `useTerminalStore`.
- `terminal_state_changed` writes terminal state to `useTerminalStore`, creating a minimal state projection when the terminal has not been registered in the browser.
- Projection idempotence key is `session_id:event_seq`; reconnect, StrictMode, or history replay must not duplicate output.
- `sessionStreamReducer` keeps filtering terminal platform events out of chat display entries.

#### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| history page contains `terminal_output` | terminal store output buffer contains the chunk in event order |
| live stream contains `terminal_output` | terminal store output buffer receives the chunk once |
| same `session_id:event_seq` is projected twice | second projection is ignored |
| `terminal_state_changed` arrives before terminal registration | terminal store creates state-only projection |
| reducer receives terminal platform event | feed entries remain unchanged |

#### 5. Good/Base/Bad Cases

- Good: user opens an old session, history terminal output is replayed into the terminal tab without polluting the chat feed.
- Base: live terminal output continues to use the dispatcher path and stays out of reducer side effects.
- Bad: terminal output is appended inside `sessionStreamReducer`, because reducers may be replayed and must remain pure.

#### 6. Tests Required

- Dispatcher test for duplicate durable terminal output not appending twice.
- Store test for state-only projection from unregistered terminal state.
- Stream/hydrator test or dispatcher batch test proving history events enter terminal store.
- Reducer regression test that terminal platform events do not create chat entries.

#### 7. Boundary Mismatch / Canonical

```ts
// Boundary mismatch: terminal output becomes a chat entry or reducer side effect.
nextState = reduceStreamState(nextState, page.events);
```

```ts
// Canonical: terminal display projection is explicit and idempotent.
projectSessionTerminalPlatformEvents(page.events, onError);
nextState = reduceStreamState(nextState, page.events);
```

### 聚合范畴 — 同类内部，绝不跨类

`useSessionFeed::classifyEntry` 把每个 entry 划成四类。**两个独立的聚合 lane：tool_burst 与 CTX side group。两者互不混。**

| 分类 | 触发 | 行为 |
|---|---|---|
| `tool_like` | item_started/item_updated/item_completed for tool kinds（commandExecution / fileChange / mcpToolCall / dynamicToolCall / fsRead / fsGrep / fsGlob / 等），且没有 bounded/truncation marker | 累积进活跃 tool group，连续 ≥2 条折成 tool burst；运行中工具也保留在 burst 内 |
| `tool_single` | 工具 entry 带 bounded/truncation marker | flush tool group，自身作为单卡展示 |
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
> **关键约定 4（运行中工具进入 burst）**：执行中的工具默认进入 tool burst，原因是工具 UI identity 来自 item id；运行态、审批态和输出摘要应在同一个工具组内展示，避免完成前后列表形态跳动。
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
