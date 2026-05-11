# 设计 Runtime Gateway 统一运行时能力网关

## Goal

设计一套通用的 Runtime Gateway / Runtime Action 协议，将 Canvas、Agent、Workflow、环境准备流程等不同调用方统一接入受控运行时能力调用层。该能力网关应能复用现有 MCP、Relay、VFS、Capability Pipeline 等基础设施，同时把权限裁决、上下文绑定、审计追踪、超时控制和结果返回收口到同一套 Invocation 模型中。

## What I Already Know

- 当前 Canvas 已经是 Project 级可运行前端资产，包含文件、sandbox 配置与数据绑定。
- Canvas runtime snapshot 目前主要支持静态文件运行和启动时数据绑定解析，不适合直接承载底层 relay/http/MCP 裸调用。
- Relay 协议已经具备云端到本机的 MCP probe/list/call 能力，也具备 workspace detect、directory browse、tool/file/shell/terminal 等本机操作通道。
- 本机后端已经有环境/工作区相关实现：prompt 首次执行时会按 workspace contract 做 `workspace_prepare`，也已有 workspace detect、browse directory、terminal spawn/input/resize/kill 等 relay/API 通路。
- SessionHub 当前会合并 runtime tools、direct MCP、relay MCP，并受 CapabilityState 约束。
- Canvas、Agent、Workflow、环境准备流程都可能需要触发“受控运行时动作”，因此不应把新协议命名或边界绑定到 Canvas。

## Requirements

- 定义通用 Runtime Gateway 架构边界，而不是 Canvas 专属 relay bridge。
- 定义 Runtime Action 作为可声明、可授权、可执行的原子动作。
- 定义 Runtime Invocation 作为一次动作调用实例，包含 actor、context、target、input、policy、trace 和 result。
- 定义 Runtime Provider 作为动作实现适配器，可分别对接 MCP、Relay、VFS、HTTP、平台内建能力和环境准备能力。
- 定义 Runtime Surface，表示某个上下文和 actor 当前可见、可调用的动作集合。
- 定义 Runtime Scope，但原则上以 Session 为唯一一等运行时宿主；Project、WorkspaceBinding、WorkflowRun、CanvasView 等只作为 Session 装配来源、绑定关系或消费视图，不应各自拥有平行的运行时上下文。
- Runtime Gateway 统一承载两类调用面：
  - Session Runtime Action：必须绑定 `session_id`，进入普通 runtime surface。
  - Setup Action：尚未有 `session_id`，服务 workspace / MCP / backend / environment 的配置、探测与准备流程；不进入普通 runtime surface。
- 支持至少以下调用方模型：
  - Canvas / iframe runtime 通过前端 Runtime Bridge SDK 调用。
  - Agent 通过 RuntimeActionToolAdapter 调用。
  - Workflow node 在生命周期执行中直接调用。
  - 环境准备业务通过 gateway 触发本机探测、准备、启动等动作。
  - 平台 UI 可通过 gateway 触发受控本机操作。
- 普通运行时调用必须绑定到 `session_id`；仅 workspace detect、browse directory、MCP probe 等创建 Session 前的操作可作为 Setup Action 存在。
- Setup Action 也必须走 Runtime Gateway 的统一 Invocation / Provider / Audit 模型；现有 workspace detect、browse directory、MCP probe、terminal/bootstrap 类 API 后续应收口为 Gateway 的薄入口，避免保留多条业务实现通路。
- 权限与工具可见性应复用或接入现有 CapabilityState / capability pipeline，不另起一套平行权限系统。
- Relay 应保持 transport 定位；上层不得直接把 relay 裸命令暴露给 Canvas、Agent 或 Workflow。
- 所有 invocation 应具备 trace_id / invocation_id，便于审计、调试和后续 session event 投影。

## Proposed Terminology

- **Runtime Gateway**：统一运行时能力网关，负责协议入口、授权裁决、路由、审计和结果归一化。
- **Runtime Action**：可调用动作，例如 `mcp.call_tool`、`vfs.read`、`environment.prepare`。
- **Runtime Invocation**：一次 action 调用实例。
- **Runtime Provider**：action 的实现适配器，例如 MCP provider、Relay provider、VFS provider、HTTP provider。
- **Runtime Surface**：当前上下文暴露给调用方的 action 集合。
- **Runtime Scope**：Runtime Surface 的生命周期宿主。原则上采用 Session-bound 模型：Gateway 本体常驻，但 action 可见性、资源句柄和权限上下文都收束到 Session；其它实体通过 SessionBinding / SessionFrame / TurnFrame / CapabilityState 间接参与。
- **Session Runtime Action**：绑定 `session_id` 的常规运行态动作，可暴露给 Agent、Canvas、Workflow node、会话 UI 等 actor。
- **Setup Action**：创建或准备 Session 前的受控平台动作，例如 workspace detect、browse directory、MCP transport probe、backend/environment probe；归 Runtime Gateway 统一协议管理，但不属于普通 Session Runtime Surface。
- **Runtime Actor**：调用身份，例如 `user_canvas`、`agent_session`、`workflow_node`、`environment_setup`、`platform_user`。
- **Runtime Bridge SDK**：嵌入式前端 runtime 的客户端桥接层，例如 Canvas iframe 中的 `window.agentdash.invoke()`。

## Technical Approach

### Conceptual Flow

```text
Runtime Client
  -> Runtime Gateway
  -> Policy / Capability Resolver
  -> Runtime Provider
  -> MCP / Relay / VFS / HTTP / Platform / Environment
  -> RuntimeInvocationResult
```

### Candidate Action Keys

- `mcp.call_tool`
- `mcp.list_tools`
- `context.read`
- `context.search`
- `relay.command`
- `http.request`
- `environment.prepare`
- `environment.probe`
- `workspace.prepare`
- `workspace.detect`
- `terminal.spawn`
- `terminal.input`
- `workflow.advance`
- `platform.query`
- `platform.mutate`

### MVP Recommendation

第一版建议聚焦以下范围：

- 后端定义 Runtime Gateway 核心值对象与 provider trait。
- 接入 `mcp.call_tool`，复用现有 relay MCP provider 和 direct MCP 能力发现。
- 接入环境/工作区类 action 的统一门面，优先复用已有 `workspace_prepare`、`workspace_detect`、`browse_directory`、terminal relay/API 通路，而不是重写本机执行逻辑。
- 将 workspace detect、browse directory、MCP probe 等创建前操作纳入 Gateway 统一协议，作为 Setup Action 暴露给平台 UI / environment setup actor；现有 API 入口后续迁移为调用 Gateway 的 thin route。
- 将原先设想的只读 VFS 调整为 `context.read` / `context.search` 候选 provider：它的价值是验证 Gateway 可按 runtime scope 读取 session/project/workspace 上下文数据，但不强制作为首个 MVP 主线。
- 实现 `RuntimeActionToolAdapter`，让 Agent 可以通过 Gateway 调用 action。
- 实现 Canvas Runtime Bridge SDK 的最小调用链，但 Canvas 仅作为消费方之一。
- 记录 invocation trace，并在需要时投影为 session event。

### Workstream Decomposition

整体计划拆成 5 条主线，每条主线都有独立执行计划，后续可拆子任务或分 PR 推进：

1. **Gateway Core Protocol**：定义 Runtime Gateway 内核协议、值对象、Provider SPI、Invocation 结果模型与错误模型。
   - 执行计划：[`plan-01-gateway-core-protocol.md`](plan-01-gateway-core-protocol.md)
2. **Session Runtime Plane**：把普通 runtime surface 收束到 Session，接入 CapabilityState、MCP direct/relay、RuntimeActionToolAdapter。
   - 执行计划：[`plan-02-session-runtime-plane.md`](plan-02-session-runtime-plane.md)
3. **Setup Action Plane**：把 workspace detect、browse directory、MCP probe、环境准备等创建前/准备期操作纳入 Gateway，现有 API 变成 thin route。
   - 执行计划：[`plan-03-setup-action-plane.md`](plan-03-setup-action-plane.md)
4. **Runtime Consumers**：接入 Canvas Bridge SDK、Agent tool adapter、Workflow node、平台 UI 等消费端，但都复用 Gateway/Surface，不各写一套执行链。
   - 执行计划：[`plan-04-runtime-consumers.md`](plan-04-runtime-consumers.md)
5. **Governance And Migration**：统一权限裁决、审计追踪、长任务 Operation、错误语义、文档与旧通路迁移验证。
   - 执行计划：[`plan-05-governance-migration.md`](plan-05-governance-migration.md)

### Implementation Progress

截至 2026-05-11，当前分支 `codex/runtime-gateway` 已完成以下落地：

1. **Gateway Core Protocol 已完成首版实现**
   - 已新增 application 层 `runtime_gateway` 模块。
   - 已定义 `RuntimeActionKey`、`RuntimeActor`、`RuntimeContext`、`RuntimeTarget`、`RuntimePolicy`、`RuntimeTrace`、`RuntimeInvocationRequest`、`RuntimeInvocationResult`、`RuntimeInvocationOutput`、`RuntimeActionDescriptor`、`RuntimeSurface`、`RuntimeProvider`、`RuntimeGateway`、`RuntimeInvocationError`。
   - Gateway 已具备 provider 注册、surface 枚举、setup/session runtime 请求校验、trace 回填与基础错误分类。

2. **Setup Action Plane 已完成当前明确的短链路迁移**
   - 已接入 `mcp.probe_transport`，并将 `POST /api/projects/{project_id}/mcp-presets/probe` 迁移为 Runtime Gateway thin route。
   - 已接入 `workspace.detect`，并将 `POST /api/projects/{project_id}/workspaces/detect` 及创建 Workspace 时的自动探测迁移到 Runtime Gateway。
   - 已接入 `workspace.detect_git`，并将 `POST /api/workspaces/detect-git` 迁移为 Runtime Gateway thin route。
   - 已接入 `workspace.browse_directory`，并将 `POST /api/backends/{backend_id}/browse` 迁移为 Runtime Gateway thin route。
   - API 层保留原 HTTP 路径与响应 DTO；业务执行收口到 application provider，provider 复用现有 relay/local handler，不重写本机实现。
   - Setup Action 已统一限制为 `RuntimeContext::Setup`，actor 仅允许 `PlatformUser` / `EnvironmentSetup`，backend 离线统一映射为 `Conflict`。

3. **契约与验证已补齐到当前实现边界**
   - 已新增 `.trellis/spec/backend/runtime-gateway.md`，记录 Setup Action 的输入输出、错误矩阵、thin route 规则与测试要求。
   - 已覆盖 Runtime Gateway/provider 单测：action key 校验、setup actor 限制、trace 回填、MCP probe、workspace detect、detect git、browse directory。
   - 已运行并通过 targeted Rust 验证：`cargo test -p agentdash-application runtime_gateway`、`cargo test -p agentdash-api backends`、`cargo test -p agentdash-api workspaces`、`cargo test -p agentdash-api rpc`、`cargo check -p agentdash-application`、`cargo check -p agentdash-api`。

4. **Session Runtime Plane 已完成 MCP action 首版闭环**
   - 已新增 Session Runtime Action：`mcp.list_tools` 与 `mcp.call_tool`。
   - Gateway 已注册 `McpListToolsProvider` / `McpCallToolProvider`，并继续复用 Session Runtime 的 actor/context 校验：必须是同一 `session_id` 的 session actor 调用。
   - Session MCP 工具面通过 `RuntimeSessionMcpAccess` 接入 SessionHub；SessionHub 从 `CapabilityState.tool.mcp_servers` 读取 canonical MCP server 列表，并复用既有 direct/relay MCP discovery 与 tool adapter。
   - Executor MCP discovery 增加 `DiscoveredMcpTool` metadata entry，用于 Runtime Gateway 输出 `runtime_name/server_name/tool_name/uses_relay/schema`，但真实执行仍由原 MCP adapter 完成。
   - `mcp.call_tool` 支持按 `runtime_name` 或 `server_name + tool_name` 定位工具；目标工具必须先出现在 capability-filtered surface 中，否则返回 `CapabilityDenied`。
   - `.trellis/spec/backend/runtime-gateway.md` 已补充 Session Runtime Action 场景、输入输出、Capability 契约和错误矩阵。

5. **RuntimeActionToolAdapter 已完成基础件**
   - 已新增 `RuntimeActionToolAdapter` 与 `RuntimeActionToolSpec`，可将选定 Runtime Action 包装为 `AgentTool`。
   - Adapter 只负责将 Agent tool call 转成 `RuntimeInvocationRequest` 并调用 `RuntimeGateway::invoke`；不直接持有底层 provider，不绕过 Gateway policy。
   - Adapter 会把 runtime action / trace 与 provider details 回填到 `AgentToolResult.details`，便于后续审计和调试。
   - 当前尚未默认注入到 session tool surface；是否暴露 generic Runtime Action tool 需等待 capability/surface 策略明确，避免和现有 per-MCP-tool schema 重复暴露。

尚未完成的内容：

1. **Session Runtime Plane 仍有后续增强**
   - `RuntimeGateway::surface_for(Session)` 仍是 action 粒度的粗 surface；具体 MCP tool surface 已由 `mcp.list_tools` 裁决，但还未升级为统一的 async / actor-aware surface API。
   - Session Runtime Action 目前先覆盖 MCP；VFS/context/workflow 等 provider 仍待后续切片。
   - `RuntimeActionToolAdapter` 已有基础件，但还未进入正式 session tool 注入策略。

2. **Runtime Consumers 尚未落地**
   - Canvas Runtime Bridge SDK 尚未接入 Gateway。
   - Agent tool adapter 基础件已完成，但自动注入、manifest 与 UI/Workflow 调用策略尚未接入。
   - Workflow node 直接调用 Runtime Action 尚未接入。
   - 平台 UI 目前仍通过既有 HTTP route 间接触发 Setup Action，尚未有统一 Runtime Bridge client。

3. **Governance / Operation / Audit 仍是后续主线**
   - Invocation trace 已有基础 ID 与错误回填，但还未投影为统一审计事件或 session event。
   - 长任务 Runtime Operation、统一超时/取消、HTTP provider allowlist 与凭据策略尚未实现。
   - `environment.prepare`、`backend.probe`、terminal/bootstrap 类 action 仍待进一步迁移设计。

### Lifecycle Model

- Runtime Gateway 服务本体跟随 API/AppState 生命周期，是常驻的无状态或弱状态调度入口。
- Runtime Surface 原则上跟随 Session 生命周期：
  - Session 是运行时能力面的唯一一等宿主，承载 VFS、MCP servers、CapabilityState、working directory、identity、environment variables、active execution 等上下文。
  - Agent、Canvas、用户会话 UI、Workflow node 等 actor 调用 Gateway 时，必须落到某个 session_id；actor 只是调用身份，不拥有独立运行时上下文。
  - WorkflowRun / LifecycleNode 若需要调用 Runtime Action，应通过其关联 Session 或为节点创建/复用受控 Session，而不是绕开 Session 单独持有工具面。
  - CanvasView 只承载 iframe bridge 的 frame nonce、用户交互校验和临时 UI 连接；CanvasView 不拥有底层权限，必须借由绑定的 session_id 调用。
  - WorkspaceBinding / Project 只提供 Session 装配输入，例如 workspace resolution、mount、MCP preset、capability policy，不作为常规 Runtime Surface 的生命周期宿主。
- 创建 Session 前确实存在少量平台操作，例如 workspace detect、browse directory、MCP transport probe。它们应被标记为 Setup Action：
  - 生命周期跟随一次平台 UI 请求或 workspace binding 创建流程。
  - 只能暴露给 platform_user / environment_setup 等受控 actor。
  - 不参与普通 Canvas / Agent / Workflow runtime surface。
  - 仍然必须走 Runtime Gateway 的统一 invocation/provider/audit 协议，不能保留独立业务执行道路。
  - 若后续需要进入长流程，应创建 Session 或 Runtime Operation，再纳入 Session-bound 管理。
- Runtime Invocation 是单次调用生命周期，必须生成 invocation_id / trace_id；长任务需要升级为可查询的 Runtime Operation，而不是让 HTTP 请求无限悬挂。
- Provider 内部资源按自身语义绑定生命周期：MCP client 可跟随 session/server，terminal process 跟随 terminal/session，workspace prepare 应按 workspace binding/run 具备幂等语义。

## Acceptance Criteria

- [x] 有明确的 Runtime Gateway / Action / Invocation / Provider / Surface / Actor 类型设计。
- [x] Canvas 不再是协议命名中心，只作为 Runtime Client 之一。
- [x] Relay 被定位为 provider/transport 内部实现，不直接暴露给上层 runtime。
- [x] Gateway 调用经过 CapabilityState 或等价 policy 裁决。
- [x] 至少完成 `mcp.call_tool` 的端到端设计，明确 direct MCP 与 relay MCP 的路由方式。
- [x] 至少完成一个非 Canvas 调用方场景设计，例如 Agent tool adapter 或 environment/workspace prepare。
- [x] 明确 Runtime Scope 与 Gateway/Surface/Invocation/Provider 资源生命周期关系。
- [x] 明确 Session 是 Runtime Surface 的唯一一等宿主，Project / WorkspaceBinding / WorkflowRun / CanvasView 不形成平行运行时上下文。
- [x] 明确 Setup Action 纳入 Runtime Gateway 统一协议，但不暴露给普通 Canvas / Agent / Workflow runtime surface。
- [x] 现有 workspace detect、browse directory、MCP probe 等入口有明确迁移策略：API 层只保留 thin route，业务执行收口到 Gateway/provider。
- [ ] 明确 HTTP action 的边界、allowlist、凭据策略、超时和响应大小限制。
- [ ] 所有 invocation 均具备 trace/audit 语义。

## Out of Scope

- 第一阶段不暴露裸 relay command 给 Canvas 或任意 iframe。
- 第一阶段不允许 Canvas 直接执行任意 HTTP fetch。
- 第一阶段不替换现有 Relay 协议，只在其上增加语义层。
- 第一阶段不重写现有 Capability Pipeline。
- 第一阶段不要求完成所有 provider，只需先打通 MVP provider。
- 第一阶段不重写已有 local workspace prepare / detect / terminal 实现，只做语义统一和调用门面收口。
- 第一阶段不保留新的平行业务通路；新增或迁移的 setup/session action 都应以 Runtime Gateway provider 作为唯一业务实现入口。

## Risks And Design Constraints

- Agent 可写 Canvas，因此 Canvas 文件本身不能成为授权来源。
- iframe runtime 必须视为不可信代码，不能直接拥有本机或平台敏感能力。
- `allow-scripts + allow-same-origin` 的 iframe sandbox 策略在引入敏感数据后需要重新评估。
- HTTP provider 必须避免 SSRF、内网探测、隐式 cookie、凭据泄露和大响应注入。
- 远程 CDN import 与敏感 runtime result 同时存在时，需要明确数据外传风险。
- 环境准备类 action 需要幂等、可重试、可审计，不能只当普通工具调用处理。

## Open Questions

- Runtime Gateway 第一版 MVP 是同时落地 Session Runtime Action + Setup Action 的协议骨架，还是先实现 Session Runtime Action 再迁移 Setup Action thin route？
- `context.read/search` 是否需要进入第一阶段，还是等 Canvas/Workflow 有明确上下文刷新场景后再纳入？

## Technical Notes

- 相关 Canvas 类型：
  - `crates/agentdash-domain/src/canvas/value_objects.rs`
  - `crates/agentdash-application/src/canvas/runtime.rs`
  - `frontend/src/features/canvas-panel/CanvasRuntimePreview.tsx`
- 相关 Relay / MCP 基础设施：
  - `crates/agentdash-relay/src/protocol.rs`
  - `crates/agentdash-api/src/relay/mcp_relay_impl.rs`
  - `crates/agentdash-local/src/handlers/mcp_relay.rs`
  - `crates/agentdash-executor/src/mcp/relay.rs`
- 相关环境/工作区本机能力：
  - `crates/agentdash-local/src/workspace_prepare.rs`
  - `crates/agentdash-local/src/handlers/prompt.rs`
  - `crates/agentdash-local/src/handlers/workspace.rs`
  - `crates/agentdash-api/src/routes/terminals.rs`
- 相关 Capability / Session 工具表面：
  - `crates/agentdash-application/src/session/hub/tool_builder.rs`
  - `crates/agentdash-spi/src/platform/tool_capability.rs`
  - `.trellis/spec/backend/capability/tool-capability-pipeline.md`
  - `.trellis/spec/backend/vfs/vfs-access.md`

## Definition of Done

- PRD 完成并经确认。
- 若进入实现，需补充 implement.jsonl / check.jsonl，包含必要 spec、PRD 和相关代码上下文。
- 实现后需通过相关 Rust/前端测试，并补充必要的跨层契约文档。
