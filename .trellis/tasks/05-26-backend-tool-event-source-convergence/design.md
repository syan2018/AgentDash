# 设计：后端工具事件事实源收束

## Architecture

目标后端链路：

```text
AgentDash-owned agent / embedded tools
  -> AgentEvent::ToolExecution*
  -> pi_agent stream mapper
  -> AgentDashThreadItem
       ├─ Codex(codex::ThreadItem)
       └─ AgentDash(AgentDashNativeThreadItem)
  -> BackboneEnvelope
```

legacy vibe-kanban 链路：

```text
vibe-kanban executor log patch
  -> legacy NormalizedEntry parser
  -> legacy ActionType adapter
  -> codex ThreadItem builders
  -> AgentDashThreadItem::Codex
  -> BackboneEnvelope
```

两条链路都进入 Backbone。AgentDash 主链路以 Codex Protocol 为默认类型来源；
Codex 已经覆盖的 item、状态和通知语义直接复用，Codex 未覆盖的 AgentDash 自有
工具事实通过 `AgentDashNativeThreadItem` 做加法扩展。

## Boundaries

### `agentdash-agent-types`

`agentdash-agent-types::protocol` 是 AgentDash 运行协议的类型出口：

```rust
pub enum AgentDashThreadItem {
    Codex(codex::ThreadItem),
    AgentDash(AgentDashNativeThreadItem),
}

pub enum AgentDashNativeThreadItem {
    FsRead { ... },
    FsGrep { ... },
    FsGlob { ... },
}
```

`CommandExecutionStatus`、`DynamicToolCallStatus`、`McpToolCallStatus`、
`PatchApplyStatus`、`DynamicToolCallOutputContentItem` 等状态与输出片段从 Codex
Protocol re-export。这样应用层处理状态时只有一个语义来源。

`AgentToolResult.details` 保持为工具私有 metadata 通道。ThreadItem 的主分类由
工具名称与结构化参数在 mapper 中直接落到 `AgentDashThreadItem`，避免把同一事实先
包装成一套中间 details enum 再投影。

### `agentdash-agent-protocol`

`BackboneEvent::ItemStarted` / `ItemCompleted` 使用 AgentDash 自有 item 通知：

```rust
pub struct ItemStartedNotification {
    pub item: AgentDashThreadItem,
    pub thread_id: String,
    pub turn_id: String,
    pub started_at_ms: i64,
}
```

Codex bridge 收到 Codex 原生通知时用 `from_codex` 包装成
`AgentDashThreadItem::Codex`。AgentDash 自有 connector 直接构造
`AgentDashThreadItem`。

`backbone::thread_item` builder 继续作为 Codex `ThreadItem` 构造 API，集中处理 Codex
内部 path 类型、serde wire shape 与状态枚举。

### `agentdash-executor`

`pi_agent::stream_mapper` 的工具 item 映射规则：

- `shell_exec` -> `AgentDashThreadItem::Codex(ThreadItem::CommandExecution)`。
- `fs_apply_patch` -> `AgentDashThreadItem::Codex(ThreadItem::FileChange)`。
- `fs_read` -> `AgentDashThreadItem::AgentDash(FsRead)`。
- `fs_grep` -> `AgentDashThreadItem::AgentDash(FsGrep)`。
- `fs_glob` -> `AgentDashThreadItem::AgentDash(FsGlob)`。
- 其他工具 -> `AgentDashThreadItem::Codex(ThreadItem::DynamicToolCall)`。

legacy vibe-kanban 代码收束到 `vibe_kanban_legacy_log_mapper.rs`：

- `VibeKanbanLogToBackboneConverter` 承接 legacy normalized log。
- `legacy_action_type_to_thread_item` 只负责 `ActionType` 到 Codex `ThreadItem` 的边界投影。
- `executor_session.rs` 从该 legacy mapper 引用 converter。

## Data Fidelity

legacy dynamic fallback 统一走：

```rust
fn legacy_dynamic_tool_call(
    item_id,
    tool,
    arguments,
    status,
    fallback_content,
    success,
) -> ThreadItem
```

`fallback_content` 非空时进入 `content_items`，保证 legacy `FileRead` / `WebFetch` /
`TaskCreate` / `Other` 的可见输出进入 Backbone。

AgentDash native read/search/list item 保留：

- 原始 `arguments`。
- Codex `DynamicToolCallStatus`。
- Codex `DynamicToolCallOutputContentItem` 输出片段。
- `success` 终态。
- read/grep/glob 的高频结构化字段，用于后续 application 与前端直接消费。

## TypeScript

生成后的前端类型形态应为：

```ts
export type AgentDashThreadItem = ThreadItem | AgentDashNativeThreadItem;
export type AgentDashNativeThreadItem =
  | { type: "fsRead"; ... }
  | { type: "fsGrep"; ... }
  | { type: "fsGlob"; ... };
```

前端 P3 之后消费 `BackboneEvent::item_started/item_completed.payload.item`，其中 Codex
原生 item 与 AgentDash native item 都是同一条 Backbone item 生命周期事件。

## Validation

- 当前协议命名、状态类型与 item union 均以 Codex 优先和 AgentDash native 加法为准。
- `ActionType` / `NormalizedEntry` 生产引用集中在 vibe-kanban legacy mapper。
- `pi_agent` 覆盖 `shell_exec`、`fs_read`、`fs_grep`、`fs_glob` 的 item 映射。
- TypeScript binding 重新生成。
- cargo 测试覆盖 agent-types / agent-protocol / agent-executor。
