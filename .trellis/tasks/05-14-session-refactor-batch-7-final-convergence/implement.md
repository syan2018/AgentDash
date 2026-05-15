# Implementation Plan: Batch 7 Final Convergence

## Current Boundary Decision

最终主链路只允许：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext projection
```

本 batch 先校正边界，再删除旧 payload。不能为了删除 `PromptAugmentInput`，把 resolved VFS / MCP / capability / context / hook / effect / working_dir 塞进 `LaunchCommand`。

## Ordered Steps

### 1. 入口意图瘦身

- [x] `LaunchCommand` 不再持有 `PromptAugmentInput`。
- [x] `PromptRequestAugmenter` 不再接收 `PromptAugmentInput` 作为输入。
- [x] `SessionLaunchPlannerInput` 不再包含 `request: PromptAugmentInput`。
- [x] `PromptAugmentInput` 不再 re-export 到生产入口。
- [x] `SessionConstructionSeed` 不再从 `session::mod` 顶层 re-export，外层引用必须显式进入 construction 模块。
- [x] local relay 不再把已组装 `Vfs` 塞进 `LaunchCommand` 或 seed，只保留 workspace root 作为来源事实，由 planner/construction 解析。
- [x] `UserPromptInput.working_dir` 移出 prompt input；当前过渡事实留在 `SessionConstructionSeed.working_dir_input`，后续迁入 construction。
- [x] `LaunchCommand` 只保留 source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload；`to_augment_input()` 已删除，API augmenter / relaxed pipeline 不再构造旧 `PromptAugmentInput`。
- [x] task `post_turn_handler` trait object 迁出 `LaunchCommand`；API bootstrap 不再创建内存 handler，也不再生成 `TerminalHookEffectBinding`；task binding 由 story step assembler 产出 durable 描述。
- [x] companion command 只保留 parent session / dispatch / slice / target binding 等策略 payload，不携带 parent VFS / MCP / context snapshot；API bootstrap 不再投影 parent VFS/MCP，当前由 application assembler 的 parent facts provider 解析。
- [x] local relay MCP 只作为 request declaration 命名与传递，不能作为 resolved MCP surface 使用。

退出检查：

```powershell
rg -n "working_dir" crates/agentdash-application/src/session/types.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/assembler.rs crates/agentdash-local/src/handlers/prompt.rs
rg -n "post_turn_handler|parent_vfs|parent_mcp_servers|parent_context_bundle" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

命中允许存在，但必须全部在 tracker 中归属到 construction / effects / source contract 的后续删除点；不能被标记完成。`UserPromptInput` 不得再出现 `working_dir` 字段。

### 2. 补全 `SessionConstructionPlan`

- [x] `ContextPlan` 持有完整 `SessionContextBundle`。
- [x] `ContextPlan` 持有 continuation context frame；该 frame 不再作为 launch planner 输出旁路存在。
- [ ] `SessionConstructionPlan` 持有 working dir plan / VFS / MCP declaration resolution / capability state / executor profile / identity projection / source trace；删除 `SessionConstructionSeed.working_dir_input` 过渡种子。
- [x] task effect binding 已进入 construction/effects durable binding，不再由 API bootstrap 绑定内存 handler或 durable binding。
- [x] local relay workspace root 由 planner/construction 解析，并记录 VFS 来源。
- [x] companion parent VFS/MCP projection 由 application assembler parent facts provider 解析；API bootstrap 上的 parent capability 临时投影已删除。
- [ ] companion context bundle / audit projection 进入 construction provider。
- [ ] context frame plan、audit projection、inspector projection 进入 construction。
- [ ] launch、context endpoint、audit、inspector 只投影同一 construction。

退出检查：

```powershell
rg -n "build_task_session_context|build_story_session_context_response|build_project_session_context_response|finalize_augmented_request" crates/agentdash-api/src/routes crates/agentdash-api/src/bootstrap
```

route/bootstrap 不得保留独立 owner / VFS / capability / context 主线。

### 3. 收敛 `LaunchExecution`

- [ ] `SessionLaunchPlanner` 消费 `LaunchCommand + SessionConstructionPlan + runtime facts`。
- [x] `LaunchExecution` 不再允许 `construction: Option<_>`；缺 owner / construction plan 会在 planner 阶段失败。
- [x] connector input 由 `LaunchExecution` 投影为 `ExecutionContext`。
- [x] `LaunchExecution` 显式包含 resolved prompt payload、construction、lifecycle、restore、hook plan、follow-up plan、runtime command plan、terminal effect plan、connector input、trace。
- [x] pending runtime commands 与 pending capability transitions 不再作为 `PlannedSessionLaunch` 旁路字段，改由 `LaunchExecution.runtime_commands` 承载。
- [x] follow-up id 从 `LaunchExecution.summary` 投影；post-turn handler 从 `LaunchExecution.terminal_effects` 投影，不再挂在 `PlannedSessionLaunch`。
- [ ] `prompt_pipeline` 只负责 claim / activate、event append、connector.prompt、accepted 后提交 meta / pending / title、processor supervision。
- [x] `prompt_pipeline` 不再拆 `SessionConstructionSeed` 的 owner / VFS / MCP / capability / context / effect 字段，字段拆解已收回 planner/construction 阶段。
- [ ] connector.prompt 失败不提交 bootstrap / pending applied / title generation 等成功副作用。

退出检查：

```powershell
rg -n "req\\.vfs|req\\.mcp_servers|req\\.capability_state|req\\.context_bundle|req\\.hook_snapshot_reload|req\\.post_turn_handler" crates/agentdash-application/src/session/prompt_pipeline.rs crates/agentdash-application/src/session/launch_planner.rs
```

### 4. 删除 `PromptAugmentInput` production handoff

- [x] API bootstrap 不再返回增强后的 `PromptAugmentInput`。
- [x] 删除 `LaunchCommand::to_augment_input()`。
- [x] `prompt_pipeline` 不再接收 `PromptAugmentInput`。
- [x] `PromptAugmentInput` 不再作为 production helper、跨 crate handoff、planner 输入或增强后输出保留。
- [x] 删除分组后的 `SessionLaunchRequest` 过渡 envelope。
- [ ] 删除当前 `SessionConstructionSeed` 过渡 seed：construction 字段必须直接进入 `SessionConstructionPlan`，hook/effect/launch 字段必须进入 `LaunchExecution` / effects 边界。

退出检查：

```powershell
rg -n "PromptAugmentInput" crates/agentdash-api/src/bootstrap crates/agentdash-application/src/session/launch.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/prompt_pipeline.rs
```

生产主链路零命中。

### 5. 拆除有职责 `SessionHub`

- [ ] construction / launch / runtime / eventing / hooks / effects / pending / adapters 能力服务独立。
- [ ] `SessionHub` 不再承载业务判断，也不能作为最终完成遮羞布。
- [ ] 新增调用点不得依赖 Hub 读取或修改跨职责状态。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
```

若仍有命中，必须逐项说明剩余职责；任何业务分支阻塞完成。

### 6. Effects / Pending / Persistence 验证

- [ ] 所有 terminal effect handler 具备 durable identity 或 typed handler。
- [ ] pending runtime command 覆盖 connector failure、apply once、failed / retry / recovery。
- [ ] 新增业务逻辑按 meta / event / outbox / runtime-command store 边界依赖。
- [ ] PostgreSQL / SQLite migration 验证通过。

## Final Validation

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-infrastructure
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
cargo test -p agentdash-application session::hub
cargo test -p agentdash-application session::terminal_effects
cargo test -p agentdash-application session::runtime_commands
cargo test -p agentdash-application session::memory_persistence
cargo test -p agentdash-application session::path_policy
cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions
rg -n "PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|LaunchCommand::.*_prepared" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

## Exit Criteria

- `LaunchCommand` 是纯入口意图，不承载 resolved facts。
- `UserPromptInput` 不承载 `working_dir`。
- `SessionConstructionPlan` 是 owner / workspace / VFS / MCP / capability / context / identity 的唯一事实源。
- `LaunchExecution` 是唯一 per-launch 策略计划。
- `PromptAugmentInput` 不再是生产主链路 handoff、planner 输入或增强后输出。
- `SessionHub` 不再是业务能力入口。
- 最终验证矩阵通过。
