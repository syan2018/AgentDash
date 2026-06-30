# 设计评估方案

## Review Shape

本任务产物是 design review package，而不是代码实现。每个 D 项按同一模板评估：

1. Boundary: 问题属于哪个 owner、哪个 contract、哪个运行边界。
2. Evidence: 当前代码中哪些路径证明存在分叉或过厚 owner。
3. Convergence Direction: 正确 owner 与数据/控制流应该如何收束。
4. Decision State:
   - `self-decided`: 代码/spec 已足够支撑唯一设计；
   - `user-decision-required`: 多个长期产品/架构方向都成立，需要用户选择；
   - `blocked-by-code-shape`: 需要先完成其它已知设计或代码收束。
5. Implementation Slices: 后续可拆的实现步骤与验证策略。

## Suggested Order

从较局部到较复杂推进：

1. D12 Relay prompt typed payload
2. D7 RuntimeGateway dynamic extension action discovery owner
3. D8 Runtime action availability 三层 owner 收束
4. D1 AgentRun visible capability / admission production boundary
5. D5 Command availability resolver / command policy 统一
6. D6 AgentRuntimeDelegate 拆 delegate set
7. D9 VFS per-mount/path authorization model
8. D10 WorkspacePlacementService directory fact transaction
9. D11 Desktop profile/claim/settings 下沉 agentdash-local
10. D4 Launch command/source 单一模型
11. D3 CompanionGate resolver 与 delivery adapters 拆分
12. D2 LifecycleDispatchService 内部 owner 拆分

排序理由：

- D12/D7/D8 局部 contract 与 runtime catalog owner 较清楚，适合先收敛设计语言。
- D1/D5/D6 处于 AgentRun runtime boundary，中等复杂但 quick convergence 已提供新事实。
- D9/D10/D11 跨 VFS / placement / local runtime，但可按 owner 拆清。
- D4/D3/D2 影响启动链路、gate 状态机和 lifecycle transaction，是最后综合项。

## Output Documents

- `design-review.md`: D1-D12 综合评估与推荐方案。
- `decision-points.md`: 只记录需要用户严肃决策的项。
- `implementation-slices.md`: 后续可拆实现任务建议，按依赖顺序组织。
- `research/*.md`: subagent research 原始证据，可按 topic 分文件。

## Subagent Strategy

使用 `trellis-research` 并行做设计 research，不让 worker 直接修改代码。建议分组：

- Runtime surfaces: D1 / D5 / D6 / D12。
- Extension and action availability: D7 / D8。
- VFS placement local runtime: D9 / D10 / D11。
- Orchestration and gate: D2 / D3 / D4。

每个 research worker 必须聚焦“如何删除错误路径或收束 owner”，而不是只提出新增 service。

## Decision Boundary

主会话可自行决定：

- 已有 spec 明确 owner 的 contract 形状；
- 错误路径明显是重复实现或历史 residual；
- 推荐方案只是把已有事实源变成唯一 owner。

需要记录给用户决定：

- 两个以上长期 owner 都合理，且会影响后续 product language；
- 改变可见 workflow/gate/launch 语义；
- 引入新的公共 contract shape，且迁移代价和产品收益需要权衡。
