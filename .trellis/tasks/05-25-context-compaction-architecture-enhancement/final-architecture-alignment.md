# 上下文压缩架构最终意图对齐

## Purpose

本文作为本任务最终 review 的共识参照。

`prd.md` 说明要解决什么问题，`design.md` 说明技术结构如何成立，`implement.md` 说明如何分阶段推进。本文只回答两个最终验收问题：

- 这个任务完成后，AgentDash 的上下文压缩架构应该是什么样子？
- 我们为什么要把架构导向这个方向？

最终目标不是让现有压缩“能跑”，而是让上下文压缩成为 AgentDash 长会话、恢复、分支、团队协作和审计能力的基础设施。

## Final Architecture Shape

最终验收时，AgentDash 的上下文压缩应形成以下架构形态。

### 1. Codex-aligned Protocol Lifecycle

所有 runtime 的 compact 行为进入同一套 Backbone lifecycle：

```text
item/started: contextCompaction
item/completed: contextCompaction
typed failure diagnostic
legacy completed marker, when provided by external runtime
```

Codex app protocol 是控制面基准。AgentDash 不再把 compact 仅视为某个 runtime 内部的静默裁剪，而是把它表达为用户、前端、SDK、审计系统都能观察到的一等 lifecycle。

最终状态：

- Codex Bridge 映射 Codex app-server 的 compact item lifecycle。
- Pi/native runtime 也输出 Codex-aligned compact lifecycle。
- legacy `ContextCompactedNotification` 只作为 completed marker 或外部 runtime audit 信号。
- compact failure 进入结构化 diagnostic，而不是伪装成成功摘要。

### 2. Durable Log As Fact Source

`session_events` 是真实历史和运行时事实源。

用户消息、assistant 输出、工具调用、ContextFrame、compact lifecycle、failure diagnostic、branch/rollback transition 都以 append-only 事件保存。compact 不改变真实历史；compact 只提交新的模型上下文 projection。

最终状态：

- UI timeline 默认展示真实事件流。
- audit / replay 可以回看完整事实历史。
- compact 覆盖的 source range 通过 event seq / MessageRef 可追溯。
- event log 与 projection store 分工清晰：前者记录发生了什么，后者记录模型应该看到什么。

### 3. Durable Checkpoint / Projection Store

成功的结构性 compact 必须生成可恢复 checkpoint 和 projection segments。

核心存储形态：

```text
session_events
session_compactions
session_projection_segments
session_projection_heads
session_projection_snapshots
session_lineage / branch metadata
```

最终状态：

- `session_compactions` 记录一次 compact 的 checkpoint：status、strategy、trigger、phase、source range、first kept pointer、token stats、summary、replacement projection。
- `session_projection_segments` 记录 summary chunk、kept tail、pruned message、tool result digest、artifact reference 等派生片段。
- `session_projection_heads` 记录当前 branch / projection kind 的 active model-visible cursor。
- snapshot 只作为 materialized cache，不承担事实源职责。

这相当于把 Codex 的 `replacement_history` 云端化、数据库化、可审计化。

### 4. ContextProjector Independent From Timeline

模型输入由后端 ContextProjector 从 durable facts 构建。

ContextProjector 的输入是 session、branch、projection head、checkpoint、segments、suffix events。输出是 AgentDash 内部语义的 `AgentContextEnvelope`，再由 ContextMaterializer 转为 provider-specific request。

最终状态：

```text
session_events + projection metadata
  -> ContextProjector
  -> AgentContextEnvelope
  -> ContextMaterializer
  -> provider request
```

每条 agent input message 都能区分：

- 来自真实事件：`origin = event`
- 来自派生投影：`origin = projection`
- 是否为 synthetic
- source event / source range / projection segment provenance

这让 `[summary of 1-80] + [81..100]`、`[1..80 pruned] + [81..100]`、mixed projection 都成为可解释的模型输入，而不会与真实聊天历史混淆。

### 5. Runtime Compaction At Provider-visible Boundary

compact 的触发和 cut 决策应基于 provider-visible payload。

最终运行路径：

```text
ContextProjector builds current envelope
  -> runtime refresh tools
  -> transform_context consumes hook steering / ContextFrames
  -> ContextMaterializer builds draft provider request
  -> pressure evaluation
  -> contextCompaction lifecycle started
  -> strategy pipeline
  -> checkpoint / segments / head committed
  -> contextCompaction lifecycle completed
  -> final provider request
```

最终状态：

- system prompt、tools、hook steering、ContextFrame 注入、provider message shape 都进入 token pressure 评估。
- `reserve_tokens` 参与 retained tail / cut 决策。
- tool call / tool result 因果边界被保留。
- summary 为空、cancel、provider error、persist failure 时 active projection head 保持原值。

### 6. Branch-aware And Team-aware Context

AgentDash 是云端协作产品，compact 必须天然支持 branch、resume、handoff、rollback 和多人审计。

最终状态：

- checkpoint 绑定 `session_id + branch_id? + base_head_event_seq`。
- 不同 branch 可以拥有不同 active projection head。
- fork 时 child 可以 materialize 自己的 initial projection。
- rollback 通过事件和 projection head 表达当前模型可见状态。
- agent turn 记录使用的 projection version / snapshot id。

这让团队成员可以回答：

- 当前模型看到了哪个上下文版本？
- 这次 compact 覆盖了哪些历史？
- 某个 branch 从哪里继承上下文？
- rollback 后为什么模型看到的是这个状态？

### 7. Product Surface Separation

前端最终应形成三种互补视图：

```text
Timeline: real event history
ContextFrame: human-readable explanation
Projection View: current model-visible context
```

最终状态：

- Timeline 展示真实历史和 compact lifecycle marker。
- ContextFrame 展示 compact summary、range、tokens、strategy、checkpoint metadata。
- Projection View 展示模型当前可见 segments，并标记 summary / pruned / original / artifact reference。

这三个视图共享事实源，但服务不同问题：用户发生了什么、系统为什么这样处理、模型实际看到了什么。

### 8. Strategy Pipeline Extensible By Design

MVP 可以先落 summary checkpoint，但架构必须允许策略扩展。

最终策略层形态：

```text
ToolResultPruning
  -> RollingSummary
  -> ReactiveEmergencyCompact
  -> BranchHandoffSummary
  -> ProviderNativeCompaction
```

最终状态：

- 低损耗 tool result pruning 可以先替换大输出为 digest / artifact reference。
- rolling summary 可以把早期历史压成 summary chunk。
- reactive compact 可以在 provider overflow 后恢复原 turn。
- branch / handoff summary 可以服务团队接手。
- provider-native compact 输出可以归一化为 AgentDash projection。

所有策略共享 checkpoint / segment / head 基础设施。

## Architecture Intent

我们把架构导向上述形态，原因有五个。

### 1. 对齐 Codex 是协议基线

AgentDash 原则上沿用 Codex app protocol。compact 在 Codex 中已经是一等 `contextCompaction` lifecycle，并且 `replacement_history` 是 resume 的关键 checkpoint。

因此 AgentDash 的控制面应向 Codex 对齐：前端、SDK、运行时和审计系统都能感知 compact lifecycle。这样后续继续接入 Codex Bridge、remote runtime 或 provider-native compaction 时，平台语义不会分叉。

### 2. PostgreSQL Durable Facts 是云端基线

AgentDash 不是单机本地 agent。会话、执行、团队协作、branch、artifact、权限和审计都需要可查询、可索引、可迁移的 durable substrate。

因此 AgentDash 不能把“当前消息数组”当作恢复事实。真正可长期维护的基础是 event log + checkpoint + projection store。

### 3. Projection 是长会话能力的中心

长会话里的关键问题不是“历史是否还在”，而是“模型当前应该看到什么”。

同一份事实历史会产生不同 projection：

- 模型续跑需要 model context projection。
- 前端聊天需要 timeline projection。
- 团队接手需要 handoff projection。
- 审计复盘需要 audit projection。

把 ContextProjector 独立出来，是为了让 resume、branch、compact、多端同步和前端展示都从同一份 durable facts 得到一致解释。

### 4. 团队协作要求可解释和可审计

在单人本地 agent 中，compact 只影响当前进程的下一次模型调用。在 AgentDash 中，compact 影响团队成员接手、分支恢复、远程执行器冷启动、历史审计和 UI 解释。

因此每次 compact 都需要回答：

- 覆盖了哪些事件？
- 保留尾部从哪里开始？
- 摘要由什么策略生成？
- 模型实际看到了哪些 projection segments？
- 失败时为什么没有改变 active context？

这些答案必须来自结构化 metadata，而不是自由文本摘要。

### 5. 当前重构是在铺未来能力的地基

本任务的 MVP 可以只实现 summary checkpoint，但设计必须服务更长的路线：

- tool result pruning
- reactive overflow recovery
- provider-native compact
- branch-local projection
- handoff summary
- projection diff
- audit replay

如果先把 checkpoint / projection / head 做稳，后续策略只是往同一个基础设施里增加 segment 类型和 strategy。如果继续沿用内存消息数组裁剪，每个后续能力都会重复解决恢复、审计、前端解释和分支隔离问题。

## Final Review Checklist

最终 review 时，应以以下结果作为验收参照。

- Compact 是 Codex-aligned lifecycle，而不是 runtime 私有副作用。
- `session_events` 保存完整事实历史。
- 成功 compact 生成 durable `session_compactions` checkpoint。
- Projection segments 能表达 summary、kept tail、pruned message、artifact reference。
- Active projection head 表达当前模型可见状态。
- Resume 使用 checkpoint + suffix，而不是从 UI message array 或 platform payload 反推。
- Agent input 由 ContextProjector / ContextMaterializer 构建。
- Agent input 中派生消息有 provenance。
- 前端 timeline 与 model projection view 分离。
- ContextFrame 解释 compact，但不承担恢复事实源。
- Branch / rollback 通过 projection head 和 lineage 坐标表达。
- Failure 不安装新的 active projection。
- Provider-visible pressure 覆盖 transform 后的真实请求形态。
- MVP summary compact 与后续 tool pruning / reactive compact / provider-native compact 使用同一套基础设施。

当这些条件成立时，本任务才算真正完成：AgentDash 拥有的不是一个更复杂的摘要函数，而是一套能承载长会话和团队协作的上下文投影基础设施。
