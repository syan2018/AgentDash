# 上下文压缩系统实践 PRD

## Goal

将 AgentDash 的上下文压缩升级为一套 **对齐 Codex app protocol、适配云端协作与 PostgreSQL 持久化的上下文 checkpoint / projection 基础设施**。

本任务不是给现有压缩链路打补丁，而是确立 AgentDash 后续长会话、resume、branch、rollback、handoff、多端同步和团队协作的模型上下文基准。

核心目标：

- 运行时协议以 Codex app protocol 的 compaction lifecycle 为基准。
- 存储与恢复以 AgentDash 自有的 PostgreSQL durable log、checkpoint、projection 为基准。
- 模型输入永远由后端 ContextProjector / ContextMaterializer 从 durable facts 构建。
- UI timeline、ContextFrame、agent input、audit view 可以共享事实源，但使用不同 projection。

## Product Intent

AgentDash 原则上沿用 Codex app protocol，因此压缩流程的产品语义需要优先向 Codex 对齐：

- compact 是一个可观察的 lifecycle，而不是静默裁剪。
- UI / SDK / runtime 应能感知 `contextCompaction` started / completed。
- 压缩结果应形成可恢复 checkpoint，后续 resume 以 checkpoint 为基线重放 suffix。
- manual、auto、pre-turn、mid-turn、model downshift 等阶段语义应进入统一模型。

但 AgentDash 不是本地单人 agent。它是云端协作产品，仓储依赖 PostgreSQL，面向团队共同使用开发。因此实现形态需要比 Codex 的本地 rollout 更结构化：

- `session_events` 保存真实发生过的事实。
- checkpoint / compaction record 保存可恢复的模型上下文基线。
- projection segments 保存 ContextProjector 产出的模型视图。
- branch / lineage / projection head 保存团队协作下的模型可见状态。

这两层判断必须同时成立：**协议基准对齐 Codex，实现基准云端化、数据库化、团队化。**

## User Value

- 长会话可以持续运行，压缩不会让用户失去对历史、决策、工具结果和当前状态的信任。
- 用户恢复会话、跨设备接手、从分支继续、回退模型可见状态时，看到的是稳定 projection，而不是临时拼出来的消息数组。
- 团队成员可以审计压缩覆盖了哪些历史、生成了什么摘要、保留了哪些尾部上下文、模型实际看到了什么。
- 后续引入 tool result pruning、provider-native compaction、session memory、branch/handoff summary 时，不需要重写 agent loop 和 session 仓储。

## Confirmed Facts

- 当前任务已有研究材料：
  - `research/context-compaction-infrastructure.md` 对比了 Codex、Claude Code、pi-mono 的 compact 策略。
  - `research-codex-session-tree.md` 梳理了 Codex rollout、fork、rollback、replacement history 与 AgentDash 数据库仓储的对应关系。
- Codex 的关键启发是：
  - `contextCompaction` 是运行时协议的一等 item lifecycle。
  - `CompactedItem.replacement_history` 是 resume 的 canonical checkpoint。
  - rollout reconstruction 从最新 replacement history 开始，再 replay 后续 suffix。
- Claude Code 的关键启发是：
  - compact 需要多层策略：microcompact、summary、reactive recovery、post cleanup。
  - boundary / preserved segment 对恢复和 partial compact 很重要。
- pi-mono 的关键启发是：
  - compaction 是 append-only session tree entry。
  - `firstKeptEntryId` 明确描述 summary 后尾部上下文从哪里开始。
  - 模型上下文由 branch path projection 构建。
- 当前项目已有若干基础种子：
  - `ProjectedTranscript`、`ProjectionKind`、`MessageRef` 已存在。
  - `ContextFrame(kind="compaction_summary")` 已进入前后端展示链路。
  - Backbone/generated protocol 已存在 `context_compacted` / `contextCompaction` 相关类型。
  - `session_events`、session repository abstraction、PostgreSQL / SQLite migration 体系已经存在。
- 现有规划中的 `04-08-session-tree-branching` 是本任务的后续分支能力任务；本任务需要先把 checkpoint / projection 基线定稳。

## Requirements

### R1. Codex App Protocol 是压缩 lifecycle 基准

- AgentDash 的压缩过程必须表达为 app protocol 可观察 lifecycle。
- 成功压缩至少产生 `contextCompaction` started / completed 语义，并能映射到 Backbone / frontend feed。
- manual compact、auto compact、pre-turn compact、mid-turn compact、model downshift compact 应共享同一事件模型。
- 协议层事件表达“压缩正在发生 / 已完成 / 失败”，存储层记录“压缩结果如何恢复”。

### R2. Durable Log 是事实源

- UI message array 不能成为模型上下文裁剪的事实源。
- `session_events` 记录真实历史、工具调用、ContextFrame、状态迁移、压缩 lifecycle 和后续 branch/rollback 事件。
- 压缩不改写真实历史；压缩只改变模型上下文 projection。
- 所有 resume、branch、handoff、audit 都应能从 durable facts + projection metadata 解释出来。

### R3. ContextProjector 独立于 UI Timeline

- Agent input 必须由 `ContextProjector` 从 durable log / checkpoint / projection segments 构建。
- 前端 timeline 默认展示真实历史；Context panel 展示模型当前可见 projection。
- Agent input message 必须能区分真实事件与派生 projection：
  - `origin: event | projection`
  - `synthetic`
  - `source_event_id`
  - `projection_segment_id`
  - `source_range`
- ContextMaterializer 负责把 AgentDash 内部 projection 转成 OpenAI / Anthropic / Gemini 等 provider-specific message shape。

### R4. Checkpoint 必须保存可恢复的 Replacement Projection

- 每次成功结构性 compact 必须持久化 checkpoint。
- checkpoint 至少包含：
  - `session_id`
  - `branch_id` 或 lineage 关联
  - `base_head_event_id`
  - `source_start_event_id`
  - `source_end_event_id`
  - `first_kept_event_id`
  - `replacement_projection`
  - `summary`
  - `strategy`
  - `trigger`
  - `phase`
  - `tokens_before`
  - `tokens_after`
  - `projection_version`
  - provenance / diagnostics
- resume 路径必须优先读取最新有效 checkpoint，再 replay checkpoint 后的 suffix。
- checkpoint 是模型恢复事实，不是单纯 UI 文本。

### R5. PostgreSQL Projection Store 是云端实现基准

AgentDash 应收敛到三层仓储形态：

1. **事实层。** `session_events`、`session_branches` / `session_lineage`、artifacts。
2. **投影层。** `session_compactions` / `session_checkpoints`、`session_projection_segments`、`session_projection_snapshots`。
3. **消费层。** `AgentContextEnvelope`、frontend `TimelineItem`、frontend `ProjectionView`。

命名可以在 design 阶段最终收口，但职责必须清晰：

- compaction / checkpoint record 记录 lifecycle、覆盖范围、策略、token、状态。
- projection segments 记录 summary chunk、pruned message、tool result digest、artifact reference、kept tail 等派生内容。
- snapshot 只作为 materialized cache，不替代事实源。

### R6. Branch-aware By Default

- checkpoint 和 projection 必须绑定 session / branch / head event。
- fork 时 child session 需要固定 fork point 对应的 projection。
- rollback 通过事件和 active projection head 表达当前模型可见状态。
- parent 后续 compact / rollback 不应改变 child 已固定的初始 projection。
- 完整 branch UI / API 可以由 `04-08-session-tree-branching` 承接，但本任务必须提供足够字段和恢复路径。

### R7. Strategy Pipeline 分层落地

压缩策略应逐步扩展为 pipeline，而不是单个 summarizer：

1. `ToolResultPruning`
   - 将大型 tool output 转为 digest + artifact reference。
   - 保留工具调用因果和关键 metadata。
2. `RollingSummary`
   - 将早期历史压成 summary chunk。
   - 明确 source range 和 first kept pointer。
3. `ReactiveEmergencyCompact`
   - provider overflow 后执行有限恢复。
4. `BranchHandoffSummary`
   - branch / handoff 场景生成面向接手者的 summary。
5. `ProviderNativeCompaction`
   - provider 支持原生 compact 时，输出仍归一化为 AgentDash projection。

MVP 不需要一次实现全部策略，但架构必须允许这些策略共用 checkpoint / projection store。

### R8. 失败不污染有效上下文

- 摘要为空、provider error、cancel、stream closed、checkpoint 写入失败时，不生成成功 checkpoint。
- 失败必须产生结构化 diagnostic / event。
- 自动压缩需要失败熔断，避免每轮重复触发。
- 压缩请求自身超窗时允许有限 retry；最终失败时保留原 projection。

### R9. ContextFrame 保持一等可视化

- 成功 compact 继续生成 `ContextFrame(kind="compaction_summary")`。
- Compaction summary section 应展示 checkpoint id、strategy、trigger、phase、source range、retained tail、tokens before / after。
- ContextFrame 是用户可见解释层；checkpoint / projection store 是恢复事实层。

## MVP Scope

第一版实践落地以“Codex-aligned checkpoint + projection 基础”作为 MVP：

- 对齐 `contextCompaction` lifecycle 的事件语义。
- 定义并落地 compaction checkpoint / projection segment 的仓储模型。
- 让 resume 使用 checkpoint + suffix，而不是从 UI message array 或单个 summary payload 推断。
- 保留现有 ContextFrame 能力，并扩展结构化 metadata。
- 为 branch/fork/rollback 保留 projection head 与 lineage 字段。

第一版可以只实现最小 summary compaction；tool pruning、provider-native compact、branch UI 可以后续展开。

## Acceptance Criteria

- [ ] PRD / design 明确 Codex app protocol 是 compact lifecycle 基准。
- [ ] 规划产物明确 durable log、checkpoint、projection、snapshot、timeline 的职责边界。
- [ ] 规划产物明确 Agent input 由 ContextProjector / ContextMaterializer 构建。
- [ ] 规划产物明确 checkpoint + suffix 的 resume 路径。
- [ ] 规划产物明确 branch-aware checkpoint / active projection head 的最低要求。
- [ ] 实现后，成功 compact 会产生 app protocol lifecycle、Backbone event、ContextFrame 和 durable checkpoint。
- [ ] 实现后，失败 compact 不会替换当前有效 projection。
- [ ] 实现后，resume 从最新有效 checkpoint replay suffix。
- [ ] 实现后，前端可以区分真实 timeline 与模型 projection view。
- [ ] 实现后，PostgreSQL / SQLite migration、Rust tests、TS protocol 生成与前端解析同步通过。

## Follow-up Design Questions

这些问题不阻塞 PRD 意图，但会影响 design.md 与 implement.md 的下一轮收口：

1. `session_compactions` 与 `session_checkpoints` 是否合并为一张表，还是前者记录 lifecycle、后者记录 projection checkpoint？
   - 推荐：先合并为一个 checkpoint-oriented compaction record，再用 `session_projection_segments` 承载细粒度投影。
2. fork 时是否默认 materialize child initial checkpoint？
   - 推荐：默认 materialize，换取 child session 的独立恢复能力。
3. MVP 是否先实现 summary checkpoint，再实现 tool result pruning？
   - 推荐：先实现 summary checkpoint 和恢复链路；tool pruning 作为第二阶段策略接入。

## Out Of Scope For MVP

- 完整 branch tree UI。
- 完整 provider-native compact adapter。
- 完整 session memory / context collapse 系统。
- 对旧压缩 payload 的长期兼容层。

这些能力依赖同一套 checkpoint / projection 基础设施，后续以独立阶段补齐。
