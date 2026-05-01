# check: backbone conformance 验证方案

## Goal

设计 Runtime Backbone 与 ACP facade 的一致性验证框架，确保协议演进可持续回归。

## Requirements

* 定义 conformance 维度：生命周期、错误、审批、工具调用、断流恢复。
* 设计样例输入输出规范与断言模型。
* 定义 Codex 协议升级后的回归检查流程。
* 输出可执行测试计划（先计划，后实现）。

## Acceptance Criteria

* [ ] 形成 conformance 测试矩阵。
* [ ] 形成样例数据与断言模板草案。
* [ ] 形成升级回归流程（与 schema tracking 衔接）。
* [ ] 形成阶段性质量门禁建议。

## Out of Scope

* 不在本任务实现完整自动化测试框架代码。
* 不覆盖 UI 层视觉回归。
