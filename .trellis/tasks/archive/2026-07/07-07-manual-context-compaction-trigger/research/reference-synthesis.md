# Reference Research Synthesis

## Scope

本综合评估只读取三份 subagent 产物，不直接读取 `references/*` 源文件：

- `research/codex-context-compaction.md`
- `research/claude-code-context-compaction.md`
- `research/pi-mono-context-compaction.md`

## Common Findings

- 三个 reference 都支持自动压缩和手动压缩，但都没有 AgentDashboard 需要的服务端 running/idle 双分支、durable pending manual request、`client_command_id` 幂等 command receipt，以及 `scheduled_next_turn` / `launched_compaction_turn` 这类明确 outcome。
- `auto/manual` 与 `running/idle` 不能完全等同。前者是触发来源：自动来自 token pressure、overflow retry、model downshift 等策略；手动来自用户命令。后者是 AgentRun 内部 runtime session 的执行状态：running 时只能排队到下个 session turn，idle 时可以主动启动 compact-only turn。当前任务只新增手动 intent 的内部 fulfillment；自动压缩仍走现有 pre-provider 策略入口。
- 三个 reference 都把压缩当作真实生命周期事件或上下文边界，而不是纯 UI 刷新。Codex 使用 `ContextCompaction` item 与 `replacement_history` checkpoint；Claude Code 使用 compact boundary + summary messages；pi-mono 使用 `CompactionEntry(summary + firstKeptEntryId)`。
- 三个 reference 都把 summary 视为后续模型的 handoff context。Codex 和 Claude Code 的 prompt 明确要求交接当前进展、约束、待办、下一步；pi-mono 强调 summarizer 不应继续对话。
- 三个 reference 都显示“手动强制”不等于无条件破坏边界。即使手动入口绕过 token threshold，也仍需要合法 cut point、保留尾部、summary install/checkpoint。
- reference 中常见的运行中手动压缩行为不是我们要照搬的形态：Codex TUI 侧可以排队但 core task 会替换 active task；pi-mono 会 abort 当前 agent operation；Claude Code 没有 durable running intent。AgentDashboard 的“不打断当前 active turn，下个 session turn 前消费”是更适合本系统的约束。

## Design Decisions Confirmed

- 保留 AgentRun scoped command 与 command receipt。不能退回裸 session API 或纯前端 slash command。
- running/idle 状态应封装在 AgentRun 内部 fulfillment 中，而不是 API/frontend/route handler 可见的 mode，也不是 compaction 末梢分支。reference 中未提供这个服务端抽象，AgentDashboard 需要补自己的内部 `schedule_next_turn` / `launch_maintenance_turn` / `reject` fulfillment 层。
- 保留 durable pending manual compaction request。运行中请求不能只存在于 UI queue 或进程内存。
- 保留 idle compact-only turn。空闲手动压缩不能复用 system delivery 触发普通 assistant response。
- 不把 automatic compaction 改造成 idle compact-only。自动压缩属于 provider 前/错误恢复前的策略判断；即使它通常发生在一个 turn 的执行链路里，它也不是用户手动命令的 running 分支。
- 保留 `compacted_until_ref` 和 `first_kept_ref` 双边界。pi-mono 的 `firstKeptEntryId` 和 Codex replacement checkpoint 都支持“明确 resume cursor”的必要性，而 AgentDashboard 的 MessageRef 边界更精确。
- 无合法可压缩区间时返回 `no_eligible_messages` no-op，不写 projection head。

## Improvements To Add Before Implementation

1. Provenance vocabulary

   `context_compacted` payload、compaction record、projection segment provenance、request result 应包含：

   - `trigger`: `auto | manual`
   - `reason`: `token_pressure | user_requested | overflow_retry | model_downshift | compaction_compatibility_changed`
   - `phase`: `pre_provider | standalone_compact_turn | overflow_retry`
   - `strategy`: `summary_prefix`
   - `implementation`: `local_summary`
   - `request_id`: manual request id when present

2. Continuation prompt

   Summary generation prompt 应要求输出可交接给下一模型的 handoff，包括：

   - primary request / user intent
   - progress and completed work
   - decisions and constraints
   - files, tools, external artifacts, or lifecycle context used
   - errors, failed attempts, and fixes
   - pending tasks and immediate next step

   Summary generation request itself must instruct summarizer not to continue the conversation. The installed compact summary context should tell the later provider request to continue from the compacted context without asking the user to confirm the summary.

3. Summary input bounding

   Summarizer input should use bounded, serialized conversation facts instead of rehydrating arbitrarily large tool outputs. Tool results and attachment bodies need either existing bounded facts or explicit truncation/diagnostic metadata.

4. Lifecycle and diagnostics

   Compaction should emit or persist enough diagnostics to distinguish:

   - requested
   - started
   - summary generated
   - projection committed
   - no eligible messages
   - failed

   Manual command result should surface `no_eligible_messages` and failure details without writing a projection.

5. Resume/checkpoint tests

   Tests should prove that resume/fork/model-context construction uses the active projection checkpoint plus suffix events, not raw pre-compaction transcript replay.

6. Context usage after compaction

   UI should distinguish provider-verified token usage from projection-estimated usage after compaction. Until a provider response gives fresh usage, projection estimates are useful but should not pretend to be measured provider accounting.

## Deferred Optimizations

- Reactive overflow compaction and retry after provider prompt-too-long errors.
- Model downshift or compaction-compatibility-change triggered compaction.
- Split-turn prefix summarization for future mid-turn compaction.
- Custom hook/extension-provided compaction summary. This should wait until the projection ownership model is fully stable.
