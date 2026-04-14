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

## 流式 Hook 规范（SSE + Fetch NDJSON）

参考 `useAcpStream` + `streamTransport` 实现（fetch 优先，sse 回退）。

### 流式 Hook 必备功能

- [ ] 连接状态追踪 (`isConnected`)
- [ ] transport 抽象，业务层不直接依赖 `EventSource`
- [ ] NDJSON transport 支持 `x-stream-since-id` 续传
- [ ] SSE transport 支持 `Last-Event-ID` 续传
- [ ] NDJSON 首次失败自动降级到 SSE
- [ ] 消息缓冲与批量刷新
- [ ] 清理函数 + HMR dispose 时统一关闭（防止连接累积）
- [ ] 手动重连方法

### 流式 Hook 依赖卫生

长连接 Hook 的连接 effect 不能把"每次 render 新建"的数组/对象放进依赖里。

```ts
// ❌ 每次 render 生成新的 []，导致 effect 持续 teardown / reconnect
useEffect(() => { connect(); return disconnect; }, [initialEntries]);

// ✅ 用模块级常量 + ref + 仅依赖 source key
const EMPTY: Entry[] = [];
const initialRef = useRef(initialEntries ?? EMPTY);
useEffect(() => { connect(); return disconnect; }, [sessionId, endpoint, connectKey]);
```

### NDJSON Envelope 契约

- `connected`：`last_event_id: number`
- `event`：`session_id`, `event_seq`, `occurred_at_ms`, `committed_at_ms`, `session_update_type`, `turn_id?`, `entry_index?`, `tool_call_id?`, `notification`
- `heartbeat`：`timestamp: number`

前端必须对未知 `type` 安全忽略；SSE 与 NDJSON 共享同一套 envelope 字段。

---

## ACP 事件归并契约（useAcpStream reducer）

`useAcpStream.applyNotification` 是 ACP 事件的唯一归并入口：

| SessionUpdate 类型 | 归并策略 | 主流绘制 |
|---|---|---|
| `tool_call` | **upsert**：按 `toolCallId` 反向查找 | 是 |
| `tool_call_update` | **merge**：合并到已有 entry | 是 |
| `*_message_chunk` / `*_thought_chunk` | **合并相邻**：同类型 + 同 turnId → `mergeStreamChunk` | 是 |
| `plan` | 直接添加 | 是 |
| `session_info_update` | 直接添加 | 选择性可见（由 Guard 决定） |
| `usage_update` | 更新 `tokenUsage` state | 否（header 圆环） |

### isPendingApproval 终态保护

终态（completed/failed/canceled/rejected）→ `false`；`approval_state=pending` → `true`；`in_progress` → `false`。

### SessionInfoUpdate 可见性

`session_info_update` 不是一律静默。交由 `AcpTaskEventGuard` 和 `AcpSystemEventGuard` 判定。

> hook_event 可见性规则、companion 事件处理等详见后端 [execution-hook-runtime.md](../backend/execution-hook-runtime.md) 及前端 Guard 组件实现。

---

## Scenario: Workflow Artifact Type Mapping

### Contracts

- `WorkflowRecordArtifactType` 前后端必须同步扩展
- Workflow 面板 type chip 直接显示 API 返回值，不允许组件层硬编码映射
- `record_artifacts[].phase_key` 和 `artifact_type` 都是正式展示字段

### Gotcha

> **Warning**: `vite preview` 服务的是 `frontend/dist` 当前构建产物，不是工作区源码。修改 types/services 后必须重新 `pnpm --dir frontend build`，否则 preview 仍服务旧 bundle。项目已踩过一次。

---

## 参考实现

- `features/acp-session/model/useAcpStream.ts` - ACP 流管理 + 事件归并 reducer
- `features/acp-session/model/useAcpSession.ts` - 聚合逻辑 + tokenUsage 暴露
- `features/acp-session/model/streamTransport.ts` - NDJSON/SSE 双通道传输

*更新：2026-04-14 — 移除与后端 execution-hook-runtime.md 重叠的 hook_event visibility 详细规则，改为引用*
