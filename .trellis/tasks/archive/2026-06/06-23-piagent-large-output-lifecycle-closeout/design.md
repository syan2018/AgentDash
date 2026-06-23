# PiAgent 大输出 lifecycle 链路收口设计

## Architecture Decision

工具结果大 body 的 runtime ref 使用单一 stable item id：

```text
{turn_id}:{tool_call_id}
```

该 id 作为以下对象的共同坐标：

- `AgentToolResult.details.lifecycle_path`
- bounded preview 文本中的 `lifecycle_path`
- `SessionToolResultCache` key 的 `item_id`
- stream mapper 产出的 tool ThreadItem id
- lifecycle VFS 的 `session/tool-results/{item_id}` 目录

选择 `{turn_id}:{tool_call_id}` 的原因是它能在 producer 边界提前生成。`entry_index` 属于 stream mapper 的展示状态，AgentLoop 在模型请求前无法可靠知道它；把 `entry_index` 放入生命周期 ref 会让 cache path 与模型可见 path 难以一致。

## Data Flow

```text
PiAgent tool execute
  -> AgentToolResult oversized
  -> bound_agent_tool_result_text(session_id, turn_id, tool_call_id)
  -> SessionToolResultCache.put_text(session_id, "{turn_id}:{tool_call_id}", original_body)
  -> bounded AgentToolResult.content + details.truncation + lifecycle_path
  -> ToolExecutionEnd / AgentMessage::ToolResult / provider follow-up
  -> stream_mapper maps same stable item id into Backbone ThreadItem
  -> SessionEvent persists bounded ThreadItem
  -> lifecycle_vfs metadata reads persisted bounded event + shared cache status
  -> lifecycle_vfs result.txt reads shared cache body or bounded miss/expired status
```

Projection and resume remain intentionally one-way:

```text
SessionEvent bounded fact -> projection/resume/continuation
```

They do not read `result.txt`, because resume must reconstruct what the model actually saw.

## Boundaries

### agentdash-agent

`agentdash-agent` owns producer-side bounding but should not depend on application services. Add a small callback surface to `AgentLoopConfig`, for example:

```rust
pub struct ToolResultRefContext {
    pub session_id: String,
    pub turn_id: String,
    pub cache_writer: Option<ToolResultCacheWriter>,
}
```

The writer receives:

- `session_id`
- stable `item_id`
- `lifecycle_path`
- original text
- original bytes

If no context is configured, bounded preview still works and cache write is skipped. Tests can exercise both isolated and wired modes.

### agentdash-executor / PiAgentConnector

`PiAgentConnector::prompt` already has `session_id` and `context.session.turn_id`. It should set the current turn ref context on the `Agent` before calling `agent.prompt(...)`. The context must be refreshed every turn, including reused hot agents.

`stream_mapper` must use the same stable item id for all tool item lifecycle events. Assistant message / reasoning synthetic chunk ids may continue using `entry_index`.

### agentdash-application

`SessionToolResultCache` remains in application layer. It is the runtime-scoped source for tool result body reads.

Lifecycle VFS reads metadata from persisted bounded ThreadItem plus cache status. It reads body only from `SessionToolResultCache`.

### bootstrap

Create one shared `Arc<SessionToolResultCache>` per runtime process and pass it to:

- PiAgent connector cache writer setup
- `LifecycleMountProvider`

`MountProviderRegistryBuilder::with_builtins` should accept the shared cache. Production construction must not silently allocate a separate empty cache for lifecycle VFS.

## Terminal Scope

Terminal output remains bounded in live event and durable event paths. This task does not require terminal `.log` to recover full PTY output. If terminal log body is improved in this task, it must use an explicit bounded retained source and preserve the same miss/status behavior.

## Compatibility

The project is pre-release, so no legacy id alias or dual lookup is needed. Tests and specs should be updated to the single stable id contract.

## Rollback

The rollback point is localized:

- Revert `AgentLoopConfig` ref context additions.
- Revert stream mapper tool item id change.
- Revert bootstrap shared cache injection.

No database rollback is required.
