# 执行计划

## 并行审查

- [x] 分派 Lifecycle / Workflow / Task 深度 review。
- [x] 分派 AgentRun / Session / Runtime Gateway 深度 review。
- [x] 分派 VFS / Local / Relay / Extension review。
- [x] 分派 Frontend / Contracts / Permission review。

## 主会话扫描

- [x] 统计模块规模、热点文件、跨层引用和重复状态/DTO 命名。
- [x] 抽样阅读 `Lifecycle`、`AgentRun`、session feed、runtime gateway、workflow/task projection 的核心文件。
- [x] 对 subagent 发现做去重和证据复核。

## 汇总

- [x] 写入 `overdesign-review.md`，按优先级和模块面组织。
- [x] 标记适合后续拆任务的清理候选。
- [x] 确认本轮没有修改业务代码。

## 验证

- `git status --short`
- 人工复核报告中的文件路径和关键证据是否真实存在。
