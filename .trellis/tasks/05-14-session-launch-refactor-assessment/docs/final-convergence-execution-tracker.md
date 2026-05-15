# Final Convergence Execution Tracker

## Conclusion

本重构还没有完成。当前分支完成了一批迁移基础，但生产主链路仍未达到目标：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

后续执行必须继续向唯一数据流收敛。`PromptAugmentInput` 与 `SessionLaunchRequest` 都已从代码主线删除，但这不等于终态：当前仍有 `SessionConstructionSeed` 作为 API/bootstrap → launch planner 的显式边界 seed。它已不再从 `session::mod` 顶层 re-export，`prompt_pipeline` 也不再拆 seed 字段，但下一步不能继续扩张 seed，必须把 construction facts 直接沉入 `SessionConstructionPlan`，把 hook/effect/launch 策略沉入 `LaunchExecution`。

## Current Code Facts

| Area | Done | Blocking Gaps |
|---|---|---|
| Entry | 生产入口大多进入 `LaunchCommand`；`start_prompt` 已收紧为测试入口；`start_prompt_with_follow_up` 已删除；`LaunchCommand` 不再持有 `PromptAugmentInput`；`LaunchCommand::to_augment_input()` 已删除；local relay 不再把已组装 `Vfs` 塞进 command 或 seed，只传 workspace root source fact；local relay MCP 已收窄为 declaration source payload；`UserPromptInput.working_dir` 已移除；task `post_turn_handler` 不再穿过 command；companion command 不再携带 parent VFS/MCP/context snapshot；未使用的 command continuation context frame 通道已删除 | `SessionConstructionSeed.working_dir_input` 仍是过渡 seed，尚未由 construction provider 直接解析 |
| Old Shells | `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput`、`PromptAugmentInput`、`SessionLaunchRequest` 已删除；旧 payload 不再从 `session::mod` re-export，也不再出现在 `crates/agentdash-application/src` / `crates/agentdash-api/src` / `crates/agentdash-local/src`；`SessionConstructionSeed` 也已撤出 `session::mod` 顶层导出，只能显式从 construction 模块引用 | `LaunchAugmentation` tuple alias 已删除；当前仅剩 `SessionConstructionSeed` 承接 API/bootstrap 到 construction planner 的过渡事实 |
| Construction | 已有 `SessionConstructionPlan` / `SessionConstructionPlanner`；`ContextPlan` 已保留完整 `SessionContextBundle` 与 continuation context frame；`UserPromptInput` 不再承载 working dir；local relay workspace root 由 seed 保存为 raw source fact，`SessionLaunchPlanner` 解析为 VFS 并在 construction trace 标记来源；task effect binding 已从 API bootstrap 的内存 handler 改为 `TerminalHookEffectBinding` durable 描述，并进入 `SessionConstructionSeed` / `SessionConstructionPlan.effects`；companion parent facts 由 bootstrap 从 parent capability state 临时投影 | working dir/MCP/capability/identity 仍先进入 `SessionConstructionSeed` 再被 planner 拆入 construction；executor profile、companion slice、audit/inspector projection 仍未完整归入 construction |
| Launch | 已有 `SessionLaunchPlanner` / `SessionLaunchExecutor`；`SessionLaunchPlannerInput` 已删除 `request: PromptAugmentInput`，并改为整体接收 `SessionConstructionSeed`；`prompt_pipeline` 不再拆 owner/VFS/MCP/capability/context/effect 字段；`LaunchExecution` 不再允许缺失 construction plan，缺 owner 时会在 planner 阶段失败 | planner 输入还不是 `LaunchCommand + SessionConstructionPlan + runtime facts`；seed 仍是 planner 输入，construction 字段仍未直接由 construction provider 产出 |
| API/bootstrap | route 层部分 launch composition 已迁到 bootstrap；bootstrap 不再返回 `PromptAugmentInput` / `SessionLaunchRequest` / `LaunchExecutionSeed`，也不再构造 task `DynPostTurnHandler` | bootstrap 仍返回 `UserPromptInput + SessionConstructionSeed`，尚未直接输出 construction/launch 显式计划；companion parent facts 仍在 bootstrap 临时投影 |
| Runtime/Hub | registry / supervisor 已拆出，live executor session 与 active turn 命名已有区分 | 多个业务方法仍在 `impl SessionHub`，Hub 仍是能力聚合入口 |
| Effects/Pending | terminal effect outbox、runtime command store 已有基础；task post-turn handler 不再作为 command trait object 传递；task effect binding 已是 durable handler 描述，planner 通过 registry 解析即时 handler，outbox replay 复用同一 payload | effect handler 幂等语义、pending apply-once、失败恢复和 migration 仍需最终验证 |
| Persistence/AppState | store adapter、ready gate、working_dir 策略已有阶段性收口 | `SessionPersistence` 底层仍是大组合接口；AppState/Hub 拆分未达到最终架构 |

## Non-Negotiable Boundaries

- `LaunchCommand` 只表达来源意图和引用：source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload。
- `LaunchCommand` 不携带 resolved VFS / MCP / capability / context / hook trigger / effect handler / working_dir / connector input。
- `UserPromptInput` 不包含 `working_dir`；working dir 最终由 construction 从 project / story / task / agent / lifecycle / local relay workspace root 解析。当前 `SessionConstructionSeed.working_dir_input` 是待删除过渡点。
- task `post_turn_handler` 不能作为 command trait object 传递；task effect 只能以 durable binding 描述进入 construction/effects，再由 registry 解析即时 handler 与 replay handler。
- companion dispatch 不传 parent VFS/MCP/context snapshot；最终由 construction 从 parent session facts 解析 companion slice。当前 bootstrap 仍有临时投影。
- local relay workspace root 是来源事实；当前由 planner/construction 解析为 VFS，不再由 adapter/augmenter 预组装。MCP 只有作为原始 declaration 才可留在 source payload，不能被命名或使用为 resolved MCP surface。

## Remaining Execution Order

### 1. Correct Entry Intent Boundary

- Keep `UserPromptInput` free of `working_dir`; move the remaining `SessionConstructionSeed.working_dir_input` transition into construction.
- Continue moving `TerminalHookEffectBinding` creation out of API bootstrap into construction provider.
- Move the current API bootstrap companion parent capability projection into construction provider.
- Keep local relay MCP input as declaration source payload and move final resolution into construction with the rest of `SessionConstructionSeed`.

Exit checks:

```powershell
rg -n "working_dir" crates/agentdash-application/src/session/types.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/assembler.rs crates/agentdash-local/src/handlers/prompt.rs
rg -n "post_turn_handler|parent_vfs|parent_mcp_servers|parent_context_bundle" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

### 2. Complete `SessionConstructionPlan`

- Put working dir plan, VFS, MCP declaration resolution, capability state, executor profile, identity projection, source trace into construction.
- Put task effect binding, companion slice, local relay workspace root resolution into construction providers.
- Put context frame plan, audit projection, inspector projection into construction.
- Make launch/query/audit/inspector project the same construction.

Exit check:

```powershell
rg -n "build_task_session_context|build_story_session_context_response|build_project_session_context_response|finalize_augmented_request" crates/agentdash-api/src/routes crates/agentdash-api/src/bootstrap
```

### 3. Collapse Launch Execution

- `SessionLaunchPlanner` consumes `LaunchCommand + SessionConstructionPlan + runtime facts`.
- `LaunchExecution` owns prompt payload, construction, lifecycle, restore, hook plan, follow-up plan, runtime command plan, terminal effect plan, connector input, trace. 当前代码已要求 `LaunchExecution` 必须持有 `SessionConstructionPlan`，不再允许 `Option` construction。
- `prompt_pipeline` executes the plan only. 当前代码已停止在 planner 输入处拆 construction seed 字段，但仍负责 turn/context frame 执行细节。

Exit check:

```powershell
rg -n "req\\.vfs|req\\.mcp_servers|req\\.capability_state|req\\.context_bundle|req\\.hook_snapshot_reload|req\\.post_turn_handler" crates/agentdash-application/src/session/prompt_pipeline.rs crates/agentdash-application/src/session/launch_planner.rs
```

### 4. Delete `PromptAugmentInput` Production Handoff

- API/bootstrap no longer returns `PromptAugmentInput`.
- Delete the remaining API augmenter / relaxed pipeline `PromptAugmentInput` construction.
- `prompt_pipeline` no longer receives `PromptAugmentInput`.
- Do not stop at `SessionConstructionSeed`; construction fields must be consumed by `SessionConstructionPlanner`, and launch/effect fields must move into `LaunchExecution` / effects boundary.

Exit check:

```powershell
rg -n "PromptAugmentInput" crates/agentdash-api/src/bootstrap crates/agentdash-application/src/session/launch.rs crates/agentdash-application/src/session/launch_planner.rs crates/agentdash-application/src/session/prompt_pipeline.rs
```

Production mainline must have zero hits.

### 5. Remove Business `SessionHub`

- Split construction / launch / runtime / eventing / hooks / effects / pending / adapters into explicit services.
- If `SessionHub` remains in an intermediate commit, it must not be marked final.

Exit check:

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
```

### 6. Finish Effects / Pending / Persistence Verification

- Terminal effects have durable identity or typed handlers.
- Pending runtime command has requested/applied/failed audit, apply-once, failure recovery.
- New business logic depends on meta/event/outbox/runtime-command store boundaries.
- PostgreSQL / SQLite migrations are verified.

## Final Validation Matrix

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

## Completion Definition

- [x] `LaunchCommand` is pure source intent.
- [x] `UserPromptInput` does not carry working dir.
- [x] `PromptAugmentInput` is not a production handoff, planner input, or augmented output.
- [x] `SessionLaunchRequest` is not a production handoff.
- [ ] `SessionConstructionPlan` is the launch/query/audit/inspector fact source.
- [ ] `LaunchExecution` is the only per-launch strategy plan.
- [ ] `prompt_pipeline` executes a plan instead of planning/fallback.
- [ ] `SessionHub` is not a business capability entrypoint.
- [ ] terminal effects are durable replay/retry/dead-letter.
- [ ] pending runtime command apply-once and recovery are auditable.
- [ ] persistence store boundaries are not bypassed by new business logic.
- [ ] final validation matrix passes.
