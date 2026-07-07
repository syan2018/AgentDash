# 手动触发上下文压缩设计

## 当前链路评审

现有自动压缩主链路是闭合的：

- `agent_loop::streaming` 在 provider 请求前调用 `RuntimeCompactionDelegate::evaluate_compaction`。
- `HookRuntimeDelegate` 通过 `BeforeCompact` hook 与 token 统计决定是否返回 `CompactionParams`，默认 `keep_last_n = 20`、`reserve_tokens = 16384`。
- `agentdash-agent::compaction::execute_compaction` 计算 cut point、调用 LLM 生成 summary，并返回替换后的 `messages` / `message_refs`、`compacted_until_ref`、`first_kept_ref`。
- `AgentEvent::ContextCompacted` 会被 pi stream mapper 转成 `PlatformEvent::SessionMetaUpdate { key: "context_compacted" }`。
- runtime session eventing 在收到 `context_compacted` 后提交 `runtime_session_compactions`、projection segment、projection head，并持久化 compaction context frame。
- 前端 `SessionChatViewModel.computeProjectionRefreshKey` 已监听 `context_compacted` 与 compaction summary frame，`SessionProjectionView` 会刷新上下文投影。

这说明手动入口应该复用现有事件和 projection commit 语义，不应该直接写 projection，也不应该只做 UI refresh。

## Reference 调研结论

三路 subagent 分别调研了 `references/codex`、`references/claude-code`、`references/pi-mono`，产物位于 `research/`：

- Codex：支持 auto/manual compaction、standalone compaction turn、`ContextCompaction` item、replacement history checkpoint；但没有 AgentDashboard 所需的 running/idle 服务端分流和 command receipt 幂等。
- Claude Code：支持 `/compact`、auto compact、prompt-too-long reactive compact、compact boundary、post-compact summary message；但运行中只体现为通用输入队列，没有 durable manual intent。
- pi-mono：支持 `CompactionEntry(summary + firstKeptEntryId)`、auto threshold、overflow 后 compact + continue、manual `/compact`；但 manual compact 会 abort 当前 operation，不适合本任务运行中不打断 active turn 的需求。

调研确认原方案的关键边界是必要的：AgentDashboard 需要自己的 AgentRun command、durable pending request、idle compact-only turn、MessageRef 双边界和 projection checkpoint。调研同时补强了几个压缩环节要求：

- `context_compacted` 与 compaction record 要有完整 provenance vocabulary。
- summary prompt 要明确是 handoff/continuation context，summary 生成请求本身不得继续对话。
- summarizer 输入需要 bounded facts，避免把巨大的 tool result 原文重新灌给 summarizer。
- no-op、failure、projection committed 都要有可观察 diagnostics。
- resume/fork 测试必须证明使用 active projection checkpoint + suffix events。

这里需要避免一个术语折叠：`auto/manual` 是触发来源轴，`running/idle` 是 AgentRun 内部 runtime session 的执行状态轴。自动压缩仍然是运行链路中的策略判断，通常发生在 provider 前或未来的 overflow retry 前；手动压缩入口对外只暴露 `compact_context` command intent，具体按 running 或 idle 调度由 AgentRun 内部维护。也就是说，本任务不是把“自动=running、手动=idle”固化，也不是让外部调用方选择 running/idle，而是让 AgentRun 内部把同一个手动 intent fulfill 成合适的执行形态。

## 产品状态机

手动压缩命令以 AgentRun 为入口。API、frontend、route handler 只提交 command intent，不传递 mode；AgentRun 内部根据当前 delivery runtime session 状态选择 fulfillment。自动压缩不进入这个 command fulfillment，它继续由 runtime compaction delegate 在 provider 前按策略触发：

| 状态 | 行为 | API outcome |
| --- | --- | --- |
| `running_active` | 持久化 one-shot manual compaction request，不影响当前 active turn，在下一个 session turn 的 pre-provider 边界消费 | `scheduled_next_turn` |
| `ready` / idle | 启动 compact-only turn，执行压缩并终止，不生成普通 assistant 回复 | `launched_compaction_turn` |
| `starting_claimed` | 拒绝，避免和正在 claim 的 launch 抢 session | `blocked` |
| `cancelling` | 拒绝，避免取消/压缩竞争 | `blocked` |
| `frame_missing` / no delivery runtime session | 拒绝，缺少可压缩目标 | `blocked` |
| `model_required` | 拒绝，summary generation 需要模型配置 | `blocked` |
| `terminal` | 默认拒绝；若未来需要支持归档会话压缩，再用单独产品语义打开 | `blocked` |

## AgentRun Internal Runtime Command Fulfillment

running/idle 状态应由 AgentRun 内部维护，不能变成 API 参数、frontend 判断、route handler 分支或 compaction 末梢分支。建议新增一层 AgentRun-internal fulfillment/orchestrator application service，职责是：

1. 读取 AgentRun workspace snapshot / delivery runtime selection / execution state。这个 state 是 AgentRun 内部事实，外部调用方不参与维护。
2. 校验 command precondition 与 stale guard。
3. 对 runtime control intent 产出内部 fulfillment 决策：
   - `ScheduleForNextTurn`: 当前 session running active，持久化 pending intent，等待下一次 session turn 的 pre-provider 边界消费。
   - `LaunchMaintenanceTurn`: 当前 session idle/ready，主动启动一个维护型 turn，例如 compact-only。
   - `Reject`: starting、cancelling、model_required、frame_missing、terminal 等不可安全执行状态。
4. 把 command receipt 的 accepted refs、result_json、duplicate replay 统一收口。response 可以报告 accepted outcome 供 UI 展示，但调用方不能基于 running/idle mode 构造请求。

compaction 在 AgentRun 内部只注册自己的 intent fulfillment 规则：

```text
CompactContextCommand
  intent: ManualContextCompaction
  running policy: ScheduleForNextTurn
  idle policy: LaunchMaintenanceTurn(compact_only)
  terminal/no target policy: Reject
```

这样后续如果出现其它 AgentRun runtime maintenance command，例如重新计算上下文投影、刷新 runtime capability、执行只维护状态的 internal turn，也可以复用同一个入口分流。compaction 下游只处理“如何消费 manual compaction request”和“如何执行 compact-only turn”，不承担 AgentRun 运行态调度。

## 后端命令层

新增 AgentRun scoped command：

- conversation command kind: `CompactContext`
- command id: `compact_context`
- command placement: `Header` 或 context panel 专用 placement；MVP 可以先用 `Header` 并由前端在上下文浮层内查找该命令。
- receipt kind: `context_compact`，需要同步更新 command receipt enum 与数据库 check constraint migration。
- endpoint: `POST /agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact`

请求体沿用 command-only 形态。请求体中不能出现 `mode` / `requested_mode` / `running` / `idle` 这类外部调度字段：

```json
{
  "client_command_id": "...",
  "command": {
    "command_id": "compact_context",
    "command_kind": "compact_context",
    "stale_guard": {
      "snapshot_id": "...",
      "run_id": "...",
      "agent_id": "...",
      "frame_id": "...",
      "active_turn_id": "..."
    }
  }
}
```

响应建议为独立 DTO：

```json
{
  "command_receipt": { "...": "..." },
  "outcome": "scheduled_next_turn | launched_compaction_turn | no_eligible_messages | blocked",
  "runtime_session_id": "...",
  "turn_id": "t...",
  "message": "..."
}
```

`AgentRunContextCompactionCommandService` 参考 `AgentRunCancelCommandService`，但 running/idle fulfillment 必须通过 AgentRun 内部状态完成：

- 校验 `client_command_id`。
- 用 command receipt claim 做幂等保护。
- 调用 AgentRun internal runtime command fulfillment service 获取内部 decision。
- `ScheduleForNextTurn`：创建 durable pending request，mark accepted，store result `{ outcome: "scheduled_next_turn" }`。
- `LaunchMaintenanceTurn`：创建 durable request，并通过 runtime port 启动 compact-only turn，mark accepted，store result `{ outcome: "launched_compaction_turn", turn_id }`。
- `Reject`：mark terminal failed 或返回 blocked outcome，按 command policy 错误语义保持一致。
- duplicate：直接返回已有 receipt，不重复创建 request 或 launch。

## Pending request 持久化

运行中“下一轮前压缩”不能只放进内存，否则当前 turn 结束后或进程重启会丢失用户意图。建议新增 session-level pending request 存储，而不是复用 frame-transition runtime command：

- 表名建议：`runtime_session_compaction_requests`
- 关键字段：
  - `id`
  - `session_id`
  - `run_id`
  - `agent_id`
  - `command_receipt_id`
  - `status`: `requested | consumed | completed | noop | failed`
  - `requested_mode`: `next_turn | compact_only`
  - `keep_last_n`
  - `reserve_tokens`
  - `requested_at_ms`
  - `consumed_turn_id`
  - `completed_compaction_id`
  - `result_json`

消费约束：

- 每个 session 同时最多一个 `requested` 手动压缩请求。
- launch/compact-only 在开始时 claim 一个 request 为 `consumed`，带上 `consumed_turn_id`。
- `ContextCompacted` 后标记 `completed`，无合法 cut point 标记 `noop`，失败标记 `failed`。

## Runtime 与 agent-loop

现有压缩逻辑嵌在 `stream_assistant_response` 开头。为支持 compact-only，先抽出共享 helper：

```rust
async fn run_compaction_preflight(
    context: &mut AgentContext,
    tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
    bridge: &dyn LlmBridge,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<CompactionPreflightOutcome, AgentError>
```

普通 provider 请求：

1. transform context
2. run compaction preflight
3. 如果压缩成功，更新 `context.messages` / `message_refs`
4. 继续 provider stream

compact-only turn：

1. emit `AgentStart` / `TurnStart`
2. run compaction preflight with manual request
3. emit `AgentEnd { messages: [] }`
4. connector/session terminal completed
5. 不调用 provider normal completion

`CompactionParams` / `CompactionResult` / `AgentEvent::ContextCompacted` 需要补充来源元数据：

- `trigger`: `auto | manual`
- `reason`: `token_pressure | user_requested | overflow_retry | model_downshift | compaction_compatibility_changed`
- `phase`: `pre_provider | standalone_compact_turn | overflow_retry`
- `strategy`: `summary_prefix`
- `implementation`: `local_summary`
- `request_id`: optional

stream mapper 把这些字段透传到 `context_compacted.value`，eventing 已经支持读取 `trigger` / `reason` / `phase` / `strategy`，因此 projection provenance 可以自然显示手动来源。

## Summary 与 continuation 语义

压缩 summary 是后续正常 provider 请求的 handoff context，不是用户可见普通回复，也不是 system delivery 继续指令。实现上分两层：

1. Summary generation prompt：把待压缩历史序列化给 summarizer，要求只总结、不继续对话。summary 应覆盖 primary request、当前进展、关键决策、约束、文件/工具状态、错误修复、待办和下一步。
2. Installed compact summary context：作为 projection segment / compaction context frame 注入模型上下文。下一次正常 provider 请求看到该 context 后应从断点继续，不询问用户确认 summary，不把 summary 当成新任务。

Summarizer 输入应优先使用已有 bounded facts。对于大 tool result、附件或外部产物，只传摘要、引用、截断片段和 diagnostic metadata，不重新展开无限大原文。

## Running 状态消费语义

运行中点击后：

1. API 创建 pending request，返回 `scheduled_next_turn`。
2. 当前 active turn 不感知该请求。
3. 下一次 session launch claim prompt 后，TurnPreparer/HookRuntime 将 request 注入当前 turn 的 compaction decision。
4. `evaluate_compaction` 优先消费 manual request，返回 manual `CompactionParams`，不再依赖 hook preset 的 token threshold。
5. 如果 `should_execute_compaction` 为 false，记录 no-op 并清理 pending request。

这里的“强制”只表示跳过 token-pressure 判断；`find_cut_point` 和 `message_refs` 边界仍然是硬约束。

运行中请求消费后需要把 manual request 标记为 `consumed`，并在 compaction 成功、no-op 或失败时写回 terminal result。这样即使前端刷新或重复提交同一 command，也不会重复排队。

## Idle compact-only 语义

空闲点击后：

1. API 创建 pending request。
2. runtime launch 使用新的 `LaunchSource::ContextCompaction` 或 launch modifier `CompactOnly`。
3. launch commit 不写 `UserInputSubmitted`，可写一个 `PlatformEvent::SessionMetaUpdate key="manual_context_compaction_requested"` 作为可观测控制事件。
4. connector/agent 进入 compact-only mode，只执行 shared compaction preflight。
5. 完成后由既有 eventing 提交 projection，并持久化 compaction context frame。

不复用 `LaunchCommand::system_delivery_input`，因为 system delivery 会进入正常模型回复路径，不符合“只更新上下文映射”的需求。

compact-only turn 应视为不可 steer 的维护 turn。用户在该 turn 期间提交的新输入应进入既有 mailbox/下一正常 turn 语义，不能作为 compact-only turn 的普通 follow-up 消息执行。

## 前端入口

入口放在 `SessionProjectionViewPanel` 顶部 refresh 旁边：

- running active: 按钮文案可为“下轮压缩”，点击后显示“已排队”。
- ready/idle: 按钮文案可为“立即压缩”，点击后显示“压缩轮已启动”。
- disabled: 使用 conversation command 的 `unavailable_reason` / `disabled_code`。

服务层新增：

- `compactAgentRunContext(runId, agentId, AgentRunCommandOnlyRequest)`

projection refresh 不需要新增特殊逻辑，因为现有 `computeProjectionRefreshKey` 已经监听 `context_compacted` 与 compaction summary frame。

压缩完成后的 token usage 展示需要区分 provider-verified usage 与 projection-estimated usage。compact-only turn 不会产生普通 provider assistant usage，因此浮层可以立即显示 projection estimate，但不要把它标成 provider 实测值。

## 测试策略

Rust：

- conversation command availability: running/ready enabled，starting/cancelling/model_required/frame_missing disabled。
- command service: duplicate receipt 不重复创建 request。
- running active: 只创建 pending request，不调用 launch。
- idle: 调用 compact-only launch，receipt result 包含 turn id。
- compaction preflight: manual request 绕过 token threshold，但无合法 cut point 返回 no-op。
- compact-only agent-loop: 不调用 provider normal stream，不产生 assistant message，仍提交 `context_compacted`。
- eventing/projection: manual metadata 进入 compaction record provenance。
- summary prompt: summarizer 不继续会话，installed compact summary 作为 continuation handoff。
- resume/fork/context projector: 使用 active compaction projection checkpoint + suffix events，不恢复已压缩前缀。
- token usage: 压缩后 projection estimate 刷新，陈旧 provider usage 不触发重复压缩。

Frontend：

- service path 与 request body。
- context panel 按钮根据 command 状态切换文案/disabled。
- click running 显示 scheduled outcome，click idle 显示 launched outcome。
- `context_compacted` 到达后 projection panel refresh。
