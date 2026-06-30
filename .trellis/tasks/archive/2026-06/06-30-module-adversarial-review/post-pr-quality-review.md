# PR #78 完成质量复核

## Scope

本文件记录对远端 PR `#78 [codex] 模块对抗审查与架构收束` / 分支
`codex/module-adversarial-review-cleanup` 的复核结果。

复核目标：

- 确认原始对抗审查任务是否达成预期目标。
- 对 PR 中各提交/各组工作项的完成质量做并行评估。
- 判断代码是否仍然存在过度只增不减、旧路径残留或事实源分叉。

本次复核只做只读审查和文档记录；未创建新的 Trellis task，未修改业务代码，未运行耗时全量测试。

## Overall Conclusion

原始 review 任务的核心目标基本达成：模块拓扑、并行 subagent 审查、综合问题清单、scope triage、quick convergence work items、D1-D12 design backlog review、decision points 和 implementation slices 都有落地产物。提交序列也能对应到 quick convergence 与后续 design slices。

但 PR 不能按“系列目标完全无残留”处理。当前有若干 P1 级质量风险，主要集中在 runtime VFS policy / AgentRun effective capability / RuntimeGateway 与 WorkspaceModule catalog owner / WorkspacePlacement transaction 边界。它们不是 P0 直接破坏性问题，但会影响本轮“唯一 owner、唯一事实源、只减不增”的完成质量判断。

对“是否仍然过度只增不减”的判断是：不能简单判定为纯膨胀，因为 workspace placement、desktop local owner、runtime gateway schema、旧 `vfs_access_grants` 命名等处都有真实旧 owner 收缩；但代码层仍呈现明显净新增和并行入口残留。剥离 `.trellis` 归档后，代码/前端/迁移约 `10753` 增、`3067` 删；进一步粗分生产代码约 `+9682/-2930`。最值得回头压缩的是 VFS `*_with_policy` 双入口、workspace-module operation context 多构建入口、AgentRun effective capability 同形 DTO/空投影方法。

## Commit / Work Item Coverage

已覆盖：

- `5540671fa docs(trellis): 记录模块对抗审查与快速收束规划`
  - 覆盖原始任务的 topology、research、adversarial review、cleanup triage 和 followups。
- `7415bee29 refactor(architecture): 收束模块审查快速清理项`
  - 覆盖 quick convergence 的 5 个工作项：Authority M1/M2、Extension Q2/Q3/Q7、VFS/Local Q4/Q5/Q6/Q8、Mailbox M3、Settings M7。
- `025f4f7b7 docs(trellis): 完成模块设计 backlog 评估`
  - 覆盖 D1-D12 的 owner / contract / implementation slice / decision 状态评估。
- `d1ff26042 refactor(relay): 收束 prompt typed payload`
  - 对应 D12 / Slice 1。
- `7b1fc7994 refactor(runtime-gateway): 收束 dynamic action catalog`
  - 对应 D7 / Slice 2。
- `d54a75abe refactor(workspace-module): 收束 runtime action availability`
  - 对应 D8 / Slice 3。
- `ca72d52f7 refactor(agentrun): 收束 admission production boundary`
  - 对应 D1 / Slice 4。
- `4bfac4104 refactor(agentrun): 收束 command availability`
  - 对应 D5 / Slice 5。
- `3d9c37766 refactor(runtime): 收束 agent runtime delegate facets`
  - 对应 D6 / Slice 6。
- `77323a3fc refactor(vfs): 收束 runtime access policy` 与 `8e7a304cb refactor(vfs): 收束 PermissionGrant path policy`
  - 对应 D9 / Slice 7。
- `f0b9ef799 refactor(workspace): 收束 placement transaction`
  - 对应 D10 / Slice 8。
- `cb36998e2 refactor(desktop): 收束 local runtime owner`
  - 对应 D11 / Slice 9。
- `0bd72e5ef chore(trellis): 归档模块收束任务`
  - 完成本轮任务归档。

明确延期：

- D4 Canonical Launch Command owner：仍需用户决策。
- D3 Shared LifecycleGate resolver：仍需用户决策。
- D2 LifecycleDispatchService internal owner split：建议等 D3/D4 决策后推进。

文档状态不一致：

- 原始 review / quick convergence 文档中的部分 residual 状态没有随后续实现回写，容易低估本 PR 已完成范围。
- `06-30-runtime-vfs-access-policy-cleanup` 的早期设计仍写 typed PermissionGrant VFS path rules 是未来 gap，但后续 PR 又新增 `requested_vfs_access` 并在 acceptance 中标记完成。实现证据支持“已接入 typed contract”，但行为语义仍有下方 P1 风险。

## P1 Findings

### 1. PermissionGrant VFS rule 进入 policy，但没有收窄最终访问面

证据：

- [runtime_surface.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:418) 先用 `RuntimeVfsAccessPolicy::whole_mounts_from_vfs(vfs)` 生成整 mount 授权。
- [runtime_surface.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:431) 之后再追加 `PermissionGrant.requested_vfs_access` 规则。
- [vfs-access.md](D:/ABCTools_Dev/AgentDashboard/.trellis/spec/backend/vfs/vfs-access.md:414) 要求运行期 VFS 准入由 tool capability、provider mount capability、`RuntimeVfsAccessPolicy` 三者取交集。
- [vfs-access.md](D:/ABCTools_Dev/AgentDashboard/.trellis/spec/backend/vfs/vfs-access.md:422) 明确 `PermissionGrant.requested_vfs_access` 是运行期授权事实。

风险：

当前实现中 PermissionGrant rule 只是增加了一条带 `RuntimeVfsAccessSource::PermissionGrant` 的规则；如果 system/runtime projection 的 whole-mount rule 已允许该 mount/path/operation，最终 `policy.admits(...)` 仍会通过。现有测试只检查 PermissionGrant source rule 存在，以及过滤 PermissionGrant source 后某路径不匹配；没有断言最终 policy 对未授权路径拒绝。

建议：

- 明确 `requested_vfs_access` 的语义是“附加授权”还是“对当前 runtime surface 的收窄授权事实”。
- 若目标是 path-level 准入，应调整 policy assembly，使 PermissionGrant path rule 能影响最终 effective policy，而不是与 whole-mount system rule 并集后被覆盖。
- 增加负例测试：mount 支持 read，但 PermissionGrant 只允许 `docs/` 时，最终 `policy.admits("workspace", "tests/lib.rs", Read)` 必须为 false。

### 2. AgentRun effective capability / admission 仍读 launch frame

证据：

- [effective_capability.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:345) 通过 `anchor.launch_frame_id` 读取 frame。
- [runtime_surface.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:310) runtime surface query 使用 `frame_repo.get_current(agent.id)` 读取当前 frame。
- [tool-capability-pipeline.md](D:/ABCTools_Dev/AgentDashboard/.trellis/spec/backend/capability/tool-capability-pipeline.md:11) 要求 AgentRun effective capability / admission 叠加当前 AgentFrame surface 与 Grant projection。

风险：

Canvas expose、PermissionGrant surface-changing effect、workspace module visibility 等写入新 AgentFrame revision 后，RuntimeGateway/VFS resource surface 可能看到 current frame，但 tool schema/admission 仍按 launch frame 判断。这样会形成 runtime surface 与 execution admission 的事实源分叉。

建议：

- 将 effective capability/admission 的 frame 读取语义与 runtime surface query 对齐。
- 增加 `launch_frame != current_frame` 的测试，覆盖 current frame 新增 capability / VFS / workspace module visibility 后 admission 的行为。

### 3. Grant projection 仍按 launch frame 查询 active grants

证据：

- [effective_capability.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:288) 使用 `list_active_by_frame(anchor.launch_frame_id)`。
- [runtime_surface.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:352) runtime surface 同样用 `anchor.launch_frame_id` 查 active grants。
- [entity.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-domain/src/permission/entity.rs:22) 把 `effect_frame_id` 注释为生效目标与主查询锚点。
- [service.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/permission/service.rs:103) 创建 grant 时写入请求中的 `effect_frame_id`。

风险：

如果授权请求绑定的是当前 adopted frame，或 frame 在 launch 后发生 revision 更替，按 launch frame 查询会导致 active grant 丢失或错误保留。

建议：

- 明确 grant effect frame 与 runtime session anchor 的关系。
- 让 grant projection 查询使用实际 effect/current frame，而不是固定 launch frame。
- 增加 launch frame 与 effect frame 不同的测试。

### 4. workspace-module runtime bridge 丢失 `vfs_access_policy`

证据：

- [runtime_bridge.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:88) 只读取 `context.session.vfs`，然后 `SharedRuntimeVfs::new(vfs)`。
- [common.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-vfs/src/tools/common.rs:55) 的 `SharedRuntimeVfs::new` 会重新编译 whole-mount policy。
- [runtime_bridge.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs:175) Canvas expose 后调用 `replace(active_vfs)`。
- [common.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-vfs/src/tools/common.rs:78) 的 `replace` 同样重新编译 whole-mount policy。
- 对比正确主路径：[provider.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/runtime_tools/provider.rs:96) 会读取 `context.session.vfs_access_policy` 并 `new_with_policy`。

风险：

workspace module 内部持有的 VFS state 与 session/runtime surface 的 policy 分叉，形成 VFS policy 旁路。

建议：

- runtime bridge 构造和 replace VFS 时必须保留或重算对应 effective `RuntimeVfsAccessPolicy`。
- 增加 workspace-module 构造/Canvas expose 后仍保留 `vfs_access_policy` 的测试。

### 5. RuntimeGateway 与 WorkspaceModule 对重复 `action_key` 的归属判定不一致

证据：

- [extension_actions.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:262) RuntimeGateway dynamic catalog 使用 `BTreeMap` 聚合 action。
- [extension_actions.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:273) Gateway 用 `or_insert_with` 保留第一个匹配安装项。
- [mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:293) WorkspaceModule 从 extension projection 生成 `action_key -> extension_key`。
- [mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:297) `collect::<BTreeMap<_, _>>()` 在重复 key 时会以后出现的 extension 覆盖。
- [mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:311) WorkspaceModule 用该 owner map 归属 Gateway descriptor。

风险：

两个 enabled extension 声明同一个 `action_key` 时，Gateway 可能实际执行第一个安装项的 action，而 WorkspaceModule 展示在最后一个 extension module 下。runtime action catalog 的 owner 尚未完全收束到 Gateway resolved catalog。

建议：

- 让 Gateway descriptor / resolved catalog 显式携带 extension identity，WorkspaceModule 直接消费该 resolved catalog。
- 或在安装/启用层拒绝 project 内重复 `action_key`。

### 6. WorkspacePlacementService 是 service 编排收束，但还不是事务边界收束

证据：

- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:152) `create_workspace` 先写 inventory。
- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:158) 之后创建 workspace。
- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:212) `update_workspace` 先 upsert inventory。
- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:218) 之后 update workspace。
- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:322) `bind_discovered` 循环 upsert inventory。
- [placement.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/workspace/placement.rs:342) 之后逐 workspace update。

风险：

如果后续 workspace 写入失败，数据库会留下已经提交的 backend inventory，但 workspace binding/shape 未同步提交；`bind_discovered` 还可能出现部分 workspace 已更新、部分未更新。考虑到本轮目标写了 placement transaction，这里目前更准确地说是“owner/service 编排收束”，不是数据库事务边界收束。

建议：

- 在 application/infrastructure 层提供共享 transaction unit。
- 或把 inventory 与 workspace 变更下沉到同一 repository transaction 内提交。

## P2 Findings

- VFS service 仍保留大量无 policy 入参即 whole-mount 的公开包装方法，例如 [service.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-vfs/src/service.rs:197) 的 `read_text_range` 包装到 `read_text_range_with_policy(..., None, ...)`。这对非 runtime browser/API surface 可能合理，但 runtime-facing 调用应通过类型或模块边界强制携带 policy。
- `AgentRunEffectiveCapabilityView` 在 application 与 ports crate 中有同名同形 DTO，并通过转换桥接；[effective_capability.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:255) 的 `schema_visible_capability_state_for_runtime_session` 当前计算 projection 后仍返回 `base_state.clone()`，有为未来语义预留但当前不承载行为的迹象。
- workspace-module runtime action availability 保留多个构建入口：[mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:72)、[mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:86)、[mod.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-workspace-module/src/workspace_module/mod.rs:97)。接口表面继续扩张。
- runtime delegate facets 已拆分，但 [delegate.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-agent-types/src/runtime/delegate.rs:121) 的 `AgentRuntimeDelegateSet::from_all_facets` 与 [planner.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application-runtime-session/src/session/launch/planner.rs:167) 的生产装配仍允许 monolithic delegate 形态继续存在。
- `registration_source` contract 是顶层字段，但物理事实仍从 backend `device` JSON 派生：[contract.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-contracts/src/backend/contract.rs:145)、[management.rs](D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/backend/management.rs:267)。当前主路径一致，但后续非 enrollment 写入仍有轻微漂移风险。
- `git diff --check origin/main..HEAD` 当前失败，原因是 7 个 Trellis markdown 文件有 extra blank line at EOF。PR 描述中声称 `git diff --check` 已通过，验收记录与当前分支状态不一致。

## Positive Findings

- RuntimeGateway dynamic action catalog 主路径总体收敛：`surface_for_actor` 做 actor/context 校验，dynamic provider 通过 `discover_actions` 暴露 concrete descriptor，`extension.runtime_action` marker 未进入 actor catalog。
- WorkspaceModule runtime action availability split 的主路径基本符合目标：operation readiness 与 module projection 分离，缺少 Gateway descriptor 时会变成 diagnostic/unavailable，而不是继续作为可执行事实源。
- 前端旧 gate 基本已移除：webview/canvas bridge 提交 `action_key + input` 给后端，执行裁决交给后端。
- desktop local runtime owner 从 Tauri `main.rs` 中真实瘦身，claim/profile/settings 下沉到 `agentdash-local`。
- workspace placement 从 API route 中迁出，`routes/workspaces.rs` 有明显删除，说明 route 层减负是真实的。
- runtime-gateway schema validator 是明确抽离，`extension_actions.rs` 有相当删除量，不是纯叠加。
- 旧 `vfs_access_grants` 已收窄为 Project VFS mount exposure，并在旧字段输入处拒绝，而不是兼容别名。
- contracts/generated 与 migrations 基础链路基本齐：WorkspaceModule readiness、Extension tab loadability、Permission VFS access DTO、`0033` user preferences migration、`0034` permission grant VFS access migration 都有对应文件。

## Verification Notes

已执行/参考：

- `git status --short --branch`
- `gh pr list` / `gh pr view 78`
- `git log --oneline --reverse origin/main..HEAD`
- `git diff --shortstat origin/main..HEAD`
- `git diff --stat origin/main..HEAD`
- `git diff --shortstat origin/main..HEAD -- . ':(exclude).trellis/*'`
- `git diff --shortstat origin/main..HEAD -- .trellis`
- 多组 `rg` 与定点文件读取
- `git diff --check origin/main..HEAD`

未执行：

- 未运行耗时全量 Rust/TS 测试。
- 未运行 `pnpm run contracts:check`。
- 未运行 `pnpm run migration:guard`。

`git diff --check origin/main..HEAD` 当前失败项：

- `.trellis/tasks/archive/2026-06/06-30-architecture-quick-convergence/work-items/01-authority-capability-admission.md`
- `.trellis/tasks/archive/2026-06/06-30-architecture-quick-convergence/work-items/02-extension-workspace-module-consistency.md`
- `.trellis/tasks/archive/2026-06/06-30-architecture-quick-convergence/work-items/03-vfs-local-guard-rails.md`
- `.trellis/tasks/archive/2026-06/06-30-architecture-quick-convergence/work-items/04-mailbox-steering-consistency.md`
- `.trellis/tasks/archive/2026-06/06-30-architecture-quick-convergence/work-items/05-settings-preference-convergence.md`
- `.trellis/tasks/archive/2026-06/06-30-module-adversarial-review/cleanup-scope-triage.md`
- `.trellis/tasks/archive/2026-06/06-30-module-adversarial-review/module-topology.md`
