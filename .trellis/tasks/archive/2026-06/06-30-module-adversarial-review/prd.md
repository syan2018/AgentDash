# 对抗性模块架构审查

## Goal

基于第一性原理重新梳理 AgentDash 的主要运行模块，并对各模块进行对抗性架构审查，识别路径冗余、概念分叉、事实源分散、职责漂移、抽象泄漏、模块过厚和不符合当前预研阶段正确形态的实现。

本任务先完成模块拓扑确认，再派发并行 subagent 审查；不直接沿用目录、crate、package 或旧报告中的切分方式。

## Background

- 旧任务 `.trellis/tasks/06-14-module-overdesign-review/` 已完成一轮模块过度设计评估和两轮收束，可作为历史基线和回归检查来源。
- 这次审查的起点不是“继续旧任务未完成项”，而是重新确认当前代码经过多轮收束后的真实模块边界。
- 初步讨论已确认以下边界调整：
  - Workspace Module 与 Extension Runtime / Plugin Surface 属于同一问题域，应合并为 Extension / Workspace Module Runtime Surface。
  - Permission 与 Capability 可按“授权事实源 -> 运行时能力解析/暴露”链路一起审查；Contract 不作为独立一级模块，只作为跨层投影证据。
  - Workflow / Task 应结合 Orchestration、Lifecycle、Companion、Routine gate 一起看，不按目录拆开。
  - API routes、generated contracts、application 聚合层主要作为装配或投影层证据，不作为第一性审查主体。

## Requirements

- 先做 module topology pass，再确认正式 subagent 分工。
- module topology pass 必须按运行链路识别模块，而不是按目录名、crate 名、package 名机械切分。
- 每个候选模块必须列出：
  - 事实源与核心状态机。
  - 主要入口 API / tool / relay command / frontend feature。
  - 跨模块依赖与可能的反向依赖。
  - 已知历史基线中对应的旧问题或已收束点。
  - 是否适合单独派发 subagent 审查，或应合并到其它问题域。
- 正式审查使用 subagent 并行进行，但必须等模块拓扑经用户确认后再派发。
- subagent 审查必须基于代码证据，不能只输出风格判断或泛泛而谈的架构建议。
- 每条问题必须至少包含文件路径和可定位的函数、类型、模块、状态字段或调用链证据。
- 审查重点是当前预研阶段应直接修正到正确形态的问题，不提出兼容层、回退方案或迁移保守方案。
- 结论必须区分：
  - 路径冗余。
  - 概念分叉。
  - 重复事实源。
  - 模块过厚。
  - 抽象泄漏。
  - 横向耦合。
  - 命名或职责漂移。
  - 单纯装配层噪音。
- Contract、API route、application facade 只在具体业务链路中检查是否越权拥有事实源或制造概念分叉，不作为独立审查模块。

## Candidate Module Topology

以下只是待验证拓扑，不是最终 subagent 分工：

1. Orchestrated Work Surface
   - Workflow / Lifecycle / Orchestration / Task / Companion / Routine gates。
2. Agent Runtime Session Surface
   - AgentRun / RuntimeSession / RuntimeGateway / mailbox / conversation control / frame construction。
3. Extension / Workspace Module Runtime Surface
   - workspace-module / extension runtime / extension host / extension SDK/UI / canvas module runtime。
4. Authority & Capability Runtime
   - PermissionGrant / policy / escalation / CapabilityResolver / tool catalog / MCP capability / VFS capability。
5. VFS & Runtime Tool Surface
   - VFS mount / VFS providers / runtime tool composer / context file discovery / mount ownership。
6. Local Runtime & Relay Surface
   - agentdash-local / relay protocol / command handlers / terminal / materialization / runner claim / desktop shell。
7. Project / Workspace / Backend Placement
   - project / workspace / backend / local runner enrollment / machine and workspace identity / settings。
8. Knowledge & Context Surface
   - skill assets / shared library / context construction / MCP presets / story and session context。

## Out of Scope

- 本任务不直接修改业务代码，除非后续明确拆出实现子任务并进入执行阶段。
- 本任务不把旧任务报告复制为新结论；旧报告只能作为 baseline 和 regression prompt。
- 本任务不单独审查 generated contract 质量，除非具体模块链路中发现 contract 形状反向支配领域事实。
- 本任务不做全量测试或修复验证，除非进入后续实现子任务。

## Acceptance Criteria

- [x] 形成一份模块拓扑确认文档，覆盖主要运行链路、事实源、入口、依赖和审查分组建议。
- [x] 用户确认正式 subagent 分工前，不派发正式对抗性审查。
- [x] 正式审查产物覆盖最终确认的主要模块，并落在本任务目录中便于复查。
- [x] 每个模块审查都包含代码证据、问题分类、影响面、建议收束边界和优先级。
- [x] 最终综合报告去重并排序问题，明确哪些适合后续拆实现任务，哪些只是观察项。
- [x] 对旧任务 `06-14-module-overdesign-review` 的结论做回归对照：标记已解决、仍残留、重新出现或被新设计取代。
- [x] 不把 API routes、contracts、application facade 当成孤立模块输出泛化建议；只在具体链路中作为证据引用。

## Notes

- 当前任务是 planning 状态。下一步是完成 `design.md` 和 `implement.md`，然后做 module topology pass。
