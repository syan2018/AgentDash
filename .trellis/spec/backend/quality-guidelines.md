# Quality Guidelines

> AgentDash 后端质量规范，包含跨层 DTO 契约。

---

## Overview

- **Linting**: Clippy (Rust)
- **格式化**: rustfmt
- **检查命令**: `cargo check`, `cargo clippy`
- **API DTO 原则**: AgentDash 业务 HTTP JSON 默认使用 `snake_case`

代码提交前必须通过格式化和基础检查；新增或修改跨层 DTO 时，必须同时核对前端类型与序列化输出是否一致。

---

## Forbidden Patterns

| 禁止模式 | 原因 | 替代方案 |
|----------|------|----------|
| `unwrap()` | 可能导致 panic | 使用 `?` 或 `match` |
| `panic!()` | 不可恢复错误 | 返回 `Result` |
| 裸 `std::sync::Mutex` | 可能死锁 | 使用 `tokio::sync::Mutex`（异步） |
| 业务 HTTP DTO 混用 `camelCase` / `snake_case` | 破坏前后端字段契约 | 统一 `#[serde(rename_all = "snake_case")]` |
| 前端 mapper 兼容"旧字段 + 新字段" | 掩盖后端契约错误 | 先修正后端 DTO |

---

## Required Patterns

- 异步函数使用 `async/await`
- 共享状态使用 `Arc<Mutex<T>>`
- 错误类型实现 `thiserror::Error`
- AgentDash 自有业务 HTTP DTO 字段名使用 `snake_case`
- 外部协议桥接数据保持上游协议原样，不在桥接层擅自改名

### 外部协议桥接例外

ACP 协议对象、第三方 SDK 透传、明确标注为"桥接层"的响应对象允许保留外部字段风格。

### RMCP Tool Router 宏约定

使用 `rmcp` 的 `#[tool_router]` / `#[tool_handler]` 时，server struct 不需要持有 `ToolRouter<Self>` 字段；宏会生成 `Self::tool_router()` 关联方法。由于 `rmcp-macros` 的 `#[tool_handler]` 默认 router 表达式是 `self.tool_router`，无字段写法必须显式指定 `#[tool_handler(router = Self::tool_router())]`，让工具注册与分发使用 `#[tool_router]` 生成的关联方法。

```rust
// ✅ Correct：只保留业务状态，测试或 schema 校验可直接调用 Self::tool_router()
#[derive(Clone)]
pub struct StoryMcpServer {
    services: Arc<McpServices>,
    story_id: Uuid,
}

#[tool_router]
impl StoryMcpServer {
    #[tool(description = "获取当前 Story 的完整上下文信息")]
    async fn get_story_context(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // ...
    }
}

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for StoryMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}
```

```rust
// ❌ Wrong：实例字段不会被业务代码读取，会触发 dead_code warning
#[derive(Clone)]
pub struct StoryMcpServer {
    services: Arc<McpServices>,
    story_id: Uuid,
    tool_router: ToolRouter<Self>,
}
```

测试工具注册或 schema 时应使用 `StoryMcpServer::tool_router().list_all()` 或 `get_tool()`，而不是在实例中缓存一份 router。

---

## Scenario: API JSON 字段命名统一

### Contracts

- Response JSON：所有层级 `snake_case`
- Request JSON：前端发 `snake_case`，后端按 `snake_case` 反序列化
- 不允许：顶层 `snake_case` 内层 `camelCase`、前端 `fooBar ?? foo_bar` 长期兼容

### Good / Bad

```rust
// ✅ Good
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectAgentSummaryResponse {
    pub display_name: String,
    pub shared_context_mounts: Vec<ProjectAgentMountResponse>,
}
```

```ts
// ❌ Bad — 后端契约失效，前端被迫双风格
const mountId = raw.mountId ?? raw.mount_id ?? "";

// ✅ Correct — 后端保证 snake_case
const mountId = String(raw.mount_id ?? "");
```

---

## Session 执行状态持久化

### 原则

Session 执行状态必须持久化在 `SessionMeta.last_execution_status`，不允许靠扫 JSONL 历史或读内存 map 推断。

```rust
// ❌ 扫 JSONL 历史推断状态
let history = store.read_all(session_id).await?;

// ❌ 纯内存 map
let running = sessions.lock().await.get(id).map(|r| r.running);

// ✅ 每次 turn 开始/结束时写入 meta
session_meta.last_execution_status = "running".to_string();
store.write_meta(&session_meta).await?;
```

### SessionMeta 写回约束

- `last_event_seq`、`last_execution_status`、`last_turn_id`、`last_terminal_message` 是事件投影字段，只能单调前进
- `save_session_meta()` 更新普通元信息时，底层必须按"合并"语义处理，保证投影字段不回退

### 冷启动 continuation

- `SessionRuntimeRegistry` 中有 runtime entry / broadcaster 不等于 connector 仍有 live session。
- 冷启动 continuation 以 `connector.has_live_session(session_id)` 判断是否需要恢复。
- 仓储恢复：connector 支持原生恢复时用 `restored_session_state`，否则退化为 continuation `system_context`

### 合法值

`last_execution_status` 只有五个合法值：`idle` / `running` / `completed` / `failed` / `interrupted`。

---

## Session Context 注入架构

### system_context vs prompt_blocks

| 字段 | 用途 | 展示 |
|------|------|------|
| `system_context` | owner 级上下文，每轮随 system prompt 注入 Agent | 不出现在用户消息流 |
| `prompt_blocks` resource block | `agentdash://project-context/` URI，仅前端展示锚点 | 渲染为 AcpOwnerContextCard |

**禁止**：在 `prompt_blocks` 中放 instruction text block；在用户消息文本中暴露技术 slot 标识。

### LaunchCommand / Launch Payload 字段变更

新增启动字段时先判定归属，再同步对应入口和测试：

| 归属 | 放置位置 | 典型字段 |
|---|---|---|
| 来源意图 | `LaunchCommand` source payload | source identity、parent/session 引用、override、follow-up hint |
| 构建事实 | `SessionConstructionPlan` | owner、workspace、working directory、VFS、MCP、capability、context、identity |
| 单轮执行策略 | `LaunchExecution` | resolved prompt payload、lifecycle、restore、hook、runtime command、terminal effect |
| Connector 投影 | `ExecutionContext` | session frame / turn frame 字段 |

同步检查 HTTP、local relay、task、workflow、routine、companion、hook auto-resume 入口。

---

## Code Review Checklist

- [ ] 无 `unwrap()` 或已标记为安全
- [ ] 错误处理完善
- [ ] 异步函数正确使用 `.await`
- [ ] 业务 HTTP DTO 输出为 `snake_case`

---

*更新：2026-04-14 — 精简 Session 持久化/Prompt Lifecycle 冗余描述，保留核心约束*
