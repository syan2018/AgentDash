# 按 Codex 协议处理来源会话标题 - Design

## Architecture

Backbone protocol 是 Codex app-server protocol 的业务超集。Codex 已有 typed thread title 消息时，AgentDash 应在 Backbone 中增加对应 typed event，而不是把它压成 `SessionMetaUpdate { key, value }`。

标题事实仍归 session application 层管理。Connector 只负责把来源协议消息翻译为 Backbone event；Session eventing / title service 负责校验、投影、持久化与广播。

## Contracts

### Backbone Platform Event

新增 typed platform event：

```rust
PlatformEvent::SourceSessionTitleUpdated {
    executor_session_id: Option<String>,
    title: String,
    preview: Option<String>,
    source: String,
}
```

字段语义：

- `executor_session_id`：来源执行器会话 ID，例如 Codex thread id。Application 可用它防止跨 session 误投影。
- `title`：来源系统给出的 user-facing title。
- `preview`：来源系统的 preview。若 `title.trim() == preview.trim()`，该事件不应覆盖标题。
- `source`：来源标识，例如 `codex`。

### Title Source

扩展 `TitleSource`：

```text
auto | source | user
```

优先级：

```text
user > source > auto
```

`source` 表示标题来自外部协议/source 的已存在元数据，不表示 provider 生成标题。

### CodexBridge Mapping

CodexBridge 在以下位置映射来源标题：

- `ThreadStartResponse.thread.name`
- `ThreadForkResponse.thread.name`
- `thread/name/updated` notification

可选映射 `thread/started` notification 中的 `thread.name`，但要避免同一 accepted 边界重复写入同一标题。

## Data Flow

```text
Codex Thread.name
  -> CodexBridge PlatformEvent::SourceSessionTitleUpdated
  -> SessionTurnProcessor persists event
  -> SessionEventingService projects event into SessionMeta
  -> SessionMeta.title_source = source
  -> SessionMetaUpdate(session_meta_updated) broadcast
```

执行器不具备来源标题能力时：

```text
TurnCommitter
  -> derive_session_title(first_user_prompt)
  -> SessionMeta.title_source = auto
```

后续来源标题到达：

```text
if current title_source != user:
  source title may replace auto/source
```

## Persistence And Frontend

- PostgreSQL / SQLite `title_source` 仍使用现有文本列，不新增字段。
- parser/writer 支持 `source`。
- 前端 `TitleSource` union 支持 `"source"`，session meta update handler 接受该值。

## Trade-Offs

- 不把 `Thread.name` 塞到 generic `SessionMetaUpdate`，因为那会隐藏协议语义并让 connector 越权表达业务 meta mutation。
- 不把 title metadata 放进 `AgentConnector::prompt` 返回值，本次先保持 Codex protocol message 通过 ExecutionStream/Backbone 进入统一事件流水线；这样更符合“Backbone 是协议超集”的方向。
- Connector capability 使用 `supports_source_session_title` 表达“执行器会通过协议事件提供标题”。它不是 `supports_title_generation`，因为业务层不要求或触发 provider 生成标题。

## Rollback

若实现后发现协议投影行为异常，可回退新增 typed event、CodexBridge 映射与 `source` title source；本地 `auto` 派生逻辑仍可独立工作。
