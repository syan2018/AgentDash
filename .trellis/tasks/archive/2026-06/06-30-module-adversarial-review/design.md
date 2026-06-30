# 对抗性模块架构审查设计

## Review Shape

本任务采用两阶段审查：

1. Module topology pass：主会话先用代码证据验证模块拓扑，输出待确认的分工边界。
2. Adversarial review pass：用户确认拓扑后，再按确认后的问题域派发 subagent 并行审查。

这样做的原因是当前仓库中的 crate、package、API route、generated contract 并不等同于业务模块边界；过早按目录派发会把错误切分固化为审查前提。

## First Principles Lens

每个模块先回答以下问题：

- 这个模块存在是为了解决哪个不可省略的问题？
- 它拥有的事实源是什么？哪些状态只是投影或缓存？
- 如果删除某个层、DTO、facade、tool、route 或 store，真实能力会损失什么？
- 哪些概念在多个路径重复出现，并且没有清晰 owner？
- 哪些抽象保护了边界，哪些抽象只是绕远路？
- 哪些前端状态、API DTO 或 runtime projection 正在反向塑造领域模型？

## Candidate Review Domains

### Orchestrated Work Surface

覆盖 Workflow、Lifecycle、Orchestration、Task、Companion、Routine gates。重点检查可执行工作单元、任务计划、人类/Agent gate、reducer、surface projection 的事实源归属。

### Agent Runtime Session Surface

覆盖 AgentRun、RuntimeSession、RuntimeGateway、mailbox、conversation control、frame construction。重点检查运行会话控制面是否重复、AgentRun 与 RuntimeSession 是否互相投影、command/action/mailbox 是否有单一 owner。

### Extension / Workspace Module Runtime Surface

覆盖 workspace-module、extension runtime、local extension host、extension SDK/UI、canvas module runtime。重点检查 module、extension、canvas、runtime tool、workspace process/env 权限是否是同一套模型的不同层，还是已经发生概念分叉。

### Authority & Capability Runtime

覆盖 PermissionGrant、policy、escalation、CapabilityResolver、tool catalog、MCP capability、VFS capability。重点检查授权事实源与运行时 capability state 的边界，避免把 contract 或 frontend catalog 当成事实源。

### VFS & Runtime Tool Surface

覆盖 VFS mount、provider、runtime tool composer、context file discovery、mount ownership。重点检查 mount 权限、owner、capability、tool 暴露之间是否有重复路径。

### Local Runtime & Relay Surface

覆盖 agentdash-local、relay protocol、command handlers、terminal、materialization、runner claim、desktop shell。重点检查本机执行面是否仍有中央 hub、协议与领域 handler 是否错层耦合。

### Project / Workspace / Backend Placement

覆盖 project、workspace、backend、runner enrollment、machine/workspace identity、settings。重点检查本机后端、云端后端、工作区、机器、runner 的归属关系是否有重复事实源。

### Knowledge & Context Surface

覆盖 skill assets、shared library、context construction、MCP presets、story/session context。重点检查上下文注入、知识资产、能力来源和 session frame construction 的所有权。

## Evidence Contract

subagent 产物必须包含：

- 证据路径和可定位符号。
- 问题类型。
- 为什么这是第一性问题，而不是局部代码风格问题。
- 影响面。
- 建议收束边界。
- 建议优先级。

## Historical Baseline

旧任务 `.trellis/tasks/06-14-module-overdesign-review/` 作为 baseline：

- 用来避免重复发现已解决问题。
- 用来识别收束后是否产生新分叉。
- 用来验证旧问题是否在新模块边界下仍成立。

旧报告不是本任务的结论来源，必须重新以当前代码为准。
