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

后续执行必须先校正边界，再删除旧 payload。不能把 `PromptAugmentInput` 拆散后塞进 `LaunchCommand`，也不能把 route/bootstrap/Hub 中的旧职责换名后当作完成。

## Current Code Facts

| Area | Done | Blocking Gaps |
|---|---|---|
| Entry | 生产入口大多进入 `LaunchCommand`；`start_prompt` 已收紧为测试入口；`start_prompt_with_follow_up` 已删除；`LaunchCommand` 不再持有 `PromptAugmentInput`；local relay 不再把已组装 `Vfs` 塞进 command；`UserPromptInput.working_dir` 已移除 | `LaunchCommand::to_augment_input()` 仍投影旧 payload；`PromptAugmentInput.working_dir_input` 仍是过渡 handoff，尚未迁入 construction；task `post_turn_handler` 与 companion parent snapshot 仍穿透入口 |
| Old Shells | `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput` 已删除；`PromptAugmentInput` 已不再从 `session::mod` re-export | `PromptAugmentInput` 仍是 API/bootstrap/application handoff，并承载 VFS/MCP/capability/context/hook/post-turn |
| Construction | 已有 `SessionConstructionPlan` / `SessionConstructionPlanner`；`ContextPlan` 已保留完整 `SessionContextBundle`；`UserPromptInput` 不再承载 working dir | working dir 解析仍经 `PromptAugmentInput.working_dir_input` 过渡；VFS、MCP、capability、executor profile、identity、companion slice、task effect binding、audit/inspector projection 仍未完整归入 construction |
| Launch | 已有 `SessionLaunchPlanner` / `SessionLaunchExecutor`；`SessionLaunchPlannerInput` 已删除 `request: PromptAugmentInput` | planner 输入还不是 `LaunchCommand + SessionConstructionPlan + runtime facts`；`prompt_pipeline` 仍接收增强 payload 并拆字段 |
| API/bootstrap | route 层部分 launch composition 已迁到 bootstrap | bootstrap 仍返回增强后的 `PromptAugmentInput`，不是 construction/launch 显式边界 |
| Runtime/Hub | registry / supervisor 已拆出，live executor session 与 active turn 命名已有区分 | 多个业务方法仍在 `impl SessionHub`，Hub 仍是能力聚合入口 |
| Effects/Pending | terminal effect outbox、runtime command store 已有基础 | task post-turn handler 仍以内存 trait object 穿透；effect durable identity、pending apply-once、失败恢复和 migration 仍需最终验证 |
| Persistence/AppState | store adapter、ready gate、working_dir 策略已有阶段性收口 | `SessionPersistence` 底层仍是大组合接口；AppState/Hub 拆分未达到最终架构 |

## Non-Negotiable Boundaries

- `LaunchCommand` 只表达来源意图和引用：source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload。
- `LaunchCommand` 不携带 resolved VFS / MCP / capability / context / hook trigger / effect handler / working_dir / connector input。
- `UserPromptInput` 不包含 `working_dir`；working dir 最终由 construction 从 project / story / task / agent / lifecycle / local relay workspace root 解析。当前 `PromptAugmentInput.working_dir_input` 是待删除过渡点。
- task `post_turn_handler` 不能作为 command trait object 传递；后续迁入 task/effects/outbox 服务边界并重新评估是否仍需要。
- companion dispatch 不传 parent VFS/MCP/context snapshot；construction 从 parent session facts 解析 companion slice。
- local relay workspace root 是来源事实；MCP 只有作为原始 declaration 才可留在 source payload，不能被命名或使用为 resolved MCP surface。

## Remaining Execution Order

### 1. Correct Entry Intent Boundary

- Keep `UserPromptInput` free of `working_dir`; move the remaining `PromptAugmentInput.working_dir_input` transition into construction.
- Replace task post-turn handler command transport with a task/effects source contract.
- Replace companion parent snapshot command transport with parent session references and slice policy.
- Rename/reshape local relay MCP input as declaration, not resolved MCP.

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
- `LaunchExecution` owns prompt payload, construction, lifecycle, restore, hook plan, follow-up plan, runtime command plan, terminal effect plan, connector input, trace.
- `prompt_pipeline` executes the plan only.

Exit check:

```powershell
rg -n "req\\.vfs|req\\.mcp_servers|req\\.capability_state|req\\.context_bundle|req\\.hook_snapshot_reload|req\\.post_turn_handler" crates/agentdash-application/src/session/prompt_pipeline.rs crates/agentdash-application/src/session/launch_planner.rs
```

### 4. Delete `PromptAugmentInput` Production Handoff

- API/bootstrap no longer returns `PromptAugmentInput`.
- Delete `LaunchCommand::to_augment_input()`.
- `prompt_pipeline` no longer receives `PromptAugmentInput`.

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

- [ ] `LaunchCommand` is pure source intent.
- [ ] `UserPromptInput` does not carry working dir.
- [ ] `PromptAugmentInput` is not a production handoff, planner input, or augmented output.
- [ ] `SessionConstructionPlan` is the launch/query/audit/inspector fact source.
- [ ] `LaunchExecution` is the only per-launch strategy plan.
- [ ] `prompt_pipeline` executes a plan instead of planning/fallback.
- [ ] `SessionHub` is not a business capability entrypoint.
- [ ] terminal effects are durable replay/retry/dead-letter.
- [ ] pending runtime command apply-once and recovery are auditable.
- [ ] persistence store boundaries are not bypassed by new business logic.
- [ ] final validation matrix passes.
