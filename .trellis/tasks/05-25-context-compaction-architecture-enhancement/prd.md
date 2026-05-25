# 上下文压缩系统架构增强

## Goal

将当前 AgentDash 上下文压缩从“触发后生成摘要并替换内存消息”的最小链路，增强为可审计、可恢复、可扩展的上下文管理系统。新系统需要吸收 Codex checkpoint 恢复模型、Claude Code 分层压缩与失败保护的优点，同时保持 AgentDash 既有 Session / Bundle / Hook / Backbone 主线清晰。

## User Value

- 长会话在接近模型上下文窗口时能继续运行，不因一次超窗或摘要失败丢失关键历史。
- 压缩后的继续执行、冷启动恢复、fork / rollback / continuation 都能基于稳定 checkpoint，而不是依赖脆弱的消息计数推断。
- 后续可以继续引入 microcompact、context collapse、provider remote compact、不同模型窗口策略等能力，而不需要重写 agent loop。
- 用户和开发者都能看到压缩发生了什么：压缩原因、覆盖边界、摘要、保留上下文和失败原因都有结构化事件与 ContextFrame。

## Confirmed Facts

- 当前核心压缩入口在 `crates/agentdash-agent/src/agent_loop/streaming.rs`，每次 provider request 前调用 `AgentRuntimeDelegate::evaluate_compaction`，随后执行 `agentdash-agent/src/compaction/mod.rs::execute_compaction`。
- 当前触发策略由 `crates/agentdash-application/scripts/hook-presets/context_compaction_trigger.rhai` 提供，主要依据最近 assistant usage 的 `last_input_tokens` 与 `context_window - reserve_tokens`。
- 当前压缩 cut 以 `keep_last_n` 消息数为主，`reserve_tokens` 只进入触发统计，没有实际控制压缩后 token budget。
- 当前摘要生成空结果时会写入占位文本并继续完成压缩，存在“压缩成功但历史细节丢失”的风险。
- 当前 `context_compacted` 事件会在 application eventing 落库前补 `compacted_until_ref`，并生成 `ContextFrame(kind="compaction_summary")`；恢复投影在 `session/continuation.rs` 使用最新 checkpoint 生成 `CompactionSummary + suffix`。
- Session spec 要求 `SessionContextBundle` 是业务上下文主数据面，Hook 输出分 Bundle 改写、per-turn steering、控制流副作用三类；压缩设计不能把静态上下文重复塞进任意 user message。
- Codex Bridge 自身有内部上下文压缩和恢复策略，AgentDash 平台压缩不应接管它的私有 transcript；平台侧只维护自己拥有的 Agent runtime 历史、事件和可视化契约。
- Codex 参考实现强调 pre-turn / mid-turn / model downshift 压缩、`Total` / `BodyAfterPrefix` budget、replacement history checkpoint 和 resume replay suffix。
- Codex 的 session tree / branch 相关逻辑以 rollout JSONL 为事实日志：fork 会读取源 rollout history 并创建新 thread；rollback 不删除历史，而是追加 `ThreadRolledBack` event 后 replay 出逻辑状态；`rollout_reconstruction` 从最新 surviving replacement-history checkpoint 开始 replay suffix。
- Codex 的 `AgentGraphStore` 只维护 spawned thread parent/child topology：child 最多一个 parent，edge 有 `open` / `closed` 状态，children/descendants 列表要求稳定排序。
- Claude Code 参考实现强调 snip / microcompact / context collapse / autocompact / reactive compact 分层、输出预算预留、prompt-too-long retry、失败熔断和空摘要失败不落地。
- AgentDash 采用数据库仓储，已有 `sessions`、`session_events`、`SessionMeta.last_event_seq` 和 session repository abstraction；比 Codex 的文本 rollout 更适合用“不可变事件日志 + checkpoint 表 + lineage 索引”维护状态。
- checkpoint 持久化形态已确认采用 `event + repository` 双写：`context_compacted` / `ContextFrame` 作为 UI 审计事实，`session_checkpoints` 作为 restore / fork / rollback 查询事实源。

## Requirements

### R1. 压缩结果必须是可靠 checkpoint

- 压缩成功后必须持久化结构化 checkpoint，至少包含摘要、覆盖边界、replacement projection 或等价可恢复数据、token 统计、触发原因、阶段与版本。
- 冷启动 continuation / executor restore 必须优先从最新有效 checkpoint 恢复，再 replay checkpoint 之后的 suffix。
- checkpoint 必须能表达“压缩摘要 + 保留尾部 + 当前 canonical context 重新投影”的结果，不能只依赖 `messages_compacted` 计数。

### R2. 压缩策略必须可扩展

- 需要引入清晰的策略层，支持至少以下类别：
  - lightweight cleanup：工具结果瘦身、旧文件读取摘要化、媒体占位、重复 context frame 折叠。
  - summary compaction：对历史前缀生成 handoff summary。
  - checkpoint projection：生成可恢复 replacement projection。
  - reactive recovery：真实 provider overflow 后尝试恢复。
- 策略选择必须由模型窗口、当前请求估算、session 阶段、connector 能力、hook policy 共同决定，而不是写死在一个 hook preset 中。
- 策略接口需要允许后续接入 provider remote compact，但本任务不要求接入具体远端 compact API。
- 策略执行范围必须受 agent ownership 约束：只有平台维护 canonical transcript 的 AgentDash native / Pi Agent runtime 使用平台压缩；Codex Bridge 等自带 runtime 的 connector 保留自身压缩逻辑，平台只消费其对外事件和最终展示数据。

### R3. 触发判断必须面向“即将发送”的请求

- provider request 发出前，必须估算 `system_prompt + messages_for_llm + tools + runtime context` 的有效 token 使用。
- 自动压缩阈值必须预留输出空间和工具调用空间；`reserve_tokens` 必须参与实际 cut / projection。
- 需要支持 `Total` 与 `BodyAfterPrefix` 两种预算口径，避免稳定 bootstrap context 反复触发压缩。
- 需要覆盖 pre-turn、mid-turn 和 model downshift 三类触发语义。

### R4. 失败必须可恢复、可观察

- 摘要为空、API error、cancel、stream closed、checkpoint 写入失败时，不得替换当前有效历史。
- 压缩失败必须产生结构化事件或 hook trace，包含失败阶段和原因。
- 自动压缩需要连续失败熔断，避免每轮重复消耗 provider 调用。
- prompt-too-long 发生在压缩请求自身时，需要有有限次数的 head truncation / group retry 方案，失败后明确保留原历史。

### R5. ContextFrame 与 Backbone 契约保持一等可视化

- 成功压缩必须继续生成 `ContextFrame(kind="compaction_summary")`，并扩展到能展示 checkpoint id、strategy、trigger phase、tokens before / after、covered boundary、retained tail 信息。
- 失败或熔断需要可审计，但不应伪装成成功的 compaction summary。
- Backbone / NDJSON / 前端 feed 必须消费结构化 payload，不把业务语义塞入自由文本。

### R6. 不引入兼容性包袱

- 项目仍处于预研阶段，不需要支持旧 checkpoint 形态或双字段兼容。
- 如果需要数据库或持久化 schema 调整，直接通过 migration 收正到目标模型。
- 文档只记录目标架构为什么这样设计，不记录过去错误实现的形状。

### R7. checkpoint 必须支持会话分支和 rollback 语义

- checkpoint 边界必须绑定 `session_id + event_seq/ref`，能够判断某个 checkpoint 是否仍属于当前 active projection。
- session fork / branch 后，child session 可以继承 parent fork point 之前的 checkpoint，但必须把 fork point 固定下来，避免 parent 后续压缩或 rollback 改变 child 的基线。
- rollback 不应通过删除 `session_events` 实现；应通过 rollback transition 和 active projection cursor 表达当前模型可见状态。
- branch topology 应进入独立 lineage 索引，表达 parent/child、fork point、relation kind 和 edge status，不与 project/story/task owner binding 混在一起。

## Acceptance Criteria

- [ ] 规划产物明确当前实现差距、目标架构、实施顺序与验证方式。
- [ ] 代码实现后，空摘要 / API error / cancel 不会写入成功 checkpoint，也不会覆盖 runtime history。
- [ ] 代码实现后，自动压缩的 cut / projection 由 token budget 驱动，`reserve_tokens` 会影响保留尾部大小。
- [ ] 代码实现后，pre-provider 估算覆盖最终 `BridgeRequest` 的 system/messages/tools 主要输入。
- [ ] 代码实现后，成功压缩会持久化一等 checkpoint，并且 continuation / executor restore 使用 checkpoint + suffix 恢复。
- [ ] 代码实现后，`context_compacted` / `compaction_summary` payload 含有结构化边界、策略、阶段、token 与 checkpoint 元数据。
- [ ] 代码实现后，checkpoint 查询会尊重 active projection cursor；rollback 后不会继续使用已越过 rollback 目标的 checkpoint。
- [ ] 规划产物说明 session lineage / fork point / checkpoint 的数据库仓储关系，并明确哪些能力留给后续 session branch 任务。
- [ ] 代码实现后，至少有单元测试覆盖：成功 checkpoint、空摘要失败、压缩请求超窗重试、token budget cut、checkpoint 恢复、失败不污染历史。
- [ ] 代码实现后，相关 Rust 检查通过，跨层 DTO 如有变化则同步 TS 生成与前端消费。

## Out Of Scope

- 不接入具体第三方 remote compact endpoint。
- 不让平台压缩系统接管 Codex Bridge 的内部上下文窗口、历史裁剪或恢复 projection。
- 完整 session tree UI、branch 管理 API、用户可操作的 fork/rollback 产品流程由子任务 `.trellis/tasks/04-08-session-tree-branching` 承接；本任务只定义并落地 checkpoint 需要依赖的最小 lineage / projection 契约。
- 不实现完整 Claude Code context collapse 存储系统；本任务只预留策略接口与 lightweight cleanup 扩展点。
- 不做旧 checkpoint / 旧事件 payload 的长期兼容。
- 不改变 AGENTS / Trellis / workflow lifecycle 的既有协作规则。

## Open Question

- 无。已确认本任务实现后先用 `trellis-update-spec` 固化 session checkpoint / lineage / projection head 基础契约；完整 fork / rollback 产品语义等子任务完成后再补充。
