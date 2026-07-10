# Workspace Module 通用双工交互系统预研

## Goal

评估 Workspace Module、Canvas、Extension、MCP 与 RuntimeGateway 的长期目标边界，设计一套用户可直接交互、AI 可双工访问、并可组合 Extension 前端组件与协议行为的通用交互系统。目标运行面不以 AgentRun 或 RuntimeSession 为前置条件。

重点验证两个产品机会：一是把现有 Canvas 内“通过代码/JSON 编排一系列 protocol 与工具调用”的解释执行能力正式暴露给 Agent；二是把 Canvas 从与 Extension、Session 强耦合的特殊工作区实现，演进为可持久化、可组合、可由人和 Agent 共同操作的交互资产/运行实例。当前保持 planning；评审通过后按 `implement.md` 的递进切片执行。

## Background

- 项目把 Agent 当前可见 MCP 工具也视为可编排行为的一部分。
- Canvas 已允许 Agent 编辑代码，并由浏览器 iframe 中的 JavaScript 组织一系列 protocol 行为；它产生了“脚本化工具计划”的效果，但当前没有独立 IR 或可供 Agent headless 调用的解释器。
- 当前 Canvas、Extension 和 Session 的实例化、可见性、运行 context 与展示链路仍有较强耦合。
- 长期目标包括：用户与 AI 对同一交互对象进行双工操作；Canvas 能拼装 Extension 暴露的前端组件，复用交互、状态和业务能力。
- 项目尚未上线，目标设计应优先保持领域和协议正确，不建设旧接口兼容层。

当前审计建议抽出 actor-neutral `OperationProgram`，并在 RuntimeGateway 内收束共同 `OperationExecutionCore`；另建 `InteractionInstance + Attachment + Command/Event` 作为双工状态事实源。Canvas 保留为交互定义与 presentation schema，Extension component MVP 使用声明式 descriptor 与隔离 iframe。

## Product Decisions

- PD1（已确认）：`OperationProgram` 是独立、actor-neutral、可版本化的合同；同步 MVP 同时接受 inline definition，Canvas、Agent 与未来 Workflow 可调用同一合同。
- PD2（已确认）：普通 Project Canvas 仍请求已删除 runtime-snapshot endpoint 的断链纳入本任务 W0，后续实施时直接修正，不另建兼容路由或独立任务。
- PD3（已确认）：RuntimeSession 是待移除的历史执行耦合，不进入目标 authority、scope、placement 或 Gateway contract；Canvas 与 Extension 必须能在没有 AgentRun、AgentFrame 和 RuntimeSession 时，以用户工作坊模式访问同一 RuntimeGateway。
- PD4（待确认）：`InteractionInstance` 的默认 owner/lifetime；AgentRun 只作为可选 attachment，不作为 standalone Canvas/Extension 的运行前提。

## Requirements

- R1：绘制当前 Workspace Module、Canvas、Extension protocol/runtime action、MCP、Agent capability、Session/AgentFrame、VFS 和前端 WorkspacePanel 的端到端事实与调用链。
- R2：识别现有 protocol 行为解释执行能力的真实 IR、执行器、权限判定、上下文注入、错误模型和持久化位置，判断其是否已经足以成为 Agent-facing capability。
- R3：设计 Agent-facing operation program 能力面，覆盖发现、描述、校验、无副作用预检、同步执行、调用方取消、结果引用与审计；Agent 不应被迫传递 Project、Session、Backend、Workspace root 等宿主权威 ID。
- R4：明确 protocol program 与直接 tool/MCP 调用、Workspace Module operation、workflow、routine、hook 和 Extension protocol 的差异，避免新造一套平行编排系统。
- R5：重新划分 Canvas Definition/Asset、Canvas Runtime Instance、Workspace presentation、Session attachment 与 AgentFrame capability，降低 Canvas 对单个 Session 和具体 Extension installation 的耦合。
- R6：定义用户与 AI 对同一交互对象的双工模型，包括状态事实源、command/event、并发写入、关注/唤醒、权限、审计和实时投影。
- R7：评估 Extension 前端组件贡献与 Canvas 组合协议，覆盖组件身份、props/schema、events/actions、state ownership、layout/slot、版本、sandbox、权限和资源加载。
- R8：明确全局 Channel 在双工交互中的职责：只承载消息、关注和异步 delivery，还是同时承载交互对象的 command/event；不得与 Workspace Module operation 或 protocol program 混成一个万能总线。
- R9：给出最小验证切片、长期目标架构、分阶段任务拆分以及必须先修正的存量边界。
- R10：将 RuntimeGateway invocation 正交拆为 principal、authority/scope、origin、execution placement 与 trace correlation；Canvas/Extension 由可信 application adapter 构造内部 envelope，浏览器不提交 Session、Backend、workspace root 等宿主权威 ID。
- R11：提供 standalone user workshop runtime surface，使 Canvas、Extension panel 与 Interaction renderer 在没有 AgentRun/RuntimeSession 时可发现并调用授权 Operation；需要本机执行时从 Project/workspace/provider binding 解析 placement。
- R12：AgentRun 通过 AgentFrame adapter 消费同一 RuntimeGateway；RuntimeSession 只允许作为迁移期间的可选 trace/delivery correlation，最终从 Gateway/provider 合同与 Canvas/Extension API 中删除。

## Acceptance Criteria

- [ ] 当前解释执行链是否真实存在、能够调用哪些 MCP/protocol/tool、由谁做 admission 均有代码证据。
- [ ] 给出 Agent-facing OperationProgram 的最小合同与至少一条代表性执行流。
- [ ] Canvas 的资产、运行实例、Session attachment 和 UI presentation 不再被视为同一对象。
- [ ] 人类与 Agent 的双工交互具有单一状态事实源和明确的 command/event/delivery 边界。
- [ ] Extension UI 组件组合协议能够描述一个组件被 Canvas 引用、渲染、收发事件和访问受限能力的完整流程。
- [ ] 明确与全局 Channel 重构任务的依赖点，但两个任务可独立评审和推进。
- [ ] 普通 Project Canvas preview 不再请求已删除的 runtime-snapshot endpoint，静态资产预览与 attached interaction runtime 使用明确不同的事实链。
- [ ] 至少一个 Canvas 与一个 Extension panel 能在没有 AgentRun、AgentFrame、RuntimeSession 的情况下发现并调用授权 Operation。
- [ ] RuntimeGateway 内部 invocation envelope 分离 principal、scope、origin、placement 与 trace；客户端不能借 origin 冒充 principal，也不提交 backend/session authority。
- [ ] AgentRun 与 standalone user workshop 通过不同 adapter 进入同一 Operation catalog/admission/dispatch 主链。
- [ ] 形成可验证的 MVP 实验与后续任务地图，并经用户评审后才进入实现。

## Out of Scope

- 本任务不立即实现可视化编辑器、通用脚本语言或 UI component marketplace。
- 本任务不默认把 Channel、workflow、tool、MCP 和 protocol program 合并为单一抽象。
