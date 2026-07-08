# Research: Claude Code context compaction

- Query: 调研 `references/claude-code` 中与 context compaction / summarization / auto compact / manual compact / resume after compact / conversation continuation 相关的实现、文档、测试。
- Scope: internal
- Date: 2026-07-07

## Scope

实际查看范围：

- 目录：`references/claude-code/src/services/compact/`、`references/claude-code/src/commands/compact/`、`references/claude-code/src/query.ts`、`references/claude-code/src/query/deps.ts`、`references/claude-code/src/utils/`、`references/claude-code/src/screens/`、`references/claude-code/src/entrypoints/sdk/`、`references/claude-code/README.md`。
- 关键词：`compact`、`autoCompact`、`manual`、`summary`、`resume`、`continue`、`conversationRecovery`、`compact_boundary`、`PreCompact`、`PostCompact`、`prompt too long`、`queryGuard`、`queue`、`idle`、`running`、`compact-only`、`next_turn`。
- 任务上下文：`.trellis/tasks/07-07-manual-context-compaction-trigger/prd.md`、`design.md`、`implement.md`。
- 相关规格：`.trellis/spec/backend/session/context-compaction-projection.md`、`.trellis/spec/backend/session/session-startup-pipeline.md`、`.trellis/spec/backend/session/streaming-protocol.md`、`.trellis/spec/backend/index.md`。
- External references: 未查外部网页或官方文档，本文件只基于本仓库本地 reference source。

Files found:

- `references/claude-code/README.md`：命令和源码目录索引，列出 `/compact`、`/resume` 和 `compact/` 模块。
- `references/claude-code/src/commands/compact/index.ts`：`/compact` slash command 元数据。
- `references/claude-code/src/commands/compact/compact.ts`：手动 compact command 的执行入口，包含 traditional、reactive、partial/session-memory 分支。
- `references/claude-code/src/services/compact/autoCompact.ts`：自动 compact 阈值、开关、失败熔断和 pre-provider compact 逻辑。
- `references/claude-code/src/services/compact/compact.ts`：共享 summary 生成、boundary/message 构造、hook、post-compact message assembly。
- `references/claude-code/src/services/compact/prompt.ts`：summary prompt 和压缩后 continuation prompt 文案。
- `references/claude-code/src/services/compact/microCompact.ts`：微压缩和 cache-edit 相关辅助链路。
- `references/claude-code/src/query.ts`：每轮 query 中 microcompact、autocompact、reactive compact、retry 和 prompt-too-long 处理。
- `references/claude-code/src/query/deps.ts`：query dependency 注入，生产实现接入 `microcompactMessages` 与 `autoCompactIfNeeded`。
- `references/claude-code/src/utils/messages.ts`：compact boundary message、compact boundary 查找、post-boundary slicing。
- `references/claude-code/src/utils/processUserInput/processSlashCommand.tsx`：slash command 结果接入，`type: "compact"` 结果阻止普通 query。
- `references/claude-code/src/utils/hooks.ts`：`PreCompact` / `PostCompact` hook 输入和执行。
- `references/claude-code/src/entrypoints/sdk/coreSchemas.ts`：compact hook 和 compact boundary SDK schema。
- `references/claude-code/src/utils/conversationRecovery.ts`：resume 时加载 transcript、恢复 skill state、处理中断对话。
- `references/claude-code/src/utils/sessionStorage.ts`：compact boundary 的 transcript parent/logical parent、preserved segment relink、resume chain 裁剪。
- `references/claude-code/src/screens/ResumeConversation.tsx`、`references/claude-code/src/main.tsx`：交互式和 CLI resume 把恢复消息传入 REPL。
- `references/claude-code/src/utils/handlePromptSubmit.ts`、`references/claude-code/src/utils/QueryGuard.ts`：运行中输入排队的通用机制。
- `references/claude-code/src/components/TokenWarning.tsx`、`references/claude-code/src/components/ContextVisualization.tsx`：context/auto-compact UI 提示。

## Key Findings

1. Claude Code 把 `/compact` 暴露为本地 slash command，语义是清理 conversation history 但保留 summary in context；README 同时把 `/resume` 描述为恢复 previous session。证据：`references/claude-code/README.md:139`、`references/claude-code/README.md:153`、`references/claude-code/README.md:168`、`references/claude-code/src/commands/compact/index.ts:4-12`。

2. 手动 `/compact` 只处理最近一次 compact boundary 之后的消息。command 入口先调用 `getMessagesAfterCompactBoundary(messages)`，避免重复压缩已经 compact 过的历史。证据：`references/claude-code/src/commands/compact/compact.ts:40-47`、`references/claude-code/src/utils/messages.ts:4618-4628`、`references/claude-code/src/utils/messages.ts:4643-4656`。

3. 手动 `/compact` 默认优先尝试 session memory compaction；无自定义指令时可能走 `trySessionMemoryCompaction`，否则走 `microcompactMessages` + `compactConversation(..., isAutoCompact=false)`。证据：`references/claude-code/src/commands/compact/compact.ts:55-80`、`references/claude-code/src/commands/compact/compact.ts:96-108`、`references/claude-code/src/commands/compact/compact.ts:120-124`。

4. 手动 `/compact` 还有 reactive-only 分支：它会执行 `executePreCompactHooks({ trigger: "manual" })`，再调用 `reactiveCompactOnPromptTooLong(... { trigger: "manual" })`。证据：`references/claude-code/src/commands/compact/compact.ts:85-94`、`references/claude-code/src/commands/compact/compact.ts:139-179`、`references/claude-code/src/commands/compact/compact.ts:197-215`。

5. slash command 返回 compact 结果后，Claude Code 会构造 post-compact messages，重置 microcompact state，并返回 `shouldQuery: false`，所以手动 `/compact` 本身不会继续发起普通 assistant query。证据：`references/claude-code/src/utils/processUserInput/processSlashCommand.tsx:679-702`、`references/claude-code/src/utils/processUserInput/processSlashCommand.tsx:512-515`。

6. 自动 compact 是 query 循环中的 pre-provider 步骤：先 microcompact，再 autocompact；如果 compact 成功，会 yield post-compact messages，并把 `messagesForQuery` 替换为 compact 后上下文，随后同一个用户 query 继续进入 provider 调用。证据：`references/claude-code/src/query.ts:412-426`、`references/claude-code/src/query.ts:453-468`、`references/claude-code/src/query.ts:470-536`、`references/claude-code/src/query/deps.ts:21-37`。

7. 自动 compact 阈值基于有效 context window 减 buffer token，并受环境变量、用户配置和 circuit breaker 控制；连续失败达到上限后会跳过 auto compact。证据：`references/claude-code/src/services/compact/autoCompact.ts:62-90`、`references/claude-code/src/services/compact/autoCompact.ts:147-158`、`references/claude-code/src/services/compact/autoCompact.ts:160-238`、`references/claude-code/src/services/compact/autoCompact.ts:241-276`、`references/claude-code/src/services/compact/autoCompact.ts:334-349`。

8. 自动 compact 失败时还有 reactive fallback：当 provider 返回 prompt-too-long 或相关 media/context 错误时，query 层会 compact、yield post-compact messages，并设置 retry state。证据：`references/claude-code/src/query.ts:593-647`、`references/claude-code/src/query.ts:1119-1165`。

9. `compactConversation` 是自动和手动路径的共享核心。它生成 summary request、执行 pre/post compact hooks、创建 boundary 和 summary messages，并返回统一 `CompactionResult`。证据：`references/claude-code/src/services/compact/compact.ts:299-309`、`references/claude-code/src/services/compact/compact.ts:387-395`、`references/claude-code/src/services/compact/compact.ts:413-418`、`references/claude-code/src/services/compact/compact.ts:440-452`、`references/claude-code/src/services/compact/compact.ts:719-748`。

10. compact 后上下文的顺序是 boundary marker、summary messages、messagesToKeep、attachments、hookResults。summary message 是一个 synthetic user message，标记 `isCompactSummary: true` 和 `isVisibleInTranscriptOnly: true`。证据：`references/claude-code/src/services/compact/compact.ts:596-624`、`references/claude-code/src/services/compact/compact.ts:330-337`。

11. compact boundary 是 system message subtype `compact_boundary`，metadata 记录 `trigger: "manual" | "auto"`、`preTokens`、可选 `messagesSummarized`、可选 `userContext`，并可通过 `logicalParentUuid` 指向 compact 前最后一条消息。证据：`references/claude-code/src/utils/messages.ts:4530-4555`、`references/claude-code/src/utils/messages.ts:4608-4612`、`references/claude-code/src/services/compact/compact.ts:596-624`。

12. partial compact 会在 boundary metadata 里记录 preserved segment，后续 resume 用它把被保留的消息片段重新接回 compact 后链路。证据：`references/claude-code/src/services/compact/compact.ts:1014-1020`、`references/claude-code/src/services/compact/compact.ts:1031-1044`、`references/claude-code/src/services/compact/compact.ts:1069-1088`、`references/claude-code/src/services/compact/compact.ts:341-356`。

13. pre/post compact hook 都携带 `trigger`，post hook 还携带 `compact_summary`；SDK schema 明确把 trigger 枚举限制为 `manual` / `auto`。证据：`references/claude-code/src/utils/hooks.ts:3961-3983`、`references/claude-code/src/utils/hooks.ts:4034-4054`、`references/claude-code/src/entrypoints/sdk/coreSchemas.ts:569-586`。

14. summary prompt 要求包含 primary request、key concepts、files/code、errors/fixes、user messages、pending tasks、current work、optional next step；compact summary user message 以“previous conversation ran out of context”的 continuation frame 开头。证据：`references/claude-code/src/services/compact/prompt.ts:61-77`、`references/claude-code/src/services/compact/prompt.ts:293-303`、`references/claude-code/src/services/compact/prompt.ts:337-345`。

15. 当 `suppressFollowUpQuestions` 为 true 时，summary user message 会追加明确 continuation 指令：不要询问、不要确认 summary、直接从断点继续。自动 compact 调用 `compactConversation` 时传入 `suppressFollowUpQuestions=true`。证据：`references/claude-code/src/services/compact/prompt.ts:357-360`、`references/claude-code/src/services/compact/prompt.ts:361-368`、`references/claude-code/src/services/compact/autoCompact.ts:279-321`。

16. resume 不只是加载原始 transcript：`loadConversationForResume` 会读取 session log、恢复 skill state、反序列化并处理 interrupted turn，再执行 `processSessionStartHooks("resume")` 把 hook messages 加回上下文。证据：`references/claude-code/src/utils/conversationRecovery.ts:164-218`、`references/claude-code/src/utils/conversationRecovery.ts:375-392`、`references/claude-code/src/utils/conversationRecovery.ts:456-568`。

17. compact boundary 会影响 transcript parent 链：写入 transcript 时 boundary 的 `parentUuid` 为 `null`、`logicalParentUuid` 指向原 parent；注释说明 compaction 后新 boundary/summary 会先出现，并通过 boundary 截断 `--continue` chain。证据：`references/claude-code/src/utils/sessionStorage.ts:1025-1042`、`references/claude-code/src/utils/sessionStorage.ts:1391-1408`。

18. resume 加载时会处理 compact preserved segment relink，裁掉最后一个 compact boundary 之前且不应保留的消息，避免恢复时重新 materialize 已压缩历史。证据：`references/claude-code/src/utils/sessionStorage.ts:1823-1836`、`references/claude-code/src/utils/sessionStorage.ts:1839-1858`、`references/claude-code/src/utils/sessionStorage.ts:1905-1939`、`references/claude-code/src/utils/sessionStorage.ts:1942-1955`、`references/claude-code/src/utils/sessionStorage.ts:3521-3541`、`references/claude-code/src/utils/sessionStorage.ts:3704-3716`。

19. 交互式 resume 和 CLI resume 都把 `loadConversationForResume` 的 messages 作为 REPL initial messages。证据：`references/claude-code/src/screens/ResumeConversation.tsx:191-197`、`references/claude-code/src/screens/ResumeConversation.tsx:276-297`、`references/claude-code/src/main.tsx:3675-3684`、`references/claude-code/src/main.tsx:3720-3740`。

20. 运行中输入排队是通用 query guard 行为，不是 compact-specific command state。`QueryGuard` 只有 `idle`、`dispatching`、`running`，`handlePromptSubmit` 在 active 时把 prompt/bash command 排进队列。未发现 `compact` 专用 running/idle 分支。证据：`references/claude-code/src/utils/QueryGuard.ts:1-18`、`references/claude-code/src/utils/handlePromptSubmit.ts:313-350`、`references/claude-code/src/utils/handlePromptSubmit.ts:426-438`。

21. UI 层有 token pressure / auto-compact 提示；当 auto compact 不可用时，会提示运行 `/compact`。证据：`references/claude-code/src/components/TokenWarning.tsx:95`、`references/claude-code/src/components/TokenWarning.tsx:166-169`、`references/claude-code/src/components/ContextVisualization.tsx:12`。

## Two Trigger Modes

Claude Code 中有两个明确 compact trigger mode：

- Automatic pre-provider compact：query 循环在 provider API call 前调用 `autoCompactIfNeeded`，阈值满足时压缩并让当前用户 query 继续执行。证据：`references/claude-code/src/query.ts:453-536`、`references/claude-code/src/services/compact/autoCompact.ts:160-238`、`references/claude-code/src/services/compact/autoCompact.ts:279-321`。
- Manual slash command compact：用户输入 `/compact` 后，slash command 本地执行 compaction，返回 `type: "compact"`，上层返回 `shouldQuery: false`，因此命令本身不会立即继续普通 assistant response。证据：`references/claude-code/src/commands/compact/index.ts:4-12`、`references/claude-code/src/utils/processUserInput/processSlashCommand.tsx:679-702`。

还有一个与 trigger mode 相近但不是用户显式 trigger 的分支：

- Reactive prompt-too-long compact：provider 拒绝请求后，query 层执行 compact 并重试。证据：`references/claude-code/src/query.ts:1119-1165`。

Running / idle 分支结论：

- not found in inspected scope: 未发现 AgentDashboard 设计里这种“运行中只记录 one-shot manual intent，下一轮 provider 前强制 compact”的专用状态或 durable request。
- not found in inspected scope: 未发现“空闲时启动 compact-only turn，只有 compaction，没有普通 assistant answer”的独立 turn 语义。
- found only as generic queue: 如果用户在 query active 时提交输入，`handlePromptSubmit` 会把 prompt/bash command 排队，之后按普通输入处理；这不是 compact-specific pending request。证据：`references/claude-code/src/utils/QueryGuard.ts:1-18`、`references/claude-code/src/utils/handlePromptSubmit.ts:313-350`。

## Resume/Continuation Semantics

Claude Code 的压缩后继续语义由三层组成：

1. Message-level boundary + summary：compact 后 message list 以 `compact_boundary` system message 开头，后接 synthetic compact summary user message。summary 文案描述这是从 context 耗尽的 previous conversation 继续，并把 summary 作为 continuation context。证据：`references/claude-code/src/services/compact/compact.ts:596-624`、`references/claude-code/src/services/compact/compact.ts:330-337`、`references/claude-code/src/services/compact/prompt.ts:337-360`。

2. Auto compact 的 same-turn continuation：自动 compact 传入 `suppressFollowUpQuestions=true`，summary user message 会要求 agent 不询问、不确认 summary、直接继续当前任务；query 层将 `messagesForQuery` 替换成 post-compact messages 后继续同一轮 provider 调用。证据：`references/claude-code/src/services/compact/autoCompact.ts:279-321`、`references/claude-code/src/services/compact/prompt.ts:357-360`、`references/claude-code/src/query.ts:470-536`。

3. Transcript resume continuation：resume 从 session log 构造 parent chain，compact boundary 截断旧 chain；如果存在 preserved segment，会 relink 保留片段；中断 turn 会追加 meta user message “Continue from where you left off.”。证据：`references/claude-code/src/utils/sessionStorage.ts:1025-1042`、`references/claude-code/src/utils/sessionStorage.ts:1391-1408`、`references/claude-code/src/utils/sessionStorage.ts:1823-1955`、`references/claude-code/src/utils/sessionStorage.ts:2069-2087`、`references/claude-code/src/utils/conversationRecovery.ts:164-218`。

not found in inspected scope:

- 未发现把 summary 注入为“特殊系统消息”的证据；实际 compact summary 是 user message，boundary 才是 system subtype `compact_boundary`。证据：`references/claude-code/src/services/compact/compact.ts:596-624`、`references/claude-code/src/utils/messages.ts:4530-4555`。
- 未发现 AgentDashboard-style frontend stream event，例如 `context_compacted` 的 durable projection payload。Claude Code 更偏向本地 message array / transcript chain / remote internal event 标记。证据：`references/claude-code/src/utils/sessionStorage.ts:1307-1315`。

## Lessons For AgentDashboard

可直接采纳：

- 保留 `trigger` 作为一等 metadata，并继续区分 `manual` / `auto`；AgentDashboard 当前设计中的 `reason="user_requested"` 可以作为比 Claude Code 更明确的补充。证据：`references/claude-code/src/utils/messages.ts:4530-4555`、`references/claude-code/src/entrypoints/sdk/coreSchemas.ts:569-586`。
- 自动 compact 应保持在 provider 调用前，并在压缩成功后继续当前 query，不把 compact 暴露成普通 assistant 回复。证据：`references/claude-code/src/query.ts:453-536`。
- 手动 compact 后不应该产生“正常 assistant answer”。Claude Code 对 `/compact` 返回 `shouldQuery: false`；AgentDashboard 的 idle compact-only turn 可以采用同样的产品语义，但需要用 runtime events 表达 completion。证据：`references/claude-code/src/utils/processUserInput/processSlashCommand.tsx:679-702`。
- summary prompt 应显式保留 primary request、current work、pending task、recent user intent 和 next step，自动/compact-only continuation 需要明确要求 agent 不询问、不确认 summary、直接继续。证据：`references/claude-code/src/services/compact/prompt.ts:61-77`、`references/claude-code/src/services/compact/prompt.ts:357-360`。
- compact boundary 应有可追踪身份，并和“最后被压缩位置 / 第一条保留位置”建立明确关系。Claude Code 用 compact boundary、logical parent、preserved segment 实现 transcript continuation；AgentDashboard 可以映射为 `compacted_until_ref`、`first_kept_ref`、projection head。证据：`references/claude-code/src/utils/messages.ts:4530-4555`、`references/claude-code/src/utils/sessionStorage.ts:1025-1042`、`references/claude-code/src/utils/sessionStorage.ts:1823-1955`。
- pre/post compact hook 或等价 lifecycle event 应带 trigger、custom instruction、compact summary，便于审计和 UI 刷新。证据：`references/claude-code/src/utils/hooks.ts:3961-3983`、`references/claude-code/src/utils/hooks.ts:4034-4054`。

不适合直接照搬：

- 不适合把本地 message array / transcript parent chain 当成 AgentDashboard 的权威 compaction 状态。AgentDashboard 已有 spec 要求 `session_compactions`、`session_projection_segments`、`session_projection_heads` 在一个 unit 内提交，并通过 projection head 控制模型输入。
- 不适合只依赖通用输入队列处理“运行中手动 compact”。Claude Code 未发现 durable request/idempotent command receipt；AgentDashboard PRD 已要求 `client_command_id` idempotency 和 running/idle 明确分支。
- 不适合照搬 `/compact` 的 purely local slash-command 模型。AgentDashboard 是后端 AgentRun runtime，需要 endpoint、request durable state、runtime stream event 和 frontend projection refresh。
- 不适合把 prompt-too-long reactive compact 作为主要 manual trigger 机制。Claude Code reactive branch 是 provider rejection 后的 fallback；AgentDashboard 的 manual trigger 应该在 provider 前强制执行，且仍遵守 eligibility/cut-point validation。

## Suggested Improvements Beyond Manual Trigger

仅列和当前 compaction 链路相关的优化建议：

1. 为 manual 和 auto compact 都记录 compact lifecycle status：started、summary requested、projection committed、failed。Claude Code 有 pre/post hooks 和 progress 回调；AgentDashboard 可以映射到 runtime event 与 projection commit diagnostics。证据：`references/claude-code/src/utils/hooks.ts:3961-4054`、`references/claude-code/src/services/compact/compact.ts:719-748`。

2. 加入连续失败熔断或降级诊断，避免 auto compact 在同一类错误上反复尝试；manual compact 失败应返回清晰 command result 和可展示 failure reason。证据：`references/claude-code/src/services/compact/autoCompact.ts:334-349`、`references/claude-code/src/services/compact/compact.ts:750-752`。

3. 在 compact commit 后刷新 token accounting，避免 resume 或下一轮立即因为陈旧 usage 再触发 compact。Claude Code preserved segment relink 会清理 stale assistant usage。证据：`references/claude-code/src/utils/sessionStorage.ts:1905-1939`。

4. compact summary 附带结构化 context frame：pending tasks、recent files、tool state、skill/tool metadata，而不只依赖自由文本 summary。Claude Code 已保留 hook results、attachments 和 invoked skill state；AgentDashboard 可以把这些落到 projection/context frame。证据：`references/claude-code/src/services/compact/compact.ts:330-337`、`references/claude-code/src/utils/conversationRecovery.ts:375-392`。

5. 增加压缩后“继续行为”测试：自动 compact same-turn continuation、手动 idle compact-only 无 assistant response、running request 下一 turn 强制 compact、resume after compact 不恢复已压缩历史。reference 中未发现对应 test files，因此 AgentDashboard 应补上这些契约测试。not found evidence: 对 `references/claude-code` 下 `(__tests__|.test.|.spec.)` 与 compact/resume/session/conversation/context 的组合搜索没有匹配。

## Caveats / Not Found

- not found in inspected scope: 未发现 compact/resume 相关 test/spec 文件。
- not found in inspected scope: 未发现 AgentDashboard-style `client_command_id` idempotency、durable manual compaction request 或 endpoint。
- not found in inspected scope: 未发现 running active manual compact intent consumed next turn 的专用实现。
- not found in inspected scope: 未发现 idle compact-only turn 的独立实现。
- not found in inspected scope: 未发现 durable projection store 等价物；Claude Code 主要使用 post-compact message list、transcript boundary、parent/logical parent chain 和 remote internal event marker。
- not found in inspected scope: 未发现 `context_compacted` frontend stream event payload；closest evidence 是 compact boundary transcript/internal event 标记。证据：`references/claude-code/src/utils/sessionStorage.ts:1307-1315`。
- 本研究没有评价 `references/claude-code` 中与 context compaction 无关的 context-management 功能，也没有查外部 Claude Code 文档。
