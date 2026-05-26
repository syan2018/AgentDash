# Spec 文档体系收敛与迁移规划

## Goal

为 AgentDash 重新定义 `.trellis/spec/` 的文档职责、目录结构和迁移路径，使 spec 能同时承载系统开发状态与长期架构收敛方向，但清晰区分不变量、当前基线和局部决策。

本任务先定义文档治理和迁移方案，再完成第一轮文档树收敛：建立 architecture 主文档、更新索引、调整 update-spec 维护规则，并瘦身高风险 appendix。

## Background

当前 `.trellis/spec/` 混合了多种材料：

- 长期架构不变量，例如 Cloud/Local 数据归属、Session launch 主线、VFS 地址模型。
- 当前实现基线，例如 crate 清单、provider/action 列表、DTO 字段。
- 局部设计决策，例如 Rhai 作为 Hook 脚本引擎、Shared Library 安装语义。
- 任务验收与过程材料，例如 `Tests Required`、`Good/Base/Bad Cases`、`Wrong vs Correct`、历史 migration 路径、日期型 changelog。

这会让后续 AI session 难以判断哪些内容是系统必须长期收敛到的 architecture attractor，哪些只是当前投影、局部选择或历史轨迹。

## Requirements

- 定义新的 spec 内容分层：
  - `Invariants`：长期结构不变量，默认不由自动 spec update 改写。
  - `Current Baseline`：当前代码对不变量的工程投影，可随实现事实维护。
  - `Local Decisions`：局部但稳定的设计选择，只记录为什么。
  - `Contract Appendices`：协议、DTO、状态流、错误语义等可执行契约附录。
- 为每个主要模块定义一个 architecture 主文档作为阅读入口，其它文档作为追加记录或契约附录。
- 设计 `.trellis/spec/` 的目标目录结构，兼容现有 backend/frontend/cross-layer/shared/guides 分层，但补足模块主文档规则。
- 设计现有 spec 的迁移策略，区分：
  - 明确全局文档。
  - 应收敛到具体模块 architecture / appendix 下的文档。
  - 直接保留并重排的内容。
  - 应迁到 task harness 的内容。
  - 应迁到 memory/journal/AGENTS 问题收纳的内容。
  - 应删除的噪音内容。
- 设计 `trellis-update-spec` skill 的规则调整点，明确 spec 维护目标、自动维护边界和不变量变更门槛。
- 给出可分批执行的迁移计划，避免一次性大规模改写造成上下文丢失。

## Non-Goals

- 不在本任务中逐字 review 每个模块真正 attractor；本轮只建立文档结构和第一轮迁移。
- 不改变 Trellis task workflow 的 phase 机制。
- 不把 `.trellis/spec/` 变成纯静态架构文档；spec 仍允许记录必要当前基线。
- 不要求所有历史信息都保留；只保留对未来开发有结构价值或排错价值的信息。

## Acceptance Criteria

- [x] `design.md` 定义新的 spec 文档本体论、目录模型、主文档模板和维护边界。
- [x] `design.md` 明确 `architecture 主文档`、`contract appendix`、`task harness`、`memory` 的职责差异。
- [x] `design.md` 包含 `trellis-update-spec` skill 的拟调整规则。
- [x] `design.md` 包含现有 `.trellis/spec` 文件的逐文件迁移矩阵，明确哪些属于全局，哪些应下沉到模块。
- [x] `implement.md` 给出分批迁移计划，并列出每批的目标文件、迁移动作和验证方式。
- [x] 方案能解释现有高风险文件如何处理，例如 `session-startup-pipeline.md`、`shared-library-contract.md`、`workflow-activity-lifecycle.md`、`architecture-evolution.md`。
- [x] 方案明确：自动 spec update 可以维护 Current Baseline / Contract Appendix，但不得自动重写 Invariants。
- [x] 第一轮迁移已建立模块 architecture 主文档，并移除 spec 中的任务验收模板噪音。

## Open Questions

- 第一轮采用“保留现有 appendix 文件名，新增 `architecture.md` 作为主入口”的低扰动迁移方式。后续模块 review 时再决定是否重命名或合并 appendix。
