# Implement Plan: Spec 文档体系收敛与迁移规划

## Phase 0: 准备与冻结范围

- [x] 确认本任务进入第一轮迁移执行，不逐字 review 每个模块真正 attractor。
- [x] 用户确认目录模型与不变量维护边界。
- [x] 明确第一轮迁移采用“新增 architecture 主文档 + 索引重排”，避免一次性重命名大量文件。

## Phase 1: 调整 Trellis update-spec 规则

- [x] 修改 `.agents/skills/trellis-update-spec/SKILL.md`：
  - 增加 `Spec Maintenance Goal`。
  - 明确 `Invariants / Current Baseline / Local Decisions / Contract Appendices`。
  - 将强制 7 段模板降级为 contract appendix 的可选模板。
  - 明确自动维护不能改写 Invariants。
- [x] 验证 skill 文案不再鼓励把 `Tests Required`、`Good/Base/Bad Cases`、`Wrong vs Correct` 写进 spec 主体。

## Phase 2: 建立 architecture 主文档骨架

- [x] 新增或整理以下主文档：
  - `.trellis/spec/backend/architecture.md`
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/workflow/architecture.md`
  - `.trellis/spec/backend/vfs/architecture.md`
  - `.trellis/spec/backend/hooks/architecture.md`
  - `.trellis/spec/backend/capability/architecture.md`
  - `.trellis/spec/frontend/architecture.md`
  - `.trellis/spec/cross-layer/architecture.md`
- [x] 每份主文档使用统一结构：
  - Role
  - Invariants
  - Current Baseline
  - Local Decisions
  - Contract Appendices
- [x] 只抽取已经明确成立的内容，不新增未确认架构判断。

## Phase 3: 存量文档归位

- [x] 按 `design.md` 的迁移矩阵标记每个现有文件的目标归属：
  - 全局保留。
  - layer index。
  - module architecture。
  - contract appendix。
  - task harness。
  - memory / AGENTS。
  - 删除。
- [x] 对明确全局文档先做最小调整：
  - `index.md`
  - `project-overview.md`
  - `tech-stack.md`
  - `communication.md`
  - `shared/index.md`
  - `guides/*`
- [x] 对应收敛到模块的文档，优先将不变量摘入模块 `architecture.md`，原文件保留为 appendix。
- [x] 对重叠文档建立主从关系：
  - `cross-layer/shared-library-contract.md` 主于 `backend/shared-library.md`
  - `session/architecture.md` 主于各 session appendix
  - `workflow/architecture.md` 主于 backend/frontend activity lifecycle appendix

## Phase 4: 更新索引与阅读顺序

- [x] 更新 `.trellis/spec/index.md`，说明 spec 内容分层和主文档优先级。
- [x] 更新 `.trellis/spec/backend/index.md`，标注 architecture 主入口与 contract appendices。
- [x] 更新 `.trellis/spec/frontend/index.md`。
- [x] 更新 `.trellis/spec/cross-layer/index.md`。
- [x] 移除 index 中的 `✅ 已更新/已创建` 这类状态列。

## Phase 5: 瘦身高风险 spec

- [x] `backend/architecture-evolution.md`
  - 抽取仍有效的分层原则到 `backend/architecture.md`。
  - 将历史内容从常规阅读路径移除。
- [x] `backend/session/session-startup-pipeline.md`
  - 将主线不变量上移到 `session/architecture.md`。
  - 删除或迁出 `Verification`、测试清单、Wrong/Correct。
- [x] `cross-layer/shared-library-contract.md` 与 `backend/shared-library.md`
  - 明确跨层契约为主，后端文档只保留后端专属基线。
- [x] `frontend/workflow-activity-lifecycle.md`
  - 保留 Activity 模型契约。
  - 迁出编辑器任务过程、测试命令和 out-of-scope。

## Phase 6: 全量分类巡检

- [x] 扫描 `.trellis/spec` 中的以下模式：
  - `Tests Required`
  - `Good/Base/Bad Cases`
  - `Wrong vs Correct`
  - `Verification`
  - `当前已`
  - `已更新`
  - `已创建`
  - 日期型 `2026-`
  - `后续增强`
  - `待办`
- [x] 对每一处做分类：保留、转写、迁出、删除。
- [x] 避免机械删除；保留能表达不变量、当前基线或局部决策理由的内容。

## Phase 7: 验证

- [x] `rg -n "Tests Required|Good/Base/Bad Cases|Wrong vs Correct|Verification" .trellis/spec`
  - 结果应仅出现在明确标注为 contract appendix 的少量文档中，或完全消失。
- [x] `rg -n "已更新|已创建|待办|后续增强" .trellis/spec`
  - 确认无 index 状态噪音或任务计划残留。
- [x] 手动检查各 layer index 的阅读顺序：
  - 先 architecture。
  - 再 contract appendices。
  - guides 只作为思考触发。
- [x] 随机抽查 3 个模块，确认新 session 能通过 architecture 主文档理解模块 attractor 与当前基线。

## Rollback / Safety

- 所有迁移分批提交。
- 每批只处理一组文档或一个模块。
- 不在同一提交中同时修改 skill 规则和大量 spec 内容。
- 不删除可能有价值的历史内容前，先确认其是否已存在于 `.trellis/tasks/archived` 或 workspace journal。
