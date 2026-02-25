# 构建执行集成层 - Rust后端与TS前端数据交换

## 目标

构建一个统一的执行集成层，实现 Rust 后端与 TypeScript 前端之间的 Agent 执行信息交换。该层将：

1. **复用** `third_party/vibe-kanban/crates/executors` 执行模块
2. **参考** `references/ABCCraft` 的 ACP 实现模式
3. **使用** ACP 协议作为前后端数据交换的通用结构
4. **支持** 多种执行器类型（MCP、Claude Code、Aider 等）

## 核心设计原则

### 1. ACP 作为通用交换结构

```
┌─────────────────────────────────────────────────────────────────┐
│                        Frontend (React/TS)                      │
│              使用 @agentclientprotocol/sdk 渲染                  │
│                   (AcpSessionEntry, AcpToolCallCard)             │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ WebSocket/NDJSON
                              │ SessionNotification
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Backend (Rust)                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  ACP Native  │  │  Executors   │  │   ACP Adapter        │  │
│  │  Connector   │  │  Wrapper     │  │   (Claude Code etc.) │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│                              │                                   │
│                    ┌─────────┴─────────┐                         │
│                    ▼                   ▼                         │
│              ┌──────────┐       ┌──────────┐                    │
│              │ ACP Agent│       │ Non-ACP  │                    │
│              │ (OpenCode│       │ Agent    │                    │
│              │ Gemini)  │       │ (Claude) │                    │
│              └──────────┘       └──────────┘                    │
└─────────────────────────────────────────────────────────────────┘
```

**关键理解**：使用 ACP 协议 ≠ 只能使用 ACP 连接 Agent
- 对于已有 ACP 后端的项目（如 OpenCode、Gemini），直接连接
- 对于非 ACP 后端（如 Claude Code），通过 wrapper 转换为 ACP 结构

### 2. 分层架构

```
Layer 1: API Layer (HTTP/WebSocket)
  - POST /api/sessions/{id}/prompt
  - WS /api/acp/sessions/{id}/stream
  - NDJSON 流协议

Layer 2: Connector Layer (Trait)
  - IAgentConnector / AgentConnector trait
  - AcpConnector (原生 ACP)
  - ExecutorsConnector (复用 vibe-kanban)

Layer 3: Adapter Layer (Wrapper)
  - NormalizedEntry → SessionNotification 转换

Layer 4: Executor Layer
  - vibe-kanban executors crate
  - StandardCodingAgentExecutor trait
  - ACP client implementation
```

## 详细需求

### 需求 1: 复用 executors 模块

**目标**: 将 `third_party/vibe-kanban/crates/executors` 集成到我们的后端

**关键发现**: vibe-kanban 已依赖官方 `agent-client-protocol` crate：
```toml
agent-client-protocol = { version = "0.8", features = ["unstable"] }
```

**关键类型映射**:

| vibe-kanban 类型 | 来源 | 用途 |
|-----------------|------|------|
| `CodingAgent` | `executors` | 支持的执行器枚举 |
| `StandardCodingAgentExecutor` | `executors` | 核心执行器接口 |
| `AcpEvent` | `executors::acp` | 原生 ACP 事件包装 |
| `NormalizedEntry` | `executors::logs` | 非 ACP 执行器输出（需转换） |
| `agent_client_protocol::SessionNotification` | `agent-client-protocol` crate | **直接使用，无需定义** |
| `agent_client_protocol::ToolCall` | `agent-client-protocol` crate | **直接使用，无需定义** |

**适配策略**:
```rust
// 包装 vibe-kanban 的执行器，统一输出为 ACP 结构
pub struct VibeKanbanExecutorAdapter {
    inner: CodingAgent,
}

impl Executor for VibeKanbanExecutorAdapter {
    async fn execute(&self, task: &Task) -> Result<EventStream, Error> {
        // 1. 调用 vibe-kanban 执行器
        let child = self.inner.spawn(...).await?;

        // 2. 将 NormalizedEntry 流转换为 agent_client_protocol::SessionNotification 流
        child.output_stream
            .map(|entry| normalized_to_session_notification(entry))
    }
}
```

### 需求 2: ACP 数据交换（使用官方 SDK）

**目标**: 直接使用官方 ACP SDK 类型进行前后端交换

**Rust 端** (`agent-client-protocol` crate 已提供):

```rust
// 无需重新定义，直接使用官方类型
pub use agent_client_protocol::{
    SessionNotification,  // 前后端交换的统一信封
    SessionUpdate,        // 更新内容联合类型
    ContentBlock,         // 内容块
    ToolCall,             // 工具调用
    ToolCallUpdate,       // 工具调用更新
    Plan,                 // 执行计划
    // ... 其他类型
};

/// 我们只需要定义应用层包装
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AppEvent {
    /// ACP 会话更新 (直接透传 SessionNotification)
    SessionUpdate(SessionNotification),

    /// 执行完成
    Completed { summary: String },

    /// 应用层错误
    Error { message: String, code: Option<String> },

    /// 权限请求
    PermissionRequest(agent_client_protocol::RequestPermissionRequest),
}
```

**TypeScript 端** (`@agentclientprotocol/sdk` 已提供):

```typescript
// 无需重新定义，直接使用官方 SDK
import {
  SessionNotification,
  SessionUpdate,
  ContentBlock,
  ToolCall,
  ToolCallUpdate,
  Plan,
} from "@agentclientprotocol/sdk";

// WebSocket 消息类型
interface WebSocketMessage {
  type: "SessionUpdate" | "Completed" | "Error" | "PermissionRequest";
  payload: SessionNotification | CompletedPayload | ErrorPayload | PermissionRequest;
}
```

### 需求 3: 实现 Connector Trait 层

**目标**: 定义统一的执行器连接接口

**接口设计** (`src/executor/connector.rs`):

```rust
/// 执行器连接器接口
#[async_trait]
pub trait AgentConnector: Send + Sync {
    /// 连接器唯一标识
    fn connector_id(&self) -> &str;

    /// 协议标识
    fn protocol_id(&self) -> &str;

    /// 执行提示
    async fn prompt(
        &self,
        session_id: &str,
        prompt: &str,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError>;

    /// 继续已有会话
    async fn continue_session(
        &self,
        session_id: &str,
        prompt: &str,
        state: SessionState,
    ) -> Result<ExecutionStream, ConnectorError>;

    /// 取消执行
    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError>;
}

/// 执行上下文
pub struct ExecutionContext {
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub executor_config: ExecutorConfig,
}

/// 执行事件流 (直接返回官方 SessionNotification)
pub type ExecutionStream = Pin<Box<dyn Stream<Item = Result<agent_client_protocol::SessionNotification, ConnectorError>> + Send>>;
```

**两种连接器实现**:

1. **AcpConnector** (`src/executor/connectors/acp.rs`)
   - 直接连接 ACP 原生 Agent
   - 参考 ABCCraft 的 `AcpAgentConnector`
   - 支持进程池和会话亲和性

2. **ExecutorsConnector** (`src/executor/connectors/executors.rs`)
   - 包装 vibe-kanban executors
   - 将 `NormalizedEntry` 转换为 `AcpEvent`
   - 支持所有 vibe-kanban 执行器类型

### 需求 4: 实现 NormalizedEntry → SessionNotification 转换器

**目标**: 将 vibe-kanban 的 `NormalizedEntry` 转换为官方 `agent_client_protocol::SessionNotification`

**转换映射** (`src/executor/adapters/normalized_to_acp.rs`):

```rust
use agent_client_protocol::{
    SessionNotification, SessionUpdate, ContentBlock, TextContent,
    ToolCall, ToolCallUpdate, ToolKind, ToolCallStatus, ToolCallContent,
    Plan, PlanEntry,
};
use executors::logs::{NormalizedEntry, NormalizedEntryType, ActionType};

/// 将 NormalizedEntry 转换为 SessionNotification
pub fn normalize_entry_to_notification(
    session_id: String,
    entry: NormalizedEntry
) -> Vec<SessionNotification> {
    match entry.entry_type {
        NormalizedEntryType::UserMessage => {
            vec![SessionNotification {
                session_id,
                update: SessionUpdate::UserMessageChunk {
                    content: ContentBlock::Text {
                        text: TextContent { value: entry.content },
                    },
                },
                ..Default::default()
            }]
        }

        NormalizedEntryType::AssistantMessage => {
            vec![SessionNotification {
                session_id,
                update: SessionUpdate::AgentMessageChunk {
                    content: ContentBlock::Text {
                        text: TextContent { value: entry.content },
                    },
                },
                ..Default::default()
            }]
        }

        NormalizedEntryType::Thinking => {
            vec![SessionNotification {
                session_id,
                update: SessionUpdate::AgentThoughtChunk {
                    content: ContentBlock::Text {
                        text: TextContent { value: entry.content },
                    },
                },
                ..Default::default()
            }]
        }

        NormalizedEntryType::ToolUse { tool_name, action_type, status } => {
            // 将 ToolUse 转换为 ToolCall/ToolCallUpdate
            let (tool_call, update) = action_to_tool_call(tool_name, action_type, status);
            vec![
                SessionNotification {
                    session_id: session_id.clone(),
                    update: SessionUpdate::ToolCall(tool_call),
                    ..Default::default()
                },
                SessionNotification {
                    session_id,
                    update: SessionUpdate::ToolCallUpdate(update),
                    ..Default::default()
                },
            ]
        }

        NormalizedEntryType::PlanPresentation { plan } => {
            vec![SessionNotification {
                session_id,
                update: SessionUpdate::Plan {
                    plan: Plan {
                        entries: parse_plan_entries(&plan),
                        ..Default::default()
                    },
                },
                ..Default::default()
            }]
        }

        // ... 其他类型
    }
}

/// 将 ActionType 转换为 ToolCall (使用官方类型)
fn action_to_tool_call(
    tool_name: String,
    action: ActionType,
    status: executors::logs::ToolStatus,
) -> (ToolCall, ToolCallUpdate) {
    let tool_call_id = generate_tool_call_id();

    let (kind, title, content, raw_input) = match action {
        ActionType::FileRead { path } => {
            let content = vec![ToolCallContent::Content {
                content: ContentBlock::Text {
                    text: TextContent {
                        value: format!("Reading {}...", path),
                    },
                },
            }];
            (
                ToolKind::Read,
                format!("Read {}", path),
                content,
                Some(json!({ "path": path })),
            )
        }

        ActionType::FileEdit { path, changes } => {
            let content = changes.iter().map(|c| ToolCallContent::Diff {
                diff: agent_client_protocol::Diff {
                    path: path.clone(),
                    old_text: Some(c.old_text.clone()),
                    new_text: c.new_text.clone(),
                },
            }).collect();
            (
                ToolKind::Edit,
                format!("Edit {}", path),
                content,
                Some(json!({ "path": path, "changes": changes })),
            )
        }

        ActionType::CommandRun { command, .. } => {
            (
                ToolKind::Execute,
                format!("Execute: {}", command),
                vec![],
                Some(json!({ "command": command })),
            )
        }

        ActionType::Search { query } => {
            (
                ToolKind::Search,
                format!("Search: {}", query),
                vec![],
                Some(json!({ "query": query })),
            )
        }

        // ... 其他 ActionType
    };

    let tool_call = ToolCall {
        tool_call_id: tool_call_id.clone(),
        kind,
        title,
        status: map_tool_status(status),
        content,
        raw_input,
        raw_output: None,
        ..Default::default()
    };

    let update = ToolCallUpdate {
        tool_call_id,
        status: None, // 后续更新
        content: None,
        raw_output: None,
        ..Default::default()
    };

    (tool_call, update)
}
```

### 需求 5: 实现 WebSocket/NDJSON 流端点

**目标**: 提供前端可以连接的流式端点

**API 设计** (`src/api/sessions.rs`):

```rust
/// WebSocket 流端点
#[get("/api/acp/sessions/{session_id}/stream")]
async fn acp_session_stream(
    path: web::Path<String>,
    connector: web::Data<Arc<dyn AgentConnector>>,
) -> Result<HttpResponse, Error> {
    let session_id = path.into_inner();

    // 建立 WebSocket 连接
    ws::start(
        AcpSessionWebSocket {
            session_id,
            connector: connector.get_ref().clone(),
        },
        &req,
        stream,
    )
}

/// NDJSON 流端点 (HTTP SSE 替代方案)
#[post("/api/sessions/{session_id}/prompt/stream")]
async fn prompt_stream(
    path: web::Path<String>,
    body: web::Json<PromptRequest>,
    connector: web::Data<Arc<dyn AgentConnector>>,
) -> Result<HttpResponse, Error> {
    let session_id = path.into_inner();

    // 获取执行流
    let mut stream = connector.prompt(&session_id, &body.prompt, context).await?;

    // 转换为 NDJSON 流 (直接序列化 SessionNotification)
    let stream = async_stream::stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok(notification) => {
                    // SessionNotification 直接序列化为 JSON
                    let json = serde_json::to_string(&notification)?;
                    yield Ok::<_, Error>(Bytes::from(format!("{}\n", json)));
                }
                Err(e) => {
                    // 错误包装为应用层消息
                    let error_msg = serde_json::json!({
                        "type": "Error",
                        "message": e.to_string()
                    });
                    yield Ok::<_, Error>(Bytes::from(format!("{}\n", error_msg)));
                }
            }
        }
    };

    Ok(HttpResponse::Ok()
        .content_type("application/x-ndjson")
        .streaming(stream))
}
```

### 需求 6: 进程管理说明

**vibe-kanban 现状**: `AcpAgentHarness` 每次 spawn 都创建新进程，**没有进程池**。

**ABCCraft 进程池作用**: 会话亲和性（同 session 路由到同进程）、弹性伸缩、空闲超时释放。

**MVP 策略**: 暂不实现进程池，直接使用 vibe-kanban 的 spawn 模式 + `SessionManager` 持久化。

### 需求 7: 前端集成

**目标**: 前端使用已有的 ACP 组件进行渲染

**已有支持** (无需重复开发):
- `useAcpStream.ts` - WebSocket 连接管理
- `AcpSessionEntry.tsx` - 会话条目渲染
- `AcpToolCallCard.tsx` - 工具调用卡片
- `AcpMessageCard.tsx` - 消息卡片
- `AcpPlanCard.tsx` - 计划卡片

**API 客户端** (`frontend/src/services/executor.ts`):

```typescript
import type { SessionNotification } from "@agentclientprotocol/sdk";

export class ExecutorService {
  /**
   * 创建流式执行会话
   * 直接接收 SessionNotification JSON，使用官方 SDK 类型
   */
  static async executeStream(
    sessionId: string,
    prompt: string,
    options: ExecuteOptions,
    handlers: StreamHandlers
  ): Promise<() => void> {
    const ws = new WebSocket(`/api/acp/sessions/${sessionId}/stream`);

    ws.onmessage = (event) => {
      const data = JSON.parse(event.data);

      // 直接反序列化为官方 SDK 类型
      if (data.sessionId && data.update) {
        // 这是 SessionNotification (来自官方 ACP SDK 结构)
        handlers.onSessionNotification?.(data as SessionNotification);
      } else if (data.type === 'Error') {
        handlers.onError?.(data);
      } else if (data.type === 'Completed') {
        handlers.onCompleted?.(data);
      }
    };

    // 发送执行请求
    ws.onopen = () => {
      ws.send(JSON.stringify({
        type: 'Execute',
        prompt,
        options
      }));
    };

    return () => ws.close();
  }
}
```

## 文件结构

```
backend/
├── Cargo.toml                           # 依赖 agent-client-protocol = "0.9.4"
└── src/
    └── executor/                        # 执行集成层
        ├── mod.rs                       # 模块入口
        ├── connector.rs                 # AgentConnector trait
        ├── connectors/
        │   ├── mod.rs
        │   ├── acp_native.rs            # 原生 ACP 连接器 (直接返回 SessionNotification)
        │   └── vibe_kanban.rs           # vibe-kanban 包装器 (NormalizedEntry → SessionNotification)
        └── adapters/
            ├── mod.rs
            └── normalized_to_acp.rs     # NormalizedEntry → SessionNotification 转换

frontend/src/
├── features/
│   └── acp-session/                   # 已有的 ACP 会话模块
│       ├── model/
│       │   ├── types.ts               # 扩展官方 SDK 类型
│       │   ├── useAcpStream.ts        # WebSocket 流处理
│       │   └── useAcpSession.ts
│       └── ui/
│           ├── AcpSessionEntry.tsx    # 直接渲染 SessionNotification
│           └── ...
└── services/
    └── executor.ts                    # 执行服务 API 客户端
```

## 实现优先级

### Phase 1: 基础接口和连接器 trait (MVP)
- [ ] 添加 `agent-client-protocol` crate 依赖
- [ ] 定义 `AgentConnector` trait
- [ ] 实现 `NormalizedEntry` → `SessionNotification` 基础转换

### Phase 2: vibe-kanban 集成
- [ ] 创建 `ExecutorsConnector` 包装器
- [ ] 集成 `CodingAgent` 枚举
- [ ] 实现配置映射

### Phase 3: ACP 原生支持
- [ ] 实现 `AcpConnector` (使用 `AcpAgentHarness`)
- [ ] 复用 `SessionManager` 进行会话持久化

### Phase 4: API 层
- [ ] WebSocket 端点
- [ ] NDJSON 流端点
- [ ] 权限请求处理

### Phase 5: 前端集成
- [ ] API 客户端服务
- [ ] 与现有 ACP 组件集成
- [ ] 端到端测试

## 验收标准

- [ ] vibe-kanban executors 成功集成并可运行
- [ ] Claude Code 执行器输出正确转换为 `agent_client_protocol::SessionNotification`
- [ ] 前端能够使用 `@agentclientprotocol/sdk` 类型直接反序列化后端消息
- [ ] 支持多种执行器类型（至少 Claude Code + 一个 ACP 原生）
- [ ] WebSocket 流稳定运行，直接透传 `SessionNotification` JSON
- [ ] 会话状态通过 `SessionManager` 正确持久化
- [ ] 前后端类型完全匹配，无需自定义中间层类型

## 依赖

### Rust 依赖 (backend/Cargo.toml)
```toml
[dependencies]
# 官方 ACP SDK
agent-client-protocol = "0.9.4"

# vibe-kanban executors (本地路径)
executors = { path = "../third_party/vibe-kanban/crates/executors" }

# WebSocket
actix-web-actors = "4"

# 流处理
futures = "0.3"
async-stream = "0.3"

# JSON
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 并发
tokio = { version = "1", features = ["sync", "time"] }
```

### TypeScript 依赖 (frontend/package.json)
```json
{
  "dependencies": {
    "@agentclientprotocol/sdk": "^0.14.1"
  }
}
```

### 版本匹配说明
- Rust: `agent-client-protocol = "0.9.4"` 
- TypeScript: `@agentclientprotocol/sdk = "^0.14.1"`

两者都由官方维护，类型定义同步更新，确保序列化后的 JSON 完全兼容。

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|-----|------|---------|
| vibe-kanban API 变更 | 中 | 创建 shim 层隔离变化 |
| ACP SDK 版本不兼容 | 中 | 锁定版本，定期升级测试 |
| 进程池资源泄漏 | 高 | 实现 Drop 守卫，健康检查 |
| 大规模并发性能 | 中 | 实现背压机制，限制并发数 |

## 参考

### 官方资源
- **ACP 协议仓库**: https://github.com/agentclientprotocol/agent-client-protocol
- **Rust Crate**: https://crates.io/crates/agent-client-protocol
- **npm SDK**: `@agentclientprotocol/sdk`

### 本地参考
- **vibe-kanban executors**: `third_party/vibe-kanban/crates/executors/` (已集成官方 crate)
- **ABCCraft ACP 实现**: `references/ABCCraft/src/backend/ABCCraft.Infrastructure.Agents/Acp/`
- **前端 ACP 组件**: `frontend/src/features/acp-session/` (已使用官方 SDK)
- **ACP Schema**: `third_party/agent-client-protocol/schema/schema.json`
