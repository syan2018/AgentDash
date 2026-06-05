# Dynamic Workflow 与 Lifecycle Activity 对齐预研

## Goal

沉淀 Claude Code Dynamic Workflows 对 AgentDash Lifecycle Activity / WorkflowGraph 模块的启发与差距判断，为后续是否引入“脚本化动态编排”建立一个新的、干净的规划起点。

本任务只保存 research 结论和产品/架构问题，不进入实现。

## Requirements

- 记录 Dynamic Workflows 的核心能力：模型生成可审脚本、独立运行时执行、脚本变量承载中间结果、agent 节点扇出/扇入、运行 journal/cache/resume、进度与成本观测。
- 对照 AgentDash 当前 WorkflowGraph / LifecycleRun / Activity runtime 模型，说明已有能力、缺口和不能直接复用的边界。
- 给出推荐学习方向：不要把现有 WorkflowGraph 强行改成脚本语言，而应考虑将静态 graph 与动态脚本统一编译到同一套 runtime scripted rule plan 与持久化状态交换快照。
- 记录过程仓储可能过重、runtime state 过度分散的风险，并把后续收敛方向写入 research。
- 明确后续正式设计前必须回答的产品问题，尤其是脚本资产归属、运行态持久化、权限继承、可恢复边界、成本控制和 UI 审批体验。
- 新增 discussion journal，保存本次讨论中的判断变化与用户补充的架构原则。
- 在 research 中补充关键事实来源复核索引，说明后续应该去哪些 spec、源码、migration 和外部资料复核结论。
- 保持当前任务只读研究性质，不修改代码、不做迁移、不启动 dev runtime。

## Acceptance Criteria

- [x] 创建新的 Trellis task，避免复用旧的 lifecycle branching 任务上下文。
- [x] 在任务目录内保存本次 research 结论。
- [x] 结论包含 AgentDash 当前模型的证据链和 Claude Dynamic Workflows 的关键差异。
- [x] 结论给出推荐方向、阶段性路线和主要风险。
- [x] 新增讨论 journal，记录研究结论如何被用户补充修正。
- [x] Research 文档包含关键事实来源复核索引。
- [ ] 若后续进入实现，应先补充 `design.md` 与 `implement.md`，并经过任务启动流程。

## Notes

- 旧任务目录中的历史 branching 设计不作为本任务依据。
- 详细研究记录见 `research.md`；讨论脉络见 `discussion-journal.md`。
- 用户贴入的两份 Dynamic Workflows 原文已复制到 `research/` 子目录，后续优先复核任务内副本。
