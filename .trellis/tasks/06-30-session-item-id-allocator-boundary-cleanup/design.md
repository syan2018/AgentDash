# Session item id allocator 边界清理设计

## Current Boundary Problem

当前实现把 session scoped readable item id 分配放在 `agentdash-agent` 的 `agent_loop.rs` 内。这个位置可以让 tool result bounding 直接拿到 `item_id` 与 `lifecycle_path`，但它也让 AgentLoop 承担了上层会话投影职责：

- `turn_###:tool_###` / `turn_###:cmd_###` 是 Backbone `ThreadItem.id`。
- 同一个 id 是 `SessionToolResultCache` key 的一部分。
- 同一个 id 被编码进 `lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt`。
- 冷启动恢复时，这些 id 必须从持久化 transcript 或 projection 事实中恢复 watermark。

这些职责属于 session runtime identity / projection 边界，而不是通用 agent execution loop。Pi connector 当前的 hydrate 修复是正确止血点，但 connector 不应长期解析 transcript JSON 来恢复 allocator state。

## Target Ownership

### agentdash-agent

保留 tool execution、tool result bounding 和模型可见 bounded text 生成。它只依赖一个抽象的 tool result address provider：

```rust
pub trait ToolResultAddressProvider: Send + Sync {
    fn tool_result_ref(
        &self,
        raw_turn_id: &str,
        raw_tool_call_id: &str,
        tool_name: &str,
    ) -> ToolResultRef;
}
```

`ToolResultRef` 可以继续包含 `item_id`、`lifecycle_path`、raw trace 和 body kind，但 `agentdash-agent` 不负责决定这些字段的格式，也不负责从历史字符串恢复 watermark。AgentLoop 只把返回值传给 bounded result、cache writer 和 event details。

### session runtime identity module

新增或收束一个 session item identity 模块，优先落在 `agentdash-executor` 中靠近 PiAgent runtime/session restore 的位置。该模块拥有：

- readable alias 计数器和 raw id 到 alias 的映射。
- `tool` 与 `cmd` 分类规则，当前 `shell_exec` 对应 command。
- terminal alias 分配。
- tool result lifecycle path 组装。
- 从 restored transcript / projection facts 派生 watermark 的逻辑。
- typed restored allocator state，例如 `SessionItemIdWatermark` 或 `RestoredSessionItemIdentityState`。

### Pi connector

Pi connector 的职责收束为 runtime 编排：

1. 从 `ExecutionTurnFrame.restored_session_state` 获取 repository restore 结果。
2. 调用 session identity 模块获得 allocator 或 restored allocator state。
3. 创建/复用 PiAgent runtime，并通过 `ToolResultRefContext` 注入 allocator/address provider。

connector 不再内联解析 `details.readable_ref.item_id` 或 `details.lifecycle_path`。这类解析若仍然是恢复事实来源，应放在 session identity 模块内，并通过单元测试覆盖。

### stream mapper

stream mapper 继续把 tool result ref 投影为 Backbone item id。它不生成新的 tool result id，只读取事件或 details 中的 item id。映射后必须保持：

```text
ThreadItem.id == details.readable_ref.item_id == lifecycle_path embedded item id
```

## Data Flow

冷启动恢复路径：

```text
repository events
  -> RestoredSessionState
  -> SessionItemIdentity::from_restored_state(...)
  -> PiAgent runtime state
  -> ToolResultRefContext { address_provider, cache_writer, raw_turn_id }
  -> AgentLoop tool result bounding
  -> AgentToolResult details + lifecycle_path
  -> stream mapper
  -> Backbone ThreadItem
```

hot runtime 路径：

```text
existing PiAgent runtime
  -> same SessionItemIdentity allocator
  -> refreshed ToolResultRefContext for current raw turn
  -> next tool result uses next session scoped id
```

## Contracts

- The allocator stores the highest used turn, tool, command, and terminal aliases for one session.
- Observing restored facts advances counters to the maximum seen value; allocation increments before formatting the next alias.
- The allocator treats malformed historical item ids as absent facts rather than failing restore.
- A raw turn id maps to one turn alias within a runtime session.
- A raw tool call id maps to one body alias per body kind.
- `shell_exec` maps to command alias `cmd_###`; other tools map to `tool_###` unless the tool taxonomy changes in the same task.
- `lifecycle_path` remains model-visible runtime address text and is not durable truth by itself.

## Migration Shape

This is a pre-release project, so the refactor should update APIs directly instead of adding compatibility shims. The preferred sequence is:

1. Introduce the new identity module and tests while keeping behavior identical.
2. Move alias formatting/parsing and allocator state out of `agent_loop.rs`.
3. Replace `ReadableIdRegistry` references in `ToolResultRefContext`, connector runtime state, stream mapper context and tests.
4. Move restored-state hydration helper out of Pi connector into the identity module.
5. Update specs to describe the new ownership.

## Tradeoffs

Keeping the allocator inside `agentdash-agent` is simpler locally but leaks session projection concerns into the execution loop and makes cold-start restore awkward. Moving it into session runtime adds one explicit dependency edge through a provider trait, but it places persistence restore, cache key shape, lifecycle VFS path shape and Backbone item identity in the same conceptual module.

Keeping JSON scanning inside Pi connector minimizes files touched but leaves restore semantics hidden in connector glue. A typed identity restore module makes tests and future non-Pi runtimes clearer.

## Rollback

Rollback is straightforward because this task should preserve wire/projection behavior. If a refactor step causes broad failures, revert that step and keep the existing bugfix behavior from commit `af8498111`; then split the allocator extraction into smaller PRs.
