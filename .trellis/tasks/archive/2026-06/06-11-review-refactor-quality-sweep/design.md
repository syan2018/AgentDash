# Design: 模块级并行 review/refactor 主控循环

## 总体策略

本任务使用“模块级并行发现，模块级成组修复”的循环。`fuck-u-code` 负责暴露热点，subagent 负责并行审查和独立模块修复，主控 agent 负责定义命题、归类结论、审查 diff、提交和维护任务记录。

## 模块处理单元

处理单元必须是一个可独立理解和验证的模块或链路。一次处理不应细到单个变量或单个 helper，也不应大到跨多个互相争用的写入区域。

模块记录包含：

- 目标边界。
- 主要文件。
- 质量问题簇。
- 架构 backlog 候选。
- 修复范围。
- 验证命令。
- commit hash。

## 并行模型

- Review subagent 可以并行审查不同模块，输出写入 `reviews/` 的结构化结论。
- 修复 subagent 只在写入范围不重叠时并行执行。
- 主控 agent 不重复 subagent 已完成的探索，重点做抽样核验、分类和整合。
- 每个 subagent prompt 必须明确身份、任务边界、不得等待其他 subagent、不得回滚他人改动。

## 问题分类

### 实现级问题

可以在模块内小步修复并验证的问题进入 `fixes/`，例如：

- 混淆命名。
- 局部重复 helper / mapper。
- 组件或函数过宽。
- 裸字段在多个层级穿透。
- 已架空但仍存在的分支或链路。
- 违反本项目 spec 的样式、类型、状态处理。

### 架构设计问题

需要跨模块设计或改变事实源的问题进入 `architecture-backlog.md`，例如：

- frontend/backend 对同一业务语义重复解释。
- application/domain/infra 边界不清。
- runtime/session/workflow 生命周期事实源分散。
- 共享 contract 无法表达真实业务边界。

## 工具使用

`fuck-u-code` 只作为热点雷达。工具输出必须记录扫描范围、命令、是否 timeout、是否存在 parser fallback 或其他可信度限制。最终是否修复由代码证据、项目 spec 和风险判断决定。

## 提交策略

每个提交以模块为单位。提交前：

- 主控审查 diff。
- 运行与模块风险匹配的最小必要 check。
- 更新 `review-index.md` 和对应 `fixes/` 记录。

提交格式：

```text
type(scope): 中文提交信息

- 分点描述具体更新
- 记录验证结果
```
