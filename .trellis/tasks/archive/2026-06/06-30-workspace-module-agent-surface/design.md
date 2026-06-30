# Workspace Module Agent Surface 深模块评估 - Design

## Problem

当前 Workspace Module Agent-facing tool surface 的外部 seam 是多个 `AgentTool`，但 implementation ownership 分散在 tool 构造器、visibility source、operation runtime source、Canvas helper、RuntimeGateway catalog/channel 和 presentation event 中。调用和测试需要理解太多 runtime facts，module depth 不足。

## Candidate Interface

候选 deep module：

```text
WorkspaceModuleAgentSurface::resolve(context) -> WorkspaceModuleSurface
WorkspaceModuleAgentSurface::execute(command) -> WorkspaceModuleOperationOutcome
```

`context` 由 Agent runtime adapter 提供，包含 project、current user、delivery runtime session、agent id、effective capability view、VFS / backend anchor 等输入。deep module 内部决定 visibility、operation readiness、runtime action catalog、Canvas access、channel readiness 和 presentation side effect。

本任务采用“正确 interface 优先”的设计口径：保留 `workspace_module_list` / `workspace_module_describe` / `workspace_module_operate` / `workspace_module_invoke` / `workspace_module_present` 五个 Agent-facing tool 名作为稳定操作语言，原因是 Canvas skill、跨层 contract 与 Agent discoverability 已经围绕这些名称形成。五个 tool 不是 deep module interface；它们只是 `WorkspaceModuleAgentSurface::resolve/execute` 外侧的 thin adapter。

用户已显著接受“删除旧处理面”作为完成标准。设计必须把旧 ownership 的去向写清楚：迁移到 deep module、降级为 thin adapter，或删除。新增 facade 但旧 helper 继续拥有规则不算完成。

## Ownership Sketch

- `WorkspaceModuleRuntimeToolProvider`：只负责从 `ExecutionContext` 创建 surface context，并装配 thin AgentTool adapters；不继续拥有 operation ownership。
- `WorkspaceModuleAgentSurface`：拥有 visible descriptor resolution、operation catalog、Canvas host command、RuntimeGateway/channel invocation、presentation notification。
- AgentTool adapters：保留现有五个 tool 名，只负责 JSON schema、input validation、调用 surface command、投影 `AgentToolResult`。
- `runtime_bridge.rs`：继续提供 runtime bridge handle / delayed injection primitive，但不拥有 business operation rules。

## Completion Standard

- `WorkspaceModuleVisibilitySource` / `WorkspaceModuleOperationRuntimeSource` 这类旧处理面不能继续作为业务规则 owner。
- `workspace_module_*` AgentTool 保留为 thin adapter；tool 名稳定不是兼容层，而是 Agent 操作语言。
- Canvas / Extension channel / presentation outcome 必须有单一 owner。
- 测试必须直接覆盖 deep module interface，而不是只通过旧 AgentTool JSON 间接覆盖。

## Evaluation Focus

1. 五个现有 tool 如何一一映射到 `resolve/execute`，并保持 adapter 足够薄。
2. 是否一次性实现完整 surface：`list` / `describe` / operation catalog / `invoke` / `present`。
3. 若分阶段，如何保证旧浅 interface 不成为长期案底。
4. Canvas host operation 与 Extension channel 是否共用 `execute(command)` outcome。
5. 现有 tests 如何迁移到 typed surface。

## Risk

- `invoke` / `present` 有 runtime surface update 和 notification side effect，一次性收束需要更完整的测试矩阵。
- 如果 interface 只包 read surface，后续可能出现 read/write 两个 shallow seams；这与本任务的极端收束目标冲突。
- RuntimeGateway channel 和 Canvas host operation 的 error taxonomy 需要统一，否则 outcome 会变成 `serde_json::Value` 搬运。

## Validation Shape

- Rust unit tests：surface resolution、descriptor visibility、operation readiness。
- AgentTool adapter tests：schema、invalid input、result projection。
- Targeted cargo tests：`cargo test -p agentdash-workspace-module workspace_module`。
