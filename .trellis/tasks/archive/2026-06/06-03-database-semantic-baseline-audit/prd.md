# 数据库语义基线正式评估

## Goal

正式评估当前开发期 PostgreSQL 初始化基线是否符合当前项目语义，而不是只满足“旧 migration 链最终 schema 可运行”。评估要识别无用表、无用字段、位置不对的字段、系统行为/投影误落业务表的问题，并给出下一轮可执行的 schema 收敛建议。

用户价值：

- 让数据库基线从 dump 产物升级为能表达当前领域事实的 schema。
- 在项目未上线阶段尽早删除历史包袱，避免 runtime/control-plane 旧模型继续固化。
- 给后续修改 `0001_init.sql`、repository、domain/spec 提供证据链和优先级。

## Confirmed Facts

- 上一轮已将 PostgreSQL migration 历史压缩为单个 `0001_init.sql`，并验证空 embedded PostgreSQL 只运行 version 1 migration。
- 当前新 baseline 仍主要来自旧 migration 链最终 schema dump，经过去除明显历史表、SQLx bookkeeping 和无用 extension 的清理。
- 用户明确希望评估是否还有其它无用字段，以及某些表中是否出现了不应属于该表的系统行为字段。
- 项目处于预研期，允许直接收敛到最正确 schema，不需要为旧数据库提供兼容迁移路径。

## Requirements

- 对当前 `crates/agentdash-infrastructure/migrations/0001_init.sql` 做正式语义审计。
- 审计粒度必须覆盖表、字段、索引/约束、默认值和数据归属。
- 区分以下类别：
  - 当前必须保留的业务事实。
  - 当前必须保留的 runtime / audit / outbox 事实。
  - 可删除且无需代码改造的历史残留。
  - 可删除但需要同步 repository/domain/API/frontend/spec 的字段或表。
  - 字段位置不对或职责混杂，需要迁移到其它表/投影/seed/runtime 的候选。
  - dump 风格命名、默认值或约束名需要重写的候选。
- 评估必须结合当前 repository SQL、domain 类型、application use case、API route、spec 和前端 contract，不只看 migration 文件。
- 输出必须给出优先级、证据、建议动作和风险。
- 本任务默认只产出正式评估与改造建议；是否实际修改 schema/code 需要在评估后单独确认或拆子任务。

## Acceptance Criteria

- [ ] 产出正式评估报告，覆盖全部当前 55 张业务表或按模块给出清晰分组。
- [ ] 每个删除/迁移建议都有源码或 spec 证据。
- [ ] 明确列出“可以立刻从 init baseline 移除”的项目。
- [ ] 明确列出“值得移除但需要代码改造”的项目。
- [ ] 明确列出“保留但应重命名/重写默认值/重写约束”的项目。
- [ ] 明确列出“当前看似奇怪但应保留”的项目，避免误删。
- [ ] 给出下一步 schema 收敛实施计划和验证建议。

## Notes

- 当前评估不以兼容旧库为目标。
- 当前评估不把“代码现在在读写”直接等同于“语义最优”；若代码读写的是历史包袱，应明确标注为需要代码改造的 schema 候选。
