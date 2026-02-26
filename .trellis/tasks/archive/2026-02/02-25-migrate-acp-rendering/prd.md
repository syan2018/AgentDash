# 迁移前端绘制组件到 ACP 协议

## 目标

将前端 Agent 会话绘制组件从当前的 `NormalizedEntry` 数据结构迁移到使用 **ACP (Agent Client Protocol)** 协议定义的结构，依赖 `@agentclientprotocol/sdk` 库。

## 背景

当前 vibe-kanban 前端使用自定义的 `NormalizedEntry` 结构来绘制 Agent 会话，该结构是后端将各种 Agent 输出（Claude Code、Gemini、Qwen 等）归一化后的结果。随着 ACP 协议的成熟，我们希望直接使用 ACP 定义的数据结构，减少后端转换层，提高协议兼容性。

## ACP 核心数据结构

### SessionNotification
ACP 的实时会话更新通知，包含以下更新类型：

```typescript
// SessionUpdate 变体
type SessionUpdate =
  | { sessionUpdate: "user_message_chunk"; content: ContentChunk }
  | { sessionUpdate: "agent_message_chunk"; content: ContentChunk }
  | { sessionUpdate: "agent_thought_chunk"; content: ContentChunk }
  | { sessionUpdate: "tool_call"; ...ToolCall }
  | { sessionUpdate: "tool_call_update"; ...ToolCallUpdate }
  | { sessionUpdate: "plan"; ...Plan }
  | { sessionUpdate: "available_commands_update"; ... }
  | { sessionUpdate: "current_mode_update"; ... };
```

### ToolCall
```typescript
interface ToolCall {
  toolCallId: ToolCallId;
  kind: ToolKind;
  title: string;
  status: ToolCallStatus;
  fields: ToolCallFields;
  content: ToolCallContent[];
  rawInput?: JsonValue;
  rawOutput?: JsonValue;
}
```

### ContentBlock
```typescript
type ContentBlock =
  | { type: "text"; text: TextContent }
  | { type: "image"; image: ImageContent }
  | { type: "terminal"; terminal: TerminalContent }
  | { type: "resource"; resource: ResourceContent };
```

## 需求

### 1. 数据结构迁移

将前端的 `NormalizedEntry` 替换为 ACP 的 `SessionNotification` 结构：

| 当前 (NormalizedEntry) | ACP 对应 |
|------------------------|----------|
| `user_message` | `user_message_chunk` |
| `assistant_message` | `agent_message_chunk` |
| `thinking` | `agent_thought_chunk` |
| `tool_use` | `tool_call` / `tool_call_update` |
| `error_message` | `error` |
| `plan_presentation` | `plan` |

### 2. 绘制组件适配

适配以下组件使用 ACP 结构：

- [ ] `DisplayConversationEntry.tsx` → `AcpSessionEntry.tsx`
- [ ] `ToolCallCard` → `AcpToolCallCard`
- [ ] `UserMessage` → `AcpUserMessage`
- [ ] `PlanPresentationCard` → `AcpPlanCard`

### 3. 实时流协议适配

WebSocket 流从 JSON Patch 改为 ACP SessionNotification 流：

```typescript
// 当前
interface PatchType {
  type: "NORMALIZED_ENTRY" | "STDOUT" | "STDERR";
  content: NormalizedEntry | string;
}

// 目标
interface AcpSessionMessage {
  sessionId: SessionId;
  update: SessionUpdate;
}
```

### 4. SDK 集成

引入 `@agentclientprotocol/sdk` npm 包：

```bash
npm install @agentclientprotocol/sdk
```

使用 SDK 提供的类型：

```typescript
import {
  SessionNotification,
  SessionUpdate,
  ToolCall,
  ToolCallUpdate,
  ContentBlock,
  Plan,
} from "@agentclientprotocol/sdk";
```

## 技术方案

### 目录结构

```
packages/web-core/src/features/acp-session/
├── model/
│   ├── types.ts              # ACP 类型扩展
│   ├── useAcpSession.ts      # 会话管理 Hook
│   └── useAcpStream.ts       # WebSocket 流管理
├── ui/
│   ├── AcpSessionList.tsx    # 会话列表容器
│   ├── AcpSessionEntry.tsx   # 条目渲染组件
│   ├── AcpToolCallCard.tsx   # 工具调用卡片
│   ├── AcpMessageCard.tsx    # 消息卡片
│   └── AcpPlanCard.tsx       # 计划卡片
└── index.ts
```

### 关键类型定义

```typescript
// model/types.ts
import { SessionNotification, ToolCallId } from "@agentclientprotocol/sdk";

export interface AcpDisplayEntry {
  id: string;
  sessionId: string;
  timestamp: number;
  update: SessionUpdate;
  // 派生状态
  isStreaming?: boolean;
  isPendingApproval?: boolean;
}

export interface AcpToolCallState {
  toolCallId: ToolCallId;
  call: ToolCall | null;
  updates: ToolCallUpdate[];
  finalResult?: unknown;
}
```

### 流处理 Hook

```typescript
// model/useAcpStream.ts
export const useAcpStream = (sessionId: string) => {
  const [entries, setEntries] = useState<AcpDisplayEntry[]>([]);
  const [toolStates, setToolStates] = useState<Map<string, AcpToolCallState>>();

  useEffect(() => {
    const ws = new WebSocket(`/api/acp/sessions/${sessionId}/stream`);

    ws.onmessage = (event) => {
      const notification: SessionNotification = JSON.parse(event.data);
      processUpdate(notification);
    };

    return () => ws.close();
  }, [sessionId]);

  return { entries, toolStates, isConnected };
};
```

## 验收标准

- [ ] 所有 `NormalizedEntry` 引用替换为 ACP 类型
- [ ] 会话绘制功能与迁移前一致
- [ ] 工具调用状态实时更新正常
- [ ] 计划展示正常
- [ ] 审批流程正常
- [ ] 支持消息聚合（连续 file_read 等）
- [ ] 支持思考过程折叠

## 依赖

- `@agentclientprotocol/sdk` npm 包
- 后端提供 ACP 格式的 WebSocket 端点

## 风险

1. ACP SDK 可能还未发布稳定版本
2. 后端需要同步支持 ACP 流输出
3. 复杂 ToolCall 内容的渲染兼容性

## 参考

- ACP 协议仓库：`third_party/agent-client-protocol/`
- 当前实现：`packages/web-core/src/features/workspace-chat/`
