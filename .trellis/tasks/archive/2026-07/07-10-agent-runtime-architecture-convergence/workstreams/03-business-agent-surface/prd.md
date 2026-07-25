# 业务 Agent Surface、Hook 与 Platform Tool Broker

## Goal

把 AgentFrame、Capability Pack、ContextFrame、tools、MCP、VFS、Skill 与 Hook policy 收敛为 protocol-neutral 业务模块，并为外部 Agent 建立真实 callable tool/hook delivery channel。

## Depends On

- `01-runtime-contract`

## Parent Design

- `../../design.md` 第 7、10、11 节
- `../../research/external-agent-feature-integration-matrix.md`
- `../../research/hook-runtime-layering.md`

## Requirements

- 定义 ContextEnvelope、ToolCatalogRevision、WorkspaceRequirement 与 surface compatibility。
- 将 Capability Pack 展开为 Skill/Tool/MCP/Workflow/Permission/Hook/Context contributions。
- 将 workflow/project/story/task/run sources编译为revisioned HookPlan与逐trigger HookRequirement，不让Executor解析业务policy。
- 将 tool assembly、context selection/delivery plan 从 application runtime session 迁出。
- 建立 direct host callback 与 session-scoped MCP Tool Broker。
- 每次 tool call 校验 binding generation、capability、permission、VFS、credential、idempotency 与 cancel/timeout。
- 区分 outer hooks、broker hooks、inner-loop hooks、same-loop mailbox 与 next-turn boundary。
- 定义 HostLifecycle、ToolBroker、DriverCallback、NativeArtifact、Observed、SteerApproximation delivery，并保持语义强度可查询。
- AgentFrame revision持有HookPlan ref/digest/requirements；plan变化先形成新frame/surface revision，再通过binding/turn adoption，不直接替换live cache。
- 保留Rhai rule/preset业务能力，但脚本evaluator、sandbox与command process作为infrastructure mechanism注入。
- `PromptOnly` 不作为 capability；required Pack contribution缺失时 typed incompatible。

## Acceptance Criteria

- [x] 外部 driver 不接收 `DynAgentTool` trait object、application delegate 或本地 VFS runtime object。
- [x] Tool schema/call/result保留 Thread/Turn/Item/Tool/Binding generation 坐标。
- [x] Brokered tool 在执行点重做 policy/VFS检查，duplicate call不重复副作用。
- [x] MCP secret只在local materialization boundary解引用。
- [x] PromptOnly/Observed/HostAdaptedBoundary 不会被UI误报为exact/native。
- [x] Capability Pack required contribution缺失不会静默部分启用。
- [x] 需要block/rewrite/same-loop decision的Hook不会被Observed/SteerApproximation误判为兼容。
- [x] 同一HookDefinition在Host/Broker/Driver之间解析为唯一route，不会重复执行。
