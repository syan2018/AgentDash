# Session Startup Pipeline

> **主题**：一个 prompt 从各种入口进入 `SessionHub` 之前，在 session 装配阶段
> 是如何被组装成 `PreparedLaunchPrompt` 的。
>
> 本 spec 只描述装配阶段（entry → builder → finalize）的目标契约；
> 装配完成后进入 `prompt_pipeline` 构造 `ExecutionContext` 的部分由
> [`execution-context-frames.md`](./execution-context-frames.md) 承担；
> 业务上下文产物 `SessionContextBundle` 的形态由
> [`bundle-main-datasource.md`](./bundle-main-datasource.md) 承担。

## 摘要

- Session 装配统一走 `SessionAssemblyBuilder` —— 5 条正交轴（Who / Where /
  What / How / Trigger）每条都由 builder 的 first-class 方法承载，调用方不再
  靠调用顺序或额外赋值语句保证不漏字段。
- 6 条入口（HTTP / Task / Workflow / Companion / Routine / Auto-resume）全部
  先进入 `LaunchCommand` source adapter；需要 owner/context/capability 补齐的
  路径投影为 `PromptAugmentInput`，预组装路径投影为 `PreparedLaunchPrompt`。
  `identity` / `post_turn_handler` / `env` 等跨轴字段由 builder 或 adapter 注入，
  不再由外围入口手工写 prompt 字段。
- `finalize_request` 的合并规则显式：`capability_state` 整体替换，`mcp_servers`
  作为 state 的 wire 投影整体替换；`vfs` 优先取 prepared，`apply_workspace_defaults` 先于覆盖；
  `identity` / `post_turn_handler` 仅在 prepared 非空时覆盖 base。
- 装配阶段是**单一写入节拍**：外围业务入口不得直接构造或改写
  `PreparedLaunchPrompt`；字段写入集中在 `LaunchCommand` adapter、
  `PromptAugmentInput::into_prepared_prompt` 与 `finalize_request`。

## 1. 五条正交轴

Session 启动输入按以下五条轴分组；每一轴有唯一权威承载字段，装配器不允许同一
轴的数据在两个字段上同时存在。

| 轴 | 职责 | 权威承载 | 备注 |
|---|---|---|---|
| **Who** | 发起人身份 / owner 归属 | `PreparedLaunchPrompt.identity` | `AuthIdentity::system_routine(id)` 承载定时任务等系统身份 |
| **Where** | 执行环境 | `PreparedLaunchPrompt.user_input.working_dir` / `vfs` / `user_input.env` | workspace_defaults 用于兜底 VFS / working_dir |
| **What** | 业务上下文 | `PreparedLaunchPrompt.context_bundle`（`SessionContextBundle`） | 详见 `bundle-main-datasource.md` |
| **How** | 能力 & 工具 | `capability_state` / `mcp_servers` | `capability_state` 是唯一运行态能力状态；`mcp_servers` 是给 session frame / relay wire 使用的投影 |
| **Trigger** | 本轮触发输入 | `user_input.prompt_blocks` + `hook_snapshot_reload: HookSnapshotReloadTrigger` + `post_turn_handler` | `HookSnapshotReloadTrigger` 仅表达"是否需要本轮重载 hook snapshot" |

**禁令**：同一数据不得跨轴重复。典型反例：静态上下文（如 companion_agents）
只进 What 轴的 Bundle，不得同时塞进 Trigger 轴的 `prompt_blocks` 或
`user_blocks`；该约束由 `bundle-main-datasource.md` 的 hook 三语义规则加固。

## 2. 核心类型契约

### 2.1 `UserPromptInput`（wire DTO）

定义位置：`crates/agentdash-application/src/session/types.rs`。

```rust
pub struct UserPromptInput {
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
    pub executor_config: Option<AgentConfig>,
}
```

- 仅用于前端 HTTP 反序列化；不承载任何后端注入字段。
- 通过 `SessionAssemblyBuilder::with_user_input(input)` 一次性消化，进入
  builder 的 `prompt_blocks` / `executor_config` / `working_dir` / `env` 字段。
- 非 HTTP 入口（task / routine / auto-resume 等）也只构造 `UserPromptInput`
  或 `PreparedSessionInputs`，再交给对应 `LaunchCommand` adapter。

### 2.2 `PreparedLaunchPrompt`（prompt pipeline 输入投影）

定义位置：`crates/agentdash-application/src/session/types.rs`。

```rust
pub struct PreparedLaunchPrompt {
    pub user_input: UserPromptInput,
    pub mcp_servers: Vec<McpServer>,
    pub vfs: Option<Vfs>,
    pub capability_state: Option<CapabilityState>,
    pub context_bundle: Option<SessionContextBundle>,
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    pub identity: Option<AuthIdentity>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}
```

- `PreparedLaunchPrompt` 不是外部 request / DTO；它是 prompt pipeline 进入
  `LaunchExecution` 前的内部输入投影。
- 生产入口不得直接调用 `PreparedLaunchPrompt::from_user_input` 或裸构造字段；
  它们必须使用 `LaunchCommand::http_prompt_input`、
  `LaunchCommand::*_prepared`、`LaunchCommand::hook_auto_resume_input` 等
  source adapter。
- 允许的内部写入点只有 `LaunchCommand` adapter、
  `PromptAugmentInput::into_prepared_prompt`、`finalize_request(base, prepared)`。
- 旧 `PromptSessionRequest` 与 `SessionLaunchIntent` 已从生产主链路删除；新增入口
  必须先进入 `LaunchCommand`，不得恢复半收敛 request / intent 壳。

### 2.3 `PromptAugmentInput`（augment 输入协议）

定义位置：`crates/agentdash-application/src/session/augmenter.rs`。

```rust
pub struct PromptAugmentInput {
    pub user_input: UserPromptInput,
    pub request_mcp_servers: Vec<SessionMcpServer>,
    pub existing_vfs: Option<Vfs>,
    pub identity: Option<AuthIdentity>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
}
```

- 仅用于 `LaunchCommand` → `PromptRequestAugmenter` 的跨层输入协议。
- 不承载 context bundle、capability_state、hook reload 等 composition 产物。
- `PromptRequestAugmenter::augment(session_id, input)` 输出
  `PreparedLaunchPrompt`，由 API 层 owner/context/capability compose 路径补齐。

### 2.4 `HookSnapshotReloadTrigger`（Trigger 轴子结构）

定义位置：`crates/agentdash-application/src/session/types.rs`。

```rust
pub enum HookSnapshotReloadTrigger {
    None,        // 普通续跑：不重载 hook snapshot、不触发 SessionStart
    Reload,      // Owner 首轮 / 冷启动续跑：重载 snapshot + 触发 SessionStart hook
}
```

- 由 **E7** 从旧 `SessionBootstrapAction::OwnerContext` 重命名收敛，语义明确为
  "本轮 prompt 是否需要重载 hook snapshot + 触发 `SessionStart` hook"。
- 与 `SessionMeta.bootstrap_state` 正交：后者是**持久化**的 session bootstrap
  阶段标记（Plain / Pending / Bootstrapped），两者不应混用。
- 取值由 compose 路径决定：
  - `compose_owner_bootstrap`（Story/Project/Routine 的 bootstrap）→ `Reload`
  - `compose_story_step` / `compose_lifecycle_node` / `compose_companion` → `None`
  - HTTP `RepositoryRehydrate(SystemContext)` 路径视重建需要可能写 `Reload`

### 2.5 `SessionAssemblyBuilder`

定义位置：`crates/agentdash-application/src/session/assembler.rs`。

Builder 按正交关注点分组，每一类字段由专属 `with_*` / `append_*` / `apply_*`
方法承载。所有 compose 函数内部必须通过 builder 构造，不允许直接裸构造
`PreparedSessionInputs` 字面量。

| 关注点 | First-class 方法 | 说明 |
|---|---|---|
| VFS（Where） | `with_vfs` / `with_companion_vfs` / `append_lifecycle_mount` / `append_canvas_mounts` | 允许追加 lifecycle mount / canvas mount |
| 能力（How） | `with_resolved_capabilities` / `with_companion_capabilities` | 传入 CapabilityResolver 结果或 companion 裁剪 |
| MCP（How） | `with_mcp_servers` / `append_mcp_servers` / `append_relay_mcp_names` | `with_*` 整体替换，`append_*` 追加 |
| 上下文（What） | `with_context_bundle` / `with_optional_context_bundle` | Bundle 是主数据面 |
| Prompt（Trigger） | `with_prompt_blocks` / `with_executor_config` / `with_hook_snapshot_reload` | Trigger 轴 |
| Workspace 默认值 | `with_workspace_defaults` / `with_optional_workspace_defaults` | 用于 `apply_workspace_defaults` 回填 |
| 用户输入聚合（Where + Trigger） | `with_user_input(UserPromptInput)` | 一次吞下 prompt_blocks / executor_config / working_dir / env |
| 独立环境变量 | `with_env(HashMap)` | 某些入口显式传入 env |
| 身份（Who） | `with_identity` / `with_optional_identity` | **E2 要求**：由 builder 而非调用方保证不漏 |
| 回调（Trigger 副作用） | `with_post_turn_handler` / `with_optional_post_turn_handler` | task / routine 注入 |
| 组合便利 | `apply_companion_slice` / `apply_lifecycle_activation` | 将多个关注点一次设置 |

产物是 `PreparedSessionInputs`（平坦结构），由 `finalize_request` 消化。

## 3. 6 条入口的统一节拍

所有后端入口必须满足：

```text
SessionAssemblyBuilder::new()
    .with_user_input(user_input)
    .with_identity(identity)
    .with_post_turn_handler(handler)          // 可选
    // ── compose 函数内部调用 .with_* / .apply_* ──
    .build()                                   // → PreparedSessionInputs
    |> LaunchCommand::*_prepared(...)          // 或 strict augment path
    |> SessionHub.launch_command(...)
```

| # | 入口 | compose 函数 | builder 承载要点 |
|---|---|---|---|
| 1 | HTTP `POST /sessions/:id/prompt` | `LaunchCommand::http_prompt_input` → `PromptAugmentInput` → `augment_prompt_request_for_owner` → `compose_owner_bootstrap` / `compose_story_step` | `identity` 来自 HTTP session；对 `RepositoryRehydrate(SystemContext)` 路径通过 `apply_plain_lifecycle_request` 写 continuation bundle |
| 2 | Task service `start_task` / `continue_task` | `compose_story_step` | builder `.with_identity(task_identity)` + `.with_post_turn_handler(task_callback)` |
| 3 | Workflow orchestrator `start_agent_node_prompt` | `compose_lifecycle_node_with_audit` | 通过 `SessionAssemblyBuilder::apply_lifecycle_activation` 吸收 lifecycle activation 结果 |
| 4 | Companion tools `dispatch` | `compose_companion` / `compose_companion_with_workflow` | 通过 `apply_companion_slice` 一次性装配父 session 切片 |
| 5 | Routine executor `execute_with_session` | `compose_owner_bootstrap` | 依 **E1**：`AuthIdentity::system_routine(routine.id)` 注入；不再漏 identity（见 `crates/agentdash-application/src/routine/executor.rs:493-523`） |
| 6 | Hub auto-resume `schedule_hook_auto_resume` | `LaunchCommand::hook_auto_resume_input` → `PromptAugmentInput` → `SharedPromptRequestAugmenter::augment` | 通过 augmenter 重建完整 `PreparedLaunchPrompt` |

**入口约束**：HTTP / Local relay / Task / Workflow / Routine / Companion /
Hook auto-resume 入口只允许构造 `LaunchCommand`，不允许直接构造
`PreparedLaunchPrompt`。`grep -r "PreparedLaunchPrompt::from_user_input" crates/`
应只命中 `LaunchCommand` / `PromptAugmentInput` / 测试。

## 4. `finalize_request` 合并语义

定义位置：`crates/agentdash-application/src/session/assembler.rs::finalize_request`。

以下规则必须保持对称、显式、可预期：

| 字段 | 合并规则 |
|---|---|
| `user_input.prompt_blocks` | `prepared` 非空覆盖；否则保留 base |
| `user_input.executor_config` | `prepared` 非空覆盖；否则保留 base |
| `user_input.env` | `prepared.env` 非空整体替换；否则保留 base |
| `user_input.working_dir` | 先执行 `apply_workspace_defaults(&mut working_dir, &mut vfs, workspace_defaults)`；随后 `prepared.working_dir` 非空覆盖 |
| `vfs` | 先执行 `apply_workspace_defaults`；随后 `prepared.vfs` 非空覆盖 |
| `mcp_servers` | **整体替换**为 `prepared.mcp_servers`（由 `CapabilityState.mcp_servers` 与显式 request/preset 投影汇总） |
| `capability_state` | 整体替换 |
| `context_bundle` | 整体替换（Bundle 是主数据面，compose 外部不应再补丁） |
| `hook_snapshot_reload` | 整体替换 |
| `identity` | `prepared.identity` 非空覆盖；否则保留 base |
| `post_turn_handler` | `prepared.post_turn_handler` 非空覆盖；否则保留 base |

**能力状态收敛背景（2026-05-08）**：`CapabilityState` 是 How 轴唯一运行态状态。
`mcp_servers` 继续存在只是给 `ExecutionSessionFrame` / relay wire 的投影；compose
内部必须先汇总到 state，再投影到 request，finalize 阶段不再做增量合并。

**`identity` / `post_turn_handler` 下沉（PR 1 Phase 1c）**：过去 routine /
task 等路径都是 `finalize_request(base, prepared); req.identity = Some(id);`
两步走，routine 在 `04-30` 前就漏填过（prd.md · A.2）。现在 builder 持有这两
个字段，`finalize_request` 统一合入，节拍单一。

**`apply_workspace_defaults` 的顺序**：必须在 `prepared.working_dir` 与
`prepared.vfs` 覆盖**之前**执行，否则 workspace 回填会被紧随其后的 prepared
覆盖吞掉。当前实现位于 `finalize_request` 第一个分支之后、vfs 覆盖分支之前，
这是 PRD Requirements §"入口节拍" 的显式约束。

## 5. 装配时序图

```mermaid
sequenceDiagram
    participant Entry as 入口 (HTTP / Task / WF / Companion / Routine / AutoResume)
    participant Builder as SessionAssemblyBuilder
    participant Compose as compose_owner_bootstrap / compose_story_step / ...
    participant Contribs as context::contribute_*
    participant Bundle as build_session_context_bundle
    participant Final as finalize_request
    participant Hub as SessionHub.start_prompt

    Entry->>Builder: new().with_user_input(...).with_identity(...).with_post_turn_handler(...)
    Entry->>Compose: compose_*(spec, builder)
    Compose->>Contribs: contribute_core_context / binding / workflow / declared_sources / ...
    Contribs-->>Compose: Vec<Contribution>
    Compose->>Bundle: build_session_context_bundle(contributions)
    Bundle-->>Compose: SessionContextBundle { bootstrap_fragments }
    Compose->>Builder: with_context_bundle(bundle) / with_vfs(...) / with_mcp_servers(...)
    Builder-->>Compose: PreparedSessionInputs
    Compose-->>Entry: PreparedSessionInputs
    Entry->>Final: finalize_request(base, prepared)
    Final-->>Entry: PreparedLaunchPrompt
    Entry->>Hub: start_prompt(session_id, req)
```

组装只做一次：compose 函数产出 `SessionContextBundle.bootstrap_fragments` 后
不再被改写；运行期 hook 的增量改动走 `bundle.turn_delta`（详见
`bundle-main-datasource.md`）。

## 6. 入口实施要点

### 6.1 HTTP 主通道

- 入口：`crates/agentdash-api/src/routes/acp_sessions.rs::prompt_session` →
  `augment_prompt_request_for_owner`。
- HTTP handler 先从 session 解析 `identity`，然后根据 owner kind（Task /
  Story / Project）分派到 `build_task_owner_prompt_request` /
  `build_story_owner_prompt_request` / `build_project_owner_prompt_request`；
  内部调 `SessionRequestAssembler::compose_*`，统一走 builder + finalize。

### 6.2 Task / Workflow / Companion / Routine

- 各 service 直接持 `SessionRequestAssembler`，调各自的 `compose_*`。
- **必须**使用 `SessionAssemblyBuilder::with_identity` 注入身份；routine 通过
  `AuthIdentity::system_routine(routine.id)` 生成系统身份（E1 决策）；如缺
  identity，视为实施违规。
- **必须**使用 builder 的 `with_post_turn_handler`（若需要 per-turn 回调）。

### 6.3 Auto-resume

- 入口：`hub.schedule_hook_auto_resume`（由 `turn_processor` 侦测到 hook
  `BeforeStop == continue` 后触发）。
- 内部路径：构造 `LaunchCommand::hook_auto_resume_input(
  UserPromptInput::from_text(AUTO_RESUME_PROMPT))` → 调
  `SharedPromptRequestAugmenter::augment`（AppState 注入的实现是
  `AppStatePromptAugmenter`，内部调 `augment_prompt_request_for_owner`） →
  与 HTTP 主通道共享同一条 compose + finalize 节拍。
- Auto-resume 限流（`hook_auto_resume_count < 2`）目标态由 `hub/hook_dispatch`
  承担（见 `execution-context-frames.md` §4）；入口契约保持不变。
- Auto-resume 的终态触发不由 `SessionTurnProcessor` 直接调用；终态 event 先落库，
  `hook_auto_resume` effect 写入 terminal effect outbox 后由 dispatcher 执行。

### 6.4 基本不变式（必须可验证）

- `rg "PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src`
  零命中；
- `grep -r "PreparedLaunchPrompt { " crates/` 在业务代码中零命中；
- `routine/executor.rs` 产出的 `PreparedLaunchPrompt.identity` 在任何执行路径
  上都是 `Some(AuthIdentity::system_routine(...))`；
- `finalize_request` 的 4 项关键规则（capability_state 整体替换、mcp_servers 投影替换、
  vfs prefer_base、workspace_defaults 顺序、identity /
  post_turn_handler 在 base 非空时保留）有对应单测（PR 1 已覆盖）。
- `working_dir` 只允许 mount root 内的相对路径。空值或空白值解析为 mount root；
  绝对路径、根路径、Windows prefix、`..` parent segment 必须拒绝，并由
  `session::path_policy` 单测覆盖。
- 云端 `AppState::new_with_plugins` 返回前必须通过 `SessionHub::assert_ready_for_app_state`
  校验 prompt augmenter、context audit bus、terminal callback、runtime tool provider、
  MCP relay provider 已绑定；不得把缺必要依赖的 hub 暴露为 ready state。

## 7. 相关 spec / PRD / code 锚点

### 相关 spec

- [`execution-context-frames.md`](./execution-context-frames.md) — finalize 之后
  构造 `ExecutionContext.SessionFrame + TurnFrame` 的形态与生命周期。
- [`bundle-main-datasource.md`](./bundle-main-datasource.md) — 装配期产出的
  `SessionContextBundle` 结构及其 Hook 三类语义接入点。
- [`./runtime-execution-state.md`](./runtime-execution-state.md) — session 进
  入运行态后 `SessionRuntime` 与 `TurnExecution` 的职责边界。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` — Hook runtime 在
  装配期 / 运行期的整体契约，与本 spec 在 Trigger 轴配合。

### PRD / 任务文档

- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/prd.md` — §
  Requirements / Acceptance Criteria / Decisions（D1 / D5 / E1 / E2 / E7）。
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/target-architecture.md`
  — §1.0 顶层架构图 / §3 五条正交轴 / §4.1 装配时序 / §6 不变式 I3 / I10。
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/01-runtime-layer.md`
  — §1 入口拓扑 / §3 finalize_request 覆盖点事实。

### 代码锚点

- `crates/agentdash-application/src/session/assembler.rs`
  - `finalize_request`（~143 行）
  - `SessionAssemblyBuilder`（~205 行起）
  - `compose_owner_bootstrap` / `compose_story_step` / `compose_lifecycle_node_with_audit`
    / `compose_companion` / `compose_companion_with_workflow`
- `crates/agentdash-application/src/session/types.rs` — `PreparedLaunchPrompt`
  / `UserPromptInput` / `HookSnapshotReloadTrigger`
- `crates/agentdash-application/src/session/context/mod.rs` —
  `apply_workspace_defaults`
- `crates/agentdash-application/src/session/augmenter.rs` +
  `crates/agentdash-api/src/bootstrap/prompt_augmenter.rs` — auto-resume 接入
- `crates/agentdash-spi/src/auth.rs::AuthIdentity::system_routine` — E1 实施
- `crates/agentdash-application/src/routine/executor.rs:493-523` — routine 入
  口装配示例
