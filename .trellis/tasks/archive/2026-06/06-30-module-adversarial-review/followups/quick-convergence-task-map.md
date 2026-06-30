# 快速收束任务映射

## 父任务

- `.trellis/tasks/06-30-architecture-quick-convergence/`

## 工作项

1. `.trellis/tasks/06-30-architecture-quick-convergence/work-items/01-authority-capability-admission.md`
   - 对应 `cleanup-scope-triage.md`：M1、M2。
   - 目标：修复两个 P0，阻断 tool-level grant 扩大 visible capability，并按 effect frame 做 admission projection。

2. `.trellis/tasks/06-30-architecture-quick-convergence/work-items/02-extension-workspace-module-consistency.md`
   - 对应：Q2、Q3、Q7。
   - 目标：schema validator 复用、extension invocation workspace resolver 去重、renderer-aware loadability。

3. `.trellis/tasks/06-30-architecture-quick-convergence/work-items/03-vfs-local-guard-rails.md`
   - 对应：Q4、Q5、Q6、Q8。
   - 目标：runtime tool name guard、workspace root guard、handler-declared scheduling、builtin VFS skill discovery identity。

4. `.trellis/tasks/06-30-architecture-quick-convergence/work-items/04-mailbox-steering-consistency.md`
   - 对应：M3。
   - 目标：合并 mailbox steering delivery executor，统一 receipt/status/error semantics。

5. `.trellis/tasks/06-30-architecture-quick-convergence/work-items/05-settings-preference-convergence.md`
   - 对应：M7。
   - 目标：旧 `user_preferences` 迁入 scoped settings，并处理数据库 migration。

## 暂不进入父任务

- Routine runtime status read model。
- Hook snapshot context finalization。
- MCP runtime binding backend anchor 时序。
- RuntimeDiscoveryPolicy typed 化。

这些属于 Medium，但要么跨 launch/context 时序，要么需要先定 owner。可在父任务第一批完成后再决定是否追加工作项。
