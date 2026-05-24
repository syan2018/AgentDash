# Prompt Pipeline 阶段边界收敛 Implement Plan

## Phase 0: Pre-Dev Context

- [x] 读取 Trellis 规范：
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/session/execution-context-frames.md`
  - `.trellis/spec/backend/session/runtime-execution-state.md`
  - `.trellis/spec/backend/session/streaming-protocol.md`
  - `.trellis/spec/backend/quality-guidelines.md`
- [x] 读取 review 来源：
  - `docs/reviews/2026-05-23-architecture-review-round/runtime-control-plane-review.md`
  - `docs/reviews/2026-05-23-architecture-review-round/platform-boundary-governance-review.md`
- [x] 运行一次基线测试，记录失败/通过状态。

## Phase 1: Behavior Characterization

目标：先锁住现有行为，再重构。

- [x] 盘点现有测试覆盖，确认是否已有以下行为测试：
  - connector setup failure 不提交 `TurnStarted`。
  - connector setup failure 不提交 bootstrap success。
  - connector setup failure 不提交 runtime command applied。
  - runtime command applied commit 失败会标记 failed。
  - active turn state 保存 mcp/vfs/capability/executor projection。
  - stream close/error/cancel terminal kind 不变。
- [x] 如缺测试，先补 characterization tests。
- [x] 新增或保留测试命名要描述业务语义，不描述旧实现文件名。

### Phase 0/1 Evidence

当前代码已具备本轮重构所需的高价值 characterization tests，不需要先补测试。基线结果：

```text
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
cargo test -p agentdash-application build_tools_filters_relay_mcp_with_initial_capability_state
  ok
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
  ok
cargo test -p agentdash-application connector_setup_failure_does_not_commit_bootstrap_or_requested_commands
  ok
cargo test -p agentdash-application start_prompt_releases_claim_when_session_meta_is_missing
  ok
cargo test -p agentdash-application cancel_marks_running_turn_interrupted
  ok
cargo test -p agentdash-application runtime_command_apply_commit_failure_marks_failed_and_returns_error
  ok
cargo test -p agentdash-application relay_prompt_registers_sink_before_remote_prompt_can_emit_notification
  ok
```

## Phase 2: Naming Audit

目标：先确定最终语言，再搬代码。

- [x] 形成 rename map，并按必要性分类：
  - 必改：旧名会误导阶段职责。
  - 可保留：旧名准确，改名收益低。
  - 待观察：先拆阶段，后决定。
- [x] 推荐 rename map：

```text
prompt_pipeline.rs              -> launch/orchestrator.rs + stage modules
SessionLaunchExecutor           -> SessionLaunchOrchestrator
SessionLaunchPlanner            -> LaunchPlanner
LaunchExecution                 -> LaunchPlan
LaunchExecutionInput            -> LaunchPlanInput
LaunchExecutionTrace            -> LaunchPlanTrace
LaunchExecutionTraceEntry       -> LaunchPlanTraceEntry
ConnectorInputPlan              -> ConnectorContextPlan
RuntimeCommandLaunchPlan        -> RuntimeCommandApplyPlan
TerminalEffectPlan              -> TerminalEffectHandlingPlan
execute_constructed_launch      -> removed; replaced by typed stage flow
```

- [x] 审计 `LaunchStrictness`：
  - 如果 `Strict` / `Relaxed` 没有实际 launch 行为差异，删除或重命名。
  - 如果保留，必须写清它到底控制什么策略。
- [x] 决定是否保留 `LaunchCommandOutcome`；若重命名，推荐 `LaunchOutcome`。

### Phase 2 Evidence

命名审计结果：

- 必改并已改：`LaunchExecution*` -> `LaunchPlan*`，`SessionLaunchPlanner*` -> `LaunchPlanner*`。
- 必改但随阶段抽取推进：`prompt_pipeline.rs` / `SessionLaunchExecutor` / `execute_constructed_launch`，因为它们仍承载实际 orchestration，必须和 Phase 4/12 一起替换。
- 待阶段类型稳定后改：`ConnectorInputPlan`、`RuntimeCommandLaunchPlan`、`TerminalEffectPlan`，它们会在 preparation / commit 抽取时更自然地落到窄类型。
- 已删除：`LaunchStrictness`。`Strict` / `Relaxed` 只改变错误文案和 trace 字段，没有 launch 策略差异；source intent 已由 `LaunchSource` 表达。
- 保留：`LaunchCommandOutcome`。它是 command facade 的返回结果，不混淆内部阶段职责。

## Phase 3: Module Skeleton Migration

目标：建立 `session/launch/` 子域，不先改变行为。

- [x] 将 `session/launch.rs` 迁移为 `session/launch/mod.rs`。`command.rs` / `plan.rs` 的真实拆分与 Phase 5 命名收敛一起完成，避免先机械拆分再重命名造成重复 churn。
- [x] 将 `session/launch_planner.rs` 迁移为 `session/launch/planner.rs`。
- [x] 将 `session/launch_service.rs` 迁移为 `session/launch/service.rs`。
- [x] 创建空/薄模块：
  - `deps.rs`
  - `orchestrator.rs`
  - `preparation.rs`
  - `connector_start.rs`
  - `commit.rs`
  - `ingestion.rs`
- [x] 更新 `session/mod.rs` re-export。
- [x] 跑编译和相关测试，确保只是模块迁移。

### Phase 3 Evidence

```text
cargo test -p agentdash-application session::launch
  ok, 4 passed
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
cargo check -p agentdash-api -p agentdash-application
  ok
```

## Phase 4: Orchestrator Extraction

目标：把 `SessionLaunchExecutor` 改成清晰的总控入口。

- [x] 新建 `SessionLaunchOrchestrator`。
- [x] 将 provider 获取、turn id 生成、prompt claim、session meta 读取、requested runtime command 读取、construction provider 调用放入 orchestrator。
- [x] 用小结构表达 launch request runtime facts，例如：

```rust
struct LaunchRuntimeFacts {
    turn_id: String,
    had_existing_runtime: bool,
    session_meta: SessionMeta,
    requested_runtime_commands: Vec<RuntimeCommandRecord>,
    context_sources: Vec<String>,
}
```

- [x] 删除或替换 `execute_constructed_launch` 的总控职责。
- [x] 保持测试入口清晰：测试可以直接走 orchestrator 或专用 fixture，但不要恢复“已组装 prompt 旁路”。

### Phase 4 Evidence

`prompt_pipeline.rs` 已移入 `session/launch/orchestrator.rs`，外部 facade 通过 `SessionLaunchService -> SessionLaunchOrchestrator::launch` 进入 launch 子域。原 `execute_constructed_launch` 名称已退出；`launch_with_construction` 现在只编排 `TurnPreparer -> ConnectorStarter -> TurnCommitter -> StreamIngestionAttacher`。

```text
cargo check -p agentdash-application
  ok
cargo test -p agentdash-application session::launch
  ok, 7 passed
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
```

## Phase 5: Plan Rename And Planner Cleanup

目标：让 planning 阶段名实相符。

- [x] 将 `LaunchExecution` 系列重命名为 `LaunchPlan` 系列。
- [x] 确保 `LaunchPlanner::plan` 只产出决策，不做 accepted 后副作用。
- [x] 将 `LaunchPlan` 中的 `context: ExecutionContext` 明确为 connector context seed；如果命名仍含糊，可拆成：

```rust
struct LaunchPlan {
    connector_context: ExecutionContext,
    ...
}
```

- [x] 更新 tests 中对 plan summary / trace / connector projection 的断言。

### Phase 5 Evidence

```text
rg -n "LaunchExecution|SessionLaunchPlanner|launch_execution|LaunchStrictness|strictness" crates/agentdash-application/src crates/agentdash-api/src
  no matches
cargo test -p agentdash-application session::launch
  ok, 4 passed
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
```

`LaunchPlan.context` 在 preparation 阶段被消费并转为 `PreparedTurn.connector_context`。connector-facing 语义已经由 stage handoff 明确表达，后续若继续拆 plan 模块时可再把 `LaunchPlan` 字段本身改名。

## Phase 6: Turn Preparation Extraction

目标：抽出 connector accepted 前的准备阶段。

- [x] 新建 `TurnPreparer` 与 `PreparedTurn`。
- [x] 从原 `execute_constructed_launch` 移入：
  - runtime/direct MCP/relay MCP tools 构建。
  - identity frame 判断与构建。
  - context bundle / hook snapshot / assignment / continuation / pending action frames 组装。
  - turn activation。
  - pending runtime context transition application。
  - owner bootstrap 的 SessionStart hook trigger。
  - queued turn start notices 收集。
  - dedupe context frames。
  - transform_context enqueue。
- [x] 辅助函数迁移：
  - `should_include_connector_startup_context`
  - `collect_queued_turn_start_frames`
  - `notice_to_context_frame`
  - `dedupe_context_frames`
  - `enqueue_context_frames_for_transform_context`
- [x] 测试 `PreparedTurn` 不提交 accepted 后事件。

### Phase 6 Evidence

`preparation.rs` 现在只产出 `PreparedTurn`，并把 connector context 放在 `PreparedTurn.connector_context`。accepted 后事件仍由后续 commit 阶段消费 accepted type 后提交；connector setup failure 测试覆盖了 preparation 后、accepted 前失败不会提交 success side effects。

## Phase 7: Connector Start Extraction

目标：把 accepted 边界变成类型。

- [x] 新建 `ConnectorStarter` 与 `ConnectorAcceptedTurn`。
- [x] 将 `connector.prompt` 调用迁入 `connector_start.rs`。
- [x] 失败路径保持：
  - `turn_supervisor.clear_turn_and_hook(session_id)`。
  - 持久化 failed terminal envelope。
  - 返回 connector error。
- [x] 成功路径只返回 `ConnectorAcceptedTurn`，不提交 user/start/context/capability/meta/runtime-command。
- [x] 测试 connector setup failure 行为不变。

### Phase 7 Evidence

```text
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
  ok
cargo test -p agentdash-application connector_setup_failure_does_not_commit_bootstrap_or_requested_commands
  ok
```

## Phase 8: Turn Commit Extraction

目标：accepted 后 commit 集中化。

- [x] 新建 `TurnCommitter` 与 `CommittedTurn`。
- [x] 迁移：
  - `commit_accepted_launch_events`
  - capability state changed emit
  - context frame emit
  - `apply_turn_start_meta`
  - session meta save
  - `commit_runtime_commands_applied`
  - title generation trigger
- [x] `TurnCommitter::commit` 必须消费 `ConnectorAcceptedTurn`。
- [x] runtime command applied commit failure 语义保持不变。
- [x] 测试 accepted 后事件/meta/runtime-command 顺序与语义。

### Phase 8 Evidence

```text
cargo test -p agentdash-application runtime_command_apply_commit_failure_marks_failed_and_returns_error
  ok
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
```

## Phase 9: Stream Ingestion Extraction

目标：stream attach 不再混在 launch commit 中。

- [x] 新建 `StreamIngestionAttacher` 与 `AttachedTurn`。
- [x] 迁移：
  - `SessionTurnProcessor::spawn`
  - processor tx registration
  - stream adapter spawn
  - stream adapter abort handle registration
  - `resolve_stream_terminal`
- [x] `spawn_stream_adapter` 移入 `ingestion.rs`。
- [x] 测试 cancel/failed/completed terminal kind 不变。

### Phase 9 Evidence

```text
cargo test -p agentdash-application cancel_marks_running_turn_interrupted
  ok
cargo check -p agentdash-application
  ok
cargo test -p agentdash-application session::launch
  ok, 7 passed
```

## Phase 10: Hook Runtime Helper Relocation

目标：`prompt_pipeline` 不再给 `SessionRuntimeInner` 补 hook 方法。

- [x] 将 `resolve_hook_session` / `reload_session_hook_runtime` / `enrich_hook_snapshot_runtime_metadata` 移到更合适模块：
  - 优先 `hooks_service` / `hook_runtime`；
  - 或 launch preparation 专用 hook helper，但不能留在 prompt pipeline。
- [x] `LaunchPlanner` 或 `TurnPreparer` 通过 hook service 调用，不直接扩展 `SessionRuntimeInner`。
- [x] 确认 hook reload / refresh / skip 语义测试通过。

### Phase 10 Evidence

Hook reload/resolve 逻辑已收口到 `SessionHookService`。`launch/orchestrator.rs` 不再实现 `SessionRuntimeInner` hook helper，planner 继续通过 `self.deps.hooks.resolve_hook_session(...)` 调用 hooks 边界。

```text
rg -n "resolve_hook_session|reload_session_hook_runtime|enrich_hook_snapshot_runtime_metadata|impl SessionRuntimeInner" crates/agentdash-application/src/session/launch crates/agentdash-application/src/session/hooks_service.rs
  only hooks_service.rs and planner hook-service call remain
cargo test -p agentdash-application live_runtime_context_transition_derives_skill_dimension_from_active_vfs
  ok
cargo test -p agentdash-application runtime_context_update_injections_are_recorded_without_direct_notification
  ok
```

## Phase 11: Dependency Narrowing

目标：减少每个阶段看到的无关依赖。

- [x] 先保留 `SessionLaunchDeps` 作为总容器。
- [x] 稳定后按阶段拆窄：

```text
LaunchPlanningDeps
TurnPreparationDeps
ConnectorStartDeps
TurnCommitDeps
StreamIngestionDeps
```

- [x] 每个阶段只接收自己需要的 service/store/connector。
- [x] 避免为了抽依赖而引入新的循环注入。

### Phase 11 Evidence

`SessionLaunchDeps` 已移入 `launch/deps.rs` 作为总装配容器，并通过窄视图喂给各阶段：

```text
LaunchPlanningDeps
TurnPreparationDeps
ConnectorStartDeps
TurnCommitDeps
StreamIngestionDeps
```

`orchestrator.rs` 仍持有总容器用于入口事实加载，各 stage struct 只持有自己的窄 deps。验证：

```text
cargo check -p agentdash-application
  ok
cargo test -p agentdash-application session::launch
  ok, 7 passed
cargo test -p agentdash-application start_prompt_records_current_turn_state
  ok
```

## Phase 12: Prompt Pipeline Retirement

目标：删除旧边界。

- [x] 删除 `prompt_pipeline.rs`，或确认它只剩临时 facade 后继续删除。
- [x] 移除所有 `prompt_pipeline` import。
- [x] 更新注释和文档中仍指向 `prompt_pipeline` 的描述。
- [x] 若规范中仍使用旧名，更新为 `session launch` / `launch orchestrator` / `launch stages`。

### Phase 12 Evidence

`prompt_pipeline.rs` 已删除，没有保留兼容 facade；核心 launch 路径全部位于 `session/launch/`。`launch/mod.rs` 只负责模块声明和 re-export，来源意图与计划类型分别拆到 `command.rs` / `plan.rs`。

`.trellis/spec/backend/session/` 已更新为 `LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn` 阶段语言。旧 `prompt_pipeline` / `LaunchExecution` 引用只保留在本任务 PRD/Design/Implement 的历史 before-state 与原始 review 资料中。

```text
cargo fmt --package agentdash-application
  ok
cargo check -p agentdash-application
  ok
cargo test -p agentdash-application session::launch
  ok, 7 passed
```

## Phase 13: Final Review Check

目标：跨上下文压缩后也能重新对齐原始意图，防止任务结束在半吊子重构状态。

- [ ] 重新读取本任务的 `prd.md`：
  - `User Intent`
  - `Before State`
  - `Expected After State`
  - `Acceptance Criteria`
- [ ] 重新读取两份原始 review 中和 session launch / prompt pipeline 相关的建议：
  - `docs/reviews/2026-05-23-architecture-review-round/runtime-control-plane-review.md`
  - `docs/reviews/2026-05-23-architecture-review-round/platform-boundary-governance-review.md`
- [ ] 对最终代码执行边界搜索，确认旧边界退场或只剩非实现性引用：

```powershell
rg -n "prompt_pipeline|execute_constructed_launch|LaunchExecution|SessionLaunchExecutor|SessionLaunchPlanner" crates/agentdash-application/src/session docs .trellis/spec
```

- [ ] 检查最终 launch 子域结构是否符合预期：

```powershell
Get-ChildItem -Recurse crates/agentdash-application/src/session/launch
```

- [ ] 检查阶段类型是否真实存在并位于核心路径：
  - `LaunchPlan`
  - `PreparedTurn`
  - `ConnectorAcceptedTurn`
  - `CommittedTurn`
  - `AttachedTurn`
- [ ] 检查 accepted 边界是否由类型流表达：
  - `TurnCommitter::commit` 必须消费 `ConnectorAcceptedTurn` 或等价 accepted 类型。
  - connector setup failure 路径不能经过 commit 阶段。
- [ ] 检查职责没有只是换文件名：
  - `preparation.rs` 不提交 accepted 后事件。
  - `connector_start.rs` 不提交 success side effects。
  - `commit.rs` 不启动 stream adapter。
  - `ingestion.rs` 不做 launch planning 或 runtime command applied。
- [ ] 检查 `SessionRuntimeInner` 没有继续通过 launch/pipeline 文件扩展 hook runtime helper。
- [ ] 跑完整验证命令，并记录任何非本任务失败。
- [ ] 对照 PRD 的 `Before State`，逐条确认对应问题已被消除；对照 `Expected After State`，逐条确认目标形态已达成。
- [ ] 若发现仍存在半吊子边界，回到对应 Phase 修正，不进入 Done Definition。

## Validation Commands

优先运行针对性测试：

```powershell
cargo test -p agentdash-application start_prompt_records_current_turn_state
cargo test -p agentdash-application build_tools_filters_relay_mcp_with_initial_capability_state
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
cargo test -p agentdash-application connector_setup_failure_does_not_commit_bootstrap_or_requested_commands
cargo test -p agentdash-application start_prompt_releases_claim_when_session_meta_is_missing
cargo test -p agentdash-application cancel_marks_running_turn_interrupted
cargo test -p agentdash-application runtime_command_apply_commit_failure_marks_failed_and_returns_error
cargo test -p agentdash-application relay_prompt_registers_sink_before_remote_prompt_can_emit_notification
```

阶段完成后运行：

```powershell
cargo test -p agentdash-application session::
cargo test -p agentdash-application relay_connector
cargo test -p agentdash-application
```

如 workspace 当前存在非本任务失败，记录失败项与原因，不把无关问题混入本任务。

## Rollback Points

- Phase 3 完成后：模块目录迁移可单独回退。
- Phase 6 完成后：preparation 抽取可单独回退。
- Phase 8 完成后：commit 抽取可单独回退。
- Phase 9 完成后：ingestion 抽取可单独回退。
- Phase 12 前不得删除旧文件，除非所有 imports 已迁移且测试通过。

## Done Definition

- `session/launch/` 成为唯一 launch 实现区域。
- 旧 `prompt_pipeline` 边界退出。
- 阶段类型表达 accepted 前后副作用边界。
- 必要命名已完成，旧误导性命名不再出现在核心 launch 路径。
- 测试覆盖并通过关键 launch 行为。
- `.trellis/spec/backend/session/` 已按最终架构更新。
- Phase 13 的 `Final Review Check` 全部完成，确认实现对齐原始 review 和本任务 Before/After。
