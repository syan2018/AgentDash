# Session identity boundary cleanup

## Goal

清理 `session_id` / `runtime_session_id` 泄漏到外围业务模块的问题，建立明确的身份边界：session 只属于会话/投递/连接层内部实现，不能作为 Workspace Module、Canvas、Workflow、VFS surface、Terminal、Permission、Mailbox 等其它事务的业务关联键。

本任务不把问题理解为“字段名不好看”，而是把 session 从跨聚合事实源中剥离。外围业务应使用自己的业务身份，例如 `AgentRun`、`AgentRun delivery`、`Workflow node run`、`Canvas mount`、`Workspace surface`、`Backend target`、`Terminal instance` 或明确的 runtime delivery abstraction；session id 只能在会话层 adapter 内部映射到这些业务身份。

## Confirmed Facts

- 当前 `agentdash-workspace-module` 直接消费 `ExecutionContext`，从中解析 `delivery_runtime_session_id`，并用该值做 module visibility、Canvas surface update、presentation notification、Canvas diagnostics 和 extension invoke 的路由坐标。
- `agentdash-workspace-module` 直接依赖 `agentdash-application-runtime-gateway` 与 `agentdash-application-vfs`，并直接构造 `RuntimeActor::AgentSession`、`RuntimeContext::Session`、`ExtensionRuntimeChannelInvokeRequest { session_id, ... }` 和 `PlatformEvent::SessionMetaUpdate`。
- `canvas.inspect_render_state` / `canvas.get_interaction_state` 当前通过 `RuntimeSessionExecutionAnchorRepository::find_by_session` 从 runtime session 反查 `run_id / agent_id`，这是 session 作为跨事务关联键的直接表现。
- `agentdash-domain::agent_run_mailbox` 仍含 `runtime_session_id` 字段，说明 AgentRun control-plane 事实中仍混入 delivery session trace。
- API/route 层仍存在 session-shaped 外围入口或 DTO，例如 Canvas DTO `session_id`、Workspace Module request `runtime_session_id`、VFS surface `SessionRuntime` source、Workflow route provenance `runtime_session_id`、Terminal route `/api/sessions/:session_id/terminals`。
- Companion、hooks、permission、task context builder 等 application 模块存在通过 runtime session 选择 current delivery、注入 notification、查询 active workflow 或记录 permission provenance 的路径。
- 会话层本身仍需要 session id：runtime session persistence、Backbone envelope、relay/local connector、live executor session、stream/event delivery、terminal stream transport 等仍是允许使用 session id 的区域。

## Requirements

- 定义 session identity 的允许边界：只有会话/投递/连接层可以持有、持久化、查询和传输 session id。
- 定义外围业务禁区：Workspace Module、Canvas、AgentRun mailbox、Workflow/Lifecycle business、VFS surface business、Terminal business、Permission grant business、Hook/Companion business 不得以 session id 作为业务入参、DTO 字段、repository 查询键、domain payload 字段或用户/Agent 可见输出。
- 建立替代身份模型：
  - AgentRun 相关操作以 `run_id + agent_id`、`AgentRunDeliveryRef` 或 `AgentRunRuntimeSurfaceTarget` 为主语。
  - Workspace Module/Canvas 相关操作以 AgentRun delivery context、Canvas mount id、module id 和 workspace surface target 为主语。
  - Workflow/Companion 相关操作以 lifecycle run、node run、frame、dispatch/request id 为主语。
  - VFS/Terminal 相关操作以 workspace surface / backend target / terminal instance 为主语，session id 只在 transport adapter 内部解析。
- 将 session-to-business 的反查集中到会话 adapter 边界，不允许外围模块直接持有 `RuntimeSessionExecutionAnchorRepository` 或调用 `find_by_session`。
- 将 RuntimeGateway 的 session-shaped actor/context 组装留在 runtime gateway adapter 内部；业务模块只表达 operation/invocation intent。
- 将 `SessionMetaUpdate` 这种 session eventing transport 留在 presentation adapter 内部；业务模块只表达 presentation/notification intent。
- 清理 API 与 generated contract 中暴露给前端/Canvas/Agent 的 session-shaped 字段，改为 AgentRun/workspace/module/canvas/terminal 业务身份。
- 保留必要的 session-layer internal trace，但其字段名、类型和文档必须明确为 adapter-only，不进入 domain business contract。
- 更新 spec，写明 session id 的使用范围与外围模块替代身份。

## Acceptance Criteria

- [ ] `agentdash-workspace-module` 不再依赖 `agentdash-spi::ExecutionContext`、`agentdash-application-runtime-gateway`、`agentdash-application-vfs`，也不再构造 `RuntimeContext::Session`、`RuntimeActor::AgentSession`、`SessionMetaUpdate`。
- [ ] `agentdash-workspace-module` 内不再出现 `session_id` / `runtime_session_id` / `delivery_runtime_session_id` 作为业务字段、函数参数或测试 fixture 语义；允许的测试字符串也必须围绕 AgentRun/workspace/module 身份命名。
- [ ] Canvas runtime snapshot、Canvas DTO、Canvas SDK/API 不向 Canvas source 或前端业务面暴露 session id。
- [ ] AgentRun mailbox domain contract 不再以 `runtime_session_id` 作为 message/source/command 的业务字段；如需审计 delivery trace，使用 adapter-only trace ref 或 AgentRun delivery identity。
- [ ] Workspace Module、Canvas diagnostics、Canvas interaction state、submit-to-Agent 均以 AgentRun 关联为事实源，不通过 runtime session 反查。
- [ ] Workflow/Lifecycle、Companion、Hook、Permission、Task context builder 中的外围业务路径不再把 runtime session id 当成跨事务关联键；必要的 live delivery 交互通过 AgentRun delivery port 或 runtime adapter 完成。
- [ ] API/generated contracts 中除 session 专属资源外，不再要求客户端提交 session id 来操作 Canvas、Workspace Module、Workflow、VFS surface 或 Terminal 业务。
- [ ] `RuntimeSessionExecutionAnchorRepository::find_by_session` 不再被 session adapter 以外的外围业务模块直接调用。
- [ ] 新增或更新 backend spec，明确 session identity isolation 规则、允许层、禁止层和替代身份。
- [ ] Rust check/test、contract generation/check、前端 typecheck 和受影响前端测试通过。

## Out of Scope

- 不要求一次性移除 session persistence、Backbone protocol、relay/local connector、live executor stream 中的 session id；这些属于会话/投递层内部事实。
- 不为旧 API/字段保留兼容层；项目未上线，contract 可直接改到正确形态。
- 不以“重命名字段”为交付目标；只有真实移除跨事务 session 关联才算完成。

## Open Questions

- 第一轮实现是否必须覆盖 Terminal/VFS/Workflow/Companion/Permission 的所有外围 session 泄漏，还是先完成 Workspace Module/Canvas/AgentRun mailbox 主链路，再用同一任务继续推进剩余路径？
