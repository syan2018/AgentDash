# Agent runtime delegate set 收束

## Goal

实现 design backlog Slice 6 / D6：把 `AgentRuntimeDelegate` 从一个覆盖 compaction、context transform、tool policy、turn boundary、provider observer 的 broad trait 收束为显式 delegate facet set。目标不是新增 hook feature，而是删除 mailbox/admission 这类 adapter 被迫实现和转发无关方法的旧路径，让 launch/prepared-turn 的 delegate 组合关系可读、可测、可替换。

## Requirements

- `agentdash-agent-types` 必须提供明确的 runtime delegate facets：
  - `RuntimeCompactionDelegate`
  - `RuntimeContextTransformDelegate`
  - `RuntimeToolPolicyDelegate`
  - `RuntimeTurnBoundaryDelegate`
  - `RuntimeProviderObserverDelegate`
- Agent loop 调用点必须按 facet 调用，不再假设一个实现者同时拥有所有 runtime lifecycle concern。
- `HookRuntimeDelegate` 可以继续实现全量 facet，因为 hook runtime 确实覆盖 compaction、context transform、tool policy、turn boundary 和 provider observer。
- AgentRun admission adapter 只能实现 tool policy facet，D1 的 admission short-circuit 必须保留，并且 admission deny 仍然发生在 inner hook tool policy 之前。
- AgentRun mailbox adapter 只能实现 turn boundary facet；它不再通过 broad forwarding wrapper 代理 compaction/context/tool/provider observer。
- Launch planner / prepared-turn composition 必须显式构造 facet set：hook facets、admission tool policy facet、mailbox turn-boundary facet 以清晰顺序组合。
- 清理旧问题优先于添加 feature：不得通过再包一层 broad delegate 或 compatibility adapter 来隐藏旧分叉；若为了迁移保留过渡 helper，必须只在本切片内使用并尽快删除 broad forwarding 语义。
- Subagent 执行约束：实现 worker 不跑大规模 Rust 编译或 broad suites；允许 scoped `rg`、format、小型定向 Rust tests。最终编译/集成校验由 check 阶段统一决定。

## Acceptance Criteria

- [x] `AgentLoopConfig` 使用 delegate set 或等价显式 facet 结构；streaming/tool/turn 调用点按 facet 访问。
- [x] `AgentRunMailboxRuntimeDelegate` 不再实现或转发 compaction、context transform、tool policy、provider observer 方法；mailbox 只处理 after_turn / before_stop 相关 turn boundary 行为。
- [x] `AgentRunAdmissionRuntimeDelegate` 不再实现或转发非 tool-policy 方法；它只负责 admission 以及 inner tool policy 顺序。
- [x] `HookRuntimeDelegate` 的各 concern 映射到对应 facets，既有 hook 行为和 tests 保持。
- [x] Launch planner 的 delegate composition 能从代码上看出 hook、admission、mailbox 的 facet 装配顺序。
- [x] Tests 覆盖 admission deny before inner hook、mailbox turn-boundary behavior、hook compaction/context/tool/provider observer 至少各一个 facet 入口。
- [x] Static check 证明 mailbox/admission broad forwarding wrapper 已删除或不再作为生产组合路径。
- [x] Specs 更新记录 runtime delegate facet owner 规则。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d6-agentruntimedelegate-delegate-set`.
- This is Slice 6 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.
