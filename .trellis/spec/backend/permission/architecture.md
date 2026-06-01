# Agent Permission System Architecture

> Agent 动态请求、获批、应用、撤销 capability 的完整生命周期管理。

## Role

Permission System 统一管理 Agent 在运行时对 capability scope 的权限事实。授权 source 可以带 runtime session/turn/tool provenance；授权 effect 必须落到 `AgentFrame` revision 或 run/agent control scope association，提供显式的、可审计的、策略驱动的 capability 授予链路。

## Invariants

- 所有非基础能力（file_read/file_write/shell 以外）在 Permission System 完全接管后均需通过 grant 获得。
- 状态转换由 domain 层 `PermissionGrant` entity 方法强制校验——application 层无法跳过状态机。
- Policy 评估是纯函数：输入为 `(requested_paths, agent_auto_grantable, lifecycle_requestable)`，输出为 `PolicyDecision`。
- Compiler 输出的 `RuntimeCapabilityTransition` 必须通过现有 capability runtime pipeline 应用（replay → replace → update_session_tools → emit delta）。
- Scope escalation 只在 action 实际成功后触发，不在 grant 审批时预创建资源。

## Module Layout

```
crates/agentdash-domain/src/permission/
├── mod.rs               # 公共导出
├── entity.rs            # PermissionGrant aggregate root + 状态机
├── value_objects.rs     # GrantScope / GrantStatus / ScopeEscalationIntent / PolicyDecision
└── repository.rs        # PermissionGrantRepository trait

crates/agentdash-application/src/permission/
├── mod.rs               # 公共导出
├── policy.rs            # PermissionPolicyService（策略评估）
├── compiler.rs          # PermissionGrantCompiler（→ RuntimeCapabilityTransition）
├── service.rs           # PermissionGrantService（lifecycle 编排）
└── escalation.rs        # ScopeEscalationCoordinator（post-action hook）

crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs
crates/agentdash-infrastructure/migrations/0071_permission_grants.sql
crates/agentdash-api/src/routes/permission_grants.rs
```

## Local Decisions

- Permission Grant 作为独立聚合根存在于 `agentdash-domain::permission`，不隶属于 workflow 或 session 模块。
- Policy 评估不依赖 repository（纯函数），数据加载由 service 层负责传入。
- Scope Escalation 通过创建 `LifecycleSubjectAssociation(role=ControlScope)` 实现，复用 workflow 模块的关联层。
- TTL 过期当前由 `expire_overdue()` 提供 batch 清理接口，未来可加入后台 scheduler。

## Contract Appendices

- [Grant Lifecycle](./grant-lifecycle.md)
- [Policy Engine](./policy-engine.md)
