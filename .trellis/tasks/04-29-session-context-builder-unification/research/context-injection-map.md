# Research: AgentDashboard Session 上下文注入地图

- **Query**: 系统性梳理一个会话从创建到发第一条用户消息、到每一轮工具调用/终止，沿途所有向 session context 注入内容的端点/通路
- **Scope**: internal（以 Rust 后端为主，辅助前端 prompt 构造点）
- **Date**: 2026-04-29
- **Target Task**: `.trellis/tasks/04-28-session-header-collapsed-agent-hydrate`

> 本报告用于支撑后续「统一 ContextBuilder」的设计讨论。当前实现中，"注入"这件事被拆散到至少 **五层** —— HTTP 入口、Assembler（compose_*）、SessionHub.prompt_pipeline、HookRuntimeDelegate（agent_loop 每轮调）、以及 PiAgent Connector 内部的 runtime system prompt 拼装。每一层都会往 system prompt / user message / 独立 "user" message 里塞东西，互相之间并不经过单一合并点。

---

## 0. 总览流程图（文字版）

```
[ 前端 ]
  POST /sessions                                         (空 session)
  POST /tasks|/stories|/projects/.../sessions/{id}/prompt (业务入口，携带 user_input)
  POST /sessions/{id}/prompt                              (裸 prompt 入口，仅 promptBlocks+workingDir+env+executorConfig)

[ agentdash-api::routes::{acp_sessions, story_sessions, project_agents, task_execution} ]
       │
       │ 1. 若有业务 owner → SessionRequestAssembler.compose_owner_bootstrap / compose_story_step / compose_lifecycle_node / compose_companion
       │    ├─ (1a) VFS 构建（workspace mount + canvas mount + lifecycle mount）
       │    ├─ (1b) Capability Resolver（agent 声明 / workflow directives → effective caps + MCP servers）
       │    ├─ (1c) Context contributor pipeline:
       │    │       CoreContext + Binding(initial_context) + DeclaredSources + StaticFragments + WorkflowContextBindings + McpContextContributor + Instruction
       │    │    → Markdown（task/story/project/workspace/workflow + declared sources）
       │    ├─ (1d) system_context = 上述 Markdown（OwnerBootstrap 时）
       │    ├─ (1e) prompt_blocks = 用户输入（+ 任务 kickoff prompt / routine 模板，视路径而定）
       │    └─ finalize_request(base, prepared) → 完整 PromptSessionRequest
       │    若 HTTP 层未走 compose（例如裸 /sessions/{id}/prompt）而 session 已绑定 owner，
       │    SessionHub 内部的 auto-resume 会经 PromptRequestAugmenter（crates/agentdash-api/src/bootstrap/prompt_augmenter.rs）再补一次。
       │
       ▼
[ SessionHub::start_prompt_with_follow_up ]           crates/agentdash-application/src/session/prompt_pipeline.rs
       │
       │ 2. 若本轮是 owner bootstrap：load_session_hook_runtime → AppExecutionHookProvider.load_session_snapshot
       │    ├─ snapshot.injections：
       │    │   ├─ builtin:companion_agents  （项目所有 agent 列表 + merged display_name + allowed_companions 过滤）
       │    │   └─ workflow:step_fragments  （由 build_workflow_step_fragments：active step 描述、port、constraints）
       │    └─ snapshot.tags / owners / metadata（workflow_key / run_id / port_keys / fulfilled_port_keys 等）
       │
       │ 3. build_session_baseline_capabilities(hook_session, discovered_skills)
       │    产出 SessionBaselineCapabilities { companion_agents, skills }
       │    → 若是 owner bootstrap 或首轮 prompt，封装为
       │      resource block `agentdash://session-capabilities/{sid}`
       │      **插到 prompt_blocks[0]**（agent 端当作 user message 首块看见）
       │
       │ 4. 首轮 prompt + title_source!=User → spawn_title_generation（独立 bridge，system prompt 固定为"你是一个标题生成器…"）
       │
       │ 5. 若 is_owner_bootstrap：emit_session_hook_trigger(SessionStart)
       │    （当前内置 hook rules 不会再往 session 额外写消息，但 provider 会在 evaluate_hook 里把
       │     snapshot.injections 再次作为 resolution.injections 返回，交给下游 transform_context 用）
       │
       │ 6. connector.prompt(..., ExecutionContext {
       │         system_context, vfs, mcp_servers, hook_session, runtime_delegate,
       │         session_capabilities, restored_session_state, executor_config, ... })
       │
       ▼
[ PiAgentConnector::prompt ]           crates/agentdash-executor/src/connectors/pi_agent/connector.rs:763+
       │
       │ 7. build_runtime_system_prompt(context, runtime_tools) ← 真正送进 LLM 的 system prompt
       │    ├─ Identity         = connector.system_prompt (settings "agent.pi.system_prompt")
       │    │                     + （可选）executor_config.system_prompt
       │    │                     + system_prompt_mode（Override / Append）
       │    ├─ Project Context  = context.system_context（就是步骤 1c 的 Markdown）
       │    ├─ Companion Agents = context.session_capabilities.companion_agents（re-rendered）
       │    ├─ Workspace        = context.vfs.mounts 清单
       │    ├─ Available Tools  = runtime_tools(内嵌) + 平台 MCP + 用户 MCP
       │    ├─ Hooks            = hook_session.pending_actions + "当前会话启用了 Hook Runtime…"
       │    └─ Skills           = session_capabilities.skills → XML <available_skills>
       │    agent.set_system_prompt(...) 把它灌进 agentdash-agent::Agent.config.system_prompt
       │
       │ 8. Agent.prompt() 进入 agent_loop.rs：
       │    每次 LLM 调用前：
       │      HookRuntimeDelegate.transform_context(AgentContext{system_prompt, messages, tools})
       │        → evaluate UserPromptSubmit hook
       │        → 可以：block / 替换最后一条 user 文本 / append 一条 "## Hook Context…" user message
       │        → 再把 runtime pending actions 作为 steering/follow_up user message append
       │
       │    每次 tool 调用前/后：BeforeTool / AfterTool hook（deny/ask/rewrite）
       │    每轮结束：AfterTurn → steering + follow_up user messages 入队
       │    BeforeStop：stop gate / blocking_review / pending_action 再次注入 user 消息并阻止停机
       │    BeforeProviderRequest：observe only（记 trace，不改消息）
       │
       ▼
[ LlmBridge（anthropic_bridge / openai_completions_bridge / openai_responses_bridge）]
       │ 9. convert_messages 时：
       │    - 过滤 stop_reason = Error/Aborted 的 Assistant 消息（commit 30991ef）
       │    - system_prompt 映射到各 API 的 system 字段
       │    - 每次 HTTP 请求重新发完整 system prompt + messages（pi-mono 风格，非 session 续跑）
       ▼
[ LLM API ]
```

---

## 1. 注入时机分组 · 注入点明细

每个表格列：**注入点 | 位置 | 触发时机 | 注入形态 | 是否可关 / 替换**

### 1.1 Session 创建阶段（空 session 被绑 owner，还没发 prompt）

这阶段不会真实"送 context"，但会把以后每轮都会读到的 owner / binding 信息落地。

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| A1 | 空 session 创建 | `crates/agentdash-api/src/routes/acp_sessions.rs:150 create_session` → `SessionHub::create_session` (`crates/agentdash-application/src/session/hub.rs:147`) | `POST /sessions` | 只写 SessionMeta，`bootstrap_state = Plain` | — |
| A2 | Owner 绑定（Story/Task/Project/Routine 入口） | `crates/agentdash-api/src/routes/story_sessions.rs`、`task_execution.rs`、`project_agents.rs`、`routine/executor.rs` | 创建"business session" 时 | 写 `SessionBinding(session_id, owner_type, owner_id, label)`；**把 `bootstrap_state` 切到 `Pending`**，下一次 prompt 会触发 OwnerBootstrap | 不能关 |

### 1.2 第一条用户消息（OwnerBootstrap + 首轮 prompt）

这是"注入点最密集"的一轮。下面按"进入路径" + "拼装动作"分列。

| # | 注入点 | 位置 | 触发时机 | 形态（拼到哪里） | 可关 |
|---|---|---|---|---|---|
| B1 | **CoreContextContributor** — Task/Story/Project/Workspace 元信息 | `crates/agentdash-application/src/context/builtins.rs:153-232` | Assembler.compose_* 执行时 | `ContextFragment`（slot: task/story/project/workspace，order 10/20/40/50）→ 汇总成 `system_context` Markdown | 替换为自定义 contributor |
| B2 | **BindingContextContributor** — Task `agent_binding.initial_context` | `crates/agentdash-application/src/context/builtins.rs:234-255` | 同上 | slot=`initial_context` order=80，`## Initial Context\n…` | Task 未配 initial_context 则不出现 |
| B3 | **DeclaredSourcesContributor** — Story/Task 声明式来源 | `crates/agentdash-application/src/context/builtins.rs:258-318` + `context/source_resolver.rs` | 同上 | 每个来源一个 fragment（order 82+），+ 警告/错误 fragment | 不配 source_refs 即关 |
| B4 | **StaticFragmentsContributor** — workspace declared sources（resolve_workspace_declared_sources） | `crates/agentdash-application/src/session/assembler.rs:983` + `context/workspace_sources.rs` | `compose_story_step`，有 workspace 时 | order=86，workspace 文件正文 | 依赖 source_refs |
| B5 | **WorkflowContextBindingsContributor** | `crates/agentdash-application/src/context/workflow_bindings.rs` | `compose_story_step`，有 active workflow 且 `workflow.contract.injection.context_bindings` 非空 | workflow_projection snapshot + 每条 binding 的 Markdown（order 83+） | 在 workflow 定义里改 `context_bindings` |
| B6 | **McpContextContributor**（每个 platform MCP config 一个） | `crates/agentdash-application/src/context/builtins.rs:386-419` | `compose_story_step` + `compose_owner_bootstrap` 对每个 `platform_mcp_configs` 产生一个 | slot=mcp_config order=85，同时把 server 加进 `mcp_servers` 列表 | 调 CapabilityResolver 输出决定 |
| B7 | **InstructionContributor** — Task 指令模板 | `crates/agentdash-application/src/context/builtins.rs:338-381` | 所有 task 路径 | slot=instruction order=90/100，`## Instruction\n…`（默认模板写死在 DEFAULT_START_TEMPLATE / DEFAULT_CONTINUE_TEMPLATE，Task 可用 `prompt_template` 覆盖；`override_prompt` 优先级最高） | Task 可 override；但"不出现"不行 |
| B8 | **Owner Context Markdown 合并** | `crates/agentdash-application/src/context/context_composer.rs` + `crates/agentdash-application/src/session/assembler.rs:602-647 build_owner_context_markdown_sync` | Story/Project owner 路径 | 调用 `build_story_context_markdown` / `build_project_context_markdown`，**把 B1~B7 汇总成单个 Markdown 字符串**，作为 `system_context` 给 SessionHub | — |
| B9 | **OwnerBootstrap → system_context 分支** | `crates/agentdash-application/src/session/assembler.rs:788-814` | `spec.lifecycle == OwnerBootstrap` | `system_context = Some(context_markdown)`，`prompt_blocks = user_prompt_blocks`（用户原文） | commit bce0825 后 context **不再**混进 prompt_blocks |
| B10 | **Kickoff Prompt（workflow AgentNode）** | `crates/agentdash-application/src/workflow/step_activation.rs:77-95 KickoffPromptFragment::to_default_prompt` | `SessionRequestAssembler::compose_lifecycle_node` + `compose_companion_with_workflow` | 作为 **user 文本 prompt_block** 注入（"你正在执行 lifecycle X 的 node Y…请先…调用 `complete_lifecycle_node`…"） | 可由 applier 替换（目前仅默认实现） |
| B11 | **Companion Dispatch Prompt** | `crates/agentdash-application/src/session/assembler.rs:348-382 apply_companion_slice` + `companion/tools.rs` | 父 session 调 `companion_request` tool 触发子 session | 作为 user 文本 prompt_block；同时继承父 `system_context` | companion 行为固定 |
| B12 | **Routine Template Prompt** | `crates/agentdash-application/src/routine/executor.rs` | Routine 触发 | 把 routine 模板渲染成 prompt_blocks；走 `compose_owner_bootstrap` | routine 可改模板 |
| B13 | **Session Capabilities Resource Block** | `crates/agentdash-application/src/session/prompt_pipeline.rs:292-311` | 每个 session 的 **首轮 prompt** 或 **owner bootstrap** 轮 | 把 `SessionBaselineCapabilities`（companion_agents + skills）序列化成 JSON，塞到 `agentdash://session-capabilities/{sid}` 的 ACP resource block，**插到 prompt_blocks[0]**（在用户消息之前作为一个"用户块"到达 agent） | 目前无开关，依赖 `session_capabilities` 非空 |
| B14 | **Hook Snapshot Injections（初次 load）** | `crates/agentdash-application/src/hooks/provider.rs:213-408 load_session_snapshot` | OwnerBootstrap 或 runtime 缺失时 | snapshot.injections 会包含：`builtin:companion_agents`（slot=companion_agents）、`build_workflow_step_fragments` 产出的 workflow 块（slot=workflow/constraint）、以及各 rule 返回的注入 | 不同 slot 可被 `filter_user_prompt_injections` 过滤 |
| B15 | **SessionStart Hook** | `prompt_pipeline.rs:333-361` + `hooks/provider.rs:444-447` | 仅 owner bootstrap 那一轮 | 触发一次 `HookTrigger::SessionStart`，当前内置 provider 返回 `injections = snapshot.injections` 原样，但这一次的 resolution 会被 **emit_session_hook_trigger** 持久化到 trace；内置 rules 目前**没有**在 SessionStart 写入额外 user message | 可在 hooks/presets/ 或 rhai 脚本挂钩 |
| B16 | **Title Generator 的独立 system prompt** | `crates/agentdash-api/src/title_generator.rs:9-15 TITLE_SYSTEM_PROMPT` | 首轮 prompt 且 `title_source != User` | **独立**的 LLM 调用，`system_prompt = "你是一个标题生成器…"`，message 只含 `AgentMessage::user(text_prompt)` | commit bce0825 特别修掉了"标题 LLM 被喂整段 agent context"的历史坑；现在完全不再混入 agent context |

### 1.3 每一轮用户消息（在 HookRuntimeDelegate.transform_context 里）

这一层是 agent_loop 在**每次要调用 LLM 前**都会过的"运行时注入口"。无论是首轮还是续跑都走这里。

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| C1 | **UserPromptSubmit Hook** | `crates/agentdash-application/src/session/hook_delegate.rs:274-364` | 每次 `transform_context`（= agent_loop 每次外循环迭代开始前） | 1) 可直接 `block_reason` 打断；2) 可改写最后一条 user 文本；3) `build_hook_injection_message` 把 resolution.injections 汇总成一条 `AgentMessage::user(...)`（"## Hook Context\n### <source>\n…"）append 到 messages 尾部 | 在 provider 里过滤或 rules/rhai 脚本返回空 injections |
| C2 | **filter_user_prompt_injections** — companion_agents baseline 过滤 | `crates/agentdash-application/src/hooks/provider.rs:48-61` | UserPromptSubmit 评估时 | 把 snapshot.injections 里 `slot == "companion_agents"` 的 baseline 过滤掉（那一份已经通过 session_capabilities / PiAgent system prompt 注入，避免重复） | 写死在 `SESSION_BASELINE_INJECTION_SLOTS` 常量 |
| C3 | **Pending Hook Actions → user message** | `hook_delegate.rs:710-736 collect_pending_hook_messages` + `build_pending_action_message` | 每次 `transform_context` / `after_turn` / `before_stop`；有 `HookPendingAction` 在队列里就把它拿出来 | `AgentMessage::user(...)`：包含 title / action_type / status / action_id / summary / 关联 injections / constraints；被消费后标记 last_injected_at_ms | 通过 `resolve_pending_action` 手动解决 |
| C4 | **transformed_message 改写** | `hook_delegate.rs:344-348` | UserPromptSubmit hook 返回 `resolution.transformed_message` 时 | 直接 **替换**最后一条 user 消息的文本（原地改写） | hook/rhai 脚本控制 |
| C5 | **BeforeProviderRequest（observe only）** | `hook_delegate.rs:643-670` + `agent_loop.rs` 每次发 bridge 请求前 | 每次 LLM 调用前 | 仅记 trace（system_prompt_len / message_count / tool_count），不改消息 | 不可关，但无副作用 |
| C6 | **Agent_loop transform_context + get_steering_messages 原生 hook** | `crates/agentdash-agent/src/agent_loop.rs:91-122, 234, 541-560` | 每次循环迭代 | `transform_context` 就是 C1；`get_steering_messages` 来自父 hub 的 pending actions 投递 | 可在 AgentConfig 里不挂 |

### 1.4 工具调用前后

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| D1 | **BeforeTool Hook** | `hook_delegate.rs:366-436` + provider script_engine | 每次 tool call 发出前 | 可以：Deny（返回 reason，tool 不执行）/ Ask（产出 approval request，走 UI） / Rewrite（直接改 tool input） | rhai 脚本 / hooks/presets |
| D2 | **AfterTool Hook** | `hook_delegate.rs:438-481` | tool 结果回来后 | 产出 diagnostics 附到结果、可请求 refresh_snapshot（把 snapshot 重新 load，下一轮 UserPromptSubmit 会注入新内容） | 同上 |
| D3 | **AfterTurn Hook → steering / follow_up** | `hook_delegate.rs:483-537` | 每轮 LLM 响应结束（非 tool loop 末尾）后 | steering = `build_hook_steering_messages`（又一条 `AgentMessage::user`，内容是 resolution.injections 的 Markdown）；pending actions 也在这里取走；两者合起来喂下一轮 | rules/脚本控制 |

### 1.5 停机/终止

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| E1 | **BeforeStop Hook（stop gate）** | `hook_delegate.rs:539-641` | agent 打算 Stop 时 | 若 `completion.satisfied==false`、或存在 `blocking_review` pending action，就返回 `StopDecision::Continue{ steering, follow_up, reason }`，把 steering 作为 user message 追加，阻止停机 | 在 hook provider 里返回 `completion.satisfied=true` 并清空 pending actions |
| E2 | **BeforeCompact Hook** | `hook_delegate.rs:146-228` | 每次 agent 决定压缩上下文前 | 可改写 `CompactionParams`（keep_last_n / reserve_tokens / custom_summary / custom_prompt） | hook provider |
| E3 | **AfterCompact Hook** | `hook_delegate.rs:230-272` + `crates/agentdash-agent/src/compaction/mod.rs` | 压缩完成后 | 只记录，不注入 | — |
| E4 | **CompactionSummary 作为首条 AgentMessage** | `crates/agentdash-agent/src/compaction/mod.rs:73-220` | 每次压缩 | 把被压缩的历史替换为 `AgentMessage::CompactionSummary { summary, messages_compacted, ... }`，后续 LLM 调用会看到它作为对话起点 | compaction 本身可由 hook 取消 |

### 1.6 Bridge / LLM 侧（真正发给 LLM 之前的最后一次改写）

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| F1 | **PiAgent `build_runtime_system_prompt`** | `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:241-422` | 每次 `PiAgentConnector::prompt` → `agent.set_system_prompt(...)` | 拼装顺序：`## Identity`（connector base + executor_config.system_prompt，Override/Append 由 `system_prompt_mode` 控制）→ `## Project Context`（= `context.system_context`）→ `## Companion Agents`（session_capabilities）→ `## Workspace`（VFS mounts）→ `## Available Tools`（内嵌 + 平台 MCP + 用户 MCP + Path convention）→ `## Hooks`（pending actions 摘要）→ `## Skills`（XML `<available_skills>`） | 这是当前**唯一**把"系统级注入"真正合并成单块 system prompt 的地方；可通过 `system_prompt_mode=Override` 或空 system_context 规避，但组件结构写死 |
| F2 | **connector.system_prompt 基础人设** | `connector.rs:1077-1105` | 启动时从 settings `agent.pi.system_prompt` 读一次 | 嵌进 `## Identity` 第一段 | settings 改字符串 |
| F3 | **executor_config.system_prompt** | `ExecutionContext.executor_config.system_prompt` + `system_prompt_mode` | 每次 prompt 时 | Append 或 Override 到 Identity | frontend `executor_config` 透传 |
| F4 | **Bridge convert_messages 过滤** | `anthropic_bridge.rs:232-236`、`openai_completions_bridge.rs:190-196`、`openai_responses_bridge.rs:190-196` | 每次 LLM HTTP 请求前 | 过滤 `Assistant { stop_reason: Some(Error \| Aborted), ... }` —— 不让失败/取消的 assistant turn 被重放（commit 30991ef） | 不可关；同时 message.rs 改成把 error text 落到 content 里兜底 |
| F5 | **PiAgent MCP Tools 注入** | `connector.rs:763-785` 附近（`agent.set_mcp_servers(...)` + `set_tools`） | 每次 prompt 时 | MCP server list → 实际打 list_tools 之后变成 tools（`## Available Tools` 外 MCP 区段） | 通过 CapabilityResolver 输出决定 |
| F6 | **Vibe Kanban Connector — system_context** | `crates/agentdash-executor/src/connectors/vibe_kanban.rs`（commit bce0825 改过） | 非 Pi 路径 | 现在显式读 `ExecutionContext.system_context` 拼到 prompt 前；先前它是从 prompt_blocks 里的 resource 拿的 | 实现固定 |

### 1.7 Bridge/桥接层重放 / 恢复（冷启动）

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| G1 | **Repository Rehydrate → RestoredSessionState.messages** | `prompt_pipeline.rs:175-190` + `session/continuation.rs build_restored_session_messages_from_events` | 进程重启后 session 第一次续跑，`has_live_runtime=false && last_event_seq>0 && !has_executor_follow_up` | 从 persisted session_events 重建一整串 AgentMessage，塞进 `ExecutionContext.restored_session_state`，PiAgent 启动前把它作为历史灌回 `Agent.state.messages` | 判定在 `resolve_session_prompt_lifecycle` |
| G2 | **Repository Rehydrate → continuation system_context** | `session/continuation.rs build_continuation_system_context_from_events` | 同上；`SystemContext` 模式（executor 不支持 repository_restore） | 从已有事件投影重建 Markdown（含摘要），作为 `system_context` 喂回去 | — |
| G3 | **PromptRequestAugmenter（SessionHub 侧的自动增强）** | `crates/agentdash-application/src/session/augmenter.rs` + `crates/agentdash-api/src/bootstrap/prompt_augmenter.rs` | SessionHub 内部构造 `PromptSessionRequest` 时（auto-resume、系统驱动续跑） | 按 session owner / agent preset / workflow 把裸请求"补齐到和主通道一致"——MCP/flow_caps/vfs/system_context/bootstrap_action | 注入逻辑与 HTTP 主通道共享 |

### 1.8 前端触发

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| H1 | `promptSession` | `frontend/src/services/executor.ts:26-31` | 用户点击发送 | `POST /sessions/{id}/prompt`，body 只含 `promptBlocks / workingDir / env / executorConfig` —— **前端不注入任何 system_context / MCP / VFS** | — |
| H2 | `executorConfig`（executor / provider_id / model_id / thinking_level / permission_policy） | `frontend/src/features/executor-selector/...` | 用户在 ExecutorSelector 里选 | 透传给后端覆盖 session_meta.executor_config；Pi 侧 thinking_level 通过 `executor_config` 进 `AgentConfig.thinking_level`，不直接注入 context | 全前端控制 |
| H3 | `working_dir` | 同上 | 用户输入或任务路径 | 作为 `resolve_working_dir` 基于 `default_mount_root` 解析出的路径参与 `## Workspace` 段 + hook snapshot metadata | 前端选 |
| H4 | agent defaults（当前仍未接入） | `frontend/src/features/executor-selector/model/useExecutorConfig.ts:8-11` | — | **当前是硬编码 `PI_AGENT + medium`**；PRD 目标就是改成从 session-bound agent 拉默认值。此前所谓"agent context"并不通过这条路，真正的 agent initial_context / prompt_template 由 B2/B7 在后端注入 | PRD 正在改 |

**前端没有 ide_selection / 选择文件挂载 / 当前目录 以外的额外 context 字段。** 所有 system/owner/workflow context 100% 由后端的 assembler + hook provider 生成，这一点是当前架构的关键事实。

### 1.9 MCP / Plugin 注入

| # | 注入点 | 位置 | 触发时机 | 形态 | 可关 |
|---|---|---|---|---|---|
| I1 | **platform_mcp_configs → McpContextContributor** | 1c + B6 | 每次 compose_* | 进 system_context Markdown 的 `mcp_config` slot + 工具清单通过 F1 `## Available Tools` | CapabilityResolver 决定 |
| I2 | **custom_mcp_servers**（CapabilityResolver 输出） | `crates/agentdash-application/src/capability/resolver.rs` | compose_* | 并入 `effective_mcp_servers`；F1 的 `## Available Tools` 会列出；tools 在 runtime 从 server list_tools 拉 | 资源解析配置 |
| I3 | **agent preset_mcp_servers（project_agent link）** | `AgentLevelMcp.preset_mcp_servers` | compose_owner_bootstrap | 同上合并 | agent link 配置 |
| I4 | **MCP Preset Probe Relay** | `crates/agentdash-mcp` + `.trellis/tasks/04-23-04-23-mcp-preset-probe-relay` | preset 发现 | 本身不注入 session context，但影响 available_presets 集合，从而影响 I1/I2 输出 | — |
| I5 | **Plugin 提供的额外 Skill 目录** | `prompt_pipeline.rs:208-225` (`extra_skill_dirs` in `SessionHub`) + `crates/agentdash-application/src/skill/` | 每次 prompt 时与 VFS skill 合并 | 进入 SessionBaselineCapabilities.skills → F1 `## Skills` XML 段 | `SessionHub::with_extra_skill_dirs` 控制 |

---

## 2. 按"注入形态"重新归类（便于写 ContextBuilder）

### 2.1 最终进入 LLM **system prompt** 的内容（由 PiAgent F1 合并，其它 executor 类似）

- Connector 基础人设（F2） + executor_config.system_prompt（F3）
- `system_context` 字符串（B1–B9 生成的那一大块 Markdown）
- `session_capabilities.companion_agents` 派生段（F1 第 2b 段）
- `context.vfs` 的 mount 清单（F1 第 3 段）
- `runtime_tools` + `mcp_servers` → "Available Tools" + Path convention（F1 第 4 段）
- `hook_session.pending_actions` + 固定 Hook 说明段（F1 第 6 段）
- `session_capabilities.skills` → XML（F1 第 7 段）

### 2.2 作为 **user message**（或 resource block）进入 LLM messages

- `agentdash://session-capabilities/{sid}` resource block（B13，**仅首轮/bootstrap**，插在 prompt_blocks[0]）
- 用户真实输入（UserPromptInput.prompt_blocks）
- Kickoff prompt（B10） / companion dispatch prompt（B11） / routine template（B12）——这些本质就是 agent 看到的"user 首轮指令"
- Hook 注入的 user 消息：
  - UserPromptSubmit 汇总（C1，每轮）
  - Pending actions 详情（C3，每轮）
  - AfterTurn steering（D3，每轮）
  - BeforeStop steering（E1，停机时）
- Compaction 替换后的 `AgentMessage::CompactionSummary`（E4）

### 2.3 **独立 LLM 调用**（不进主 session 的 LLM context）

- Title generator（B16）——独立 bridge call，system_prompt = TITLE_SYSTEM_PROMPT，只见 user 原文

---

## 3. 最近相关提交串联

| Commit | 影响 | 与哪个注入点相关 |
|---|---|---|
| `bce0825` fix(session): title gen no longer mixes agent context | **OwnerBootstrap 不再把 context_markdown 塞进 prompt_blocks**，改只走 `system_context`；Vibe Kanban connector 也改读 `system_context` | B8 / B9 / F1 / B16（让 title generator 看到干净的 user prompt） |
| `866e42a` fix(session): 修复摘要 system prompt 无法识别 | OpenAI 两个 bridge 里 compaction 摘要的 system prompt 字段拼装修正 | E4 / F1 compaction 段 |
| `30991ef` skip error/aborted assistant messages in LLM context | 三个 bridge 在 convert_messages 时过滤 `stop_reason=Error\|Aborted` 的 Assistant；message.rs 同步把 error text 落到 content | F4 |
| `40f29fd` 修复挂起会话自动终止流程 | SessionHub 侧 recover_interrupted_sessions 更完整地发终态通知 | A1/G1 的 lifecycle 判定 |

---

## 4. 对"统一 ContextBuilder"的初步观察

> 这部分是事实性观察，不是方案建议。

1. **"注入"的主干其实有两条独立链路，彼此几乎不知道对方存在**：
   - **Compose 链路**（1.2 B1–B12 + workflow + mcp）：一次性、预拼好 `system_context` 字符串；入 SessionHub 之后就只是一块不可分解的 Markdown。
   - **Hook 链路**（1.3–1.5 C/D/E）：完全在 agent_loop 内循环，每轮动态拼 user message；它不读 system_context 的结构化数据，反而会另起炉灶把 `snapshot.injections` / pending actions 格式化成新 user message。
   - 结果是：**同一件事经常出现在两处**（如 companion_agents baseline 段：一次通过 `SessionBaselineCapabilities` 进 F1 system prompt、一次在 hook snapshot injections 里，所以 provider 特地加了 `filter_user_prompt_injections`/`SESSION_BASELINE_INJECTION_SLOTS` 去重）。

2. **`system_context` 是当前唯一的"预拼 Markdown"合流点**，但它：
   - 只在 `OwnerBootstrap`/`RepositoryRehydrate(SystemContext)`/`compose_lifecycle_node` 等少数分支下被真正填；
   - 进入 `ExecutionContext` 之后对下游 connector 来说是"不透明字符串"，PiAgent 只能整段放到 `## Project Context` section（F1）；
   - ContextContributor pipeline 的 fragments 顺序/slot 是在 `ContextComposer` 内隐式收敛的，**composer 之后丢失了所有 slot/order/source 元信息**——hook runtime / executor 无法再按来源局部改写。

3. **Session capabilities 是第二个"隐式契约层"**：它走 resource block（B13）+ F1 重渲染两条路径，前端/调用方并不直接可见；`SESSION_BASELINE_INJECTION_SLOTS = ["companion_agents"]` 是这个契约的硬编码白名单。

4. **Executor 层（F1）有自己的拼装顺序**，和 Assembler 的 fragment order 无关。`build_runtime_system_prompt` 固定为 Identity → Project Context → Companion Agents → Workspace → Available Tools → Hooks → Skills；Markdown 内部 heading 层级由 connector 控制。任何 ContextBuilder 改造要么对接到 F1 之前（把 system_context 拆成结构化，让 connector 自己合并），要么替代 F1。

5. **Workflow/Lifecycle** 注入不少是 **通过 capability directive → MCP server** 实现的，真正"写进 context"的只有 B5 + B10 两处（binding 内容 + kickoff prompt）；workflow snapshot 更多是走 hook metadata（`ActiveWorkflowMeta`）给 rules 用。

6. **前端目前的贡献面极窄**（H1–H3），这意味着 ContextBuilder 的关键调整点都在 Rust 层；PRD 中"agent defaults 自动加载"只影响 `executor_config`（H2），不影响 B/C/D/E/F 这些注入通道。

7. **没有被覆盖**的角落（研究中未发现针对它们的注入）：
   - 没有"MEMORY.md / CLAUDE.md / AGENTS.md 自动加载"的 Rust 实现。`.trellis/workflow.md` 是作为 `workflow_context_bindings` 的 locator 手动挂进来的（见 `crates/agentdash-domain/src/workflow/value_objects.rs:1055` / `entity.rs:555` 默认 contract）；不是隐式全局文件。
   - `SessionContext Mount`、`Address Space Snapshot` 在 `.trellis/tasks/03-30-session-context-mount` / `04-17-address-space-snapshot` / `04-08-cross-mount-shell-materialization` 下有设计记录，但在运行时注入层面尚未看到统一实现——`AppExecutionHookProvider` 没有处理它们，`context/vfs_discovery.rs` 只是 VFS 发现层。

---

## Caveats / Not Found

- **ide_selection / 光标位置 / 选中文件** 这类 VSCode 风格字段：当前 `UserPromptInput` schema（`session/types.rs:13-22`）里**不存在**；前端 `PromptSessionRequest`（`frontend/src/services/executor.ts:19-24`）也没有。
- **CLAUDE.md / MEMORY.md 自动读取**：全仓搜索未见专门逻辑。Trellis spec 里的 `.trellis/workflow.md` 是作为 workflow_context_binding 的默认 locator 存在的，不是隐式自动注入。
- **Plugin 对 session 的注入点**：除 `SessionHub::with_extra_skill_dirs`（I5）外，未发现插件 API 能直接往 session context 塞 Markdown；如果存在也只能走 hook provider / rhai 脚本间接实现。
- 未完整阅读：`crates/agentdash-agent/src/agent_loop.rs` 的全部 1800 行；`crates/agentdash-application/src/companion/tools.rs` 的 companion slice 细节；`crates/agentdash-application/src/hooks/presets.rs` 每条 preset 的 rhai 脚本（仅看了 4 条脚本文件名：`stop_gate_lifecycle_advance.rhai` / `port_output_gate.rhai` / `subagent_result_channel.rhai` / `subagent_inherit_context.rhai`，这些会在 hook 链上产生自己的 injections）。
