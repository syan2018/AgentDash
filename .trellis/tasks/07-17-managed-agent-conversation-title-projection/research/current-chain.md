# 会话标题链路研究

## 研究范围

- 当前分支：`codex/agent-runtime-architecture-convergence`
- 对照：本地 `main`
- 依赖：workspace 固定的 `codex-app-server-protocol` `rust-v0.144.1`
- 关注链路：Managed Agent / Codex adapter → Managed Runtime journal/projection →
  AgentRun list/workspace → frontend refresh

## 标准协议结论

当前固定版本已经提供非实验性的通用通知：

```text
method: thread/name/updated
payload:
  threadId: string
  threadName?: string | null
```

Rust owned DTO 为：

```rust
ThreadNameUpdatedNotification {
    thread_id: String,
    thread_name: Option<String>,
}
```

本仓库生成代码已经包含该 DTO：

- `crates/agentdash-agent-protocol/src/generated/codex_v2.rs`
- `packages/app-web/src/generated/codex-app-server-protocol/v2/ThreadNameUpdatedNotification.ts`

上游 Codex 的 `thread/name/set` 会规范化输入、持久化 thread name，并发布同一
`ThreadNameUpdatedNotification`。`threadName=None` 是标准清除语义，不能被 adapter
过滤掉。

## 当前断点

### 1. Adapter 把标准事件降成自有事件

`crates/agentdash-integration-codex/src/mapping.rs` 已严格解码
`ThreadNameUpdatedNotification`，但随后把它改写成：

```rust
PlatformEvent::SourceSessionTitleUpdated {
    executor_session_id,
    title,
    preview,
    source,
}
```

这一步：

- 丢失了 `threadName=None` 清除语义；
- 把标准 payload 扩成 AgentDashboard 自有 `source/preview` 契约；
- 让 Codex 与后续 Managed Agent 无法天然共享同一个 reducer。

`crates/agentdash-integration-codex/src/driver.rs::bind_presentations` 对 bind/read
返回的 `thread.name` 又合成一次相同自有事件。

### 2. Runtime 没有当前名称投影

`RuntimeJournalFact::Presentation` 会把 durable presentation 写入 canonical journal，
但 `RuntimeThreadState::apply_journal_fact` 当前只从 terminal `ItemCompleted` 建立
transcript，其他 presentation 不更新当前态。

`RuntimeThreadState`、`RuntimeSnapshot` 与 driver `DriverTranscript` 均没有当前
thread name 字段。因此：

- journal/live stream 可能看得到标题事件；
- `runtime.inspect` 永远读不到当前标题；
- replay/restart 后 AgentRun query 没有可消费的当前态。

### 3. AgentRun 回退到了身份名

`crates/agentdash-application/src/agent_run_projection.rs` 的列表标题来源是：

```text
LifecycleAgent.workspace_title
  → Project Agent label
  → source label
```

`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` 的 workspace shell
也只读取 `LifecycleAgent.workspace_title`，缺失时回退 Project Agent 名。虽然列表和
workspace 已经查询 Runtime snapshot，它们只消费 status/active turn，没有消费标题。

前端列表只是渲染后端 `entry.title`，因此“孤零零的 Agent 名”不是前端截断问题，而是
后端 composition 缺少 Runtime conversation name。

## `main` 对照结论

`main` 的旧链路包含：

- RuntimeSession 层的自动标题生成；
- `WorkspaceTitlePort`；
- `AgentRunWorkspaceTitleAdapter`；
- 把自动标题写回 `LifecycleAgent.workspace_title`；
- `SessionMetaUpdate`/workspace title 广播。

这条链能让列表出现标题，但代价是把 Agent/Runtime 的 conversation fact 复制成
Lifecycle workspace fact。显式用户命名与自动命名因此共享可写字段和优先级逻辑。

当前重构删除旧 RuntimeSession 时把复制链删除了，却没有在新 Managed Runtime 中建立
标准名称 projection，最终暴露了缺口。正确修复不是恢复旧 adapter，而是让 Runtime
直接投影标准 `thread/name/updated`。

## Managed Agent 能力落点

`agentdash-integration-native-agent::NativeAgentDriver` 已持有解析后的
`Arc<dyn LlmBridge>`；`LlmBridge` 提供非流式 `complete(BridgeRequest)`，而
`BridgeRequest` 可以显式传入 messages、system prompt 与空 tools。

因此适合的边界是：

1. `agentdash-agent` 提供 provider-neutral `ConversationNamer`；
2. 输入是一次成功 turn 的 immutable user/assistant messages；
3. 输出是规范化的 conversation name 字符串，而不是新事件；
4. Native adapter 管理 one-shot trigger、binding generation 和 event sink；
5. adapter 把字符串映射为标准 `ThreadNameUpdatedNotification`。

这样业务仍在 Agent 层，协议依赖仍停留在 adapter，Agent Core 不依赖 Backbone/Codex
DTO。

## 去重、顺序与恢复

- Driver event admission 已按 `binding_id + generation + source_thread_id` 校验，旧
  generation 的迟到任务会被 quarantine。
- Runtime 对同一 binding 的 read-transition-CAS 边界串行化，durable journal sequence
  给出唯一归约顺序。
- 名称 reducer 采用 last accepted event wins；相同值重复应用是状态幂等。
- Native thread 内只允许一个 naming job。事件 sink 成功后标记已有名称。
- cold bind/rebind 从 Runtime 当前投影读取名称；已有名称不再生成。若此前命名调用失败
  且投影仍为 `None`，后续成功 turn 可再次尝试。
- 不需要新的“标题生成状态”表、标题专用 idempotency key 或第二个结果事件。

## 刷新链路

现有协议已经有：

```text
ControlPlaneProjectionChanged
  projection = agent_run_list
  reason = title_changed
```

AgentRun list store 已监听项目事件并在 `projection=agent_run_list` 时重新查询。已打开
workspace 的 control-plane model 也已有 workspace/list refresh executor，但尚未把
标准 `thread_name_updated` 识别成 shell refresh 原因。

所需补口：

- 标准名称 presentation durable commit 后，由 AgentRun projection notifier 根据
  runtime thread anchor 发布现有项目投影失效通知；
- workspace session 收到同一标准 presentation 后刷新 shell/list；
- 通知只提示重新读取，不承载标题事实，故不形成第二事实源。

## 相关规范

- `.trellis/spec/backend/agent-runtime-kernel.md`
- `.trellis/spec/backend/agent-runtime-native-adapter.md`
- `.trellis/spec/backend/agent-runtime-codex-adapter.md`
- `.trellis/spec/backend/agent-runtime-agentrun-facade.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
