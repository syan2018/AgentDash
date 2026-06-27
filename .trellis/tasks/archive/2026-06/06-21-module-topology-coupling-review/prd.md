# 项目主模块拓扑与耦合关系 review 编排

## Goal

组织一轮面向初版收尾阶段的项目架构 review。主会话负责拆分 review 范围、定义产物格式、调度 subagents 和汇总后续候选任务；实际模块盘查由细粒度 subagents 并发完成。

本轮重点不是直接修代码，而是系统化回答：

- 项目主要模块分别承担什么职责。
- 每个模块的主链路拓扑是什么。
- 模块之间通过哪些事实源、DTO、port、runtime surface、事件流或 UI 状态发生耦合。
- 哪些耦合关系会阻碍后续开发，值得拆成后续修复任务。

## Context

- 项目处于预研与初版收尾阶段，不需要兼容方案或回退方案。
- 近期已有 `.trellis/tasks/06-14-module-overdesign-review/` 做过过度设计 review，本轮需要避免重复，只把已有结论作为基线。
- 本轮 review 应覆盖后端、前端、跨层协议、本机/Relay/Desktop、Extension/VFS/Permission/Capability、Session/AgentRun、Workflow/Lifecycle/Task 等主要模块。
- 主会话不亲自下判断式 review；主会话只负责组织、约束、调度和整合 subagent 产物。

## Requirements

- 使用 subagents 以高并发、分轮次方式完成 review。
- 每个 subagent 的 review 范围必须足够窄，产物必须落盘到本 task 的 `research/` 目录。
- 每个模块产物必须包含模块职责、主链路拓扑、上下游耦合点、耦合关系性质、风险等级和后续深挖问题。
- Review 问题必须聚焦架构事实源、模块边界、跨层契约、控制面入口、运行态状态归属和 DTO/事件流漂移。
- 输出应形成后续可拆 Trellis task 的候选 backlog，而不是泛泛的代码质量意见。
- 不修改业务代码，不运行大规模测试；必要时只做静态检索和轻量命令验证。
- 所有任务文档与 review 输出使用中文。

## Out Of Scope

- 本轮不直接修复 review 发现的问题。
- 本轮不为旧接口设计兼容层。
- 本轮不把当前任务上下文写入长期 spec，除非最终发现稳定架构事实需要沉淀。
- 本轮不重复 `06-14-module-overdesign-review` 已覆盖的具体问题，只引用其结论作为已有基线。

## Acceptance Criteria

- [x] `research/` 中至少包含第一轮主模块拓扑盘查报告。
- [x] `design.md` 明确 review 范围、分轮策略、subagent 角色、产物 schema 和整合规则。
- [x] `implement.md` 明确每一轮的并发调度清单、输入上下文、输出文件和下一轮触发条件。
- [x] 最终汇总文件列出各模块主链路拓扑、跨模块耦合矩阵、风险分级和后续 Trellis task 候选项。
- [x] 每个候选后续任务都有明确问题、影响范围、建议 owner 模块和验收方向。
- [x] 主会话只做组织和整合，不把未由 subagent 产出的模块 review 结论伪装成已验证事实。

## Open Questions

- 后续是否立即按 `research/followup-backlog.md` 创建子 task，需由排期决定。
