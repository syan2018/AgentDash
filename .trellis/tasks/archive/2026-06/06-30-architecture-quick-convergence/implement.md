# 执行计划

## Parent Coordination

- [x] On resume or context compaction, read `.trellis/tasks/06-30-module-adversarial-review/followups/autonomy-protocol.md` before acting.
- [x] 启动前复核 5 个工作项文档。
- [x] 分批分配工作项，优先 Authority quick fix。
- [x] 派发 worker 时强调：目标是基于第一性的收束清理，优先删除错误路径、事实分叉和重复实现；不要用新增并行路径掩盖旧问题。
- [x] 派发 worker 时限制大规模 Rust 编译/全量测试；实现阶段可做 cheap focused verification，昂贵编译和广覆盖检查留给 check/integration 阶段。
- [x] 收集每个工作项验证结果。
- [x] 运行父级集成复核。
- [x] 回写 review task 的 issue status。

## Suggested Order

1. `work-items/01-authority-capability-admission.md`
2. `work-items/02-extension-workspace-module-consistency.md`
3. `work-items/03-vfs-local-guard-rails.md`
4. `work-items/04-mailbox-steering-consistency.md`
5. `work-items/05-settings-preference-convergence.md`

除第 1 项优先外，其余可并行。Settings 任务因 migration 独立处理。

## Validation

- 每个工作项运行自己的 targeted checks。
- 父任务最终运行：
  - `git status --short`
  - relevant backend checks from touched crates
  - relevant frontend typecheck/test if touched
  - `pnpm run migration:guard` if settings migration lands

## Integration Result

### Work Item Completion

- Authority capability admission:
  - tool-level grant no longer mutates visible `CapabilityState`;
  - runtime projection uses frame-scoped active grants;
  - full tool execution admission port wiring remains a D1 design residual.
- Extension / WorkspaceModule consistency:
  - shared JSON schema subset validator;
  - shared extension invocation workspace resolver;
  - renderer-aware extension loadability, including `canvas_panel` UI-only readiness.
- VFS / Local guard rails:
  - duplicate callable tool name guard in runtime tool composer and session tool assembly;
  - shared workspace root guard for tool/process execution;
  - handler-declared relay dispatch plan;
  - builtin VFS skill discovery passes launch identity.
- Mailbox steering:
  - delegate and scheduler steering consume a shared delivery executor;
  - receipt/status/error/event/payload cleanup semantics are aligned.
- Settings preference convergence:
  - `hide_system_steer_messages` moved to scoped settings;
  - old BackendRepository preference port removed;
  - migration `0033_migrate_user_preferences_to_settings.sql` migrates legacy data and drops `user_preferences`.

### Final Checks

- `cargo fmt --check`
- `pnpm run migration:guard`
- `pnpm run contracts:check`
- Check workers additionally ran targeted Rust checks/tests, frontend typecheck/tests, app-web lint, and migration/contracts verification for their assigned scopes.

### Residuals

- D1 must handle production wiring of tool invocation admission through `AgentRunEffectiveCapabilityPort::admit_tool`.
- Existing broad clippy debt remains outside this work item scope.
