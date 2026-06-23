# Release 前模块与 crate 拆分边界调研

## Goal

以 session 为主切入点，完整 review AgentDash 后端模块耦合、lifecycle/agentrun/session/runtime surface 边界，并为 release 前 crates 拆分建立阶段化调研与实施追踪任务。

本任务定位为 release 前的架构调研与实施追踪母任务。它先回答模块真实关系，再把后续 crates 拆分、module 移动、API 边界收束拆成可执行子任务。当前最重要的判断是：`session` 在产品与应用语义上不应继续作为一等业务模块暴露；它应收束为 AgentRun/Lifecycle 运行面的 `RuntimeSession` delivery / trace substrate，公开业务入口由 AgentRun / Lifecycle / RuntimeGateway 等控制面承担。

## Context

- 项目当前未上线，release 前模块拆分可以直接走正确模型；调研不为旧路径设计兼容层。
- `.trellis/spec/backend/session/architecture.md` 已定义当前 `Session` 的目标语义是 `RuntimeSession`：拥有 turn、tool、event、resume、debug、projection、trace lineage，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface。
- `.trellis/spec/backend/runtime-gateway.md` 已定义 Session MCP Action 的 surface 来源应为 AgentRun / Lifecycle current runtime surface query，而不是 `SessionHub` 或 idle fallback。
- `.trellis/spec/backend/workflow/architecture.md` 已定义 `RuntimeSessionExecutionAnchor` 是 `RuntimeSession` 反查 run / agent / frame / orchestration node 的权威索引，AgentRun current frame surface 才是 VFS/MCP/capability 的事实源。
- `crates/agentdash-application/src/session` 当前包含 76 个 Rust 文件，其中大量职责已经跨入 AgentRun frame surface、capability projection、hook runtime、VFS/resource surface、permission/adoption 和 API consumer 查询。
- 已存在相关任务：
  - `.trellis/tasks/06-23-session-hub-boundary-cleanup`
  - `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence`
  - `.trellis/tasks/06-19-lifecycle-mount-architecture-cleanup`
  - `.trellis/tasks/06-14-module-overdesign-review`

## Requirements

- 建立项目级后端模块关系图，覆盖 `session`、`agent_run`、`lifecycle`、`workflow`、`runtime_gateway`、`vfs`、`canvas`、`workspace_module`、`permission`、`capability`、`hooks`、`extension_runtime`、API bootstrap/routes 与 domain repository 暴露面。
- 明确 `session` 的真实位置：作为 RuntimeSession delivery/trace/runtime coordination substrate 被 AgentRun/Lifecycle 控制面使用，而不是一等业务入口、surface query facade 或跨模块 helper 集散地。
- 分阶段记录 review 结果。每个模块 review 必须独立落到 `research/*.md`，包含文件证据、调用点、耦合方向、目标归属、拆分建议和风险。
- 重点评估 `session/hub`、`session_construction.rs`、`RuntimeGateway` MCP session action、VFS surface resolver、Canvas/Extension/Terminal consumer、Permission/WorkspaceModule surface update 与 AgentFrame/anchor 暴露关系。
- 输出 release 前 crates 拆分候选图，说明每个候选 crate/module 的职责、依赖方向、应公开的 port/DTO、应隐藏的 implementation detail，以及第一批可执行拆分子任务。
- 评估当前 application 单 crate 内横向 import 对整洁架构的破坏点，给出 visibility、module facade、trait port、query/update service 边界的收束策略。
- 保持 clean architecture 判定：API 只做入口/DTO/错误映射；application 负责 use case 和 query/update facade；domain 只承载 entity/value/repository trait；infrastructure/executor 实现外部 adapter。
- 调研过程不得修改业务代码；本任务当前阶段只写 Trellis 任务文档、review brief、research 报告和后续实施拆分计划。

## Research Workstreams

1. Session runtime inventory：盘点 `session` 目录所有文件、exports、调用点和保留/迁出职责。
2. AgentRun/Lifecycle control-plane：盘点 `AgentFrame`、`RuntimeSessionExecutionAnchor`、current runtime surface query/update、Lifecycle node/run relation。
3. API/RuntimeGateway consumers：盘点 `session_construction.rs`、Canvas、VFS surfaces、Extension runtime、Terminal、RuntimeGateway bootstrap/provider 对 session/AgentFrame 的耦合。
4. Business surface update paths：盘点 Canvas、WorkspaceModule、Permission、Capability、Hook、MCP/VFS/Skill runtime update 写入 AgentFrame 或 active runtime adoption 的路径。
5. Crate split and dependency map：基于 Cargo/module imports、spec 和上面四类结果提出 release 前拆分顺序。

## Out Of Scope

- 本阶段不执行源代码重构。
- 本阶段不设计旧 API 或旧数据库字段兼容策略；后续实施任务如涉及 migration，按 migration 规范单独处理。
- 本阶段不替代已有 `06-23-session-hub-boundary-cleanup` 的具体修复任务；本任务吸收其证据并决定更高层拆分路径。

## Acceptance Criteria

- [x] `research/01-session-runtime-inventory.md` 完成，列出 `session` 文件分组、外部调用点、保留职责、迁出职责和首批删除/私有化候选。
- [x] `research/02-agentrun-lifecycle-surface.md` 完成，明确 AgentRun/Lifecycle 对 RuntimeSession、AgentFrame、surface query/update、anchor 的 owning boundary。
- [x] `research/03-api-runtime-gateway-consumers.md` 完成，列出 API/RuntimeGateway current surface consumer 的耦合路径与目标 application facade。
- [x] `research/04-business-surface-update-paths.md` 完成，列出业务模块 surface-changing/update/adoption 路径及归属收束建议。
- [x] `research/05-crate-split-coupling-map.md` 完成，给出 release 前 crates/module 拆分候选图、依赖方向和实施批次。
- [x] `design.md` 汇总 target architecture，明确 `session` 在 lifecycle/agentrun 大模块下的归属方式，以及哪些 public API 应从 session facade 移走。
- [x] `implement.md` 给出后续实施任务树和阶段顺序，能直接拆出 Trellis child tasks。
- [x] 所有结论都有文件路径或 spec/task 证据；无法从代码直接判定的内容记录为架构决策问题，而不是猜测。

## Notes

- 本任务的核心产出是 review evidence 和后续拆分计划，不以本轮代码 diff 为完成条件。
