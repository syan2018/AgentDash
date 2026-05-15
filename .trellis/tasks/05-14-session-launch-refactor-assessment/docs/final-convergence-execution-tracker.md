# Final Convergence Execution Tracker

## Conclusion

本重构仍未最终完成，但主链路已经推进到目标形态的前半段：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

当前唯一执行计划是 `.trellis/tasks/05-14-session-refactor-batch-7-final-convergence/implement.md` 的 6 个固定 commit slice。每次上下文压缩或重新领取任务时，只找第一个未完成 slice 继续执行，不再创建 child task，不再重写方向。

## Commit Slice State

| Slice | Status | Scope |
|---|---|---|
| Commit 1 | 已完成 | source intent、provider 命名、旧 augmenter/seed 壳删除 |
| Commit 2 | 已完成 | `SessionConstructionFacts` production handoff 删除，provider 直接返回 `SessionConstructionPlan` |
| Commit 3 | 已完成 | context/query/audit/inspector 与 launch 同源 |
| Commit 4 | 已完成 | `prompt_pipeline` 收缩为 `LaunchExecution` 执行器 |
| Commit 5 | 未开始 | 拆掉有职责 `SessionHub` |
| Commit 6 | 未开始 | effects / pending / persistence 最终验证与任务收口 |

## Current Code Facts

| Area | Done | Blocking Gaps |
|---|---|---|
| Entry | 生产入口进入 `LaunchCommand`；`LaunchCommand` 不携带 resolved VFS/MCP/capability/context/hook/effect/working_dir；`UserPromptInput.working_dir` 已移除；task handler、companion parent snapshot、local relay resolved VFS 已迁出 command | 继续保持 source payload 只能由 command 进入 construction/launch，不能新增半成品 handoff |
| Old Shells | `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput`、`PromptAugmentInput`、`SessionLaunchRequest`、`SessionConstructionSeed`、`SessionConstructionFacts` 已从生产代码删除 | 后续任何同类 wrapper / payload 命中都视为回归 |
| Construction | `SessionConstructionProvider` 直接返回 `SessionConstructionPlan`；assembler 将 VFS、MCP、capability、context bundle/frame、executor profile、prompt projection、task effect binding 写入 plan；Task / Story / Project session detail 与 `/sessions/{id}/context` 均投影同一 `build_session_context_plan`；companion dispatch 使用本次 child session plan，parent session 只作为 source policy 解析 parent facts | audit/inspector projection 仍需最终核对，companion context bundle 仍需在最终验证中确认 |
| Launch | `SessionLaunchPlannerInput` 已是 `LaunchCommand + SessionConstructionPlan + runtime facts`；`LaunchExecution` 强制持有 construction，承载 resolved prompt、runtime commands、terminal effects、connector input projection | planner 后续只能处理 per-launch 策略；不能回到 owner/surface/context 重建 |
| API/bootstrap | bootstrap 不再返回 `UserPromptInput + SessionConstructionFacts`；route 层不再持有旧 prompt envelope；Task / Story / Project session detail 不再独立调用 construction planner 或自行构造 runtime surface | 后续重点转入 pipeline execution 和 Hub 拆分 |
| Runtime/Pipeline | pipeline 已从 provider 获取 construction plan，不再拆 facts；connector.prompt 接受后才提交 pending capability applied event、context frame、bootstrap meta、pending applied 与 title generation；失败路径只清理 turn 并写 failed terminal | 后续重点是拆掉 `SessionHub` 业务入口 |
| Runtime/Hub | registry / supervisor 已拆出一部分，live executor session 与 active turn 命名已分离 | 多个业务方法仍在 `impl SessionHub`，Hub 仍是能力聚合入口 |
| Effects/Pending | terminal effect outbox、runtime command store 已有基础；task effect binding 已是 durable 描述 | effect handler 幂等语义、pending apply-once、失败恢复和 migration 仍需最终验证 |
| Persistence/AppState | store adapter、ready gate、working_dir 策略已有阶段性收口 | `SessionPersistence` 底层仍是大组合接口；AppState/Hub 拆分未达到最终架构 |

## Non-Negotiable Boundaries

- `LaunchCommand` 只表达来源意图和引用：source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload。
- `LaunchCommand` 不携带 resolved VFS / MCP / capability / context / hook trigger / effect handler / working_dir / connector input。
- `UserPromptInput` 不包含 `working_dir`；prompt projection 由 `SessionConstructionPlan.prompt` 承接，不通过 provider 改写 `UserPromptInput` 回传。
- task `post_turn_handler` 不能作为 command trait object 传递；task effect 只能以 durable binding 描述进入 construction/effects，再由 registry 解析即时 handler 与 replay handler。
- companion dispatch 不传 parent VFS/MCP/context snapshot；construction 从 parent session facts 解析 companion slice。
- local relay workspace root 是来源事实；MCP 只有作为原始 declaration 才可留在 source payload，不能命名或使用为 resolved MCP。
- relaxed launch 也必须经过 construction provider 路径；缺失 construction provider 时失败，不能降级成裸 plan。

## Remaining Execution Order

### Commit 3: Context / Query / Audit / Inspector 同源

- context endpoint 只调用 construction query/use case，投影 `SessionConstructionPlan`。
- route/bootstrap 删除 task/story/project context response 主线重建分支。
- audit / inspector 所需字段进入 `ConstructionProjections`。
- owner 排序只来自 `SessionOwnerResolver`。

退出检查：

```powershell
rg -n "build_task_session_context|build_story_session_context_response|build_project_session_context_response|finalize_augmented_request" crates/agentdash-api/src/routes crates/agentdash-api/src/bootstrap
cargo test -p agentdash-application session::construction
cargo check -p agentdash-api
```

### Commit 4: `prompt_pipeline` 收缩为执行器

- `SessionLaunchPlanner` 输出完整 `LaunchExecution`。
- `prompt_pipeline` 只做 claim / activate、event append、connector.prompt、accepted 后 meta/pending/title 提交、processor supervision。
- connector.prompt 失败不得提交 bootstrap completed、pending applied、title generation 等成功副作用。
- hook session、runtime delegate、restore state、terminal effect handler 的解析归入 launch/effects 边界。

退出检查：

```powershell
rg -n "req\.vfs|req\.mcp_servers|req\.capability_state|req\.context_bundle|req\.hook_snapshot_reload|req\.post_turn_handler" crates/agentdash-application/src/session/prompt_pipeline.rs crates/agentdash-application/src/session/launch_planner.rs
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
```

### Commit 5: 拆掉有职责 `SessionHub`

- 拆出 core / ownership / construction / launch / runtime / eventing / hooks / effects / pending / adapters 能力服务。
- `SessionHub` 若仍存在，只能作为依赖装配壳或测试 handle，不承载业务判断。
- 新调用点依赖具体能力服务，不再通过 hub 读写跨职责状态。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
cargo check -p agentdash-application
cargo test -p agentdash-application session::hub
```

### Commit 6: Effects / Pending / Persistence 最终验证

- terminal event 先落库，effect 进入 durable outbox；handler 有 durable identity 或 typed handler。
- pending runtime command 覆盖 requested / applied / failed，具备 apply-once 和失败恢复测试。
- 新增业务逻辑依赖 meta / event / outbox / runtime-command store 边界。
- PostgreSQL / SQLite migration 通过。
- 父任务 tracker、closure checklist、session startup spec 与代码事实一致。

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
rg -n "PreparedSessionInputs|finalize_request|PreparedLaunchPrompt|SessionLaunchPlan|AugmentedLaunchInput|PromptSessionRequest|SessionLaunchIntent|LaunchCommand::.*_prepared|PromptAugmentInput|SessionConstructionFacts|SessionConstructionSeed" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src
git diff --check
```

## Completion Definition

- [x] `LaunchCommand` is pure source intent.
- [x] `UserPromptInput` does not carry working dir.
- [x] `PromptAugmentInput` is not a production handoff, planner input, or augmented output.
- [x] `SessionLaunchRequest` is not a production handoff.
- [x] `SessionConstructionFacts` is not a production handoff.
- [ ] `SessionConstructionPlan` is the launch/query/audit/inspector fact source.
- [ ] `LaunchExecution` is the only per-launch strategy plan.
- [ ] `prompt_pipeline` executes a plan instead of planning/fallback.
- [ ] `SessionHub` is not a business capability entrypoint.
- [ ] terminal effects are durable replay/retry/dead-letter.
- [ ] pending runtime command apply-once and recovery are auditable.
- [ ] persistence store boundaries are not bypassed by new business logic.
- [ ] final validation matrix passes.
