# Session Pipeline 架构级重构

## Goal

把 `session → agent` 管线从"7 个概念+5 处入口+散落字段靠调用顺序暗保证"的状态，收敛到"清晰分层、字段单一权威来源、Bundle 成为主数据面"的形态。

目标形态的核心命题：

1. **5 条正交轴（Who / Where / What / How / Trigger）** 在类型系统里显式分组，不再揉在单一结构。
2. `SessionContextBundle` 是业务上下文的**唯一主数据面**；`assembled_system_prompt` 字符串只是过渡字段。
3. 入口节拍收敛到单一 `SessionStartupBuilder`；`identity` / `post_turn_handler` 等字段由 builder 而非调用方保证不漏。
4. Hook 的 ①Bundle 改写 / ②本轮 user-message / ③本轮控制流副作用 三类语义物理分离；`HOOK_USER_MESSAGE_SKIP_SLOTS` 废除。
5. `hub.rs` 从 2800 行门面+实现混合体拆回纯协调器；`turn_processor` 只管 per-turn 事件流。
6. contribute_\* 层的 workflow_context / workspace / SessionPlan / source_resolver 四处重复收敛到单一 helper。

## What I Already Know

本任务以 `research/pipeline-review/` 四份 review 报告作为事实基底：

- `00-refactor-plan.md`：本次重构的蓝图、7 个 PR 的排序与决策点。
- `01-runtime-layer.md`：hub / prompt_pipeline / turn_processor / SessionRuntime / 入口拓扑的事实映射。
- `02-context-layer.md`：context/ 目录、contribute_\* 函数、SessionPlan 嵌入/外挂、slot/order 散落、hook fragment_bridge 接线状态。
- `03-connector-hook-layer.md`：`ExecutionContext` 12 字段的生产/消费矩阵、PiAgent system prompt 现状、ExecutionContext 构造点、Composite 路由、Hook 双轨。

以下事实在本 PRD 写入时已确认（2026-04-30）：

- 6 条入口调 `start_prompt`，compose 节拍分散在 3 处；`routine/executor.rs:500-515` 漏填 `identity`。
- `finalize_request`（`assembler.rs:105-140`）不对称：`mcp_servers` 整体替换 vs `relay_mcp_server_names` extend；`vfs` 三重分支。
- `ExecutionContext` 12 字段，三个 connector 消费几乎不重叠（见 `03-*` §4.2）。
- `crates/agentdash-executor/` 对 `context_bundle` 引用为 0；Bundle 只在 application 层被渲染为字符串。
- PiAgent 首轮 `set_system_prompt` 后后续 turn 不再更新（`connector.rs:353-366`）——`PiAgentSessionRuntime { agent, tools }` 结构逼出的隐藏行为。
- Hook 运行时 fragment_bridge 只 emit audit，未 merge Bundle；`HOOK_USER_MESSAGE_SKIP_SLOTS = &["companion_agents"]` 是双路径手动去重的证据。
- `fragment_bridge::From<&SessionHookSnapshot> for Contribution` 在生产代码里零调用。
- `workflow_context` / `workspace` / SessionPlan 三处重复渲染；`source_resolver` vs `workspace_sources` 四个 fragment helper 逐行重复。
- `SessionRuntime` 是 session 级（至进程退出），但承载了大量 per-turn 字段（`processor_tx` / `hook_auto_resume_count` / `cancel_requested`）。
- `turn_processor` 在每条 notification 的 hot path 上同步 `executor_session_id` 和写 `SessionMeta`。
- `hub.rs` 2800 行同时承担门面 / factory / tool builder / hook dispatch / cancel 重放 / companion wait registry 等 8 项职责。
- 前置任务 `04-29-cloud-agent-context-bundle-convergence` 已归档；它的若干目标（task bootstrap 不 prepend、compose_lifecycle_node 产 Bundle、slot 白名单单点）已部分落地，由本任务继续收口剩余项并扩展到全管线级。

## Requirements

### 概念与边界

- `ExecutionContext` 拆为 `ExecutionSessionFrame`（不可变：session_id/turn_id/working_directory/vfs/env/executor_config/identity）+ `ExecutionTurnFrame`（per-turn：context_bundle/assembled_tools/hook_session/runtime_delegate/restored_session_state/flow_capabilities/[过渡] assembled_system_prompt）。
- `SessionRuntime` 不再直接持 `processor_tx` / `hook_auto_resume_count` / `cancel_requested` 等 per-turn 字段；这些进入 `SessionRuntime.current_turn: Option<TurnExecution>`。
- `PromptSessionRequest` 作为 HTTP wire DTO 继续存在（向前兼容），内部路径改走强类型 `SessionStartupPlan`。
- `ExecutionTurnFrame.context_bundle` 成为所有 connector 获取业务上下文的**唯一结构化来源**；`assembled_system_prompt` 字段标 deprecated，仅供未迁移的 connector（Relay / vibe_kanban 初期）使用。

### 入口节拍

- 所有 6 条入口统一走 `SessionStartupBuilder::from_entry(user_input, hints).owner(...).compose().finalize()`。
- `identity` / `post_turn_handler` 由 builder 的 first-class 方法承载，不再由调用方在 `finalize_request` 之后追加。
- `finalize_request` 行为对称：mcp_servers 与 relay_mcp_server_names 的合并策略一致（API 明确分 `with_*`（替换）和 `append_*`（追加））。
- `vfs` 覆盖规则用显式 `prefer_base: bool`；`apply_workspace_defaults` 在 `prepared.vfs` 覆盖前计算。

### Bundle 主数据面

- `ExecutionTurnFrame.context_bundle: Option<SessionContextBundle>` 由 `prompt_pipeline` 透传 `req.context_bundle`。
- `system_prompt_assembler` 保留，导出 `render_runtime_section(bundle) -> String` 作为共享渲染入口；pi_agent 按需调用。
- PiAgent 每轮 prompt 前比对 `bundle_id`，变化则热更 system prompt（通过新增 `update_session_system_prompt` 风格 API）。
- 运行期 Hook fragment 回灌到 `ExecutionTurnFrame.context_bundle.turn_delta: Vec<ContextFragment>`（新增字段）：bootstrap 期 fragment 与 per-turn 增量物理分离；Inspector 同时可见。

### Hook 三类语义分离

- `HookRuntimeDelegate.transform_context` 返回新结构：
  ```
  TransformContextOutput {
      bundle_delta: Vec<ContextFragment>,   // 回灌 ExecutionTurnFrame.context_bundle.turn_delta
      steering_messages: Vec<AgentMessage>, // 只承载 per-turn steering，不再塞静态上下文
      control: HookControlDecision,          // Allow / Block { reason } / Rewrite
  }
  ```
- `HOOK_USER_MESSAGE_SKIP_SLOTS` 删除；companion_agents 由 Bundle 合并规则去重。
- `session-capabilities://` resource block 的 user-blocks 注入路径删除；companion_agents 只走 Bundle 一条路径。
- `fragment_bridge::From<&SessionHookSnapshot> for Contribution` 接入 `prompt_pipeline` 初始化阶段。

### contribute_\* 去重

- `workflow_context` slot 渲染抽出共享 `render_workflow_injection(workflow, bindings_opt, mode)`；三处调用点（`contribute_workflow_binding` / `contribute_lifecycle_context` / `compose_companion_with_workflow`）统一调它；companion+workflow 路径走审计总线。
- `workspace` slot 渲染单源：`workspace_context_fragment` 接参数支持带/不带 `status` 等视图变体；`contribute_core_context` 内的 workspace 分支删除。
- SessionPlan 统一外挂：`contribute_story_context` / `contribute_project_context` 不再内置 session_plan.extend；每个 compose 在外层显式 push；`compose_lifecycle_node` 补上 SessionPlan。
- `compose_story_step` 复用 `contribute_story_context`（Story owner）+ 新增 `contribute_task_binding`（task-only 字段）；消除 task 路径对 story 领域的内联重写。
- `source_resolver` 与 `workspace_sources` 合并为单一 `DeclaredSourceResolverRegistry`，fragment helper 集中到 `context/rendering/declared_sources.rs`。
- 所有 slot 的默认 order 集中到 `context/slot_orders.rs`；`HOOK_SLOT_ORDERS` 引用同一常量。

### 运行时模块边界

- `hub.rs` ≤ 500 行；其余职责拆为 `hub/facade.rs` / `hub/factory.rs` / `hub/tool_builder.rs` / `hub/hook_dispatch.rs` / `hub/cancel.rs`。
- `turn_processor` 不再写 `SessionMeta`（`executor_session_id` 同步抽到 persistence listener）、不再直接改 `SessionRuntime.running`（通过 `TurnEvent::Terminal` 让 hub 处理）、不再递增 `hook_auto_resume_count`（processor 只发"请求 auto-resume"信号，限流在 hub 侧）。
- `SessionRuntime.hook_session` 与 `ExecutionContext.turn.hook_session` 通过 `Arc` 共享，prompt_pipeline 不再双向写入。
- `event_bridge` 的 `_tx` 占位参数删除。

### 字段冗余收敛

- `working_directory` 单一权威：`ExecutionSessionFrame.working_directory`；`ActiveSessionExecutionState` 不再独立存。
- `executor_config` 解析在 builder 阶段完成（`req.user_input.executor_config` ∪ `session_meta.executor_config` ∪ preset 补全），落到 `ExecutionSessionFrame.executor_config`；`ActiveSessionExecutionState` 与 `SessionMeta` 不再各存解析中间态。
- `effective_capability_keys` 要么真消费、要么删除 dead_code 字段。
- `identity` 只在 `ExecutionSessionFrame` 持有；`ActiveSessionExecutionState.identity` 删除。
- `mcp_servers` 在 `ExecutionSessionFrame`（下发给 relay）+ `ExecutionTurnFrame.assembled_tools`（pi_agent 已实例化）两处，其他冗余位置全部清除。

## Acceptance Criteria

- 6 条 `start_prompt` 入口全部通过 `SessionStartupBuilder` 装配；`routine/executor.rs` 的 `identity` 不再为空。
- `finalize_request` 的单测覆盖：mcp_servers 覆盖 / append；relay_mcp_server_names 覆盖 / append；vfs prefer_base 开关；workspace_defaults 顺序。
- `ExecutionContext` 定义拆为 `{ session: SessionFrame, turn: TurnFrame }`；三 connector 编译并现有单测全绿；`hub.replace_runtime_mcp_servers` 不再构造 "ghost ExecutionContext"。
- `ExecutionTurnFrame.context_bundle` 存在；PiAgent 内可读 Bundle，并能按 `bundle_id` 变化触发 system prompt 热更（至少有单测锁定该行为）。
- Hook delegate `transform_context` 新签名；`HOOK_USER_MESSAGE_SKIP_SLOTS` 从代码库中删除；`session-capabilities://` resource block 注入代码路径删除；`companion_agents` 渲染只经 Bundle 一条路径。
- `workflow_context` 渲染三处合并为单一 helper；`workspace` slot 渲染单源；SessionPlan 统一外挂；`compose_lifecycle_node` 调 SessionPlan。
- `compose_story_step` 调用 `contribute_story_context`；task 路径不再内联重写 story 级 fragment。
- `source_resolver` 与 `workspace_sources` 合并；fragment helper 单点。
- `hub.rs` 行数 ≤ 500；hub 子模块按 PRD 列出的拆分落位。
- `turn_processor` 不写 `SessionMeta`，不改 `SessionRuntime.running`，不递增 `hook_auto_resume_count`。
- `SessionRuntime` per-turn 字段下沉到 `TurnExecution`；`current_turn: Option<TurnExecution>` 字段就位。
- `working_directory` / `executor_config` / `mcp_servers` / `identity` / `effective_capability_keys` 的冗余副本全部清除（副本数减到 PRD 定义的上限）。
- `cargo test --workspace` 全绿；关键 e2e 回归：HTTP prompt / task start / workflow orchestrator / companion dispatch / routine tick / hub auto-resume / cancel 各路径。
- Context Inspector 在 per-turn 路径能看到 Hook `bundle_delta` 事件。

## Definition of Done

- 7 个 PR 全部合入，且每个 PR 按 Implementation Plan 列出的"完成信号"自验通过。
- 所有变更覆盖相应单测；`cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` 绿灯。
- 以下三份 spec 写入 `.trellis/spec/backend/`（D6 决策）：
  - `session-startup-pipeline.md` — 5 条正交轴、compose 节拍、finalize 规则、`SessionStartupBuilder` 契约。
  - `execution-context-frames.md` — `SessionFrame` / `TurnFrame` 字段所有权、生命周期、热更策略。
  - `bundle-main-datasource.md` — Bundle 作为主数据面、Hook 三类语义、`turn_delta` vs bootstrap fragment 的分离。
- `research/pipeline-review/00-refactor-plan.md` 保留作为蓝图；每个 PR 完成后回写"完成信号验证结果"小节。
- PR 描述明确列出：解决的 Acceptance Criteria 条目、引入的新概念、影响的入口路径。

## Technical Approach

### Approach A：分 7 个 PR 顺序推进（推荐并采纳）

每个 PR 独立可验证，按"低风险高收益 → 高风险高收益"排序：

- PR 1 入口节拍统一 + `finalize_request` 对称化
- PR 2 `ExecutionContext` 分层（`SessionFrame` + `TurnFrame`）
- PR 3 Bundle 进 `TurnFrame`；PiAgent 可读 Bundle；`assembled_system_prompt` 标 deprecated
- PR 4 Hook fragment 回灌 Bundle `turn_delta`；`HOOK_USER_MESSAGE_SKIP_SLOTS` 废除；三类语义拆分
- PR 5 contribute_\* 去重 + SessionPlan 统一外挂 + source_resolver/workspace_sources 合并
- PR 6 `hub.rs` 拆分到子模块
- PR 7 `turn_processor` 净化 + `SessionRuntime` per-turn 字段下沉

**依赖关系**：PR1→PR2→PR3→PR4；PR5 并行于 PR1-4；PR6 在 PR1-5 之后；PR7 可在 PR2 之后开始。

### Approach B：一次性大 PR（已拒绝）

合入风险、review 压力、回归面积都无法接受。

### Approach C：只做 PR 3+4（只收 Bundle 主数据面）（已拒绝）

不触及入口节拍与 ExecutionContext 分层，后续 PR 会继承当前字段冗余，两次重构等于没重构。

### Decision

采纳 Approach A。每 PR 独立 commit，按"完成信号"验证。

## Implementation Plan

### PR 1 · `finalize_request` 对称化 + 入口节拍统一收口

**改什么**：

- `session/assembler.rs`：`finalize_request` 把 `mcp_servers` 改为按调用方意图分 `with_*` / `append_*` 两组语义；`vfs` 覆盖规则加 `prefer_base: bool`；`apply_workspace_defaults` 提前执行。
- 新增 `SessionStartupBuilder`（工作名，可放 `session/startup_builder.rs` 或合并到 assembler）：所有入口都调它，`identity` / `post_turn_handler` 进 builder first-class 方法。
- 5 条入口适配：`routes/acp_sessions.rs` / `task/service.rs` / `workflow/orchestrator.rs` / `companion/tools.rs` / `routine/executor.rs` / `hub.rs`(auto-resume)。
- `routine/executor.rs:500-515` 补上 `identity`。

**完成信号**：

- 5 条入口 snapshot 测试：同一 `UserPromptInput + IntentHints` 输入，产出的 `PromptSessionRequest` 后端注入字段全部齐全。
- `finalize_request` 对称性单测（mcp / relay_mcp / vfs / workspace_defaults 四组）。
- `routine` 路径 `identity` 非空回归测试。

### PR 2 · `ExecutionContext` 拆 `SessionFrame` + `TurnFrame`

**改什么**：

- `spi/connector.rs`：新增 `ExecutionSessionFrame` / `ExecutionTurnFrame`；`ExecutionContext` 改为 `{ session, turn }`。
- `session/hub_support.rs`：`ActiveSessionExecutionState` 持 `ExecutionSessionFrame`，不再重复存 working_directory/mcp_servers/executor_config/identity 等。
- `session/prompt_pipeline.rs:269-314`：字段构造按新分组。
- `session/hub.rs:441-500`：`replace_runtime_mcp_servers` 直接用 `SessionFrame`，删除 ghost ExecutionContext 构造。
- 三 connector（pi_agent / relay / vibe_kanban）访问路径适配（只是 `context.x` → `context.session.x` / `context.turn.x`）。
- `effective_capability_keys` 决策：若无人读则删除；若确实后续 tool provider 要读，本 PR 就接入。

**完成信号**：

- 所有 connector 单测通过。
- `ActiveSessionExecutionState` 不再出现 `mcp_servers` / `working_directory` / `executor_config` / `identity` 字段。
- `hub.replace_runtime_mcp_servers` 行数下降且不再构造 ExecutionContext 字面量。

### PR 3 · Bundle 进 `TurnFrame`；PiAgent 读 Bundle

**改什么**：

- `spi/connector.rs`：`ExecutionTurnFrame.context_bundle: Option<SessionContextBundle>`；`assembled_system_prompt` 标 `#[deprecated]`。
- `session/system_prompt_assembler.rs`：导出 `render_runtime_section(bundle, ...)`；`assemble_system_prompt` 拆为 renderer composition。
- `connectors/pi_agent/connector.rs`：`is_new_agent` 分支优先读 `turn.context_bundle`，缺失则 fallback `assembled_system_prompt`；后续 turn 比对 `bundle_id` 变化时重 set。
- 新增 `AgentConnector::update_session_context_bundle(session_id, bundle)`（可选 trait 方法，default no-op）。
- Relay / vibe_kanban：本 PR 不改；继续吃 `assembled_system_prompt`。

**完成信号**：

- PiAgent 单测：同 session 两轮 prompt，第二轮 bundle_id 变化 → 验证 `set_system_prompt` 被调用。
- `grep context_bundle` 在 `crates/agentdash-executor/` 命中 > 0。
- application 预渲染的 `assembled_system_prompt` 仍可用（backward-compat）。

### PR 4 · Hook 三类语义分离 + Bundle turn_delta

**改什么**：

- `spi/session_context_bundle.rs`：新增 `turn_delta: Vec<ContextFragment>` 字段；`render_section` 支持合并 bootstrap + turn_delta。
- `session/hook_delegate.rs`：`transform_context` 返回新结构；`after_turn` / `before_stop` 同步拆分。
- `session/hook_delegate.rs:806`：`HOOK_USER_MESSAGE_SKIP_SLOTS` 删除。
- `hooks/fragment_bridge.rs`：`From<&SessionHookSnapshot> for Contribution` 接入运行时；`hook_injection_to_fragment` 既写 audit 又产出 Bundle fragment。
- `session/prompt_pipeline.rs:379-397`：删除 `session-capabilities://` resource block 注入 user_blocks 的路径。
- `session/baseline_capabilities.rs`：companion_agents 走 Bundle contribute（不再独立 section）。
- `session/system_prompt_assembler.rs`：`## Companion Agents` section 改为 Bundle render 产出的一部分。

**完成信号**：

- `HOOK_USER_MESSAGE_SKIP_SLOTS` 从代码库消失；`companion_agents` 只在 Bundle 出现。
- `session-capabilities://` user_blocks 注入代码路径删除。
- hook delegate 单测覆盖 bundle_delta / steering_messages / control 三路。
- Inspector 能查询 `hook:<trigger>` 类型的 turn_delta 事件。

### PR 5 · contribute_\* 去重 + 路径统一

**改什么**：

- `context/rendering/workflow_injection.rs`（新文件）：`render_workflow_injection(workflow, bindings_opt, mode)`。
- `context/workflow_bindings.rs` / `session/assembler.rs:1296`(lifecycle) / `session/assembler.rs:1610+`(companion+workflow) 都调它。
- `context/builtins.rs`：`workspace_context_fragment` 扩参数；`contribute_core_context` 的 workspace 分支删除。
- `story/context_builder.rs` / `project/context_builder.rs`：去除内置 `build_session_plan_fragments.extend`；所有 compose 在外部独立 push。
- `session/assembler.rs:compose_lifecycle_node_with_audit`：补 `build_session_plan_fragments`。
- `session/assembler.rs:compose_story_step`：调 `contribute_story_context` 而非重写；新增 `contribute_task_binding`（task-only 字段）。
- `context/source_resolver.rs` + `context/workspace_sources.rs`：合并为 `context/declared_source_registry.rs` + `context/rendering/declared_sources.rs`；fragment helper 单点。
- `context/slot_orders.rs`（新文件）：所有 slot 默认 order 集中常量；`HOOK_SLOT_ORDERS` 引用同一常量。

**完成信号**：

- `workflow_context` render 共享 helper；三处调用点都测试调过。
- `workspace` slot 渲染单源；task 与 owner 路径输出 diff 只差 `status` 字段。
- lifecycle bundle 覆盖率比重构前增大（包含 SessionPlan）。
- task 路径 snapshot 测试：fragment 集合等价于重构前 + story 内的增量 fragment。
- `source_resolver` / `workspace_sources` 合并后单测全跑通。

### PR 6 · `hub.rs` 拆分

**改什么**：

把 `session/hub.rs` 按职责拆到 `session/hub/` 子模块：

- `session/hub/mod.rs` + `session/hub/facade.rs`：`SessionHub` 保留对外 API（start / cancel / subscribe / delete / ensure）。
- `session/hub/factory.rs`：`SessionHubFactory` 持 `base_system_prompt` / `user_preferences` / `runtime_tool_provider` / `mcp_relay_provider`。
- `session/hub/tool_builder.rs`：`build_tools_for_execution_context` 独立，签名 `(session: &ExecutionSessionFrame, mcp: &[McpServer]) -> Vec<DynAgentTool>`。
- `session/hub/hook_dispatch.rs`：`emit_session_hook_trigger` / `ensure_hook_session_runtime` / `schedule_hook_auto_resume`。
- `session/hub/cancel.rs`：`cancel` + interrupted 事件补发。
- `session/hub/companion_wait.rs`：从 hub 字段归位到 `companion/` 域。
- `session/event_bridge.rs` 的 `_tx` 占位参数删除。

**完成信号**：

- `hub.rs` 行数 ≤ 500。
- 每个子模块职责单一且可独立测试。
- `hub` 不再直接依赖 `agentdash-executor::mcp::discover_*`（通过 `tool_builder` 间接依赖）。

### PR 7 · `turn_processor` 净化 + `SessionRuntime` per-turn 字段下沉

**改什么**：

- `session/hub_support.rs`：`ActiveSessionExecutionState` 改名为 `TurnExecution`；吸收 `processor_tx` / `hook_auto_resume_count` / `cancel_requested` / `current_turn_id`。`SessionRuntime.current_turn: Option<TurnExecution>`。
- `session/turn_processor.rs`：
  - 停止直接写 `SessionMeta.executor_session_id`（抽到 persistence listener）。
  - 停止直接写 `SessionRuntime.running` / `processor_tx`（通过 `TurnEvent::Terminal` 让 hub 清理）。
  - 停止递增 `hook_auto_resume_count`（只发 `AutoResumeRequested` 信号）。
- `session/hub/hook_dispatch.rs`：auto-resume 限流逻辑在此。
- `session/prompt_pipeline.rs`：不再双向写 `hook_session`（`SessionRuntime.hook_session` 单一权威）。
- `session/continuation.rs` 内的 `build_companion_human_response_notification` 挪到 `companion/` 域（这是 §6.2 里的寄生函数）。

**完成信号**：

- `turn_processor` 代码库内无 `session_meta.` / `sessions.lock` / `hook_auto_resume_count` 写操作。
- cancel 路径 e2e（HTTP / hub / connector 三路）测试通过。
- auto-resume 限流单测在 hub 侧。

## Decisions（from Part D of refactor plan + 2026-04-30 execution kickoff）

本次重构在规划阶段锁定以下决策；若实施中发现反例回炉讨论即可。

### 架构决策（D1-D6）

- **D1 — wire DTO 保留**：`PromptSessionRequest` 继续作为序列化 DTO 存在；内部路径走强类型 `SessionStartupPlan`。避免 relay / 插件协议掣肘。
- **D2 — 立刻 split ExecutionContext**：PR 2 一次到位，不走"先加字段、下轮再拆"的两步过渡。
- **D3 — Hook 折中（新增 turn_delta）**：Bundle 上加 `turn_delta: Vec<ContextFragment>` 与 bootstrap fragment 物理分离。bootstrap 期 fragment 与 per-turn 增量语义不同，应结构上分开；Inspector 同时可见两者。
- **D4 — PiAgent 按 bundle_id 热更**：bundle_id 变化时触发 `set_system_prompt`。前提条件：PR 7（runtime 清理）已完成，否则 cache invalidation 点太多。
- **D5 — hub 分批拆**：不一次性大改；companion_wait / tool_builder 先独立，hub.rs 再瘦身。PR 6 内部按子项顺序执行。
- **D6 — 写 spec**：在 DoD 阶段写入 `.trellis/spec/backend/session-startup-pipeline.md` / `execution-context-frames.md` / `bundle-main-datasource.md` 三份。

### 实施决策（2026-04-30 kickoff 对齐，锁执行前）

- **E1 — Routine identity 用 system identity**：routine/executor.rs 生成 `AuthIdentity { auth_mode: Personal, user_id: "system:routine:<id>", is_admin: false, groups: [], provider: Some("system.routine") }`。审计可见，无权限脱出，hook/permission 走常规逻辑。
- **E2 — Builder 形态：扩展现有 `SessionAssemblyBuilder`**：不新建 `SessionStartupBuilder` 包装层、不引入 `SessionStartupPlan` 强类型；在现有 builder 上加 `with_identity` / `with_post_turn_handler` / `with_user_input` first-class 方法；`PreparedSessionInputs` 同步扩字段；`finalize_request` 合入新字段。PR 1 最小改动面。
- **E3 — Commit 粒度：PR 级**：每个 PR（7 个）内部按逻辑分 2-4 commit；PR 之间不停顿连续推进。
- **E4 — Open Q 执行前全锁**：实施中如遇新 open question 按推荐方案或最佳判断走（不中断），结束后在 journal 汇总回炉清单。
- **E5 — `effective_capability_keys` 删除**：ActiveSessionExecutionState 上的 dead_code 字段 PR 2 顺手删；FlowCapabilities.enabled_clusters 已覆盖 cluster 裁剪语义。
- **E6 — HookRuntimeDelegate 签名直改**：`transform_context` 等 trait 方法 PR 4 签名变更不提供 default impl（消费者全在内部）；外部 plugin 若未来需要，再开兼容层。
- **E7 — `SessionBootstrapAction` → `HookSnapshotReloadTrigger`**：PR 4 拆 session-capabilities resource block 后 PR 内顺手重命名；同步审视 `SessionMeta.bootstrap_state` 字段是否还需要。
- **E8 — PR 5 附带清理**：① `CompanionSliceMode` 对父 bundle 做 fragment 级裁剪；② 删除 `build_continuation_bundle_from_markdown` 的 `static_fragment` 包装，task continuation 直接组装 Bundle 不再绕一圈 markdown。

## Out of Scope

- **Bundle α 化**（删除 `assembled_system_prompt`，所有 connector 自渲染）：列入未来工作。需 Relay 协议扩展或 relay 侧本地渲染方案先拍板。
- **Relay 协议扩展**：不新增 `context_bundle` 字段；relay 路径继续吃 `assembled_system_prompt` 字符串。
- **vibe_kanban 连接器深改**：保持 `assembled_system_prompt` 消费路径，不跟进 Bundle。
- **AGENTS.md / MEMORY.md 隐式发现**：由 `04-29-agents-md-discovery-loading` 任务承接，与本任务独立。
- **D2a 激进方案**（Hook 副作用 / pending action / BeforeStop / compaction 全部 fragment 化）：存档在 `04-29-session-context-builder-d2a-exploration`，本任务不采纳。
- **前端 Inspector UI 新增视图**：本次只保证 DTO 向后兼容；UI 改造视 Context Inspector 当前能力按需跟进。
- **Connector Factory 注册机制重构**：Composite routing 目前按 executor 字符串分发已够用，不在本轮触碰。

## Technical Notes

- 核心文件锚点见 `research/pipeline-review/00-refactor-plan.md` Part F。
- 本任务实施过程中可能遇到以下"已知技术债"需要顺手处理（若 cost 小则 bundle 进对应 PR，否则开 TODO 记录）：
  - `continuation.rs` 包含 `build_companion_human_response_notification`（与 continuation 无关）。
  - `event_bridge.rs` 的 `_tx` 参数未使用。
  - `ContextContributor` trait 是否仍有引用待核实（research 报告显示 grep 未找到定义）。
  - `build_declared_source_warning_fragment`（builder.rs:159）与 `contribute_declared_sources` 内部 warning 段的双路径（builtins.rs:292）是否会 double warning。
- **测试策略**：
  - 每个 PR 前先给触及的 compose 场景加 snapshot 测试（锁 fragment 结构）；PR 内部重构；PR 后 snapshot 等价验证。
  - 关键 e2e：HTTP prompt 完整链路 / task start / workflow agent node / companion dispatch / routine tick / auto-resume / cancel × 3 路径 / compaction。
- **审计事件**：本任务新增的 `AuditTrigger` 变体（若有）必须保持前端 Inspector DTO 可消费；向后兼容扩展而非破坏性变更。

## Open Questions

> 大部分 kickoff 前的 Open Question 已在 `## Decisions` 的 E1-E8 锁定。本段只保留实施时
> 会自然浮现、需要观察代码现场决定的小项。

- **PR 7 里 `persistence listener` 位置**：新建 `session/persistence_listener.rs` 还是合并到 `session/persistence.rs`？倾向新建（职责独立）。实施时若发现与 persistence 交织过密再合并。
- **Relay PR 3 回归**：确认 `assembled_system_prompt` 继续填值时 relay 无行为回退；不是设计决策，是 PR 3 验证动作。
- **`SessionMeta.bootstrap_state` 的去留（E7 连带）**：如 `SessionBootstrapAction` 重命名后语义变为纯 hook 触发器，是否还需要持久化到 SessionMeta？实施时视迁移成本决定。
- **`continuation.rs::build_companion_human_response_notification` 的归位目录（PR 7 附带）**：挪到 `companion/notifications.rs` 还是 `companion/runtime.rs`？倾向前者。
- **实施过程中新发现的 question**：按 E4 走推荐方案，结束后 journal 汇总。

## References

- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/00-refactor-plan.md` — 完整重构蓝图
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/01-runtime-layer.md` — Runtime 层事实映射
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/02-context-layer.md` — Context 层事实映射
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/03-connector-hook-layer.md` — Connector/Hook 层事实映射
- `.trellis/tasks/archive/2026-04/04-29-cloud-agent-context-bundle-convergence/prd.md` — 前置任务（已归档）
- `.trellis/tasks/archive/2026-04/04-29-session-context-builder-unification/prd.md` — Bundle 引入的基础任务
