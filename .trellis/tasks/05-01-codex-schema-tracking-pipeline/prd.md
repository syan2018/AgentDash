# pipeline: codex schema 跟踪与diff

## Goal

建立 Codex App Server 协议变更的持续跟踪流水线，支持发布触发采集和每周汇总审阅。

## Requirements

* 自动采集版本与 schema 快照（TS/JSON Schema）。
* 自动产出结构 diff（新增/变更/废弃/破坏）。
* 生成变更摘要，写入协议 ADR 决策入口。
* 对接“单线快跟”策略，输出升级建议。

## Acceptance Criteria

* [ ] 能按版本生成并保存 schema 快照。
* [ ] 能生成可读 diff 报告与分类结果。
* [ ] 能形成每周汇总视图（变更列表 + 建议动作）。
* [ ] 能把变更事件映射到父任务治理流程。

## Out of Scope

* 不负责具体执行器能力实现。
* 不负责前端渲染模型改造。
