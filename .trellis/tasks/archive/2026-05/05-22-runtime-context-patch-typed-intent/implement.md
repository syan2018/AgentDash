# Runtime Context Patch Typed Intent 标准化执行计划

## Checklist

- [x] 读取上一轮归档任务：
  - `.trellis/tasks/archive/2026-05/05-22-session-vfs-skill-baseline-convergence/prd.md`
  - `.trellis/tasks/archive/2026-05/05-22-session-vfs-skill-baseline-convergence/design.md`
  - `.trellis/tasks/archive/2026-05/05-22-session-vfs-skill-baseline-convergence/pipeline-reference.md`
- [x] 读取相关规范：
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/session/runtime-execution-state.md`
  - `.trellis/spec/backend/session/execution-context-frames.md`
- [x] 审查当前代码入口：
  - `crates/agentdash-application/src/session/types.rs`
  - `crates/agentdash-application/src/session/capability_state.rs`
  - `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
  - `crates/agentdash-application/src/session/capability_service.rs`
  - `crates/agentdash-application/src/session/prompt_pipeline.rs`
  - `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
  - `crates/agentdash-application/src/workflow/step_activation.rs`
  - `crates/agentdash-application/src/workflow/agent_executor.rs`
- [x] 审查前端右侧栏相关入口：
  - `packages/app-web/src/pages/SessionPage.tsx`
  - `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx`
  - `packages/app-web/src/features/workspace-panel/workspace-data-context.tsx`
  - `packages/app-web/src/stores/workspaceTabStore.ts`
  - `packages/app-web/src/services/session.ts`

## Phase Goals

### Phase 1: Patch Schema

目标：先把 payload 类型改成 typed intent，切断 `ToolDimension` / `CompanionDimension` replacement 字段。

- [x] 新增 `RuntimeToolIntent`、`RuntimeMcpIntent`、`RuntimeCompanionIntent`、`RuntimeVfsIntent` 等价类型。
- [x] 将 `RuntimeContextPatch` 字段调整为 typed intent shape。
- [x] 删除 `RuntimeContextPatch::from_target_state` 的 production 路径。
- [x] 提供 source projection builder：`RuntimeContextPatch::from_effective_runtime_projection(...)`。
- [x] 更新 serde 测试，断言 payload JSON 不含 `state`、`tool`、`companion` replacement。

完成标准：

- patch 类型命名与字段表达 intent。
- 编译器能暴露所有旧 replacement 读写点。

### Phase 2: Replay Pipeline

目标：让所有 replay 调用只依赖 typed intent，不再直接读 patch 内的 projection 维度。

- [x] 重写 `apply_runtime_context_patch` 并新增 `replay_runtime_context_patch`。
- [x] replay output 明确暴露 pending effective MCP / VFS 结果，供 construction finalize 使用。
- [x] 更新 `session_construction_bootstrap`，不再读 `patch.tool` / `patch.vfs_overlay` 的旧字段。
- [x] 更新 `prompt_pipeline`，不再直接从 patch 读取 `tool.mcp_servers`。
- [x] 保持 `normalize_capability_state_dimensions` 作为 final projection gate。

完成标准：

- context query、next-turn launch、pending apply event 使用同一 replay helper。
- final `CapabilityState` 与现有行为等价。

### Phase 3: Source Intent Construction

目标：pending command 写入前从 source action 构造 patch，而不是从 `after_state` 反推。

- [x] 扩展 `PendingRuntimeContextTransitionInput`，显式携带 `RuntimeContextPatch`。
- [x] workflow pending path 从 `StepActivation` 构造 patch。
- [x] runtime hub 写入 `PendingCapabilityStateTransition` 时使用 input patch。
- [x] 更新 live path 与 pending path 的边界：live 可以使用 `after_state` 做 connector hot update，pending persisted payload 使用 typed intent。
- [x] 清理测试里对 `RuntimeContextPatch::from_target_state` 的依赖。

完成标准：

- production 代码不存在 full-state -> patch 的反推路径。
- pending command payload 可以从 source intent 追溯。

### Phase 4: Tests And Specs

目标：补齐标准化后的契约证据，并更新规范。

- [x] 更新 `.trellis/spec/backend/session/session-startup-pipeline.md` 的 Runtime Context Patch Replay 场景。
- [x] 更新 `.trellis/spec/backend/session/runtime-execution-state.md`，明确 runtime command payload 保存 typed intent。
- [x] 更新 repository / hub / construction / prompt pipeline 聚焦测试。
- [x] 运行 Rust 聚焦验证。
- [x] 运行前端 typecheck。

完成标准：

- spec 与代码一致。
- 新旧测试都证明 payload intent、replay projection、runtime event 三者闭环。

### Phase 5: Session 右侧栏 Current State 收束

目标：让右侧栏展示的 context / runtime surface / capabilities 明确关联当前 session runtime state，避免多份局部 snapshot 拼装出过期上下文。

- [x] 抽出 `useSessionRuntimeState`，统一管理 session context 与 hook runtime。
- [x] state key 使用 `session_id + owner/source key`；key 变化时不暴露旧 key projection。
- [x] `SessionPage` 改为把统一 `WorkspaceRuntimeData` 传给 `WorkspacePanel`，减少拆散字段。
- [x] `WorkspacePanel` / `WorkspaceDataProvider` 只读取统一 state 对象。
- [x] `canvas_presented`、`capability_state_changed` 等 runtime 更新触发 state invalidate/refetch。
- [x] 前端测试覆盖 session 切换不展示旧 context，既有 projection 测试覆盖 VFS tab 使用 final `runtime_surface`。
- [x] 更新 frontend spec，记录 Session right panel 消费 current runtime projection state。

完成标准：

- 右侧栏没有与当前 session key 不匹配的 context 展示路径。
- context refresh 是状态机事件，不产生新的长期快照事实源。
- WorkspacePanel 的数据边界清晰，可审计。

## Validation Commands

```bash
cargo test -p agentdash-application runtime_context_patch
cargo test -p agentdash-application runtime_command_store
cargo test -p agentdash-application pending_capability_state_transition
cargo test -p agentdash-application pending_runtime_context_transition
cargo test -p agentdash-application session::construction
cargo test -p agentdash-application prompt_pipeline
cargo test -p agentdash-application session::launch
cargo test -p agentdash-api session_context
cargo check -p agentdash-application
cargo check -p agentdash-api
pnpm --filter app-web typecheck
pnpm --filter app-web lint
pnpm --filter app-web test -- useSessionRuntimeState.test.ts
pnpm --filter app-web test -- SessionPage.hook-runtime.test.tsx
pnpm --filter app-web test -- ContextOverviewTab.projection.test.tsx
python ./.trellis/scripts/task.py validate .trellis/tasks/05-22-runtime-context-patch-typed-intent
git diff --check
```

如测试过滤名与实际函数名不匹配，使用 `rg` 定位后切到最窄可运行过滤。

## Review Gates

- 确认 `RuntimeContextPatch` 不含 `ToolDimension` / `CompanionDimension` replacement 字段。
- 确认 production 代码没有 `from_target_state` 或等价 full-state 反推 helper。
- 确认 pending transition persisted payload 来自 typed source intent。
- 确认 construction / prompt pipeline 不再直接读 patch projection 字段。
- 确认 `CapabilityState` 仍由 replay + normalizer 生成闭包状态。
- 确认右侧栏只展示当前 session runtime state，session/source key 不匹配时不使用旧投影。
- 确认 runtime event 刷新右侧栏走 invalidate/refetch 状态机。
- 确认没有数据库 schema churn。

## Risky Files

- `crates/agentdash-application/src/session/types.rs`
- `crates/agentdash-application/src/session/capability_state.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-application/src/session/capability_service.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`
- `crates/agentdash-application/src/session/memory_persistence.rs`
- `crates/agentdash-application/src/session/launch.rs`
- `crates/agentdash-application/src/session/hub/tests.rs`
- `crates/agentdash-application/src/workflow/step_activation.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
- `packages/app-web/src/pages/SessionPage.tsx`
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx`
- `packages/app-web/src/features/workspace-panel/workspace-data-context.tsx`
- `packages/app-web/src/stores/workspaceTabStore.ts`
- `packages/app-web/src/services/session.ts`

## Rollback Points

- Phase 1 后如果 schema 影响面超出预期，先保留新 typed structs 并停止 wiring。
- Phase 2 后如果 replay 与 final projection 不一致，优先修 replay output，不绕过 normalizer。
- Phase 3 后如果 source intent 缺字段，补 source builder，而不是恢复 after-state 反推。
