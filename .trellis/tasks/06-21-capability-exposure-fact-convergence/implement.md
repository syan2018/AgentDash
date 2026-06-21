# Capability Exposure 执行计划

## Phase 1: Fact Source Design

- [ ] 选择 AgentFrame exposure fact model。
- [ ] 定义 PermissionGrant approve/revoke/expire 如何产生 AgentFrame runtime effect。
- [ ] 定义 Canvas expose 与 WorkspaceModule visibility 的恢复顺序。

## Phase 2: Implementation Slices

- [ ] PermissionGrant approve/revoke/expire 与 AgentFrame capability effect 一致。
- [ ] Canvas expose 从 AgentFrame exposure 派生 live VFS / hook runtime refresh。
- [ ] WorkspaceModule visibility resolver 从 AgentFrame exposure 读取 runtime refs。
- [ ] RuntimeGateway action/channel admission 对齐。

## Validation

```powershell
cargo test -p agentdash-application permission
cargo test -p agentdash-application canvas
cargo test -p agentdash-application workspace_module
pnpm run frontend:check
```

