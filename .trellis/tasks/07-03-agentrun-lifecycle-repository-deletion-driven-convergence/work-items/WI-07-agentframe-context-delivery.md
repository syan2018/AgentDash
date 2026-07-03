# WI-07 AgentFrame ContextDelivery

## Objective

重建 AgentFrame 内部 surface 和 ContextDelivery 输入事实，使能力与认知状态只锚定在 AgentFrame revision，而 ContextFrame emission 只来自 accepted input fact。

## Decisions

D-011, D-012, D-014, D-017

## Research Inputs

- `research/agentframe-context-surface.md`
- `research/database-physical-design.md`

## Scope

- 设计 AgentFrame canonical surface：capability、VFS、MCP、executor、visible workspace refs、context surface。
- 删除 historical frame revision 原地 append visible refs 的路径。
- 删除 capability/VFS/MCP 多列覆盖式双源写入。
- 明确 current/applied frame binding，不再用 highest revision 直接代表 runtime current truth。
- 引入 `ContextDeliveryRecord` 或等价 accepted input fact。
- 让 ContextFrame emission、connector input、runtime turn、applied frame 可互相追溯。

## Out Of Scope

- accepted boundary 集成交给 WI-05。
- current delivery selection 交给 WI-06。
- contracts/frontend projection 展示交给 WI-09。

## Dependencies

依赖 WI-00 inventory 和 WI-06 delivery binding 方向。

## Implementation Notes

- `AgentFrameRepository` 可以保留为 revision surface store。
- 细粒度 mutation helper 应改为 frame aggregate update 或 frame surface command port。
- 如果为了查询保留物理列，应作为 generated projection 或只读缓存，不成为独立写源。

## Acceptance

- AgentFrame revision append-only，历史 revision 不被 runtime path 原地修改。
- Agent capability / cognition 判断只从 AgentFrame effective surface 得出。
- ContextFrame 不再由 launch、commit、transition、compaction 多处独立构造。
- applied frame 和 launch frame 的关系可审计。

## Validation

- frame builder / repository roundtrip 测试覆盖 append-only revision。
- ContextDeliveryRecord 到 ContextFrame emission 的映射测试。
- `rg "append_visible|get_current\\(agent_id\\)"` 清点并替换不符合目标的路径。
