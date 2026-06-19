# AgentRun lifecycle surface projector 标准化

## Goal

把 AgentRun lifecycle VFS surface 的构造从分散 helper 调用收束为一个明确的 application-layer projector。

目标是让 owner bootstrap、plain companion child、companion+workflow child、AgentRun workspace query、后续 routine / workspace module 等路径都通过同一套 typed input 生成 lifecycle runtime surface，而不是各自手动拼 mount、bootstrap builtin skill、合并 metadata 或决定 query/launch 行为。

本任务进入 planning。实现阶段应在设计确认后再启动。

## Background

上一轮 `06-15-agentrun-lifecycle-companion-convergence` 已完成 companion payload `message` 收束，并补上了 AgentRun lifecycle VFS skill projection metadata 的保留与合并。

当前代码已经有可复用 helper：

- `build_agent_run_lifecycle_vfs_with_skills(base_vfs, anchor, project_id, skill_asset_keys)`
- `append_lifecycle_skill_asset_projection(vfs, project_id, skill_asset_keys)`
- `project_active_workflow_lifecycle_vfs(vfs, active_workflow)`
- `project_companion_system_to_agent_run_lifecycle(...)`

但业务语义仍散在调用方：

- 调用方自己决定是否 ensure `companion-system` / `workspace-module-system`。
- 调用方自己传 `Vec<String>` skill keys。
- lifecycle mount metadata 仍是散装 JSON 字段。
- query 侧用 `&[]` 表达“只保留已有 projection，不追加新 skill”，语义不够显式。
- `agent_run_session` 与 `node_runtime` 仍是两套 mount scope；前者服务 AgentRun workspace 中的 message stream / trace evidence，后者服务 orchestration node artifacts/records 写入。
- node runtime 是 orchestration-owned execution behavior。RuntimeSessionExecutionAnchor 可以记录 session 是由哪个 orchestration node 启动的，但这是一条 launch evidence reference，不代表 node runtime 从属于 session。
- `lifecycle` 不会拆成多个 connector-visible mount。它是唯一的通用聚合面，Agent 当前状态决定 `session/*`、`node/*`、`artifacts/*`、`records/*`、`skills/*` 等路径下是否存在对应投影。
- 当前 `RuntimeSessionExecutionAnchor` 同时承载 session -> run / agent / frame 反查和 optional orchestration node reference，容易被误读成“session 关联 orchestration”。标准化时应把它定位为 runtime trace lookup index；node projection 的领域事实来自 orchestration/node coordinate。
- 目标模型中，运行时对外入口除消息流之外不应以 `RuntimeSession` 为索引；统一外部索引是 AgentRun control-plane identity。`RuntimeSession` 只表达 message stream / connector trace substrate。

## Requirements

- 提供一个 AgentRun lifecycle surface projector，作为业务侧构造 AgentRun lifecycle VFS surface 的标准入口。
- projector input 必须类型化表达：
  - base VFS
  - AgentRun runtime address
  - optional message stream ref
  - project id
  - projection mode
  - builtin skill policy
  - explicit SkillAsset keys
  - optional orchestration node projection
- projector output 必须类型化表达：
  - final VFS
  - lifecycle mount
  - effective SkillAsset keys
  - 可供 capability / resource surface 消费的 projection facts
- builtin skill 不应由调用方直接拼字符串 key；调用方应表达 `CompanionSystem`、`WorkspaceModuleSystem`、`RoutineMemory` 等业务意图。
- lifecycle mount metadata 应收束到 typed struct，再序列化为 mount metadata。
- query 侧和 launch 侧应使用同一个 projector，通过 mode/policy 区分行为。
- projector 必须保留现有 SkillAsset projection metadata 合并语义。
- projector 必须保持 AgentRun workspace resource surface 与 connector-visible VFS 的事实源一致。
- projector 必须把 AgentRun identity、optional message stream、orchestration node execution 和 SkillAsset projection 表达为可组合 surface facets；所有 facet 都投影到同一个 `lifecycle` 聚合面，不引入 `lifecycle-session` / `lifecycle-node` 这类并行 mount。
- `node_runtime` 的领域所有权仍归 orchestration/node coordinate；通用 `lifecycle` 聚合面只负责在 Agent 状态包含该 coordinate 时暴露对应 node 路径投影。
- projector input 必须把 AgentRun runtime address、message stream ref 与 orchestration node projection 明确分开；不得把 session anchor 上的 optional node fields 当作 node ownership 来源。
- 除 message stream 相关路径与 connector trace 读取外，新 runtime surface 不应要求调用方以 `runtime_session_id` 作为业务入口。
- 实现阶段不应引入前端硬编码 builtin skill 可见性。
- 本项目未上线，不需要兼容旧 helper 调用形态；实现时可直接迁移调用点。

## Acceptance Criteria

- [ ] PRD 明确 projector 要解决的业务问题、覆盖路径与不变量。
- [ ] 设计文档明确 `AgentRunLifecycleSurfaceProjector` 的 input/output 类型、mode、skill policy、orchestration node projection。
- [ ] 设计文档明确现阶段保留 orchestration-owned node write semantics，并通过 `lifecycle` 聚合面的路径投影暴露。
- [ ] 设计文档明确 message stream projection 与 node runtime 的组合关系：message stream 可以作为 trace evidence，node runtime 仍由 orchestration/node coordinate 拥有。
- [ ] 设计文档明确 `lifecycle` 是唯一 connector-visible 聚合面，路径投影由 Agent 状态决定，不存在双 lifecycle mount 方案。
- [ ] 设计文档明确 `RuntimeSessionExecutionAnchor` 是 trace lookup index，不是 session-owned orchestration association。
- [ ] 设计文档明确 AgentRun 是 runtime 对外业务索引，RuntimeSession 只作为 message stream / connector trace substrate。
- [ ] 实施计划列出 owner bootstrap、companion、workflow node、AgentRun workspace query、spec/tests 的迁移顺序。
- [ ] 实施计划列出需要新增或调整的测试。
- [ ] 实现后业务调用方不再直接散落组合 `ensure_*_skill_asset` + `append_*_skill_key` + `append_lifecycle_skill_asset_projection` + lifecycle mount helper。
- [ ] 实现后 workspace query 不再用 `&[]` 这种隐式方式表达 preserve-only skill projection。
- [ ] 实现后 lifecycle mount metadata 的关键字段由 typed metadata 构造。
- [ ] 实现后聚焦测试覆盖 graphless ProjectAgent、plain companion child、companion+workflow child、workspace query preserve-only projection。

## Scope

- AgentRun lifecycle surface projector 设计。
- lifecycle mount metadata 类型化。
- builtin lifecycle skill policy 类型化。
- 当前 helper 调用点迁移。
- backend specs 同步。
- 聚焦测试。

## Out Of Scope

- 重新设计 companion payload contract。
- 重新定义 AgentRun mailbox command model。
- 前端 UI 重构。
- 直接读取 embedded skill bundle 绕过 Project SkillAsset。

## Closed Decision

- `lifecycle` 永远保持唯一 connector-visible 通用聚合面，不做双 lifecycle mount。
- node execution 的领域所有权归 orchestration/node coordinate；`lifecycle` 聚合面只根据 Agent 状态投影相关路径。
- 本任务不把 artifacts/records 写入语义迁移到 session-owned subtree。
- `RuntimeSessionExecutionAnchor` 上的 orchestration fields 只作为 launch evidence reference；projector 的 node projection 以 orchestration/node coordinate 为事实源。
- Runtime 外部业务入口统一面向 AgentRun；RuntimeSession 只在 message stream / connector trace 语境中出现。

原因：AgentRun 是控制面业务身份，session 是消息流载体。标准化目标是让 projector 基于 AgentRun identity 生成同一个 `lifecycle` aggregate mount 的 projection facts，而不是把不同领域所有权合并到 session 上。

## Planning Status

- 当前状态：planning。
- 下一步：用户 review 规划文档后，进入实现或继续拆子任务。
