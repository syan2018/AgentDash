# 执行计划

## Parent Coordination

- [ ] On resume or context compaction, read `.trellis/tasks/06-30-module-adversarial-review/followups/autonomy-protocol.md` before acting.
- [ ] 启动前复核 5 个工作项文档。
- [ ] 分批分配工作项，优先 Authority quick fix。
- [ ] 收集每个工作项验证结果。
- [ ] 运行父级集成复核。
- [ ] 回写 review task 的 issue status。

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
