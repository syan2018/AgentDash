# Runtime Gateway

> application 层的统一运行时能力调用入口，承载 Session Runtime Action 与 Setup Action 的调用协议。

---

## 核心抽象

`RuntimeGateway`（`agentdash-application/src/runtime_gateway/gateway.rs`）：

- `register(provider)` — 注册 RuntimeProvider
- `surface_for(context)` — 按 ActionKind 过滤（不做 actor 校验，仅调试用）
- `surface_for_actor(actor, context)` — 完整 actor/context 校验（**消费端必须使用此入口**）
- `invoke(request)` — 执行 action

`RuntimeProvider` trait：`action_key()` + `action_kind()` + `invoke()`

---

## Action 分类

| Kind | Actor 约束 | Context 约束 | 示例 |
|------|-----------|-------------|------|
| Session | `AgentSession` / `UserCanvas` / `WorkflowNode` / `SessionUser` | `RuntimeContext::Session` | `mcp.list_tools`, `mcp.call_tool` |
| Setup | `PlatformUser` / `EnvironmentSetup` | `RuntimeContext::Setup` | `mcp.probe_transport`, `workspace.detect`, `workspace.browse_directory` |

---

## Actor / Context 校验规则

| 条件 | 错误 |
|------|------|
| Session context 的 `session_id` 为空 | `InvalidRequest` |
| Session context 搭配无 session actor | `CapabilityDenied` |
| actor/session context 的 `session_id` 不一致 | `CapabilityDenied` |
| Setup context 搭配 Agent/Canvas/Workflow/SessionUser actor | `CapabilityDenied` |
| Session actor 查询 Setup context | `CapabilityDenied` |
| action key 未注册 | `ProviderUnavailable` |

---

## surface_for vs surface_for_actor

- `surface_for(context)` 只按 `RuntimeActionKind` 过滤 provider，**不做 actor 校验**，仅用于内部调试
- `surface_for_actor(actor, context)` 做完整 actor/context 绑定校验，**消费端必须使用此入口**

消费端必须使用 `surface_for_actor`，`surface_for` 不能作为授权 manifest。

---

## Session MCP Action

Session Action baseline：`mcp.list_tools`、`mcp.call_tool`

关键约束：
- MCP surface 的唯一能力来源是 `CapabilityState`（空 CapabilityState 不暴露任何工具）
- Provider 通过 `RuntimeSessionMcpAccess` 进入 SessionHub，不直接读 MCP preset/agent config
- 所有工具暴露都必须经过 `capability_state.is_capability_tool_enabled()`
- `surface_for(Session)` 只表达 action 粒度可用性；具体 MCP tool surface 由 `mcp.list_tools` 输出

---

## Setup Action

Setup Action baseline：`mcp.probe_transport`、`workspace.detect`、`workspace.detect_git`、`workspace.browse_directory`

关键约束：
- API route 只做鉴权、请求解析、组装 `RuntimeInvocationRequest`，业务必须进入 provider
- Setup Action 不进入 Session Runtime Surface
- HTTP route 保持原响应契约，不让前端看到 Gateway 内部 envelope

API route 不直接调用底层业务函数，必须通过 Gateway invoke。

---

## RuntimeActionToolAdapter

Adapter 是 AgentTool → Gateway 的桥接基础件，不是产品注入策略：

- Adapter 不自行做 capability 裁决；由 Gateway/provider 负责
- Adapter 不直接调用底层 provider，必须通过 `gateway.invoke()`
- 不应把裸 Runtime Action 直接作为 Agent 工具面默认注入
- 平台自定义工具应显式定义工具名/参数/权限，内部选择性复用 Gateway invocation

---

## Canvas Runtime Bridge

Canvas iframe 通过 Gateway 调用 Session Action 的约束：

- Canvas iframe 代码不可信，不能直接拿 relay/MCP/http secret
- iframe SDK 只发送 `action_key` + `input`，actor/context 由父页面/API route 组装
- Canvas 专用 `/runtime-invoke` 不接受 iframe 传入的 actor/context/trace
- API route 必须再次校验 Session 与 Canvas Project 绑定关系
