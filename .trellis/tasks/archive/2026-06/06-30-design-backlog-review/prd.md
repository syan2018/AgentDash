# 模块设计 backlog 完整评估

## Goal

承接 `.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md`，对 D1-D12 从简单到复杂完成完整设计评估。目标不是马上实现所有设计项，而是把每个 Design backlog item 收束成清晰的 owner、contract、可执行方案和决策状态。

## Requirements

- 覆盖 D1-D12 全部设计项，不遗漏 quick convergence 后新增的 residual。
- 从较局部、owner 边界较清晰的项开始，逐步推进到 Lifecycle / Gate / Launch 这类高耦合项。
- 每个设计项必须给出：
  - 问题边界与代码证据；
  - 当前错误路径或概念分叉如何被收束；
  - 推荐 owner 与 contract shape；
  - 实施顺序、迁移/contract 影响和验证方式；
  - 决策状态：`self-decided`、`user-decision-required`、`blocked-by-code-shape`。
- 对不需要严肃产品/架构取舍的项，直接给出可执行设计方案。
- 对需要用户决策的项，记录明确决策点、备选方案、推荐方案、权衡和后续可解锁工作。
- 不创建 D1-D12 的 Trellis child tasks；本任务先完成 review/design package，后续是否拆实现任务由设计结论决定。
- 派发 subagents 时强调第一性收束：优先清理旧错误路径、事实源分叉和 owner 漂移，不以新增并行路径替代真正收束。
- research/check worker 避免大规模 Rust 编译；设计评估以代码证据、spec、targeted query 和必要 small checks 为主。

## Design Items

- D1. AgentRun visible capability 与 admission decision 的完整生产边界。
- D2. LifecycleDispatchService 内部 owner 拆分。
- D3. CompanionGate resolver 与 delivery adapters 拆分。
- D4. Launch command/source 单一模型。
- D5. Command availability resolver / command policy 统一。
- D6. AgentRuntimeDelegate 拆 delegate set。
- D7. RuntimeGateway dynamic extension action discovery owner。
- D8. Runtime action availability 三层 owner 收束。
- D9. VFS per-mount/path authorization model。
- D10. WorkspacePlacementService 统一 directory fact transaction。
- D11. Desktop profile/claim/settings 下沉 agentdash-local。
- D12. Relay prompt typed payload。

## Acceptance Criteria

- [x] 创建完整设计评估文档，覆盖 D1-D12。
- [x] 每个设计项都有代码证据、owner 结论、contract shape、实施步骤和验证策略。
- [x] 需要用户决策的项有独立决策记录，包含选项、推荐方案和取舍。
- [x] 不需要用户决策的项给出可直接拆实现任务的方案。
- [x] quick convergence 后的 residual 明确反映到 D1/D7 等相关设计项。
- [x] 用 subagents 并行完成独立设计 research，并由主会话综合成一份一致的设计 package。
- [x] 输出后续实现拆分建议，避免把设计项直接变成一组未经收束的子任务。
