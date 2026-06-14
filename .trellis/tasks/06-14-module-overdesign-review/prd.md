# 模块过度设计重新评估

## Goal

对当前项目进行一次新的跨模块架构 review，重点识别过厚模块、过度抽象、事实源分散、跨层耦合和可以清理收敛的边界问题，并在 review 结论基础上执行第一轮高优先级清理收束。

本任务不沿用既有 `architecture-backlog-followup` 任务。`overdesign-review.md` 是本任务的父级评估基线；后续实现直接在本任务下收束，不再额外创建新的父任务。

## Requirements

- 使用 subagent 并行审查多个模块面，主会话负责统一口径、二次核对和综合排序。
- `Lifecycle` / `AgentRun` 相关链路作为重度 review 对象，覆盖 backend、frontend、runtime projection、session feed、workspace/run 控制面。
- 识别重点放在“当前预研项目应直接修正到正确形态”的问题，不提出长期兼容层、回退方案或保守迁移方案。
- 每个问题必须给出真实代码证据，至少包含文件路径和可定位的函数、类型、模块或状态来源。
- 对每个问题判断是否属于过度设计、模块过厚、重复事实源、抽象层泄漏、横向耦合或命名/职责漂移。
- 输出清理建议时优先说明目标边界和为什么这么收敛，不把过去错误实现当作文档重点。
- 第一轮实现只处理事实源正确性和控制面收束，不展开低优先级装配层大拆分。
- 第一轮并行工作流控制在三条以内：Lifecycle runtime truth source、AgentRun control surface、Permission / contract capability surface。
- VFS / Local / Extension 的装配层瘦身暂作为后续候选，不进入第一轮实现。

## Acceptance Criteria

- [x] 形成一份总评估报告，覆盖主要 backend crate、frontend feature、contracts/shared、本机 runtime/relay/extension 边界。
- [x] `Lifecycle` / `AgentRun` 至少各有一节深入结论，包含主要耦合来源、过厚位置、事实源归属问题和建议拆分方向。
- [x] 每个结论都包含证据路径、影响面、建议优先级和可执行的清理方向。
- [x] subagent 产物落在本任务 `research/` 目录，便于复查。
- [x] 不修改业务代码；如发现需要立即修复的问题，只记录为后续任务候选。
- [x] Lifecycle cancel 通过 orchestration reducer materialize，Task projection 不再从缺失关系推断 Failed。
- [x] AgentRun command/control surface 收敛为单一 workspace conversation/mailbox 投影，RuntimeSession runtime-control 不再复制 AgentRun action/mailbox 控制面。
- [x] PermissionGrant 成为 capability grant 的唯一授权事实源，pending grant query 和 typed permission DTO 不再依赖 companion JSON 或 `JsonValue` 核心字段。
- [x] 第一轮实现完成后运行针对性后端/frontend 检查，并记录未处理的 VFS / Local / Extension 后续任务候选。

## Notes

- Review / research 阶段已经提交为 `bf18fc30 docs(trellis): 记录模块过度设计评估`。
- 后续实现阶段沿用本任务，不另建父任务。
