# Design

## Problem Statement

当前系统把 `session_id` 同时用作 live connector delivery、Backbone event stream、runtime gateway session action、AgentRun current delivery lookup、Canvas/Workspace Module visibility、Workflow/Companion coordination、VFS surface source、Terminal lookup 和 permission provenance。这个模型让 session 从会话层实现细节升级成跨聚合业务关联键，导致外围模块必须理解 session runtime substrate。

本任务的目标是把 session 重新收束为会话/投递层内部 identity。外围业务只能接触自己的业务身份或明确的 delivery abstraction，不能直接依赖 session id。

## Boundary Model

### Allowed Session Zone

允许直接持有或传输 `session_id` 的区域：

- runtime session persistence 与 session repository。
- Backbone envelope、session event stream、connector prompt/update/steer transport。
- relay/local executor routing、live executor session registry。
- RuntimeGateway provider 内部的 Session Action admission 和 relay payload generation。
- API/application 中专门命名为 session adapter / runtime delivery adapter 的桥接模块。

这些区域的职责是把一个 live delivery channel 路由到真实执行端。它们可以保存 session id，但不能让它成为上层业务合同。

### Forbidden Business Zone

以下区域不能把 session id 作为业务事实源、公开 DTO、repository 查询主键或 Agent/用户可见输出：

- `agentdash-workspace-module`
- Canvas runtime/business/API/SDK contract
- AgentRun mailbox/domain/control-plane contract
- Workflow/Lifecycle business orchestration
- Companion/Hook business tools and request model
- VFS surface business and materialization request model
- Terminal business model outside stream transport adapter
- Permission grant business provenance
- Task/context business projection

这些模块如需操作 live delivery，必须通过外层 port 获得业务身份已解析的 context。

## Replacement Identities

### AgentRun Delivery

引入或统一使用一个不暴露 session id 的 AgentRun delivery context：

```rust
pub struct AgentRunDeliveryContext {
    pub run_id: Uuid,
    pub agent_id: String,
    pub project_id: Uuid,
    pub delivery_id: AgentRunDeliveryId,
    pub current_user: Option<ProjectAuthorizationContext>,
    pub surface_revision: Option<i64>,
}
```

`delivery_id` 是 AgentRun 侧 identity，不是 runtime session id。adapter 可以在内部维护：

```rust
AgentRunDeliveryId <-> runtime_session_id
```

但这个映射不进入 workspace-module/domain/API contract。

### Workspace Module Runtime Adapter

把现在 `agentdash-workspace-module` 内的 `RuntimeToolProvider` 实现外移到 session/runtime adapter crate 或 application adapter 模块。推荐形态：

```text
agentdash-workspace-module
  pure module/canvas business

agentdash-workspace-module-runtime
  RuntimeToolProvider implementation
  ExecutionContext -> AgentRunDeliveryContext projection
  RuntimeGateway / channel / VFS / presentation transport adapter
```

如果不新增 crate，也必须让 adapter 模块位于 application/API composition 边界，并且保持 `agentdash-workspace-module` 本体不依赖 SPI、RuntimeGateway、application-vfs。

### Presentation

Workspace Module / Canvas 只产出：

```rust
WorkspaceModulePresentationIntent
WorkspaceModulePresentationDiagnostic
```

由外层 `AgentRunWorkspacePresentationPort` 决定如何投递到前端。当前可以仍走 session event stream，但 `SessionMetaUpdate` 只能在 adapter 内部构造。

### Invocation

Workspace Module 只解析并校验：

```rust
WorkspaceModuleInvocationIntent {
    module_id,
    operation_key,
    input,
    dispatch,
}
```

外层 `WorkspaceModuleOperationInvoker` 负责把 intent 翻译到：

- RuntimeGateway Session Action
- Extension protocol channel
- Canvas host operation
- future non-session runtime target

业务层不构造 `RuntimeActor::AgentSession` 或 `RuntimeContext::Session`。

### Canvas Diagnostics / Interaction

Canvas runtime state repository 已以 `run_id + agent_id + canvas_mount_id` 保存 latest snapshot。读取端应直接消费 AgentRun delivery context：

```rust
latest_canvas_runtime_observation(
    delivery: &AgentRunDeliveryContext,
    canvas_mount_id: &str,
)
```

不允许在 workspace-module 中通过 `RuntimeSessionExecutionAnchorRepository::find_by_session` 反查。

### VFS and Terminal

VFS surface 和 Terminal route 当前存在 session-shaped public/API contract。目标是：

- VFS source 使用 workspace surface / AgentRun runtime surface / Canvas mount 等业务 source。
- Terminal 操作使用 terminal instance id 或 AgentRun delivery terminal target。
- session stream routing 只由 terminal adapter 解析，不暴露给业务 API。

## Repository and Domain Changes

`RuntimeSessionExecutionAnchor` 当前位于 domain workflow 并作为多处反查入口使用。目标不是简单删除，而是收束为 session adapter 内部 index：

- 业务查询改为 AgentRun/runtime surface query port。
- 外围模块禁止直接依赖 anchor repository。
- anchor repository trait 可保留在 session/runtime adapter 所需层，但命名、文档和依赖方向必须表达 adapter index，而不是跨业务事实源。

AgentRun mailbox 中的 `runtime_session_id` 字段需要替换为 AgentRun delivery identity 或 adapter-only trace ref。Mailbox 是 AgentRun control-plane 事实，不能把 session id 存为业务源。

## API and Contract Changes

项目未上线，不保留旧字段兼容层。需要直接修改 generated contracts：

- Canvas runtime/submit/read routes 不接受 session id。
- Workspace Module HTTP/API 不接受 runtime session id。
- VFS surface public source 不使用 `SessionRuntime` 作为业务 source。
- Workflow/Companion/Permission API 不把 runtime session id 暴露为用户必须提交的字段。
- Terminal API 从 `/sessions/{session_id}/terminals` 收束为业务 target route，或拆成纯 session stream adapter route。

## Migration Notes

数据库 migration 需要按字段性质处理：

- 纯 session persistence 表保留 session id。
- 外围业务表中用于业务关联的 runtime session id 字段需要迁移到 AgentRun delivery/workflow/terminal 等业务 identity。
- 审计类字段如仍需保留底层 delivery trace，必须命名为 adapter trace，并且不得被业务查询依赖。

## Risks

- 这是身份模型收束任务，影响面会跨 API、application、domain、contracts、frontend 和 DB migration。
- 现有 RuntimeGateway Session Action 仍以 session actor/context 为 admission 模型；短期只能把它隔离到 adapter 内，不能把 Gateway 本身一次性完全去 session 化。
- Terminal/VFS/Workflow/Companion 的 session 使用可能包含一部分真实 transport 需求，需要逐项区分“允许的会话层 transport”和“禁止的业务关联”。

## Validation Strategy

- 静态 grep gate：在禁止目录中搜索 `session_id`、`runtime_session_id`、`delivery_runtime_session_id`、`find_by_session`、`RuntimeContext::Session`、`RuntimeActor::AgentSession`、`SessionMetaUpdate`。
- Crate dependency gate：`agentdash-workspace-module` 不依赖 SPI、RuntimeGateway、application-vfs。
- Contract gate：generated TS contracts 不在 Canvas/Workspace Module/AgentRun mailbox/VFS surface business DTO 中暴露 session id。
- Behavior tests：Canvas diagnostics、interaction state、submit-to-Agent、workspace_module invoke/present、workflow/companion coordination 通过 AgentRun/business identity 工作。
