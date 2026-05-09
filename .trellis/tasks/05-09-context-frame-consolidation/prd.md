# ContextFrame 统一上下文帧收束

## Goal

将 Agent 实际收到的系统信息、动态上下文、能力变化、pending action、auto-resume、compaction summary 等统一收束为 `ContextFrame`。前端会话流应按 frame 标准化绘制，使用户能逐块追踪 Agent 行为被哪些上下文影响，而不是依赖 `AgentBaseline`、`HookTrace`、零散 system prompt 拼接或 Agent 自述来反推。

## 背景

`agent-steering-visibility` 已经引入 `RuntimeContextNotice`，解决了 capability / workflow runtime update 的一部分可视化问题。但当前系统仍存在多条 Agent-visible 信息入口：

- 启动期 `SessionStart` snapshot injection 被前端显示成 `BASE / Agent baseline`。
- `SessionContextBundle.bootstrap_fragments`、VFS、skills、project guidelines、hook runtime 状态等仍被直接拼进 system prompt。
- tool、VFS、MCP、skills 等能力提示有多处 legacy prompt builder，未统一由 capability 系统维护并产出 frame。
- `HookTurnStartNotice` 虽然可以携带 `RuntimeContextNotice`，但最终会合并成一条 user message。
- `HookPendingAction`、auto-resume prompt、compaction summary 仍各自手写 Markdown 或静默处理。
- 前端仍存在 `已注入动态上下文`、`baseline_initialized` 等 legacy 展示路径。

用户期望的产品语义是：所有实际注入给 Agent 的消息都能在前端以一块一块的形式清晰追踪。`ContextFrame` 是比 `RuntimeContextNotice` 更准确的抽象，因为它覆盖的不只是 runtime notice，而是所有进入 Agent context 的 frame。

## Review Findings

### P1: `AgentBaseline` 是用户不可理解的 legacy 概念，应从用户可见 UI 中剃掉

启动阶段 workflow snapshot 先被构造成 `HookInjection`：

- `crates/agentdash-application/src/hooks/provider.rs`
- `crates/agentdash-application/src/hooks/workflow_contribution.rs`

随后同一批 snapshot injection 又被合并到 `SessionContextBundle.bootstrap_fragments`，并进入 system prompt：

- `crates/agentdash-application/src/session/prompt_pipeline.rs`

但 `SessionStart` hook trace 仍将该行为命名为 `baseline_initialized`：

- `crates/agentdash-application/src/session/hub_support.rs`
- `crates/agentdash-spi/src/hooks/trace.rs`

前端再渲染为 `BASE / Agent baseline 已注入 N 项上下文`：

- `frontend/src/features/session/ui/SessionSystemEventCard.tsx`

这使用户看到的是内部生命周期词，而不是 Agent 实际收到的 workflow / project / context 内容。应改为 `ContextFrame(kind=bootstrap_context | workflow_context | project_context)`。

### P1: system prompt 混入过多动态上下文，导致 Agent-visible 信息不可审计

`system_prompt_assembler.rs` 当前把以下内容直接拼为一个完整 system prompt：

- `Identity`：base system prompt、agent-level system prompt、user preferences。
- `Project Guidelines`：AGENTS.md / MEMORY.md。
- `Project Context`：来自 `SessionContextBundle` 的 task/story/project/workflow/constraint/companion 等 fragments。
- `Workspace`：VFS mounts、default mount。
- `Hooks`：hook runtime 状态与 pending action 数量提示。
- `Skills`：可见 skills 列表和加载说明。

其中只有稳定 Agent 身份定义适合长期留在 core system prompt；其余大多是会随 session、owner、workflow、capability、workspace 或 turn 变化的动态上下文，原则上应作为 `ContextFrame` 生成、持久化、渲染，并由 frame renderer 投递给 Agent。

### P1: tool / VFS / MCP / skills 能力提示仍散落在 prompt builder 中，所有权错误

当前能力提示存在几条 legacy 路径：

- `session/plan.rs` 生成 `legacy:session_plan` fragments：`vfs`、`tools`、`runtime_policy` 等。
- `system_prompt_assembler.rs` 直接写入 `Workspace`，把 VFS mounts/default mount 拼到 system prompt。
- `system_prompt_assembler.rs` 根据 `runtime_tools` 推导 read tool，再拼出 `<available_skills>`。
- `agentdash-spi/src/platform/skill.rs` 注释仍声明 connector 将 skill 注入 system prompt。
- `agentdash-spi/src/platform/tool_capability.rs` 仍保留 `format_tool_for_prompt` 这类工具 prompt formatter。

这些信息本质上不是 system prompt assembler 的职责，而是 capability surface 的职责。tool、VFS、MCP、skills 都应由 capability registry/state 维护，统一生成 `ContextFrameSection`：

- `tool_surface` / `tool_surface_delta`
- `workspace_surface`
- `mcp_surface`
- `skill_surface`
- `capability_delta`

system prompt assembler 不应再从运行时工具列表反推能力提示，也不应手写 VFS/skills guidance；它只消费已经决定好的 frame render output，或者完全不参与这些动态能力帧。

### P1: `HookTurnStartNotice` 的 UI frame 与 Agent message 不是严格一一对应

`RuntimeContextNotice` 会生成结构化事件，但 turn start 消费时会把多个 notice 拼接成一条 `AgentMessage::user`，再额外包上 `[运行时上下文更新] notice_id/source`。这造成：

- 前端看到多张 notice card。
- Agent 实际收到一条合并 user message。
- `agent_visible_text` 不是完整消息字面量，只是其中一段。

`ContextFrame` 应成为消息投递单位：每个 frame 明确 `delivery_channel`、`message_role`、`rendered_text`，Agent 消息由 frame list 渲染，前端也按同一 frame list 绘制。

### P1: `HookPendingAction` 仍是 bespoke Markdown 注入，未进入统一 frame 管线

`HookPendingAction` 在 turn start 被构造成 `[待处理 Hook 事项]` user message，文案分散在 `hook_delegate.rs` 与 `hook_messages.rs`。pending action 对 Agent 行为影响很强，应作为 `ContextFrame(kind=pending_action)`，包含 action id/type/status、summary、关联 injections、要求 Agent 执行的 instruction、owner 信息等结构化 section。

### P1: auto-resume 是系统发起的 Agent-visible prompt，但目前像普通用户输入

Hook auto-resume 使用 `AUTO_RESUME_PROMPT` 构造普通 `PromptSessionRequest::from_user_input(...)`。这会让系统推动 Agent 继续执行，但前端缺少“系统自动续跑提示已注入”的 frame。应改为 `ContextFrame(kind=auto_resume)` 或至少在该 prompt 启动时同时持久化 frame，并在会话流中标记其系统来源。

### P1: compaction summary 当前存在 system prompt 注入路径，应迁出为 continuation frame

`continuation.rs` 明确保留了 `render_system_context_markdown(&transcript, owner_context) -> system prompt 注入` 路径。compaction checkpoint 会被投影为 `AgentMessage::CompactionSummary`，再在 `render_system_context_markdown` 中渲染成 `#### 历史摘要` 并拼入 `## Session Continuation`。

这条路径把“上一段历史被压缩成什么”藏进 system prompt，前端 stream 又静默 `context_compacted`。结果是 Agent 后续行为被 summary 影响，但用户看不到具体是哪份 summary 在起作用。

正确形态应是：

- compaction checkpoint 生成 `ContextFrame(kind=compaction_summary)`。
- frame section 包含 summary、tokens_before、messages_compacted、compacted_until_ref、timestamp。
- continuation 时 Agent 收到的历史摘要文本由该 frame render。
- 前端正常展示 compaction frame，不再把关键上下文变化静默掉。

### P2: capability/tool surface 部分已经 frame 化，但命名与范围偏窄

`RuntimeContextNotice` 当前覆盖 capability delta、tool schema delta、workflow context update 等 runtime notice。它应被重命名/升级为 `ContextFrame`，成为通用上下文帧，而不是继续作为 runtime steering 的局部实现。`runtime_context_notice` session meta key 也应硬切为 `context_frame`。

### P2: 前端 legacy hook trace 展示仍会泄漏错误抽象

前端仍保留：

- `BASE`
- `Agent baseline 已注入...`
- `已注入动态上下文`
- `已追加流程约束（steering）`

这些应只在 debug/legacy verbose 模式下出现。正常会话流应以 `ContextFrameCard` 为唯一 Agent-visible context UI。

## Target Architecture

### Core Type: `ContextFrame`

`ContextFrame` 是跨层 wire envelope，不承担具体业务 frame 的文本渲染职责。各模块应先产出自己的 typed metadata，再由该模块内聚地生成：

- `sections`：给前端标准化绘制的结构化数据。
- `rendered_text`：Agent 实际收到的文本。

也就是说，`tool_surface`、`capability_delta`、`workflow_context`、`pending_action`、`auto_resume`、`compaction_summary` 等 subtype 应各自拥有 metadata -> sections/rendered_text 的 builder。禁止继续沉淀一个按所有 `ContextFrameSection.kind` 做大 `match` 的中心化 renderer；公共层只允许做信封字段组装、投递与持久化。

### Frame 汇聚点

多个模块可以在同一个 delivery boundary 产出多个 `ContextFrame`。这些 frame 不应在 UI 中平铺成一串 verbose 卡片，而应有明确汇聚点：

- 后端 `HookTurnStartNotice` 队列是当前 turn-start delivery boundary 的汇聚点。它可以批量消费多个 frame，并以 batch envelope 投递给 Agent，但 frame 本身仍保持独立 metadata、sections 与 rendered_text。
- 前端 `useSessionFeed` 是用户侧会话流的汇聚点。相邻的 `context_frame` session meta event 应聚合为一张“Agent 上下文批量更新”卡片，默认展示 frame/section/kind 摘要，展开后再显示每个 frame。
- 后续 bootstrap、pending action、auto-resume、compaction 迁移进 `ContextFrame` 后，也进入同一汇聚点，而不是各自新增一条专用 UI 流。

建议字段：

- `id`
- `kind`
- `source`
- `session_id`
- `turn_id`
- `phase_node`
- `created_at_ms`
- `delivery_status`
- `delivery_channel`
- `message_role`
- `rendered_text`
- `sections`
- `references`

`delivery_channel` 示例：

- `system_prompt`
- `turn_start`
- `user_prompt`
- `provider_request`
- `continuation`

`message_role` 示例：

- `system`
- `user`
- `developer`（如果 connector 支持；不支持时由 renderer 映射）

`references` 用于连接底层状态：

- capability state revision
- hook trace sequence
- pending action id
- compaction checkpoint ref
- workflow run/step id
- context bundle id

### Section Kinds

首批需要支持：

- `identity`：稳定 Agent 身份定义。默认留在 core system prompt，但可作为 debug/audit frame 记录。
- `project_guidelines`：AGENTS.md / MEMORY.md 等项目规则摘要与完整文本。
- `user_preferences`：用户偏好。
- `project_context`：task/story/project/required context/static fragments。
- `workflow_context`：active step、workflow guidance、lifecycle node、runtime policy。
- `workspace_surface`：VFS mounts、default mount、working directory。
- `tool_surface`：初始 provider tool list 摘要。
- `tool_surface_delta`：由 `CapabilityStateDelta` 生成的工具变化。
- `mcp_surface`：当前可见 MCP server / transport / scope 摘要。
- `skill_surface`：当前可见 skills 列表、触发策略、加载路径。
- `capability_delta`：capability/MCP/VFS 变化。
- `hook_injection`：普通 hook 注入。
- `pending_action`：HookPendingAction。
- `auto_resume`：系统自动续跑提示。
- `compaction_summary`：压缩摘要与边界。
- `system_notice`：其他系统级告知。

### System Prompt 收束原则

core system prompt 保留：

- Agent / platform 的稳定身份定义。
- 模型必须遵守的不可变协议。
- 解释 `ContextFrame` 是动态上下文权威来源的简短规则。

应迁出 core system prompt，改由 `ContextFrame` 投递：

- Project Guidelines。
- User Preferences。
- Project Context / SessionContextBundle fragments。
- Workflow Context / Lifecycle Runtime Policy。
- Workspace / VFS surface。
- Hook Runtime 状态。
- Skills / skill surface。
- Tool schema / tool surface。
- MCP surface。
- Pending action。
- Auto-resume。
- Compaction summary。

### Capability Ownership Principles

- Capability registry/state 是 tool、VFS、MCP、skills 的唯一权威来源。
- `ContextFrame` 只消费 capability state/delta 生成 section，不从 system prompt assembler 或 session plan legacy markdown 反推能力。
- `system_prompt_assembler` 不得直接生成 tool/VFS/MCP/skills guidance。
- 初始能力面由 `ContextFrame(kind=capability_surface)` 或多个 surface sections 展示；运行期变化由 `CapabilityStateDelta` 生成 delta frame。
- skills 需要纳入 capability 维度，至少表达 visible/disabled-for-model-invocation/source/path/description；不再通过 `<available_skills>` 隐式塞进 system prompt。
- `legacy:session_plan` 的 `vfs`、`tools`、`runtime_policy` fragments 需要删除或降级为 debug audit，不能进入 Agent-visible system prompt。

## Proposed Implementation Plan

### Phase 1: Inventory 与类型收束

- 新增 `ContextFrame` / `ContextFrameSection` 类型，替代或重命名 `RuntimeContextNotice`。
- 明确 frame renderer 是 Agent-visible text 的唯一来源。
- 将 session meta event key 从 `runtime_context_notice` 硬切为 `context_frame`。
- 明确 capability state/registry 到 frame sections 的转换层，禁止 prompt builder 自己生成 tool/VFS/MCP/skills 能力提示。
- 增加 tests 确认 `rendered_text` 与 sections 同源。

### Phase 2: Capability surface 迁移

- 将现有 capability delta / tool schema delta / workflow context update 迁移到 `ContextFrame`。
- 将 tool surface、VFS/workspace surface、MCP surface、skill surface 都改由 capability state/registry 生成 frame section。
- 删除或停用 `session/plan.rs` 中进入 Agent context 的 `legacy:session_plan` 能力 fragments。
- 删除 `system_prompt_assembler` 中直接拼接 Workspace / Skills 的能力提示。
- 删除 `STEER`/`RuntimeContextNotice` 命名残留。
- 保持运行时工具变化只发送 delta，不发送完整 schema dump。

### Phase 3: Bootstrap/system prompt 瘦身

- 将 `SessionContextBundle.bootstrap_fragments` 渲染为 `ContextFrame(kind=bootstrap_context/project_context/workflow_context)`。
- 移除用户可见 `baseline_initialized` / `AgentBaseline` 展示。
- `HookTrace` 仅记录 lifecycle/audit，不再作为 context 内容 UI。
- `system_prompt_assembler` 只保留稳定 identity/protocol，其余动态 sections 从 frames 渲染或由 connector 按 frame list 投递。

### Phase 4: Pending/action/auto-resume/compaction 纳入 frame

- `HookPendingAction` 生成 `ContextFrame(kind=pending_action)`，Agent message 与前端卡片同源。
- Hook auto-resume 生成 `ContextFrame(kind=auto_resume)`，并标记系统发起。
- compaction summary 生成 `ContextFrame(kind=compaction_summary)`；删除 `render_system_context_markdown` 将 compaction summary 拼入 system prompt 的路径，前端不再静默关键上下文变化。

### Phase 5: 前端统一渲染

- 新增或重命名为 `ContextFrameCard`。
- 正常会话流只用 `context_frame` 展示 Agent-visible context。
- `baseline_initialized`、`context_injected`、`steering_injected` 仅保留 debug/verbose。
- 每张 card 默认展示摘要，可展开 sections 与 `rendered_text`。

### Phase 6: 清理与验证

- 删除旧 `AgentBaseline` 文案和 BASE badge。
- 删除 RuntimeContextNotice 过窄命名。
- 更新 specs。
- 增加 backend/frontend 回归测试。
- 手动验证 Plan -> Apply、pending action、auto-resume、compaction 场景。

## Legacy Review Notes

首批硬切后应区分“已可删除”和“暂不能删除”的旧链路：

- 已可删除：`RuntimeContextNotice` / `runtime_context_notice` / `agent_visible_text` 运行时代码与前端解析命名；这些应硬切为 `ContextFrame` / `context_frame` / `rendered_text`。
- 已可隐藏：普通会话流中的 `baseline_initialized`、`context_injected`、`steering_injected` hook trace 卡片。它们不再作为 Agent-visible context UI，只有 debug/verbose 才可观察。
- 暂不能直接删除：`baseline_initialized` / `baseline_refreshed` 作为 SessionStart hook trace 决策名仍承担 bootstrap lifecycle audit；在 bootstrap/project guidelines/system prompt 瘦身完成前，直接删除会让启动期上下文链路更不可追踪。
- 下一批必须迁移：bootstrap fragments、pending action、auto-resume、compaction summary 都应产出 `ContextFrame` subtype，并进入同一 turn-start/feed 汇聚点。完成后再删除 legacy hook trace injection 语义。

## Implementation Progress

- 2026-05-09：已将 `RuntimeContextNotice` / `runtime_context_notice` /
  `agent_visible_text` 硬切为 `ContextFrame` / `context_frame` / `rendered_text`。
- 2026-05-09：已新增 `bootstrap_context` frame，将用户偏好、项目规则与
  `SessionContextBundle` 启动片段从 core system prompt 迁出。
- 2026-05-09：已新增独立启动 surface frames：`workspace_surface`、
  `skill_surface`、`hook_runtime_surface`。这些 frame 各自持有 typed metadata 与
  renderer，不合并成单个“大 surface”；前端只在 feed 层把相邻 `context_frame`
  折叠为批量更新卡片。
- 2026-05-09：`system_prompt_assembler` 已瘦身为只渲染稳定 Identity；
  Workspace / Hook Runtime / Skills / Project Context 不再由 core system prompt 拼接。
- 2026-05-09：已新增 `auto_resume` frame，系统续跑提示的真实文本会以
  `delivery_channel=user_prompt` 持久化并展示。
- 2026-05-09：已新增 `compaction_summary` frame，`context_compacted` 会伴随持久化
  可见的摘要、token、消息数与 compacted boundary。
- 仍待迁移：`pending_action` 仍需专用 ContextFrame subtype；continuation 的
  `render_system_context_markdown` 命名与 bootstrap 包装仍是后续清理点。

## Acceptance Criteria

- [ ] 前端会话流中不再出现用户可见 `Agent baseline` / `BASE` / `已注入动态上下文` 作为新路径展示。
- [ ] 所有 Agent-visible 动态上下文都持久化为 `context_frame` session meta event。
- [ ] Agent 实际收到的 frame 文本与前端“Agent 实际收到的文本”展开区一致。
- [ ] system prompt 不再直接拼入 Project Guidelines、Project Context、Workflow Context、Workspace、VFS、MCP、Skills、Hook Runtime 状态、compaction summary 等动态内容。（除 compaction summary 迁移仍待后续批次）
- [ ] tool、VFS、MCP、skills 的提示全部由 capability state/registry 生成 `ContextFrameSection`，不是由 prompt assembler 或 legacy session plan markdown 生成。
- [ ] pending action、auto-resume、compaction summary 均有专用 `ContextFrameSection`。（auto-resume / compaction summary 已完成）
- [ ] capability/tool surface runtime update 继续由 capability state delta 驱动，不退回 full schema dump。
- [ ] `legacy:session_plan` 中的 `vfs`、`tools`、`runtime_policy` 不再进入 Agent-visible prompt。
- [ ] hook trace 只表达 lifecycle/audit，不作为 Agent-visible context card 的数据源。
- [ ] 前端能按 frame kind 标准化渲染摘要、section 细节、完整文本。

## Test Plan

### Backend

- `ContextFrame` renderer 单测：sections 与 `rendered_text` 同源。
- bootstrap context frame 单测：workflow/project fragments 不再通过 baseline trace 对用户展示。
- system prompt assembler 单测：动态 sections 已迁出，只保留 identity/protocol。
- capability surface frame 单测：tool/VFS/MCP/skills sections 来自 capability state/registry，不来自 prompt assembler。
- legacy session plan 单测：`legacy:session_plan` 能力 fragments 不再进入 Agent-visible prompt。
- pending action frame 单测：action id/type/status/injections/instruction 都进入 sections 与 rendered text。
- auto-resume frame 单测：系统续跑 prompt 产生 frame 并标记 delivery/source。
- compaction frame 单测：summary 与 compacted boundary 进入 frame，且不再拼入 system prompt continuation markdown。
- event persistence 单测：`context_frame` 作为 platform session meta update 持久化。

### Frontend

- `ContextFrameCard` 渲染各 section kind。
- legacy `hook_trace context_injected/baseline_initialized` 在非 verbose 下不冒充 context frame。
- `rendered_text` 展开区展示完整 Agent-visible 文本。
- pending action / auto-resume / compaction card 快照或行为测试。

### Manual

- 在 `http://localhost:5380/dashboard/agent` 流转 builtin workflow admin Plan -> Apply。
- 验证用户能看到启动上下文、workflow guidance、tool delta、pending action 的实际注入文本。
- 验证 Agent 回复不再是用户理解上下文变化的唯一来源。

## Out of Scope

- 不做旧字段兼容；项目未上线，直接硬切。
- 不新增数据库表；优先沿用 session event / platform event 持久化链路。
- 不在本任务内设计完整 prompt diff/time-travel inspector；只确保 frame 可持久化、可渲染、可审计。
- 不改变业务 workflow 语义；只收束 Agent-visible context 的表达与投递。

## Technical Notes

- 当前相关任务：`.trellis/tasks/05-09-agent-steering-visibility`。
- 当前局部实现：`ContextFrame`、`ContextFrameCard`、相邻 `context_frame`
  feed 聚合。
- 主要后端入口：
  - `crates/agentdash-application/src/session/system_prompt_assembler.rs`
  - `crates/agentdash-application/src/session/prompt_pipeline.rs`
  - `crates/agentdash-application/src/session/bootstrap_context_frame.rs`
  - `crates/agentdash-application/src/session/surface_context_frames.rs`
  - `crates/agentdash-application/src/session/hook_delegate.rs`
  - `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
  - `crates/agentdash-application/src/session/tool_schema_notice.rs`
  - `crates/agentdash-application/src/hooks/provider.rs`
  - `crates/agentdash-spi/src/hooks/mod.rs`
- 主要前端入口：
  - `frontend/src/features/session/ui/SessionSystemEventCard.tsx`
  - `frontend/src/features/session/ui/ContextFrameCard.tsx`
- `frontend/src/features/session/model/contextFrame.ts`
- `frontend/src/features/session/model/useSessionStream.ts`
- 需要迁移/删除的 legacy 能力提示入口：
  - `crates/agentdash-application/src/session/plan.rs` 的 `legacy:session_plan` vfs/tools/runtime_policy fragments。
  - `crates/agentdash-application/src/session/system_prompt_assembler.rs` 的 Workspace / Skills 拼接。（已迁出到独立 surface frames）
  - `crates/agentdash-spi/src/platform/skill.rs` 中 connector 注入 skill 到 system prompt 的旧语义。（已改为 `skill_surface`）
  - `crates/agentdash-spi/src/platform/tool_capability.rs` 的 prompt formatter 旧语义。
- compaction system prompt 路径：
  - `crates/agentdash-application/src/session/continuation.rs` 的 `render_system_context_markdown` 当前会把 `AgentMessage::CompactionSummary` 渲染为 `#### 历史摘要`。
