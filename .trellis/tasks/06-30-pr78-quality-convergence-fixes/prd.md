# PR78 质量风险快速收口

## Goal

对 PR `#78` 的 post-review 结果做一轮快速收口修复，优先消除已确认的 P1 运行时事实源分叉、VFS policy 旁路和可执行 catalog owner 不一致问题，让本轮架构收束更接近“唯一 owner / 唯一事实源 / 旧路径退出”的预期。

## Background

复核记录位于 `.trellis/tasks/archive/2026-06/06-30-module-adversarial-review/post-pr-quality-review.md`。该复核确认原始 review / quick convergence / design backlog 的目标覆盖基本成立，但指出若干 P1/P2 质量风险仍会影响 PR 完成质量。

本任务只处理无需用户决策、可在当前 PR 分支快速收口的问题。不处理 D3 Shared LifecycleGate resolver、D4 Canonical Launch Command owner、D2 LifecycleDispatchService owner split 等明确延期设计项。

## Requirements

- 修复 runtime VFS policy 事实源分叉：
  - workspace-module runtime bridge 构造或替换 `SharedRuntimeVfs` 时必须保留当前 `ExecutionContext.session.vfs_access_policy` 或同源 policy。
  - runtime-facing VFS 路径不得静默退回 whole-mount policy 造成旁路。
- 修复 AgentRun effective capability / admission 与 PermissionGrant projection 的 frame source：
  - schema-visible capability 与 tool admission 应基于 current/effect frame，而不是固定 launch frame。
  - active grant projection 应按真实 effect/current frame 查询，避免 launch frame 与 current frame 分叉。
- 收束 PermissionGrant VFS policy contribution 行为：
  - `requested_vfs_access` 的测试必须覆盖最终 effective policy 的 allow/deny，而不只检查 rule 存在。
  - 如当前 whole-mount system rule 与 PermissionGrant path rule 的语义需要保留并集，必须明确记录边界；否则实现应让 path-level grant 影响最终准入。
- 修复 RuntimeGateway 与 WorkspaceModule 对 duplicate `action_key` 的 owner 不一致：
  - RuntimeGateway resolved catalog 与 WorkspaceModule module ownership 必须使用同一 action owner。
  - 重复 `action_key` 必须被同一 owner 规则处理，不能 Gateway 执行第一个而 WorkspaceModule 展示最后一个。
- 做低风险清理：
  - 修复 `git diff --check origin/main..HEAD` 报出的 Trellis markdown EOF 空行。
  - 不做与本轮 P1/P2 无关的重构。

## Acceptance Criteria

- [ ] `workspace_module` runtime bridge 消费并保留 `ExecutionContext.session.vfs_access_policy`，Canvas expose / VFS replace 后不回退 whole-mount policy。
- [ ] AgentRun effective capability / admission 在 `launch_frame != current_frame` 场景下使用 current/effect frame 的 surface 和 grants。
- [ ] PermissionGrant VFS access 测试覆盖最终 `RuntimeVfsAccessPolicy::admits` 的负例。
- [ ] RuntimeGateway / WorkspaceModule duplicate `action_key` owner 行为一致，并有 focused test 覆盖。
- [ ] `git diff --check origin/main..HEAD` 不再因 Trellis markdown EOF 空行失败。
- [ ] 运行相关 focused Rust tests；如跳过某项 broad check，记录原因。

## Out of Scope

- 不实现 D3 / D4 / D2 设计延期项。
- 不引入兼容层或旧字段回退。
- 不做全量事务框架重构；WorkspacePlacement transaction 边界如超出快速范围，只保留为后续风险记录。
