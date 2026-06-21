# Capability Exposure 执行计划

## Phase 1: Fact Source Design

- [x] 选择 AgentFrame exposure fact model，详见 `research/ce02-ce04-implementation-scope.md`。
- [x] 修正为 AgentRun-first 唯一路径：AgentFrame 是 AgentRun model-visible surface revision；PermissionGrant 是 AgentRun-scoped 授权/护栏系统，只由 AgentRun effective capability/admission 服务消费。
- [x] 定义 Canvas expose 与 WorkspaceModule visibility 的恢复顺序。

## Implementation Order

- [ ] CE05: define AgentRun effective capability/admission boundary; replace `CapabilityResolver.granted_capability_keys` direct active-grant override with AgentRun final visible capability / admission result.
- [ ] CE02a: PermissionGrant approve/revoke/expire remains AgentRun-scoped Grant system; tool-internal capability permission is read only through AgentRun admission projection.
- [ ] CE02b: Classify grants that extend Agent toolset; only those model-visible effects write AgentFrame revision through AgentRun capability service.
- [ ] Shared helper: persist AgentFrame revision for surface-changing commands, then adopt that persisted revision into active runtime cache/tools/hook runtime when a delivery runtime exists.
- [ ] CE03: Canvas expose writes AgentFrame revision first through AgentRun capability service, then reconstructs live VFS / hook runtime / WorkspaceModule presentation.
- [ ] CE04: Extract WorkspaceModule visibility resolver from tool code; resolver reads final visible capability via AgentRun effective capability view and selected current frame.
- [ ] Cleanup: fold replaced paths into AgentRun boundary, including active-grant resolver input, production row-update exposure append writers, live VFS-first Canvas exposure and local WorkspaceModule visibility bypass.

## Phase 2: Implementation Slices

- [ ] PermissionGrant approve/revoke/expire 与 AgentRun admission projection 一致；工具集拓展类 grant 在改变模型可见 surface 时写 AgentFrame revision。
- [ ] Canvas expose 从 AgentFrame exposure 派生 live VFS / hook runtime refresh。
- [ ] WorkspaceModule visibility resolver 从 AgentFrame exposure 读取 runtime refs。
- [ ] RuntimeGateway action/channel admission 对齐。

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
