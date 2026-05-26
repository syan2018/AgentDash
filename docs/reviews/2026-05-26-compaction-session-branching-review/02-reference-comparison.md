# Reference 对照

## Codex

### Compaction

Codex 的 compaction 分手动 compact 与 auto compact。auto compact 不是单一 “超过窗口就压缩”，而是在 turn 生命周期里区分 pre-turn、mid-turn、模型切换前 compact。核心判断读取 active context token，并支持按完整上下文或 body-after-prefix 计算预算。相关代码在 `references/codex/codex-rs/core/src/session/turn.rs` 的 `auto_compact_token_status`、`run_pre_sampling_compact`、`maybe_run_previous_model_inline_compact`、`run_auto_compact`。

manual compact 是 app-server 的 `thread/compact/start` operation，但客户端看到的是 turn 生命周期和 `contextCompaction` item。core side compact handler 创建 turn context 并运行 `CompactTask`，避免客户端理解额外后台任务模型。

compact 后 Codex 持久化的是 `CompactedItem { message, replacement_history }`，不是只有摘要文本。resume/fork 时 `rollout_reconstruction.rs` 从后往前找最新 `replacement_history` checkpoint、rollback marker、turn settings、reference context item，然后只重放 checkpoint 之后的 suffix。这个 reducer 是 Codex compaction、resume、fork、rollback 行为一致的关键。

Codex 还特别处理 initial context injection。mid-turn compact 需要把 canonical initial context 插在最后一个真实用户消息前；manual/pre-turn compact 会清空 reference context，等待下一次 regular turn 再完整 reinject。这把 “压缩历史” 和 “系统/环境基线是否有效” 分成两个语义。

### Session / Thread Fork

Codex 的 `InitialHistory` 明确区分 `New`、`Cleared`、`Resumed`、`Forked`。普通 thread fork 创建新 thread id 并保留来源关系；subagent fork 会先 flush parent rollout，再按 fork mode 截断最近 N turns，用 `InitialHistory::Forked` 启动子线程。

Codex 还把 parent-child subagent lineage 放在单独 graph store，不塞进 message history。`AgentGraphStore` 只负责 `parent_thread_id -> child_thread_id` edge、Open/Closed 状态、direct children、descendants。

### 对本项目的启发

- checkpoint 应包含可直接恢复的 replacement projection/history，而不是只包含自然语言摘要。
- resume/fork/rollback 最好共享同一套 reconstruction/materialization reducer。
- usage 是恢复状态的一部分，Codex resume/fork 后会立刻发 token usage update。
- thread id、live session root、forked-from/thread-spawn edge 是不同概念，不能用一个字段混用。

## Claude Code

### Compaction

Claude Code 同时支持手动 `/compact` 与自动 compaction。`autoCompact.ts` 的 `shouldAutoCompact()` 用当前 messages token 与模型阈值比较；阈值不是完整 context window，而是扣除了 summary 输出预算后的 effective context window。它还会跳过 `querySource === "session_memory" || "compact"`，避免 summarizer 自己再次触发 compaction，并有连续失败熔断。

`compact.ts` 的 summary 生成采用 forked-agent/cache sharing 模型。summary request 会尽量复用主对话 prompt cache；cache-safe 参数包含 system prompt、tools、model、message prefix、thinking config 等。

历史保留有 full compact 和 partial compact。full compact 用 boundary marker + summary messages + attachments 替代主历史；partial compact 支持从 pivot 两侧选择 “摘要一侧、保留一侧”。summary user message 会带 `isCompactSummary` 等 metadata。prompt 层要求模型输出 `<analysis>` 和 `<summary>`，进入上下文前会剥离 analysis，只保留格式化 summary。

Claude Code 的 compact 后恢复材料不只是一段摘要，还会恢复最近读文件附件、async agent 状态、plan、plan mode、已调用 skill、deferred tools 等。这体现了 compaction 后第一轮能否继续工作，取决于恢复运行上下文，而不只是 token 变少。

### Branch / Sidechain

Claude Code `/branch` 是新 session 文件级 fork。`branch.ts` 读取当前 transcript，过滤主对话消息，生成新 session id，重写每条消息的 `parentUuid` 为线性链，并写入 `forkedFrom: { sessionId, messageUuid }`。

session JSONL 通过 `parentUuid` 形成消息链，resume 时按 leaf 还原；一个 session 文件可有多个 resume leaf。subagent 是 sidechain transcript，带 `agentId` 和 `isSidechain`，可以加载 agent transcript。

### 对本项目的启发

- 自动 compaction 需要输出预留、递归保护、失败熔断和 post-compact 防抖。
- fork 的 traceability 可以放在 lineage edge，也可以放在每条消息；本项目选择 edge/projection provenance 是合理的，但要保证 fork point 足够稳定。
- 如果未来要支持可恢复 subagent，`spawned_agent` relation 不能只是 fork endpoint 的一个 enum 值，需要有 agent transcript/runtime 语义。

## pi-mono

### Compaction

pi-mono 的 compaction 有手动 `session.compact()`、自动 threshold/overflow、extension `ctx.compact()` 三个入口，但都走 session manager 的一等 entry。`CompactionEntry` 包含 `summary`、`firstKeptEntryId`、`tokensBefore`、`details`、`fromHook`；`appendMessage()` 明确禁止直接写 `CompactionSummaryMessage`，要求用 `appendCompaction()`。

`buildSessionContext()` 会从当前 leaf 回溯 branch path，找到最新 compaction 后先加入 compaction summary，再从 `firstKeptEntryId` 开始追加保留 tail 和 compaction 后消息。summary 生成支持 “更新旧摘要 + recent tail 保留”，还会带文件操作上下文。

### Branch Tree

pi-mono 的核心 session model 是 append-only tree：每个 entry 有 `id/parentId`，leaf 指针决定当前上下文。branch navigation 不改历史、不复制文件，而是在同一 session 文件内移动 leaf。离开一个 branch 时可以生成 `branch_summary`，挂到目标上下文里，后续 summary 可以继续吸收 branch summary。

跨 session fork 也存在，但语义不同：新 JSONL header 指向 `parentSession`，runtime 会 tear down 当前 session 后以 `session_start reason: "fork"` 创建新 runtime。

### 对本项目的启发

- 如果要做同 session branch tree，`branch_id` 需要变成完整产品模型，而不只是 projection column。
- branch summary/handoff 是 “从一个分支回到另一个分支时保留成果” 的机制，和当前 child session fork 是不同问题。
- UI 上 compaction 和 branch summary 应该是结构节点，可被用户 inspect，而不是隐藏在普通消息流里。

## 横向对照

| 维度 | Codex | Claude Code | pi-mono | AgentDashboard 当前 |
| --- | --- | --- | --- | --- |
| Compaction checkpoint | `replacement_history` durable checkpoint | compact boundary + summary messages + attachments | `CompactionEntry` 一等 entry | `session_compactions` + `summary_chunk/context_envelope` segments + projection head |
| Runtime 触发 | pre-turn / mid-turn / manual | manual / auto / reactive | manual / auto / extension | provider 请求前 hook |
| Boundary | rollout reconstruction reducer 理解 checkpoint | compact boundary / parentUuid logical chain | `firstKeptEntryId` | application 从 events + count 推导，runtime summary 默认无 ref |
| Fork 语义 | 新 thread / subagent thread spawn | 新 session 文件 / sidechain agent transcript | 同 session tree + 跨 session fork | 新 child session + lineage edge + child initial projection |
| Rollback | marker + reconstruction | parentUuid / leaf restore 相关 | leaf navigation | projection head rollback，不删事件 |
| Lineage | thread spawn edge store | `forkedFrom` / sidechain metadata | parentSession / entry tree | `session_lineage` edge store |
| UI | turn/item operation + usage replay | session leaf / branch command | explicit tree selector | projection / lineage 诊断面板，项目列表一层 relation child |

## 设计判断

AgentDashboard 现在更接近 “Codex checkpoint projection + Claude 新 session fork” 的混合：它没有 pi-mono 的同 session leaf tree，也没有 Claude 的每条消息 parentUuid rewrite，而是用 projection store materialize model context，用 `session_lineage` 记录跨 session provenance。这个方向是可以继续扩展的，前提是把以下边界提前固定：

- `branch_id` 只表示 projection namespace，直到明确引入同 session branch tree。
- `session_lineage` 只表示跨 session relation edge。
- `fork` endpoint 只创建 `fork` relation；`companion`、`spawned_agent`、`rollback_branch` 由专用 use case 创建。
- compaction event 必须携带 durable boundary 或 checkpoint id，不能长期依赖全量历史推导。
- Codex bridge 要么提供完整 projection commit 材料，要么明确把 `thread/compacted` 当作 UI telemetry。
