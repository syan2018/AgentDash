# AgentRun 与 RuntimeSession 层级关系收束设计

## Goal

明确 AgentRun、AgentFrame、RuntimeSession、Session construction、CapabilityState 之间的目标层级关系，并形成一套可执行的收束路线。MCP declaration 命名收束是第一阶段切口，任务目标是确定从 Session 本体向 AgentRun/AgentFrame 本体迁移后的正确模型。

最终希望达成的心智模型：

```text
AgentRun = 用户可继续交互的执行身份
AgentFrame revision = AgentRun 当前可执行运行面的事实源
RuntimeSession = 执行 AgentFrame 后产生的 runtime adapter / trace / connector lifecycle
Session persistence = runtime trace、events、commands、terminal effects、lineage 的保存层
```

## Background

本项目处于预研阶段，允许优先选择最正确的模型形态，不需要为旧 API 或旧数据库字段保留兼容层。若后续实现需要 schema 调整，按项目 migration 机制处理。

近期 AgentRun MCP runtime binding 和 AgentRun workspace 相关工作已经把用户侧入口推向 AgentRun/AgentFrame，但部分运行面投影仍需要继续收束到 AgentFrame revision 事实源：

- `RuntimeMcpServerDeclaration` 是 MCP runtime-resolved declaration 的 canonical 名称。它没有 runtime-session ownership 字段，会被写入 `CapabilityState.tool.mcp_servers`、`SessionConstructionPlan.projections.mcp_servers`、`AgentFrame.mcp_surface_json`。
- `McpRuntimeBindingContext` 表达 MCP runtime binding resolver 使用的 final VFS / workspace facts。
- `RuntimeSessionMcpAccess`、`RuntimeSessionRefDto`、`delivery_runtime_session_id` 等名字仍然有存在价值，因为它们描述的是 runtime session trace / connector 生命周期。
- 目前真正需要判断的是：哪些 Session 名字只是旧命名，哪些 Session 结构还在承担过多控制面职责。

## Confirmed Intent From Discussion

- “标准收束”只能修正命名和边界表达，仍保留 `CapabilityState`、`SessionConstructionPlan`、`AgentFrame` 多处同步 runtime surface 的结构。
- “激进收束”更接近目标态：让 `AgentFrame revision` 成为 AgentRun 当前可执行运行面的唯一事实源，RuntimeSession 回到执行适配器和 trace 的位置。
- MCP declaration 是暴露问题的触发点，但完整收束会涉及整个 session 与 agent 层级关系。
- 目标不是删除所有 Session 概念，而是把 Session 压回 runtime trace / transport / connector lifecycle 的边界内。

## Requirements

- 梳理当前 AgentRun、AgentFrame、RuntimeSession、SessionConstructionPlan、CapabilityState、RuntimeGateway、MCP runtime binding 的职责边界。
- 明确目标本体论：AgentRun 负责 continuity identity，AgentFrame revision 负责 executable surface truth，RuntimeSession 负责 execution trace / adapter。
- 区分应保留的 RuntimeSession 概念与应迁移的 Session-first 事实源职责。
- 评估 `RuntimeMcpServerDeclaration`、`RuntimeMcpServer`、`McpRuntimeBindingContext`、`SessionRuntimeControl*`、`SessionConstructionPlan.projections`、`CapabilityState.tool.mcp_servers` 等命名和结构的归属。
- 形成一条分阶段收束路线，避免一次性混合命名、事实源、API、前端、持久化和 migration 改动。
- 后续实现方案应优先追求正确模型，不引入兼容适配层；如需要数据库调整，纳入 migration 设计。
- 文档记录应表达目标模型和选择原因，避免围绕历史实现展开负面清单。

## Acceptance Criteria

- [ ] `design.md` 明确当前模型、目标模型、标准收束与激进收束的区别。
- [ ] `design.md` 给出 AgentRun / AgentFrame / RuntimeSession / CapabilityState / SessionConstructionPlan 的目标职责表。
- [ ] `design.md` 明确 MCP declaration 在目标模型中的 canonical 名称和事实源位置。
- [ ] `implement.md` 给出分阶段执行路线，每阶段有目标、主要改动面、验证重点和回滚关注点。
- [ ] 任务上下文索引覆盖 session startup、execution frame、runtime gateway、capability pipeline、MCP runtime binding、lifecycle control-plane 既有资料。
- [ ] 仍需用户决策的问题被列为 open questions，并附推荐答案与取舍。
- [ ] 任务保持 planning 状态，待设计审阅后再进入实现。

## Open Questions

1. 是否正式把“激进收束”定为长期目标状态？
   推荐答案：是。标准收束作为阶段 1 的低风险落地方式，但设计文档应以激进收束为目标态。

2. 第一批实现是否只做 MCP declaration / runtime binding context 命名收束？
   推荐答案：是。先固定 MCP declaration 与 binding context 词汇，为后续事实源迁移铺路。

3. `CapabilityState.tool.mcp_servers` 的目标角色是保留为 frame draft 字段，还是最终从长期 state 中移除？
   推荐答案：先降级为 `FrameSurfaceDraft` 的中间结果，等 runtime launch 读 AgentFrame surface 后再评估是否从长期 state 中移除。

4. AgentRun workspace 是否应拥有独立的 control/action DTO？
   推荐答案：是。`SessionRuntimeControl*` 可保留给 runtime trace/detail 入口，AgentRun workspace 应表达 AgentRun command/control 语义。
