# Session → Agent 管线彻底 Review 与重构方案

- **Date**: 2026-04-30
- **输入**: `01-runtime-layer.md` / `02-context-layer.md` / `03-connector-hook-layer.md`
- **范围**: 从 HTTP route / task service / workflow orchestrator / companion / routine / auto-resume 入口开始，
  经 compose → finalize_request → SessionHub.start_prompt → prompt_pipeline → connector.prompt → turn_processor → hook delegate
  整条链路的概念边界、数据流、冗余与耦合盘点，以及对应的分阶段重构计划。
- **阅读顺序**: Part A 诊断 → Part B 目标形态 → Part C 迁移路径 → Part D 决策点。
  想直接看"干什么"跳 Part C，想理解"为什么"看 Part A + B。

---

## Part A · 现状诊断：为什么边界模糊

### A.1 概念拓扑（当前）

七个互相邻居的概念 + 它们各自持有哪些字段，用表格对齐。

| 概念 | 定义位置 | 本质是什么 | 关键字段 | 生命周期 |
|---|---|---|---|---|
| `UserPromptInput` | `session/types.rs:13` | 纯前端反序列化 DTO | `prompt_blocks` / `working_dir` / `env` / `executor_config` | 单次 HTTP 请求 |
| `PromptSessionRequest` | `session/types.rs:27` | 后端完整请求（DTO 外 + 注入字段） | `user_input` + 6 个后端注入字段 + `context_bundle` + `identity` + `post_turn_handler` | 单次 compose 到 connector.prompt 的短链 |
| `PreparedSessionInputs` | `session/assembler.rs:80` | compose 的平坦输出结构 | 与 `PromptSessionRequest` 高度重叠的 11 个字段 | 从 compose 到 `finalize_request`（函数内） |
| `SessionContextBundle` | `spi/session_context_bundle.rs:21` | 结构化 fragment 合集（语义化 slot/order/scope） | `fragments: Vec<ContextFragment>` + `bundle_id` + `phase_tag` | 跟随宿主 `PromptSessionRequest` / 之后被 render 成字符串 |
| `ExecutionContext` | `spi/connector.rs:47` | connector 入参 union type（三种 connector 各取所需） | 12 个字段（`assembled_system_prompt` / `assembled_tools` / `mcp_servers` / `vfs` / `hook_session` / `runtime_delegate` / ...） | per-turn（每次 `connector.prompt` 重建） |
| `SessionRuntime` | `session/hub_support.rs:167` | 内存 session 级 runtime 状态 | `tx` / `running` / `active_execution` / `hook_session` / `processor_tx` / `hook_auto_resume_count` | 从 `ensure_session` 到 `delete_session`（进程级） |
| `ActiveSessionExecutionState` | `session/hub_support.rs:184` | per-turn 运行时快照 | `mcp_servers` / `vfs` / `working_directory` / `executor_config` / `flow_capabilities` / `effective_capability_keys`(dead)/ `identity` | 当前 turn 内 |
| `SessionMeta` | `session/types.rs:222` | 持久化元信息 | `id` / `title` / `executor_config` / `executor_session_id` / `bootstrap_state` / `visible_canvas_mount_ids` | 伴随 session 持久化 |

**结构性问题**：
- `PromptSessionRequest` / `PreparedSessionInputs` / `ExecutionContext` / `ActiveSessionExecutionState` 四者对 **executor_config / mcp_servers / vfs / flow_capabilities / working_dir** 这一组核心字段都各存一份，在不同阶段手工搬运。
- `SessionContextBundle` 本来应该成为业务上下文的"单一主数据面"，但实际被提前渲染成 `assembled_system_prompt` 字符串后 executor 层就看不到 bundle 了（见 `03-connector-hook-layer.md` §2）。
- `SessionRuntime` 是 session 级的，但承载了大量 per-turn 字段（`processor_tx` / `hook_auto_resume_count` / `cancel_requested`），与 `ActiveSessionExecutionState` 职责重叠。

### A.2 6 条入口，3 套 compose 节拍

从 `01-runtime-layer.md` §1 汇总：

| # | 入口 | compose 节拍 | augmenter | identity / post_turn_handler |
|---|---|---|---|---|
| 1 | HTTP `POST /sessions/:id/prompt` | `augment_prompt_request_for_owner` 内 compose + finalize | ✅（入口就是 augmenter 路径） | 由 HTTP handler 显式填 |
| 2 | Task service | Service 内自 compose + finalize | ❌ | 手动显式填 `task/service.rs:272-281` |
| 3 | Workflow orchestrator | Service 内自 compose + finalize | ❌ | 手动填 |
| 4 | Companion tools | Service 内自 compose + finalize | ❌ | 手动填 |
| 5 | Routine executor | Service 内自 compose + finalize | ❌ | **没填**（`routine/executor.rs:500-515`） |
| 6 | Hub auto-resume | 裸 req → `SharedPromptRequestAugmenter.augment` → 回到入口 1 节拍 | ✅ | augmenter 内部填 |

**问题**：
- "前 compose 后 start" 节拍由 5 处分散维护，`identity` / `post_turn_handler` 必须手动落，routine 路径已经漏填。
- augmenter 只兜底一条路径（auto-resume），名义是"统一入口"，实际是"auto-resume 的特型"。
- `augment_prompt_request_for_owner`（`routes/acp_sessions.rs:954`）名义在 api crate，但做的事情等价于"compose 路径决策 + fragment 合并 + continuation 生成"，这些都应该属于 application 层的职责。

### A.3 finalize_request 的不对称

`assembler.rs:105-140`：

- `mcp_servers` 整体替换 vs `relay_mcp_server_names` 用 `extend` → 两字段语义不一致。
- `vfs` 三重分支：base 无 / prepared 有 → 用 prepared；base 有 / prepared 有 → **仍以 prepared 为准**。HTTP 透传 `vfs` 在 compose 产出 vfs 时会被直接覆盖。
- `identity` / `post_turn_handler` 不管，调用方负责。
- `apply_workspace_defaults` 紧接着被 `prepared.vfs` 覆盖，顺序脆弱。

### A.4 hub.rs 2800 行的职责分类

从 `01-runtime-layer.md` §6 汇总，`hub.rs` 当前同时承担：

1. **门面**：对外 `start_prompt` / `cancel` / `delete_session` / `ensure_session` / `subscribe`。
2. **Factory 寄存**：`base_system_prompt` / `user_preferences` / `runtime_tool_provider` / `mcp_relay_provider` 都挂在 hub。
3. **工具构建**：`build_tools_for_execution_context` 直接依赖 `agentdash-executor` crate 内 `mcp::discover_*`（跨 crate 层级穿透）。
4. **Hook 触发**：`emit_session_hook_trigger` / `ensure_hook_session_runtime`。
5. **Cancel 重放**：`cancel` 内扫 `persistence.list_all_events` 补发 `turn_interrupted`。
6. **Compaction boundary 推导**：从事件仓储推导 compaction 节点。
7. **Companion wait registry**：`hub.rs:38`。
8. **Auto-resume 调度**：`schedule_hook_auto_resume`。

**一个 `hub.rs` 既是门面又是协调器又是事件仓储 facade，又持工具 builder + hook 触发 + companion wait registry。**

### A.5 prompt_pipeline 是 500 行的上帝函数

`start_prompt_with_follow_up`（`prompt_pipeline.rs:23-490+`）同时做：

1. Prompt payload 解析
2. VFS / working_dir 解析
3. Hook session runtime 加载或 refresh
4. Restored session state 构建（仅 cold start ExecutorState）
5. Skill + guideline 扫描
6. Session baseline capabilities 组装
7. System prompt 组装（调 `assemble_system_prompt`）
8. `ExecutionContext` 裸结构字面量构造（13 字段）
9. SessionMeta 回写
10. Title generation spawn
11. User message & turn_started 持久化
12. SessionStart hook 触发
13. connector.prompt 调用
14. SessionTurnProcessor spawn
15. Stream adapter spawn

"模块边界 = hub.rs 太大挪出来"，不是职责分割。

### A.6 turn_processor 的 per-turn / session 级混杂

从 `01-runtime-layer.md` §6：

- `handle_notification` 里做 `executor_session_id` 同步（session meta 级副作用，每条 notification 都做）。
- `hook_auto_resume_count` 在 processor 内递增（session 级状态）。
- processor 终止时直接写 `SessionRuntime.running` / `current_turn_id` / `processor_tx`。
- 调 `hub.schedule_hook_auto_resume`（反向依赖回 hub）。

**名义 per-turn，实际频繁改 session 级状态。**

### A.7 ExecutionContext 是"三种 connector 的 union type"

`03-connector-hook-layer.md` §4.2 表格：三个 connector 对 12 个字段的消费几乎不重叠。

- PiAgent：读 `assembled_system_prompt` / `assembled_tools` / `hook_session` / `runtime_delegate` / `restored_session_state` / `executor_config` / `turn_id`。
- Relay：读 `mcp_servers` / `vfs` / `working_directory` / `environment_variables` / `executor_config` / `identity`。
- vibe_kanban：读 `assembled_system_prompt` / `vfs` / `working_directory` / `environment_variables` / `executor_config`。

**没有一个字段是三个 connector 都消费的"公共"字段（除了 `executor_config`）。**

### A.8 Bundle 不是运行时主数据面

从 `03-connector-hook-layer.md` §2-3：

- `SessionContextBundle` 在 `PromptSessionRequest` 上出现；
- `system_prompt_assembler::assemble_system_prompt` 把它 `render_section(...)` 成字符串塞进 `assembled_system_prompt`；
- `ExecutionContext` **没有 `context_bundle` 字段**；
- `grep context_bundle` 在 `crates/agentdash-executor/` 命中为 0。

Bundle 是"组装原料而非主数据"。executor 只看字符串，这让"Context Inspector 看到的 bundle" 与"LLM 实际吃到的 system prompt" 之间隔了一层黑盒渲染。

### A.9 Hook 的双轨数据面

| 轨道 | 何时 | 结果 |
|---|---|---|
| 组装期（一次） | `compose_*` 前 `provider.load_session_snapshot` | `SessionHookSnapshot.injections` → `Contribution::from(...)` → 进 Bundle |
| 运行期（每次 hook trigger） | `HookRuntimeDelegate.transform_context / after_turn / before_stop` | `HookInjection` → `build_hook_injection_message` → **user message**；同时 `emit_hook_injection_fragments` → audit bus（**不回注 Bundle**） |

- 组装期双用途的同一 `HookInjection` struct 在组装期变 Bundle fragment、运行期变 user message string，依赖**手动约定**决定转向哪条路径。
- 去重靠 `HOOK_USER_MESSAGE_SKIP_SLOTS = &["companion_agents"]`（`hook_delegate.rs:806`）写死白名单。
- `transform_context` 同时产出 "prompt_blocks 改写" 与 "本轮 blocked"（`TransformContextOutput { messages, blocked }`）——① prompt 类与 ③ 控制流类在同一方法耦合。

### A.10 contribute_* 的三处重复渲染

`02-context-layer.md` §2.2：

- **`workflow_context` slot**：`contribute_workflow_binding`（task）/ `contribute_lifecycle_context`（lifecycle）/ `compose_companion_with_workflow` 手工 upsert（companion）三份各写一套 goal + instructions + bindings。
- **`workspace` slot**：`contribute_core_context`（task，含 status）/ `workspace_context_fragment`（owner 共享，不含 status）/ `build_workspace_snapshot_from_entries`（workspace_sources 里的 snapshot view）三份。
- **SessionPlan 嵌入 vs 外挂**：owner 路径 `contribute_story_context` / `contribute_project_context` 内部直接调 `build_session_plan_fragments`；task 路径 `compose_story_step` 在外部独立 push；lifecycle 路径完全不走 SessionPlan。
- **`source_resolver` vs `workspace_sources`**：同一 `ContextSourceRef` 列表按 kind 分流到两套实现，`fragment_slot / fragment_label / render_source_section / display_source_label` 四个 helper 逐行重复。
- **task 路径不复用 `contribute_story_context`**：story 层后续新增 fragment 不会自动进 task 路径。

### A.11 companion_agents 的三条路径

同一份数据被渲染三次（`02-context-layer.md` §7.2）：

1. `SessionBaselineCapabilities.companion_agents` → SP `## Companion Agents` section
2. `HookInjection { slot: "companion_agents" }` → `build_hook_injection_message` 内被 `HOOK_USER_MESSAGE_SKIP_SLOTS` 过滤（但只在 transform_context 路径过滤；after_turn/before_stop 不过滤）
3. `agentdash://session-capabilities/{session_id}` resource block → user_blocks 首部

三条路径之间由 `HOOK_USER_MESSAGE_SKIP_SLOTS` + prompt_pipeline 显式逻辑手动协调。

### A.12 PiAgent "首轮 set system prompt 后不更新"

`03-connector-hook-layer.md` §2.2：

- `is_new_agent == true` → `agent.set_system_prompt(assembled)`。
- `is_new_agent == false` → **不读 `assembled_system_prompt` 也不读 Bundle**，沿用上次的 system prompt。
- `PiAgentSessionRuntime { agent, tools }` 不存 cached system prompt，所以即使 application 侧重新 assemble 了，connector 也不会热替换。

**这是结构逼出来的隐藏行为，不是显式设计。**

### A.13 字段冗余表

| 字段 | 权威来源 | 实际副本位置（数） |
|---|---|---|
| `working_directory` | `req.user_input.working_dir` → `resolve_working_dir(...)` | req 原值 / `ExecutionContext.working_directory` / `ActiveSessionExecutionState.working_directory` / `SessionSnapshotMetadata.working_directory` — 4 份 |
| `executor_config` | `req.user_input.executor_config` or `session_meta.executor_config` | req / ExecutionContext / ActiveSessionExecutionState / SessionMeta — 4 份（还有 preset 补全） |
| `mcp_servers` | compose 产出 | req.mcp_servers / ExecutionContext.mcp_servers / ActiveSessionExecutionState.mcp_servers / `build_tools_for_execution_context` 内 partition 一次 — 4+ 份 |
| `flow_capabilities` + `effective_capability_keys` | CapabilityResolver | req / ExecutionContext / ActiveSessionExecutionState（keys 已 dead_code）/ hook_session | 语义重叠 |
| `identity` | HTTP handler 填入 | req / ExecutionContext / ActiveSessionExecutionState（纯透传，无逻辑消费） |
| `hook_session` | `load_session_hook_runtime` | `SessionRuntime.hook_session`（Arc 共享）/ ExecutionContext.hook_session / `HookRuntimeDelegate` 内部 |

每一份修改都需要协同，实际没有单一权威。

---

## Part B · 目标形态：用五条轴定义清晰边界

### B.1 五条正交轴

提议把"session 启动到 agent 执行"看成 **5 条正交轴** 的组合，每条轴都要有单一权威来源：

1. **Who** — 用户身份 / owner 归属（`identity` / `SessionOwnerCtx`）
2. **Where** — 执行环境 / 工作空间（`working_directory` / `vfs` / `env`）
3. **What** — 业务上下文（task / story / project / workflow / lifecycle / declared sources / AGENTS.md）
4. **How** — 执行能力 + 工具（`FlowCapabilities` / `assembled_tools` / `mcp_servers`）
5. **When/Trigger** — 本轮触发输入（`prompt_blocks` / `bootstrap_action` / hook side effects）

当前问题是这五条轴全部揉在 `PromptSessionRequest` / `ExecutionContext` 上，**没有分组、也没有不同生命周期的结构**。

### B.2 建议的概念分层

```
┌─────────────────────────────────────────────────────┐
│  Entry DTO                                          │
│  ┌──────────────────┐   ┌─────────────────────┐     │
│  │ UserPromptInput  │ + │ IntentHints         │     │  ← HTTP / routine / auto-resume
│  │ (prompt_blocks,  │   │ (turn_trigger,      │     │
│  │  env, wd, cfg)   │   │  post_turn_handler, │     │
│  └──────────────────┘   │  identity)          │     │
│                         └─────────────────────┘     │
└─────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────┐
│  Compose Pipeline (单一入口)                         │
│  SessionAssemblyPipeline::compose(                   │
│      entry,                                          │
│      owner_scope,          // Story | Project | Task│
│      audit_session_key,    // 必传                  │
│      ...                                             │
│  ) -> SessionStartupPlan                             │
│                                                      │
│  产出一个结构化对象：                                 │
│    - 5 条轴分组字段                                   │
│    - 不含原 prompt_blocks 覆写 / vfs 覆写等副作用      │
└─────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────┐
│  Start Pipeline (SessionHub.start_plan)              │
│  1. Materialize context (bundle render → resources)  │
│  2. Build ExecutionContext (per-turn view)           │
│  3. Persist turn_started + meta                       │
│  4. connector.prompt                                  │
│  5. Spawn turn processor                              │
└─────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────┐
│  ExecutionContext = SessionFrame + TurnFrame         │
│    SessionFrame (不可变)                              │
│      - session_id / turn_id                          │
│      - working_directory / vfs / env                 │
│      - executor_config / identity                    │
│      - flow_capabilities (只读快照)                   │
│    TurnFrame (per-turn 可变)                          │
│      - context_bundle (主数据面)                      │
│      - assembled_tools                                │
│      - hook_runtime_delegate                          │
│      - restored_session_state                         │
│      - system_prompt_renderer (Closure / trait obj)   │
└─────────────────────────────────────────────────────┘
```

关键变化：
- `PromptSessionRequest` 消失（或缩为 `UserPromptInput + IntentHints`）；`PreparedSessionInputs` 消失——两者合并为 `SessionStartupPlan`（强类型、不可变）。
- `ExecutionContext` 分 `SessionFrame` / `TurnFrame` 两部分，每部分单一权威来源；把"用于渲染 system prompt 的 bundle"从字符串还原回结构化对象。
- `SessionRuntime` 保持 session 级，但把 `processor_tx` / `hook_auto_resume_count` 等 per-turn 字段全移到 `ActiveSessionExecutionState`；或者更进一步把 `ActiveSessionExecutionState` 更名为 `TurnExecution` 并作为 `SessionRuntime.current_turn: Option<TurnExecution>` 的显式字段。

### B.3 Bundle 作为主数据面的两种定位

方案 α（最彻底）：ExecutionContext 直接持有 `context_bundle`，connector 自行渲染。
- 优点：executor 能真正访问 Bundle；Inspector 与 LLM 输入完全一致；PiAgent 可以做"每 turn 按 scope + slot 白名单 delta 渲染"。
- 代价：render 逻辑要从 application 下沉到各 connector，或者做一个共享 render trait。Relay connector 需要把 Bundle 序列化下发。

方案 β（渐进）：ExecutionContext 同时持有 `bundle` + `assembled_system_prompt`，优先 bundle 的 connector 自行渲染，fallback 到字符串。
- 优点：relay 可继续吃字符串，pi_agent 逐步迁移到 bundle-aware。
- 代价：暂时 double 字段。

**推荐 β（渐进），最终迁到 α**。

### B.4 Hook 的三类语义物理分离

| 类别 | 当前位置 | 目标位置 |
|---|---|---|
| ① 改 Bundle（静态/半静态） | 组装期 `Contribution::from(&SessionHookSnapshot)` | 保持；新增运行期 `HookRuntimeDelegate.emit_fragment_into_bundle`（通过 channel 回灌） |
| ② 改 prompt_blocks（本轮 user message） | `transform_context.messages` | 只保留"steering 增量消息"语义，不用于"注入静态上下文" |
| ③ 本轮 side effect（block / deny / rewrite） | `transform_context.blocked` / `before_tool_call` / `before_stop` | 拆出独立 `HookControlDecision` 结构，与 ① / ② 分方法 |

消除 `HOOK_USER_MESSAGE_SKIP_SLOTS`——所有 "应该只进 bundle" 的注入由 Bundle 承担，白名单自然失效。

### B.5 contribute_* 统一

- 一个领域对应一个 contributor（`story` / `project` / `workspace` / `task` / `workflow` / `lifecycle` / `declared_sources` / `mcp` / `hook_snapshot`）。
- 任一 compose 场景只需声明"我的 phase 需要哪些 contributor"，builder 自动按序喂给 reducer。
- `workflow_context` / `workspace` / `runtime_policy` slot 渲染抽成共享 helper（`render_workflow_injection` / `render_workspace_view` / `render_lifecycle_runtime_policy`），三个调用点都调 helper。
- `source_resolver` 与 `workspace_sources` 合并到单一 `DeclaredSourceResolverRegistry`，fragment helper 单点。
- SessionPlan fragment 一律走 contributor（不再嵌入到 story/project contributor 内部），每个 phase 决定是否包含。

### B.6 运行时模块目标形态

`session/` 目录按职责分三层：

```
session/
├── model/              # 数据结构：types / plan / meta
├── startup/            # compose pipeline（assembler + augmenter 合并）
│   ├── pipeline.rs     # SessionStartupPipeline::compose
│   ├── contributors/   # contribute_* 纯函数
│   └── finalize.rs     # SessionStartupPlan → 下游所需
├── runtime/            # session 级 runtime
│   ├── hub.rs          # 极简门面（≤ 500 行）
│   ├── session_runtime.rs
│   ├── turn_execution.rs
│   └── auto_resume.rs
├── turn/               # per-turn 处理
│   ├── processor.rs    # 只管 stream → persist → terminal
│   ├── event_bridge.rs # hook trace → ACP
│   └── post_turn_handler.rs
└── integration/        # 跨模块 facade
    ├── companion_wait.rs
    ├── continuation.rs
    └── system_prompt/  # render 层（bundle → string）
```

---

## Part C · 迁移路径：7 个 PR（按风险与收益排序）

先整理出低风险高收益的 PR 放前面，高风险的放后面。每个 PR 独立可验证。

### PR 1 · `finalize_request` 对称化 + 入口节拍统一收口

**问题**：A.2 + A.3。5 处 compose 入口分散，`finalize_request` 不对称。

**动作**：

1. 把 `mcp_servers` 改为 `extend`（或把 `relay_mcp_server_names` 改为"整体替换"，取决于哪边语义对）。统一后在 `SessionAssemblyBuilder::append_mcp_servers` 里做"追加"、`with_mcp_servers` 里做"替换"，两种 API 显式。
2. `vfs` 覆盖规则改为显式 `prefer_base: bool` 参数；`apply_workspace_defaults` 挪到 `finalize_request` 前，不再和 prepared.vfs 抢位置。
3. 引入 `SessionStartupBuilder`（工作名）：所有 5 条入口都调 `SessionStartupBuilder::from_entry(user_input, hints).owner(...).compose().finalize()`，以替代各 service 自己拼 `finalize_request`。
4. 把 `identity` / `post_turn_handler` 挪到 `SessionStartupBuilder` 的 first-class 方法，不再需要调用方在 `finalize_request` 之后赋值。
5. 修复 `routine/executor.rs:500-515` 漏填 identity 的 bug（顺手）。

**收益**：每个入口只写一遍节拍；`identity` / `post_turn_handler` 不会再漏填；`finalize_request` 语义对称。

**风险**：低。纯内部重构，无外部契约变化。

**测试**：
- `finalize_request` 对称性单测
- 5 条入口各自的 snapshot 回归（所有后端注入字段都齐）

**影响文件**：`session/assembler.rs`, `routes/acp_sessions.rs`, `task/service.rs`, `workflow/orchestrator.rs`, `companion/tools.rs`, `routine/executor.rs`, `session/hub.rs`（auto-resume 路径）。

---

### PR 2 · ExecutionContext 分层：SessionFrame + TurnFrame

**问题**：A.7 + A.13。12 字段 union type，冗余严重。

**动作**：

1. 新增 `ExecutionSessionFrame`（执行环境不可变 + 身份）：`session_id` / `turn_id` / `working_directory` / `vfs` / `environment_variables` / `executor_config` / `identity`。
2. 新增 `ExecutionTurnFrame`（per-turn 可变 + 上下文 + 工具）：`context_bundle: Option<SessionContextBundle>` / `assembled_tools` / `hook_session` / `runtime_delegate` / `restored_session_state` / `flow_capabilities` / `assembled_system_prompt`（过渡字段）。
3. `ExecutionContext` 改为 `{ session: SessionFrame, turn: TurnFrame }`。所有 connector 签名不变，但内部按字段分组访问。
4. `ActiveSessionExecutionState` 改成持有 `ExecutionSessionFrame` + 额外 per-session 状态（如 `hook_auto_resume_count`），不再重复存 `working_directory` / `mcp_servers` 等。
5. `hub.replace_runtime_mcp_servers`（`hub.rs:441-500`）不再构造"ghost ExecutionContext"；提供 `build_tools_for_session_frame(session: &SessionFrame, mcp: &[McpServer])` 签名明确。
6. 删掉 `effective_capability_keys: #[allow(dead_code)]`，要么真消费要么真删。

**收益**：
- 12 字段 → 7+6 分层，语义清晰。
- `ActiveSessionExecutionState` 不再和 ExecutionContext 拷贝字段。
- `hub.replace_runtime_mcp_servers` 不再需要构造伪造 context。

**风险**：中。所有 connector 都要适配新结构；尽量保 backward-compat 只改内部组织。

**测试**：既有 connector 单测全跑通（无行为变化）。

**影响文件**：`spi/connector.rs`, `session/hub_support.rs`, `session/hub.rs`（replace_runtime_mcp_servers）, `session/prompt_pipeline.rs`（构造点）, `connectors/pi_agent/connector.rs` / `relay_connector.rs` / `vibe_kanban.rs`（字段访问适配）。

---

### PR 3 · Bundle 进 ExecutionContext.TurnFrame

**问题**：A.8 + A.12。

**动作**：

1. 在 PR 2 的 `ExecutionTurnFrame` 上新增 `context_bundle: Option<SessionContextBundle>`。prompt_pipeline 透传 `req.context_bundle`。
2. 保留 `assembled_system_prompt`（过渡），但标 `#[deprecated(note = "connector 应读 context_bundle 自行按 scope/slot 渲染")]`。
3. `system_prompt_assembler` 保留，但导出一个 `render_runtime_section(bundle) -> String` 给各 connector 调用，而不是在 application 层一次性拼。
4. PiAgent 新增热更路径：`update_session_context_bundle(session_id, bundle)`，在每轮 prompt 开始时比对 bundle_id 是否变化，变化则重 set system prompt。先 log 再实现，避免副作用风险。
5. Relay connector 暂时继续读 `assembled_system_prompt`（因为要下发字符串）。

**收益**：
- Bundle 真正进 connector 边界；Inspector 所见 = LLM 所见。
- 打开 PiAgent 热更 system prompt 的门（解决 A.12）。

**风险**：中。新增字段 backward-compat，但要校对所有 connector 至少不 panic。

**测试**：
- PiAgent 单测：同一 session 两轮 prompt，第二轮 bundle 变化时 system prompt 应更新（现阶段先加 assert 验证是否更新，行为切换可放 PR 6）。
- Inspector DTO 兼容测试。

**影响文件**：`spi/connector.rs`, `session/prompt_pipeline.rs`, `connectors/pi_agent/connector.rs`, `system_prompt_assembler.rs`。

---

### PR 4 · Hook Fragment 回灌 Bundle + 三类语义分离

**问题**：A.9 + A.11。

**动作**：

1. 运行期 `HookRuntimeDelegate.emit_hook_injection_fragments` 除了 emit audit，还应产出一份 `Vec<ContextFragment>` 通过 channel/callback 回灌到 `SessionRuntime.current_turn.context_bundle`（PR 3 引入字段后才有位置）。
2. Split `HookRuntimeDelegate.transform_context` 返回值：
   ```
   TransformContextOutput {
       bundle_delta: Vec<ContextFragment>,   // 回灌 bundle
       steering_messages: Vec<AgentMessage>, // 强本轮 steering
       control: HookControlDecision,          // Allow / Block { reason }
   }
   ```
3. `build_hook_injection_message` 逐步缩小职责：只保留"steering 增量"语义，不再承担"注入静态上下文"。
4. `companion_agents` / `session-capabilities` resource block 统一走 Bundle：
   - `SessionBaselineCapabilities.companion_agents` → Bundle fragment `companion_agents` slot（与 hook 用同一 slot）；
   - 删掉 `prompt_pipeline.rs:379-397` 在 user_blocks 首部塞 session-capabilities resource 的路径；
   - 删掉 `HOOK_USER_MESSAGE_SKIP_SLOTS`。
5. fragment_bridge `From<&SessionHookSnapshot> for Contribution` 接入运行时路径（当前 0 引用）。

**收益**：
- Hook 的 ①②③ 三类语义彻底分离，新增 hook 类型不用再担心误触 user message。
- companion_agents 的"三条路径"合并为一条。
- 去掉手动白名单技术债。

**风险**：中-高。`transform_context` 签名变化，下游若有实现需要适配（可加 default impl）。

**测试**：
- hook delegate 单测补充 bundle_delta 维度
- companion agent 渲染不重复的回归测试

**影响文件**：`session/hook_delegate.rs`, `hooks/fragment_bridge.rs`, `session/prompt_pipeline.rs`, `session/system_prompt_assembler.rs`, `session/baseline_capabilities.rs`。

---

### PR 5 · contribute_* 去重与路径统一

**问题**：A.10。

**动作**：

1. **workflow_context 共享 helper**：抽出 `render_workflow_injection(workflow, bindings_opt, mode) -> Vec<ContextFragment>`，`contribute_workflow_binding` / `contribute_lifecycle_context` / `compose_companion_with_workflow` 都调它。`compose_companion_with_workflow` 顺手走审计总线。
2. **workspace slot 单源**：`workspace_context_fragment` 扩参数支持带/不带 `status`；删掉 `contribute_core_context` 内的 workspace 渲染。
3. **SessionPlan 统一外挂**：`contribute_story_context` / `contribute_project_context` 不再内置 session_plan.extend；每个 compose 场景显式 push `Contribution::fragments_only(session_plan.fragments)`。Lifecycle 补上 SessionPlan 调用。
4. **task 路径复用 contribute_story_context**：`compose_story_step` 调 `contribute_story_context`（Story owner）+ `contribute_task_binding`（task-only 字段），消除重复。
5. **source_resolver / workspace_sources 合并**：抽 `SourceResolverRegistry::resolve_all(sources, ctx) -> Resolved`，内部按 kind 分流到 manual / http / mcp / workspace；fragment helper 单点（移至 `context/rendering/declared_sources.rs`）。
6. **order 常量集中**：建 `context/slot_orders.rs`，集中管理所有 slot 的默认 order（目前散落 10/20/30/35/36/37/38/40/48/49/50/60/80/82/83/84/85/86/89/90/96/100/200）。`HOOK_SLOT_ORDERS` 引用同一常量。

**收益**：
- workflow_context / workspace 渲染不再三处漂移。
- task 与 owner 路径复用 story contributor。
- lifecycle bundle 不再最薄。
- order 数字有单一来源。

**风险**：中。需要完备的 bundle snapshot 回归测试（对比 5 条 compose 产出的 bundle 结构在重构前后是否等价）。

**测试**：
- 对每个 compose 场景写 snapshot 测试锁定 fragment 结构
- `source_resolver` 单测迁移到新 registry

**影响文件**：`context/builtins.rs`, `context/workspace_sources.rs`, `context/source_resolver.rs`, `context/workflow_bindings.rs`, `story/context_builder.rs`, `project/context_builder.rs`, `session/assembler.rs`（lifecycle / companion 分支）, `session/plan.rs`。

---

### PR 6 · SessionHub 拆分

**问题**：A.4。hub.rs 2800 行。

**动作**：

把 hub.rs 按 A.4 列出的 8 项职责拆分：

1. `hub/facade.rs`：`SessionHub` 只保留对外 API（start / cancel / subscribe / delete / ensure）。
2. `hub/factory.rs`：`SessionHubFactory` 持有 `base_system_prompt` / `user_preferences` / `runtime_tool_provider` / `mcp_relay_provider`，负责构造 hub。
3. `hub/tool_builder.rs`：`build_tools_for_execution_context` 独立，接受 `ExecutionSessionFrame`（PR 2 后）。
4. `hub/hook_dispatch.rs`：`emit_session_hook_trigger` / `ensure_hook_session_runtime` / `schedule_hook_auto_resume` 归一。
5. `hub/cancel.rs`：`cancel` + interrupted 事件补发。
6. `hub/companion_wait.rs`：current `session/companion_wait.rs` 已独立，只是从 hub 字段归到 companion 模块。
7. **hub.replace_runtime_mcp_servers 彻底重写**：PR 2 完成后直接用 `ExecutionSessionFrame` 调 `tool_builder`。
8. **event_bridge 的 `_tx` 占位参数删除**（`event_bridge.rs:29`）；它实际通过 `persist_notification` 里的 `tx.send` 广播，不需要外部 tx。

**收益**：hub 从 2800 行降到 ≤ 500 行；职责单一。

**风险**：低（纯移位），前提是上面 PR 1-5 已经把字段依赖理清。

**影响文件**：`session/hub.rs` 拆成 `session/hub/` 子模块。

---

### PR 7 · turn_processor 净化 + SessionRuntime per-turn 字段挪窝

**问题**：A.6 + A.1（SessionRuntime/ActiveSessionExecutionState 重叠）。

**动作**：

1. 把 `SessionRuntime.processor_tx` / `cancel_requested` / `current_turn_id` / `hook_auto_resume_count` 移到 `ActiveSessionExecutionState`（改名 `TurnExecution`）。`SessionRuntime.current_turn: Option<TurnExecution>` 一字段承载所有 per-turn 状态。
2. `turn_processor.handle_notification` 的 `executor_session_id` 同步抽出去到 `persistence` 侧 listener；processor 不再读写 `SessionMeta`。
3. `hook_auto_resume_count` 的递增 / 限流判定挪到 hub 侧 `schedule_hook_auto_resume` 内（processor 只发"请求 auto-resume"信号）。
4. processor 终止时不再直接改 `SessionRuntime.running = false`；通过 `TurnEvent::Terminal` 交给 hub 处理。
5. `SessionRuntime.hook_session` 和 `ExecutionContext.turn.hook_session` 统一通过 `SessionRuntime.hook_session` 的 Arc 共享，prompt_pipeline 不再双向写入。

**收益**：
- turn 级 / session 级职责不再混杂；processor 只管事件流和终态判定。
- SessionRuntime 回归 session 级承载（subscribe / hook_session / current_turn 指针）。

**风险**：中。processor 和 hub 的 ownership 关系需要重走；cancel 路径要仔细验证。

**测试**：
- cancel 路径 e2e（HTTP cancel / hub cancel / connector cancel 三路都跑通）
- auto-resume 限流单测

**影响文件**：`session/hub_support.rs`, `session/turn_processor.rs`, `session/hub.rs`, `session/prompt_pipeline.rs`, `session/continuation.rs`。

---

### 可选 PR 8 · Bundle α 化（ExecutionContext 只持 Bundle）

**问题**：A.8 遗留。

**动作**：

1. 删除 `ExecutionContext.TurnFrame.assembled_system_prompt`。
2. PiAgent 完全走 Bundle 渲染（在 factory 初始化时注入 `BundleRenderer`）。
3. Relay connector 在 prompt 前本地把 Bundle render 成字符串传下游（或扩 relay 协议支持 Bundle 序列化）。
4. `system_prompt_assembler` 保留为 `BundleRenderer` 的默认实现，executor crate 重用它。

**风险**：高（Relay 协议变更 / 各 connector 都要热改）。PRD 明确本轮不做；属于未来动作。

---

## Part D · 需要你拍板的决策点

### D1 · `PromptSessionRequest` / `PreparedSessionInputs` / `SessionStartupPlan` 三合一到什么程度

- 选项 A：**彻底合并**，`PromptSessionRequest` 消失，只保留 `UserPromptInput + IntentHints` 作为入口 DTO，`SessionStartupPlan` 作为内部结构。
- 选项 B：**保留 `PromptSessionRequest` 作为序列化层**（比如 relay / 插件需要），内部走 `SessionStartupPlan`。
- 选项 C：**不合并**（最小改动），只把 `PreparedSessionInputs` 与 `PromptSessionRequest` 的后端注入字段挪到共享结构。

**推荐 B**：保留 wire 语义，内部收敛；不会因为插件/relay 协议改动而掣肘。

### D2 · ExecutionContext 是否立刻 split 到 SessionFrame / TurnFrame

- 选项 A：**立刻 split（PR 2）**，后续 PR 都基于新结构。
- 选项 B：**先加 `context_bundle` 字段，下一轮再 split**，降低单 PR 体积。

**推荐 A**：split 是一次性的设计变更，拖到下轮反而增加"两种 ExecutionContext 并存"的过渡期成本。

### D3 · Hook Bundle 回灌 vs 保留 audit-only

- 选项 A：**运行时 Hook 也 merge 回 Bundle**（A.9 里的 ①），`HOOK_USER_MESSAGE_SKIP_SLOTS` 废除。
- 选项 B：**保持 audit-only**，运行时 Hook 只改本轮 user message（② + ③），不参与 Bundle 合并。
- 选项 C：**折中**：新增一个 `SessionContextBundle::turn_delta: Vec<ContextFragment>` 字段承载运行期追加，与 bootstrap fragment 物理分开。

**推荐 C**：运行时 fragment 与 bootstrap fragment 语义不同（一个是 per-turn 增量，一个是 session 级背景），应在结构上分开；既能让 Inspector 看到运行期动态，也不会污染 bootstrap bundle。

### D4 · PiAgent 首轮后 system prompt 是否热更

- 选项 A：**不热更**（保留 A.12 现状，最多触发 warning）。
- 选项 B：**bundle_id 变化时热更**，通过 `update_session_tools` 风格 API。
- 选项 C：**按 fragment scope 变化触发 steering message**（不改 system prompt，只在下一轮 prompt 插入一条"context updated" user message）。

**推荐 B**，但必须配合 PR 7（turn 结构清理）后做，否则 cache invalidation 点太多。

### D5 · `SessionHub` 拆分前是否先砍掉 `companion_wait` 这类寄生字段

- 选项 A：**PR 6 里一次性拆**。
- 选项 B：**companion_wait / tool_builder 先独立，hub.rs 后面再拆门面**。

**推荐 B**：让 hub.rs 逐步瘦身，每次 PR 只剪一两个寄生字段，降低 review 难度。

### D6 · 本次 review 是否产出 `.trellis/spec/` 条目

上述设计决策（五条轴、ExecutionContext 分层、Bundle 主数据面）都具备写成 spec 的价值。建议：
- `.trellis/spec/backend/session-startup-pipeline.md`：概念分层、compose 节拍、finalize 规则。
- `.trellis/spec/backend/execution-context-frames.md`：ExecutionContext 的 SessionFrame/TurnFrame 契约、字段所有权。
- `.trellis/spec/backend/bundle-main-datasource.md`：Bundle 作为主数据面的定义、Hook 三类语义、`HOOK_USER_MESSAGE_SKIP_SLOTS` 废弃路径。

---

## Part E · 风险与顺序建议

### E.1 推荐 PR 排序

```
PR 1 (入口节拍)  ─┐
                 ├→ PR 2 (ExecutionContext split)
PR 7 (runtime   ─┘        │
     cleanup，先做          ├→ PR 3 (Bundle 进 TurnFrame)
     processor 部分)         │         │
                            │         ├→ PR 4 (Hook 回灌 Bundle)
PR 5 (contribute 去重) ─────┘         │
                                      └→ PR 6 (Hub 拆分)
                                              │
                                              └→ [可选 PR 8] Bundle α 化
```

### E.2 每个 PR 的"完成信号"

| PR | 完成信号 |
|---|---|
| 1 | 5 条入口都调 `SessionStartupBuilder`；`routine/executor.rs` identity 不漏；单测覆盖对称性 |
| 2 | `ExecutionContext { session, turn }`；三 connector 编译且单测通过 |
| 3 | `ExecutionTurnFrame.context_bundle` 存在；PiAgent 能读到；`assembled_system_prompt` 标 deprecated |
| 4 | `HOOK_USER_MESSAGE_SKIP_SLOTS` 删除；companion_agents 只在 bundle 一处渲染；Inspector 能看到运行期 hook fragment |
| 5 | workflow_context / workspace / SessionPlan 三份渲染都调同一 helper；task 路径复用 contribute_story_context；source_resolver/workspace_sources 合并 |
| 6 | hub.rs 行数降至 ≤ 500；companion_wait / tool_builder / hook_dispatch / cancel 归位 |
| 7 | SessionRuntime 不再持 per-turn 字段；turn_processor 不写 SessionMeta；auto-resume 限流在 hub 侧 |

### E.3 关键回归风险

- **auto-resume 路径**：涉及 hub ↔ processor 反向调用；PR 7 最危险。
- **cancel 路径**：PR 7 会改动 processor 退出逻辑；必须 e2e 验证 HTTP cancel / inline cancel / connector cancel。
- **Bundle snapshot 锁定**：PR 5 前必须 snapshot 所有 compose 输出的 bundle 结构（fragment 数量 / slot / order / content），否则容易引入沉默语义漂移。
- **Relay connector 在 PR 3 之后**：它仍靠 assembled_system_prompt，需要把 Bundle render 内聚到"送 relay 前最后一步"。
- **hook_delegate 签名变更**：PR 4 会改 `TransformContextOutput`；trait 默认 impl 降低外部破坏性。

### E.4 本次 task PRD 与重构方案的关系

当前 task PRD (`prd.md`) 聚焦"云端主路径收口"，对应本方案的 **PR 3 + PR 4 局部 + PR 5 部分**（workflow_context 共享 helper + lifecycle bundle + slot 白名单共享）。

但 PRD 里已经部分与现状脱钩（见 `02-context-layer.md` §8）：
- Task owner bootstrap 的 prompt resource prepend 已经拆过；
- compose_lifecycle_node 已经产 Bundle；
- RUNTIME_AGENT_CONTEXT_SLOTS 已经单点维护。

**建议**：拿本重构方案当蓝图，对 PRD 做一次"锚点校准" — 把 PRD 的 PR1-PR5 重新映射到本方案的 PR 编号，并把"已经完成"的子项划掉。

---

## Part F · 附：现状再快速核对清单

出于"memory 可能过期"的保险，本节列出当前代码（2026-04-30 读到的）中与本方案直接相关的 file:line，方便执行时一次性定位。

- 入口相关：
  - `crates/agentdash-api/src/routes/acp_sessions.rs:919` — HTTP prompt 入口
  - `crates/agentdash-application/src/task/service.rs:283` — task 调 start_prompt
  - `crates/agentdash-application/src/workflow/orchestrator.rs:706` — workflow 调 start_prompt
  - `crates/agentdash-application/src/companion/tools.rs:422` / `:1576` — companion 调
  - `crates/agentdash-application/src/routine/executor.rs:209` — routine 调（且 identity 漏填在 :500-515）
  - `crates/agentdash-application/src/session/hub.rs:948-979` — auto-resume
- Compose 核心：
  - `crates/agentdash-application/src/session/assembler.rs:105-140` — `finalize_request`
  - `crates/agentdash-application/src/session/assembler.rs:710` — compose_owner_bootstrap
  - `crates/agentdash-application/src/session/assembler.rs:913` — compose_story_step
  - `crates/agentdash-application/src/session/assembler.rs:1230` — compose_lifecycle_node_with_audit
  - `crates/agentdash-application/src/session/assembler.rs:1445` / `:1565` — compose_companion / compose_companion_with_workflow
- Bundle / Contribute：
  - `crates/agentdash-spi/src/session_context_bundle.rs` — Bundle 定义
  - `crates/agentdash-application/src/context/builder.rs:103` — `build_session_context_bundle`
  - `crates/agentdash-application/src/context/builtins.rs` — contribute_core / binding_initial_context / declared / mcp / instruction
  - `crates/agentdash-application/src/context/workspace_sources.rs` / `source_resolver.rs` — declared source 两套路径
  - `crates/agentdash-application/src/context/workflow_bindings.rs` — contribute_workflow_binding
  - `crates/agentdash-application/src/story/context_builder.rs` / `project/context_builder.rs` — contribute_story / project
  - `crates/agentdash-application/src/session/plan.rs` — SessionPlan fragments
- Hook / Runtime：
  - `crates/agentdash-application/src/hooks/fragment_bridge.rs` — hook_injection_to_fragment
  - `crates/agentdash-application/src/session/hook_delegate.rs:315-409` — transform_context
  - `crates/agentdash-application/src/session/hook_delegate.rs:806` — HOOK_USER_MESSAGE_SKIP_SLOTS
  - `crates/agentdash-application/src/session/hook_runtime.rs` — HookSessionRuntime
  - `crates/agentdash-application/src/session/baseline_capabilities.rs` — session_capabilities.companion_agents
- Runtime 层：
  - `crates/agentdash-application/src/session/hub.rs:34` — sessions
  - `crates/agentdash-application/src/session/hub_support.rs:167-196` — SessionRuntime / ActiveSessionExecutionState
  - `crates/agentdash-application/src/session/prompt_pipeline.rs:23-490` — start_prompt_with_follow_up 上帝函数
  - `crates/agentdash-application/src/session/turn_processor.rs:67-275` — per-turn 驱动
  - `crates/agentdash-application/src/session/system_prompt_assembler.rs:39-205` — assemble_system_prompt
- Connector：
  - `crates/agentdash-spi/src/connector.rs:46-75` — ExecutionContext
  - `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:308-494` — PiAgent prompt 路径
  - `crates/agentdash-executor/src/connectors/composite.rs:143-268` — connector 路由

---

（全文完 - 约 900 行）
