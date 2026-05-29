# Agent Permission System 评估

## Position

Story 解耦不应只新增 Story 专用 permission 表。更正确的方向是维护一套 Agent permission system：

- 权限跟随 Agent / ProjectAgent assignment，而不是跟随 Session owner type。
- Agent 可以主动申请权限，申请进入统一 permission request 状态机。
- 权限是否自动批准、需要人工审批、需要 platform broker 裁决，由 policy engine 管理。
- 权限 grant 是权威事实，Capability runtime 只消费已生效 grant。
- Permission capability 可以编译/解析成 tool capability、tool filter、MCP server set、VFS/mount access 等运行时能力。
- Story scope permission 是这套系统的一种 scope，不是独立权限宇宙。

该方向延续既有设计：

- `05-26-companion-interaction-capability-grant` 已定义 `capability_grant_request -> permission / grant record -> RuntimeCapabilityTransition -> CapabilityState replay -> tool hot update`。
- `05-26-companion-interaction-persistence-model` 已指出 permission grant request 与 human approval / pending interaction 需要统一事实源。
- `05-17-backend-capability-expansion-governance` 已给出 capability request 的状态机、policy decision、ack、TTL、revoke 经验。

## Current Gaps

当前能力系统已经有 capability 到工具面的部分投影，但缺一层 Agent 权限事实：

- `ToolCapability` 与 `CapabilityState` 描述运行时可见能力，但不是“谁因为什么拥有权限”的权威事实。
- `allowed_owner_types` 按 SessionOwnerType 做硬边界，无法表达 Agent assignment、审批、临时授权、撤销、过期。
- companion capability grant 设计已有申请链路，但尚未成为通用 Agent permission system。
- Story 权限若单独建表，会和 Project 权限、backend capability expansion、companion grant request 形成多套授权状态机。

## Target Concepts

### AgentPermissionRequest

Agent 主动申请权限或平台自动生成申请时创建：

```text
AgentPermissionRequest
- id
- project_id
- requester_agent_id
- requester_session_id / lifecycle_run_id
- source_kind: tool_request | lifecycle_policy | human_action | platform_broker | routine_run
- target_scope_kind: project | story | task | lifecycle_run | backend | workspace | mcp_server
- target_scope_id
- requested_permission_caps
- requested_tool_paths
- reason
- risk_level
- requested_ttl / expires_at
- status: created | pending_policy | pending_approval | approved | rejected | cancelled | expired
- policy_decision
- approval_ref
- created_at / updated_at
```

Request 表达“Agent 想要什么”，不表达最终已生效工具面。

### AgentPermissionGrant

审批或 policy 通过后创建/更新：

```text
AgentPermissionGrant
- id
- request_id
- project_id
- agent_id
- scope_kind
- scope_id
- permission_caps
- tool_capabilities
- tool_filters
- mcp_servers
- status: active | revoked | expired | superseded
- granted_by_kind: policy | user | system | platform_broker
- granted_by_id
- effective_at
- expires_at
- revoked_at
- audit_reason
```

Grant 是权限事实源。任何 runtime capability 只能来自 active grant、Agent base config、Project policy 或 lifecycle contract 的可解释组合。

### Permission Capability

Permission cap 是高层授权语义，例如：

- `project.story.create`
- `story.manage`
- `story.task.dispatch`
- `workflow.lifecycle.modify`
- `backend.workspace.prepare`
- `mcp.server.use:{server}`

Permission cap 不等同于工具 cap。它需要经过 resolver 编译为：

- `ToolCapability` key，例如 `story_management`、`task_management`、`workflow_management`。
- tool path allow/deny，例如 `story_management::create_story`。
- MCP server visibility。
- VFS/mount access。
- runtime policy / approval requirements。

### Capability Resolver

Resolver 输入：

```text
Agent assignment
Agent base permissions
Project permissions / policy
Lifecycle contract
Active AgentPermissionGrant
Runtime association
Session/runtime constraints
```

Resolver 输出：

```text
CapabilityState
Tool capability set
Tool policy filters
MCP server set
ContextFrame capability explanation
Tool schema delta
```

它替代 `SessionOwnerType.allowed_owner_types` 作为主要能力来源。Session owner type 只能作为 runtime context hint，不能作为授权事实。

## Story Flow Under Agent Permission System

```text
ProjectAgent assignment
  -> base permission caps include project.story.create or can_request(project.story.create)
  -> LifecycleRun inspection Agent finds issue
  -> Agent calls story management tool or companion capability request
  -> Platform broker creates AgentPermissionRequest if permission is missing or elevated
  -> policy auto-approves or asks user/admin
  -> AgentPermissionGrant becomes active
  -> grant compiles into story_management / task_management tool caps
  -> CapabilityState updates current runtime tools
  -> Agent creates Story or requests Story-scope management
  -> Story page lists authorized Agent sessions via grants + runtime associations
```

Story-specific permission state can be a scoped `AgentPermissionGrant(scope_kind=story)`. It does not require `OwnedAgent` as a separate concept.

## Approval And Policy

Policy layer should support:

- auto approve low-risk permissions from trusted Agent assignment。
- require human approval for Story management, workflow mutation, backend expansion, broad VFS access。
- reject impossible permissions based on Project policy / lifecycle contract。
- partial grant: approve subset of requested tool paths。
- TTL / lease for temporary permissions。
- revoke active grant and push runtime capability shrink。

Human approval should reuse durable interaction / pending action direction from companion interaction design. Approval result updates request/grant state; companion response is only conversational feedback.

## Runtime Apply

Approved grants enter capability runtime:

```text
AgentPermissionGrant(active)
  -> PermissionCapabilityCompiler
  -> RuntimeCapabilityTransition
  -> CapabilityState replay
  -> build_tools_for_execution_context
  -> connector.update_session_tools or next-turn apply
  -> capability_state_changed + tool_schema_delta
```

Live apply is allowed when connector supports tool hot update. Otherwise grant is active but tools appear on next turn/session preparation.

Revocation/expiry uses the same pipeline in reverse:

```text
grant revoked/expired
  -> capability shrink transition
  -> CapabilityState replay
  -> tool schema delta removes paths
```

## Relation To Story Decoupling

This system helps Story decoupling because:

- Story no longer needs an OwnedAgent entity.
- Story no longer infers authority from companion session.
- Story only queries active grants and runtime associations to know which Agent sessions can act.
- LifecycleRun can stay runtime-only; permission belongs to Agent + scope + grant.
- Tool availability becomes explainable and auditable.

## First Implementation Slice

Do not start by building every scope. Start with a narrow but general skeleton:

1. Domain model:
   - `AgentPermissionRequest`
   - `AgentPermissionGrant`
   - permission cap value objects
   - status machine and audit fields

2. Resolver:
   - compile active grants to `ToolCapability` + tool filters
   - emit explanation records
   - keep existing `CapabilityState` as runtime projection

3. Story scope MVP:
   - `project.story.create`
   - `story.manage`
   - `story.task.dispatch`
   - Story page query for authorized Agent runtime sessions

4. Request entry:
   - Project story management tool path
   - companion `capability_grant_request`
   - policy auto approval for explicitly pre-granted Agent base permissions

5. Runtime apply:
   - next-turn apply first
   - live tool update when existing connector path supports it

## Open Design Decisions

- Permission cap naming and hierarchy.
- Whether `AgentPermissionGrant.tool_capabilities` is stored as compiled projection or derived on read.
- Approval ownership: Project owner/editor, current user, system policy, or platform broker.
- Whether permission request and durable interaction share one table or remain linked records.
- How ProjectAgent base config declares “has permission” vs “can request permission”.
