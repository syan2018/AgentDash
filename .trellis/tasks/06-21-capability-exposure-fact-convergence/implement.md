# Capability Exposure 执行计划

## Phase 1: Fact Source Design

- [x] 选择 AgentFrame exposure fact model，详见 `research/ce02-ce04-implementation-scope.md`。
- [x] 修正为 AgentRun-first 唯一路径：AgentFrame 是 AgentRun model-visible surface revision；PermissionGrant 是 AgentRun-scoped 授权/护栏系统，只由 AgentRun effective capability/admission 服务消费。
- [x] 定义 Canvas expose 与 WorkspaceModule visibility 的恢复顺序。

## Implementation Order

- [x] CE05: define AgentRun effective capability/admission boundary; replace `CapabilityResolver.granted_capability_keys` direct active-grant override with AgentRun final visible capability / admission result.
- [x] CE02a: PermissionGrant approve/revoke/expire remains AgentRun-scoped Grant system; tool-internal capability permission is read only through AgentRun admission projection and is consumed by production runtime tool assembly.
- [x] CE02b: Classify grants that extend Agent toolset; only those model-visible effects write AgentFrame revision through AgentRun capability service.
- [x] Shared helper: persist AgentFrame revision for surface-changing commands, then adopt that persisted revision into active runtime cache/tools/hook runtime and emit runtime context notifications when a delivery runtime exists.
- [x] CE03: Canvas expose writes AgentFrame revision first through AgentRun capability service, then reconstructs live VFS / hook runtime / WorkspaceModule presentation.
- [ ] CE04: Extract WorkspaceModule visibility resolver from tool code; resolver reads final visible capability via AgentRun effective capability view and selected current frame.
- [ ] Cleanup: fold replaced paths into AgentRun boundary, including active-grant resolver input, production row-update exposure append writers, live VFS-first Canvas exposure and local WorkspaceModule visibility bypass.

## Phase 2: Implementation Slices

- [x] PermissionGrant approve/revoke/expire 与 AgentRun admission projection 一致；工具级 grant 进入生产 runtime tool assembly，工具集拓展类 grant 在改变模型可见 surface 时写 AgentFrame revision。
- [x] Canvas expose 从 AgentFrame exposure 派生 live VFS / hook runtime refresh。
- [ ] WorkspaceModule visibility resolver 从 AgentFrame exposure 读取 runtime refs。
- [ ] RuntimeGateway action/channel admission 对齐。

## CE02 First Slice Notes

- `AgentRunGrantProjection` 已按路径粒度分类：工具级路径进入 admission projection，能力级路径写入 AgentFrame surface revision。
- PermissionGrant approve/revoke 与单个 grant expiry 已使用同一分类：admission-only grant 不创建 AgentFrame revision，工具集拓展类 grant 写入模型可见的 AgentFrame revision。
- Bulk overdue grant expiry 已改为 application-owned path：repository 列出 overdue active grants，`PermissionGrantService` 逐条复用单 grant expiry 分类逻辑后再持久化 Expired 终态。
- 持久化 surface revision 后的 active-runtime adoption helper 已落地：helper 只读取最新持久化 AgentFrame fact，并同步 active runtime cache、connector tools、hook runtime target 与 runtime context 通知，不额外写 revision。
- 当前切片已补齐 CE02 剩余的 bulk expiry owner path 与 active-runtime adoption helper；Canvas expose 仍归 CE03。

## CE02 Completion Notes

- PermissionGrant approve/revoke/expire 与 bulk overdue expiry 均走 AgentRun Grant 分类：工具级 path 只影响 AgentRun admission projection 并由 runtime tool assembly 消费，能力级 path 写入 AgentFrame surface revision。
- `SessionCapabilityService::adopt_persisted_agent_frame_revision` 是后续 Canvas expose / surface-changing command 的统一 active-runtime adoption 入口；CE02 不直接实现 Canvas expose。
- PermissionGrant approve/revoke API 在 Grant effect 后调用 active-runtime adoption；connector/hook/runtime adoption failure 作为 API 错误暴露，避免授权状态与 live runtime surface 静默漂移。
- CE03 可在此 helper 基础上实现 frame-first Canvas exposure recovery。

## CE03 Completion Notes

- Canvas create/present 生产路径不再先更新 live VFS，也不再调用 row-update append writer；`SessionCapabilityService::expose_canvas_mount_revision_and_adopt` 先基于当前 AgentFrame VFS surface 写入新的 `canvas_expose` revision。
- 新 revision 同时携带 Canvas mount、visible canvas mount id、`canvas:{mount_id}` workspace module ref 与派生后的 skill baseline；缺少 AgentFrame VFS fact 时显式失败，避免把 live cache 作为事实源。
- 持久化 revision 后复用 `adopt_persisted_agent_frame_revision` 同步 active runtime cache、connector tools、hook runtime 与 runtime context 通知；`workspace_module_present` 只在 Canvas exposure/adoption 成功后 emit `workspace_module_presented`。

## Validation

```powershell
cargo test -p agentdash-application permission
cargo test -p agentdash-application permission::service
cargo test -p agentdash-application permission::compiler
cargo test -p agentdash-application canvas
cargo test -p agentdash-application workspace_module
cargo test -p agentdash-application session::capability_state
pnpm --filter app-web test -- AgentRunWorkspacePage.workspace-module.test.ts
pnpm run contracts:check
pnpm run frontend:check
```
