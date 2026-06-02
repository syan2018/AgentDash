# Lifecycle 控制面重构结构性审计

## Goal

基于 `06-01-lifecycle-control-plane-concept-alignment` 后续大型重构的扫描结果，创建一份结构性审计任务：不是继续证明“旧路径已经删掉多少”，而是把每一个暴露出的未覆盖点还原为背后的模型内聚性、边界归属、事实源 ownership 与跨层耦合问题。

本任务的核心目标是找出当前新路径是否已经足够高内聚、低耦合、可维护；凡是重构没有直接覆盖、仍靠旧 session/runtime/route-local/view stitching 补洞的地方，都需要判断它是否说明目标模型本身还没有被正确封装。

## Requirements

- 先记录当前扫描已经暴露出的原始问题 checklist，不在 checklist 中提前粉饰、归并或解释。
- 按 checklist 顺序逐项分析，不把多个问题混成一团，也不把“清理旧字段”伪装成架构完成。
- 每个问题必须回答：
  - 暴露出的现象是什么。
  - 是否隐藏模型过度耦合或边界不清。
  - 哪个事实源、事务边界、查询边界或 projection 边界不够清楚。
  - 最合适的封装应是什么。
  - 后续应如何彻底解决，而不是机械打补丁。
- 分析优先级必须以结构风险排序：运行闭环正确性、统一入口、事实源 ownership、跨层契约、前端 projection、验证闭环。
- 本任务只进入规划与结构性分析，不直接实现代码修复。

## Acceptance Criteria

- [ ] `raw-exposed-issues-checklist.md` 完整记录这轮 subagent 扫描暴露出的每一个问题。
- [ ] `structural-analysis.md` 按 checklist 顺序逐项给出“问题 -> 耦合分析 -> 解决方案”。
- [ ] 每个问题都明确标记是否属于模型过度耦合、事实源不清、入口分叉、projection 泄漏或验证不足。
- [ ] `implement.md` 给出后续彻底修复的顺序，不把机械清理排在结构性封装之前。
- [ ] 后续实现任务能够直接从这些结构性问题拆分，而不是继续围绕旧路径搜索替换。

## Notes

- 本任务的输入是上一轮审计 subagents 的结论和原 task 的目标模型，不重新把代码扫描结果当作设计结论。
- 当前阶段不要 `task.py start`；需要先让这份结构性审计文档被 review。
