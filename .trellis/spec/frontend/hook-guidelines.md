# Hook Guidelines

> AgentDashboard 前端 Hook 开发规范。

---

## Hook 分类

### 1. 基础 Hooks（复用逻辑）

位于 `src/hooks/`，提供通用功能：

```ts
// hooks/use-theme.ts
export function useTheme() {
  // 主题切换逻辑
}
```

### 2. Feature Hooks（业务逻辑）

位于 `features/{feature}/model/`，封装特定业务：

```ts
// features/acp-session/model/useAcpSession.ts
export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  // ACP 会话管理逻辑
}
```

---

## 命名规范

| 类型 | 命名 | 示例 |
|------|------|------|
| 基础 Hook | use + 功能 | `useTheme`, `useLocalStorage` |
| 业务 Hook | use + 领域 | `useAcpSession`, `useStoryStore` |
| 流 Hook | use + Stream | `useAcpStream` |

---

## Hook 结构模板

```ts
/**
 * Hook 功能描述
 *
 * 详细说明用途、参数和返回值
 */

import { useState, useEffect, useCallback } from "react";

export interface UseXOptions {
  /** 描述 */
  key: string;
}

export interface UseXResult {
  /** 描述 */
  data: T;
  /** 是否加载中 */
  isLoading: boolean;
  /** 错误信息 */
  error: Error | null;
  /** 重新获取 */
  refetch: () => void;
}

export function useX(options: UseXOptions): UseXResult {
  // 1. 状态定义
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // 2. 回调函数使用 useCallback
  const fetchData = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await api.fetch(options.key);
      setData(result);
    } catch (e) {
      setError(e as Error);
    } finally {
      setIsLoading(false);
    }
  }, [options.key]);

  // 3. 副作用使用 useEffect
  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // 4. 返回结果
  return {
    data,
    isLoading,
    error,
    refetch: fetchData,
  };
}
```

---

## 流式 Hook 规范（SSE + Fetch NDJSON）

参考 `useAcpStream` + `streamTransport` 实现（fetch 优先，sse 回退）：

```ts
export interface UseStreamOptions {
  sessionId: string;
  endpoint?: string;
  onEntry?: (entry: Entry) => void;
  onError?: (error: Error) => void;
}

export interface UseStreamResult {
  entries: Entry[];
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
}

export function useAcpStream(options: UseStreamOptions): UseStreamResult {
  // 1. 默认使用 FetchNdjsonTransport（支持 header + 自定义重连）
  // 2. NDJSON 首次连接失败时自动回退 EventSourceTransport
  // 3. 统一通过 transport 生命周期更新 isConnected/isLoading
  // 4. 清理逻辑统一走 streamRegistry（组件卸载 + HMR dispose）
}
```

### 流式 Hook 必备功能

- [ ] 连接状态追踪 (`isConnected`)
- [ ] 提供 transport 抽象，业务层不直接依赖 `EventSource`
- [ ] NDJSON transport 支持 `x-stream-since-id` 续传
- [ ] SSE transport 支持 `Last-Event-ID` 续传
- [ ] NDJSON 首次失败自动降级到 SSE
- [ ] 消息缓冲与批量刷新（避免频繁重渲染）
- [ ] 错误处理（区分连接错误和解析错误）
- [ ] 清理函数（useEffect 返回时关闭 transport）
- [ ] HMR dispose 时统一关闭所有流连接（防止连接累积）
- [ ] 手动重连方法（重新创建 EventSource 实例）

### 流式 Hook 依赖卫生

长连接 / 流式 Hook 的连接 effect 必须避免把“每次 render 都新建”的数组、对象、函数包装值直接放进依赖里。

错误示例：

```ts
export function useAcpStream({ initialEntries = [] }: Options) {
  useEffect(() => {
    connect();
    return disconnect;
  }, [initialEntries]);
}
```

上面这种写法会在每次 render 生成新的 `[]`，导致 effect 持续 teardown / reconnect。对于会话流，这会表现为：

- 页面一直停在“连接中…”
- 历史 hydrate 被重复执行
- stream transport 被反复关闭重建

正确做法：

- 用模块级常量承接默认空数组 / 空对象
- 用 `ref` 保存“仅供下次 source reset 使用”的初始值
- 连接 effect 只依赖真正的 source key（如 `sessionId` / `endpoint` / `connectKey`）
- 如果 props 变化不应该触发重连，就不要把它直接放进连接 effect 依赖

```ts
const EMPTY_INITIAL_ENTRIES: Entry[] = [];

export function useAcpStream({ initialEntries }: Options) {
  const normalizedInitialEntries = initialEntries ?? EMPTY_INITIAL_ENTRIES;
  const initialEntriesRef = useRef(normalizedInitialEntries);

  useEffect(() => {
    initialEntriesRef.current = normalizedInitialEntries;
  }, [normalizedInitialEntries]);

  useEffect(() => {
    connect();
    return disconnect;
  }, [sessionId, endpoint, connectKey]);
}
```

### 环境变量约定

- `VITE_API_ORIGIN`（可选）：
  - 未设置：前端请求走相对路径（通常通过 Vite proxy 到后端）
  - 设置后：请求直接拼接为 `${VITE_API_ORIGIN}/api/...`，用于绕过 dev proxy

### NDJSON Envelope 契约（前端消费）

- `connected`：
  - 字段：`last_event_id: number`
- `event`：
  - 字段：
    - `session_id: string`
    - `event_seq: number`
    - `occurred_at_ms: number`
    - `committed_at_ms: number`
    - `session_update_type: string`
    - `turn_id?: string | null`
    - `entry_index?: number | null`
    - `tool_call_id?: string | null`
    - `notification: SessionNotification`
- `heartbeat`：
  - 字段：`timestamp: number`

前端必须：
- 对未知 `type` 安全忽略（不抛异常中断流）
- 仅当 payload 能解析成带 `notification` 的会话 envelope 时入队
- `SSE` 与 `NDJSON` 必须共享同一套 envelope 字段，不能让降级通道丢 `tool_call_id / turn_id / entry_index`

---

## Store Hook 规范

使用 Zustand 创建 Store：

```ts
// stores/storyStore.ts
import { create } from 'zustand';

interface StoryState {
  // 状态
  stories: Story[];
  isLoading: boolean;

  // 动作
  fetchStories: (backendId: string) => Promise<void>;
  createStory: (backendId: string, title: string) => Promise<void>;
}

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  isLoading: false,

  fetchStories: async (backendId) => {
    set({ isLoading: true });
    try {
      const response = await api.get(`/stories?backend_id=${backendId}`);
      set({ stories: response, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },
  // ...
}));
```

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 忘记 useCallback 导致无限重渲染 | 依赖变化的回调使用 useCallback |
| useEffect 缺少依赖项 | 完整填写依赖数组 |
| Hook 条件调用 | 始终在组件顶层调用 Hook |
| 返回过多状态 | 使用对象返回，方便解构 |

---

## ACP 事件归并契约（useAcpStream reducer）

`useAcpStream.applyNotification` 是 ACP 事件的唯一归并入口。以下是各事件类型的处理契约：

### 事件处理矩阵

| SessionUpdate 类型 | 归并策略 | 主流绘制 | 备注 |
|---|---|---|---|
| `tool_call` | **upsert**：按 `toolCallId` 反向查找，存在则覆盖，否则新建 | 是 | 对齐 Zed `upsert_tool_call` |
| `tool_call_update` | **merge**：合并到已有 entry；找不到锚点则创建孤立条目 | 是 | 孤立 update 不丢弃 |
| `agent_message_chunk` | **合并相邻**：同类型 + 同 turnId + 文本类型 → `mergeStreamChunk` | 是 | |
| `user_message_chunk` | 同上 | 是 | |
| `agent_thought_chunk` | 同上 | 是 | |
| `plan` | 直接添加新条目 | 是 | |
| `session_info_update` | 直接添加新条目 | **默认否**（选择性可见） | `task_*`、关键 lifecycle、`hook_event`、关键 companion/system 事件由 UI guard 决定是否绘制 |
| `usage_update` | 直接添加新条目 + 实时更新 `tokenUsage` state | **否**（header 圆环） | 用量通过 header 小圆环展示 |
| 其他 | 直接添加新条目 | 否 | |

### isPendingApproval 终态保护

```
status === "completed" | "failed" | "canceled" | "rejected"
  → isPendingApproval = false（覆盖）
rawOutput.approval_state === "pending"
  → isPendingApproval = true
status === "in_progress"
  → isPendingApproval = false
status === "pending" 且未进入审批
  → isPendingApproval = false
其他
  → 保留已有值
```

### ToolCallStatus 扩展

SDK v0.14 定义 `"pending" | "in_progress" | "completed" | "failed"`。
后端可能发送 Zed 扩展状态 `"canceled" | "rejected"`。
前端使用 `ExtendedToolCallStatus` 类型兼容。

### 错误模式：tool_call_update 覆盖 approval 状态

```ts
// 错误：直接用 incoming status 设置 isPendingApproval
isPendingApproval: update.status === "pending"

// 正确：终态覆盖 + 非终态保留
if (isTerminalToolCallStatus(incomingStatus)) {
  nextPendingApproval = false;
} else if (incomingApprovalPending) {
  nextPendingApproval = true;
} else if (incomingStatus === "in_progress") {
  nextPendingApproval = false;
} else if (incomingStatus === "pending") {
  nextPendingApproval = false;
}
```

### SessionInfoUpdate 可见性契约

`session_info_update` 不是“一律静默”。

当前前端约定：

- 任务语义事件：交给 `AcpTaskEventGuard`
- 系统 / companion / hook 事件：交给 `AcpSystemEventGuard`
- 其余未命中的 `session_info_update`：保留在 entries 中，但不渲染

当前必须可见的系统事件至少包括：

- `executor_session_bound`
- `turn_interrupted`（注意：`turn_started` / `turn_completed` **已静默**——会话运行状态由发送按钮表达）
- `turn_failed`
- `hook_event`（见下方 decision 级过滤规则）
- `companion_dispatch_registered`
- `companion_result_available`
- `companion_result_returned`

### hook_event 可见性规则（decision 级过滤）

`hook_event` 不是无条件显示。`AcpSystemEventGuard` 对其做 decision 级二次过滤：

**静默决策**（不在会话流中渲染卡片）：
- `stop` — 自然结束放行，turn 结束已由消息列表末尾表达
- `terminal_observed` — 纯技术终态记录，无用户感知价值
- `refresh_requested` — 内部快照刷新机制，用户无需感知
- `allow` — before_tool 放行，常规工具调用不需要在对话流占位
- `effects_applied` — after_tool 效果记录，高频且通常无用户可感知内容
- `noop` — 多个 trigger 的"无操作"决策，无实际效果
- `notified` — after_compact 通知，compaction 发生已由摘要消息表达
- `baseline_initialized` — session_start baseline 初始化，一次性技术事件
- `baseline_refreshed` — session_start baseline 刷新，一次性技术事件

**例外**：即使是静默决策，若携带 `block_reason`、`completion`、非空 `injections`，或“有用户可读信息量”的 `diagnostics` 则仍显示。

其中 diagnostics 的判定规则：
- 仅有 `session_binding_found` / `active_workflow_resolved` 这类背景诊断，不应提升为会话流可见事件
- 具备用户可读 `summary/message/detail` 且不属于上述背景码，才视为可见

decision 通过解析 `event.code`（格式 `hook:{trigger}:{decision}`）获取。

### Session Baseline Capabilities 分层

- `companion_agents` 和 `skills` 属于会话级稳定能力描述，统一收入 `SessionBaselineCapabilities` 数据契约
- 这些信息不应在每轮 `UserPromptSubmit` 以动态注入消息重复追加
- 后端在首轮 prompt 时以 `agentdash://session-capabilities/{session_id}` 资源块注入会话流
- 前端通过 `AcpSessionCapabilityCard` 将资源块解析为可展开的交互面板（companion agent chips + skill rows）
- `context_injected` 事件主要承载真正的动态治理信息（workflow 约束、pending action 等），避免被静态能力清单淹没
- Session context 页面通过 `SessionCapabilitiesSurfaceCard` 展示 capability 详情
- `SessionContextResponse.session_capabilities` 在 API 层动态构建，支持实时反映 hook runtime 和 address space 变更

### hook_event 渲染分级

后端 `should_emit_hook_trace_event` 是第一道过滤，前端 Guard 是第二道。
通过 Guard 的 hook_event 再按 decision 分两路渲染（见 `AcpSystemEventCard.tsx`）：

| 类型 | decision | 渲染方式 |
|------|----------|----------|
| 高优先级干预 | `deny/ask/rewrite/continue` 或有 `block_reason` | 完整大卡片（warning/error 色调） |
| 信息型 | `context_injected/steering_injected/phase_advanced` 等 | 可展开细条（默认折叠） |

`hook_event` 卡片（高优先级路径）必须能直接给出：

- `trigger`
- `decision`
- `completion`
- `matched_rule_keys`
- `diagnostics`

不能要求用户再去展开 hook runtime 面板才能理解为什么当前会话继续、停止或推进 phase。

### context_fragments 展示规则

`HookRuntimeSurfaceCard` 中，`context_fragments` 不能仅显示计数 badge。
每个 fragment 必须有可展开的 `HookContextFragmentRow`：slot badge + label + 展开 content。

---

## Scenario: Workflow Artifact Type Mapping

### 1. Scope / Trigger

- Trigger: 前端展示 workflow run 的 `record_artifacts`
- Trigger: 后端新增 `WorkflowRecordArtifactType`
- Trigger: 使用 `vite preview` / MCP 做真实联调

### 2. Contracts

- `frontend/src/types/index.ts` 中的 `WorkflowRecordArtifactType` 必须与后端 DTO 同步扩展
- `frontend/src/services/workflow.ts` 的 normalize / map 层必须无损保留新 artifact type
- Workflow 面板中的 type chip 必须直接显示 API 返回值，不允许在组件层二次硬编码“check phase -> phase_note”
- `record_artifacts[].phase_key` 与 `record_artifacts[].artifact_type` 都属于正式展示字段，不能在 mapper 中丢失

### 3. Validation & Error Matrix

| 场景 | 预期行为 | 风险 |
|---|---|---|
| 后端返回 `artifact_type=checklist_evidence` | UI type chip 显示 `checklist_evidence` | 正常 |
| type union / normalize 未同步 | UI 会被静默降级成 `phase_note` | 严重误导联调判断 |
| `vite preview` 仍在服务旧 dist | 即使源码已修复，UI 仍可能显示旧值 | 必须重建再验 |

### 4. Wrong vs Correct

#### Wrong

```ts
function normalizeWorkflowRecordArtifactType(value: string): WorkflowRecordArtifactType {
  switch (value) {
    case "phase_note":
      return value;
    default:
      return "phase_note";
  }
}
```

结果：

- 后端明明已经返回 `checklist_evidence`
- 但前端在 mapper 边界静默降级，导致 Workflow 面板显示错误

#### Correct

```ts
function normalizeWorkflowRecordArtifactType(value: string): WorkflowRecordArtifactType {
  switch (value) {
    case "session_summary":
    case "journal_update":
    case "archive_suggestion":
    case "phase_note":
    case "checklist_evidence":
      return value;
    default:
      return "phase_note";
  }
}
```

### 5. 联调 Gotcha

> **Warning**: 使用 `vite preview` 做 MCP 联调时，前端展示取决于 `frontend/dist` 当前构建产物，而不是工作区源码本身。
>
> 如果刚修改了 `services/workflow.ts` / `types/index.ts` / 相关页面组件，却没有重新执行 `pnpm --dir frontend build`，preview 仍可能继续服务旧 bundle，表现为“浏览器 fetch 是新值，但 UI 还是旧值”。
>
> 当前项目已真实踩到过一次：后端与浏览器直接 fetch 都返回 `checklist_evidence`，但旧 dist 中的 normalize 仍只认 `phase_note`，最终 Workflow 面板错误显示 `phase_note`。遇到这种现象，应先重建 dist，再继续判断是否真有前端逻辑 bug。

---

## 参考实现

- `features/acp-session/model/useAcpStream.ts` - ACP 流管理 + 事件归并 reducer
- `features/acp-session/model/useAcpSession.ts` - 聚合逻辑 + tokenUsage 暴露
- `features/acp-session/model/streamTransport.ts` - NDJSON/SSE 双通道传输
- `stores/storyStore.ts` - Zustand Store
