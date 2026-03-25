# Workflow Hook 收口与精简

## Goal

围绕当前 AgentDashboard 的 workflow hook 实现，明确运行时 authority、削减重复注入链路，并把容易误导的 `HookPolicy` 命名收口为只读视图语义。

## Problem Statement

当前实现已经形成了可用的 Hook Runtime 主链，但仍存在几类显著冗余：

- `HookPolicy` 以“规则”的外观存在，但真实执行 authority 仍是 `normalized_hook_rules()`
- workflow `agent_instructions` 同时被表达为 fragment 与 constraint，实际注入时会重复出现在同一条 hook 消息中
- SessionPlan、connector system prompt、dynamic hook 三条链会重复表达工具面、路径策略、workflow 说明
- `SessionStart` 已进入契约层，但尚未形成清晰的 baseline setup 职责

## Requirements

- 明确 workflow / hook runtime / session bootstrap 三层的唯一职责
- 明确 hook 可执行规则与前端可观测视图的 authority 分工
- 输出可落地的分阶段重构计划，优先处理低风险高收益的收口项
- 先把 `HookPolicy` 更名为更准确的只读视图命名，避免继续扩散误导语义
- 将关键决策补充到 code-spec，供后续实现继续沿用

## Non-Goals

- 本轮不重写现有 rule engine
- 本轮不改动 workflow completion 的既有行为闭环
- 本轮不把 SessionStart 全量接入新的文本注入机制
- 本轮不强制完成 session plan / system prompt / dynamic hook 的所有去重实现

## Acceptance Criteria

- [ ] 仓库内新增正式的 workflow hook 精简计划文档
- [ ] 计划文档明确分阶段范围、目标、依赖、风险与验证方式
- [ ] 后端 spec 明确 `HookPolicyView` 是派生只读视图，不是执行 authority
- [ ] 合同层 / API / 执行层 / 前端类型中的 `HookPolicy` 命名完成收口
- [ ] 关键引用更新后通过基础检索与至少一轮类型/编译检查

## Technical Notes

- 命名收口优先采用类型名变更，字段名 `policies` 暂不强改，减少跨层破坏面
- 后续 phase 的重点不是继续增加 view/model，而是收口 authority 与注入层级
- `SessionStart` 建议保留，并在后续 phase 中定位为 baseline setup trigger，而非额外文本注入入口
