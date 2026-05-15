# Final Convergence Execution Tracker

## Conclusion

本重构仍未最终完成，但生产主链路已经收敛到目标形态：

```text
LaunchCommand
  -> SessionConstructionPlan
  -> LaunchExecution
  -> ExecutionContext connector projection
  -> SessionEvent / TerminalEffectOutbox
```

当前唯一执行计划是 `.trellis/tasks/05-14-session-refactor-batch-7-final-convergence/implement.md`。每次上下文压缩或重新领取任务时，只找第一个未完成 commit slice 继续执行，不创建 child task，不重写方向。

## Commit Slice State

| Slice | Status | Scope |
|---|---|---|
| Commit 1 | 已完成 | source intent、provider 命名、旧 augmenter/seed 壳删除 |
| Commit 2 | 已完成 | `SessionConstructionFacts` production handoff 删除，provider 直接返回 `SessionConstructionPlan` |
| Commit 3 | 已完成 | context/query/audit/inspector 与 launch 同源 |
| Commit 4 | 已完成 | `prompt_pipeline` 收缩为 `LaunchExecution` 执行器 |
| Commit 5 | 已完成 | 拆分 core / eventing / runtime / control 能力服务，迁移 API/local 直接调用点 |
| Commit 6 | 已完成 | launch/hook/effects/capability 调用点迁入具体服务 |
| Commit 7 | 已完成 | 删除 Hub facade 调用残留，迁移 companion / hook auto-resume / tests / title 到具体服务 |
| Commit 8 | 已完成 | runtime tool provider、companion/canvas/workflow tools 改用具体 service bundle，删除 Hub handle 服务定位器 |
| Commit 9 | 未开始 | 解除 launch planner/executor、terminal effects 与 Hub 执行期参数依赖 |
| Commit 10 | 未开始 | effects / pending / persistence 语义验证、migration 核验与父任务文档最终收口 |

## Current Code Facts

| Area | Done | Blocking Gaps |
|---|---|---|
| Entry | 生产入口进入 `LaunchCommand`；`LaunchCommand` 不携带 resolved VFS/MCP/capability/context/hook/effect/working_dir；`UserPromptInput.working_dir` 已移除；task handler、companion parent snapshot、local relay resolved VFS 已迁出 command | 继续保持 source payload 只能由 command 进入 construction/launch，不能新增半成品 handoff |
| Old Shells | `PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput`、`PromptAugmentInput`、`SessionLaunchRequest`、`SessionConstructionSeed`、`SessionConstructionFacts` 已从生产代码删除 | 后续任何同类 wrapper / payload 命中都视为回归 |
| Construction | `SessionConstructionProvider` 直接返回 `SessionConstructionPlan`；assembler 将 VFS、MCP、capability、context bundle/frame、executor profile、prompt projection、task effect binding 写入 plan；Task / Story / Project session detail 与 `/sessions/{id}/context` 均投影同一 `build_session_context_plan`；companion dispatch 使用本次 child session plan，parent session 只作为 source policy 解析 parent facts | audit/inspector projection 需要在最终验证中逐项核对；companion context bundle 需要在最终验证中确认 |
| Launch | `SessionLaunchPlannerInput` 已是 `LaunchCommand + SessionConstructionPlan + runtime facts`；`LaunchExecution` 强制持有 construction，承载 resolved prompt、runtime commands、terminal effects、connector input projection | planner 后续只能处理 per-launch 策略；不能回到 owner/surface/context 重建 |
| Runtime/Pipeline | pipeline 已从 provider 获取 construction plan，不再拆 facts；connector.prompt 接受后才提交 pending capability applied event、context frames、bootstrap meta、pending applied 与 title generation；失败路径只清理 turn 并写 failed terminal | 需要继续把 launch/hook/effects/pending/control 从 Hub 业务门面迁出 |
| Service Split | `SessionCoreService`、`SessionEventingService`、`SessionRuntimeService`、`SessionControlService` 已抽出；API/local 的 core/eventing/cancel/control 调用点已迁移到具体服务；boot reconcile、terminal cancel、stall detector 也改依赖 runtime service | Commit 6 必须继续迁出 launch/hook/effects/pending/tool-builder |
| SessionHub | registry / supervisor 已拆出一部分，live executor session 与 active turn 命名已分离；API/task/routine/workflow orchestrator 的 launch/hook/effects/capability 主调用已迁入具体服务；正在删除 Hub facade 同名入口 | `SessionHub` 仍在 AppState/local 装配、runtime tool provider handle、advance-node 工具服务定位与 session 内部实现中残留；Commit 8 必须删除内部业务依赖或收缩为装配对象 |
| Effects/Pending | terminal effect outbox、runtime command store 已有基础；task effect binding 已是 durable 描述 | effect handler 幂等语义、pending apply-once、失败恢复和 migration 仍需最终验证 |
| Persistence/AppState | store adapter、ready gate、working_dir 策略已有阶段性收口 | `SessionPersistence` 底层仍是大组合接口；AppState service set 需要按最终能力服务暴露 |

## Non-Negotiable Boundaries

- `LaunchCommand` 只表达来源意图和引用：source、actor、target ids、prompt、executor override、follow-up hint、特殊来源策略 payload。
- `LaunchCommand` 不携带 resolved VFS / MCP / capability / context / hook trigger / effect handler / working_dir / connector input。
- `UserPromptInput` 不包含 `working_dir`；prompt projection 由 `SessionConstructionPlan.prompt` 承接，不通过 provider 改写 `UserPromptInput` 回传。
- task `post_turn_handler` 不能作为 command trait object 传递；task effect 只能以 durable binding 描述进入 construction/effects，再由 registry 解析即时 handler 与 replay handler。
- companion dispatch 不传 parent VFS/MCP/context snapshot；construction 从 parent session facts 解析 companion slice。
- local relay workspace root 是来源事实；MCP 只有作为原始 declaration 才可留在 source payload，不能命名或使用为 resolved MCP。
- relaxed launch 也必须经过 construction provider 路径；缺失 construction provider 时失败，不能降级成裸 plan。
- `SessionHub` 不能作为最终业务能力入口；任何保留命中必须是装配壳或测试 fixture。

## Remaining Execution Order

### Commit 5: 拆分 session 业务能力服务

- 抽出 core / eventing / runtime / control 能力服务。
- API/local route handler 改依赖具体服务，不通过 Hub 访问 CRUD、event stream、state query、cancel、tool approval、companion response。
- Hub facade 中已迁出的方法删除或标为 Commit 6 必删的临时内部调用。

退出检查：

```powershell
rg -n "session_hub\s*\.\s*(get_session_meta|get_session_metas_bulk|create_session|create_session_with_title_source|list_sessions|inspect_execution_states_bulk|inspect_session_execution_state|delete_session|mark_owner_bootstrap_pending|inject_notification|subscribe_after|subscribe_with_history|list_event_page|build_projected_transcript|cancel|approve_tool_call|reject_tool_call|respond_companion_request|recover_interrupted_sessions|find_stalled_sessions)" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-application/src
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::hub
git diff --check
```

### Commit 6: 迁移 session launch 调用至能力服务

- `SessionLaunchService` 接管 API/task/routine/workflow/local prompt 的 launch 调用。
- `SessionHookService` 接管 API/workflow 的 hook runtime 读取与确保。
- `SessionEffectsService` 接管 AppState 启动期 outbox replay。
- `SessionCapabilityService` 接管 workflow phase apply、runtime MCP/capability 查询与 construction parent facts。
- task / routine / workflow orchestrator 不再保存 `SessionHub` 字段。

退出检查：

```powershell
rg -n "impl SessionHub|pub struct SessionHub" crates/agentdash-application/src/session
rg -n "session_hub" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-application/src/task crates/agentdash-application/src/workflow crates/agentdash-application/src/routine crates/agentdash-application/src/reconcile crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
git diff --check
```

### Commit 7: 删除 Hub facade 调用残留

- 删除 Hub 上已经迁到具体 service 的同名方法。
- companion / hook auto-resume / session tests / terminal effects tests / title 手动设置全部调用具体 service。
- 删除无调用的 `hub/cancel.rs`。
- 保留事实：本提交不是最终态，只负责把已确定的 facade 残留删干净；内部业务实现去 Hub 放在 Commit 8。

退出检查：

```powershell
rg -n "pub async fn launch_command|pub async fn launch_command_with_outcome|pub async fn respond_companion_request|pub async fn replay_terminal_effect_outbox|pub async fn set_user_title|pub async fn cancel|pub async fn approve_tool_call|pub async fn reject_tool_call|pub async fn find_stalled_sessions" crates/agentdash-application/src/session
rg -n "\.launch_command\(|\.launch_command_with_outcome\(|\.respond_companion_request\(|\.replay_terminal_effect_outbox\(|\.set_user_title\(|\.cancel\(|\.approve_tool_call\(|\.reject_tool_call\(" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
git diff --check
```

### Commit 8: 移除 runtime tools 的 Hub 服务定位器

- `SharedSessionHubHandle` 删除，替换为 `SharedSessionToolServicesHandle`。
- `RelayRuntimeToolProvider` 不再保存 Hub，只保存 core / eventing / control / launch / hooks / capability / companion wait registry bundle。
- companion / canvas / workflow runtime tools 不再持有 Hub。
- `RuntimeSessionMcpAccess` 的实现从 Hub 移到 `SessionCapabilityService`。

退出检查：

```powershell
rg -n "SharedSessionHubHandle|session_hub_handle|impl RuntimeSessionMcpAccess for SessionHub" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
git diff --check
```

### Commit 9: 解除 launch / effects 与 Hub 依赖

- `SessionLaunchService` 持有明确 launch deps；`SessionLaunchExecutor` / planner 不再接收 Hub。
- hook runtime 解析、hook trigger dispatch、auto-resume 调度进入 hook service/deps。
- runtime capability / MCP / live transition / pending transition 进入 capability service/deps。
- `SessionTerminalEffectDispatcher` 由 effects service/deps 驱动，不再读取 Hub。
- `SessionTurnProcessor` 依赖 eventing/runtime/effects 等明确服务或 deps，不再持有 Hub。

退出检查：

```powershell
rg -n "SessionLaunchExecutor::new\\(&.*hub|SessionLaunchPlanner::new\\(.*hub|SessionTerminalEffectDispatcher::new\\(&.*hub|SharedSessionHubHandle|session_hub_handle|session_hub: Option<SessionHub>|impl RuntimeSessionMcpAccess for SessionHub" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "impl SessionHub" crates/agentdash-application/src/session
cargo fmt --check
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::hub
git diff --check
```

### Commit 10: Effects / Pending / Persistence 验证与任务收口

- terminal event 先落库，effect 进入 durable outbox；handler 有 durable identity 或 typed handler。
- effect 支持 retry、dead-letter、replay 与审计。
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
- [x] `SessionConstructionPlan` is the launch/query/audit/inspector fact source for current context endpoints.
- [x] `LaunchExecution` is the only per-launch strategy plan.
- [x] `prompt_pipeline` executes a plan instead of planning/fallback.
- [x] API/local CRUD/event/runtime/control entrypoints no longer go through `SessionHub`.
- [ ] `SessionHub` is not a business capability entrypoint.
- [ ] launch planner/executor/effects/runtime tools do not depend on `SessionHub`.
- [ ] terminal effects are durable replay/retry/dead-letter.
- [ ] pending runtime command apply-once and recovery are auditable.
- [ ] persistence store boundaries are not bypassed by new business logic.
- [ ] final validation matrix passes.
